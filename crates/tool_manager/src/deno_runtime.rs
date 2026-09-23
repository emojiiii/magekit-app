//! yt-dlp YouTube EJS 使用的 Deno runtime 管理。

use crate::error::{ToolManagerError, ToolManagerResult};
use magekit_shared::{
    create_tokio_command, get_tools_dir, resolve_deno_path, resolve_deno_path_for_ytdlp,
};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::Mutex;

const LATEST_VERSION_URL: &str = "https://dl.deno.land/release-latest.txt";
const MAX_ARCHIVE_SIZE: usize = 128 * 1024 * 1024;
const MAX_RUNTIME_SIZE: u64 = 512 * 1024 * 1024;
static INSTALL_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// 已检测到的 Deno runtime 信息。
#[derive(Debug, Clone)]
pub struct RuntimeInfo {
    /// Deno 版本。
    pub version: String,
    /// Deno 可执行文件路径。
    pub path: PathBuf,
    /// 是否由 MageKit 管理。
    pub managed: bool,
    /// 版本是否满足 yt-dlp EJS 的最低要求。
    pub supported: bool,
}

fn install_lock() -> &'static Mutex<()> {
    INSTALL_LOCK.get_or_init(|| Mutex::new(()))
}

fn executable_name() -> &'static str {
    if cfg!(windows) { "deno.exe" } else { "deno" }
}

fn install_dir() -> ToolManagerResult<PathBuf> {
    get_tools_dir()
        .map(|path| path.join("deno"))
        .map_err(|error| ToolManagerError::internal(error.to_string()))
}

fn managed_path() -> ToolManagerResult<PathBuf> {
    Ok(install_dir()?.join(executable_name()))
}

fn supported_target() -> ToolManagerResult<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        ("windows", "aarch64") => Ok("aarch64-pc-windows-msvc"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        (os, arch) => Err(ToolManagerError::unsupported_platform(format!(
            "Deno release asset is unavailable for {os}-{arch}"
        ))),
    }
}

fn parse_version(value: &str) -> Option<(u32, u32, u32)> {
    let value = value.trim().strip_prefix('v').unwrap_or(value.trim());
    let mut parts = value.split('.');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.split(['-', '+']).next()?.parse().ok()?,
    ))
}

fn is_supported_version(version: &str) -> bool {
    parse_version(version).is_some_and(|parts| parts >= (2, 3, 0))
}

fn version_tag(value: &str) -> ToolManagerResult<String> {
    let value = value.trim();
    let normalized = value.strip_prefix('v').unwrap_or(value);
    if parse_version(normalized).is_none()
        || !normalized
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
    {
        return Err(ToolManagerError::Config(format!(
            "Invalid Deno release version: {value}"
        )));
    }
    Ok(format!("v{normalized}"))
}

async fn read_version(path: &Path) -> ToolManagerResult<String> {
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        create_tokio_command(path).arg("--version").output(),
    )
    .await
    .map_err(|_| ToolManagerError::Timeout {
        operation: "read Deno version".into(),
    })??;
    if !output.status.success() {
        return Err(ToolManagerError::internal(format!(
            "Deno version command failed with {:?}",
            output.status.code()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .map(str::to_owned)
        .ok_or_else(|| ToolManagerError::internal("Could not parse Deno version output"))
}

/// 检查 MageKit 管理或系统 PATH 中可用的 Deno。
pub async fn status() -> ToolManagerResult<Option<RuntimeInfo>> {
    let Some(path) = resolve_deno_path() else {
        return Ok(None);
    };
    let version = read_version(&path).await?;
    let managed = managed_path().is_ok_and(|managed| managed == path);
    let supported = is_supported_version(&version);
    Ok(Some(RuntimeInfo {
        version,
        path,
        managed,
        supported,
    }))
}

/// 确保存在 yt-dlp EJS 要求的 Deno 2.3 或更新版本。
///
/// 若系统已提供兼容版本则直接复用；否则从 Deno 官方 Releases 下载到 MageKit 工具目录。
pub async fn ensure() -> ToolManagerResult<RuntimeInfo> {
    ensure_for_ytdlp(None).await
}

/// 确保指定 yt-dlp 可执行文件能找到兼容的 Deno runtime。
pub async fn ensure_for_ytdlp(yt_dlp_path: Option<&Path>) -> ToolManagerResult<RuntimeInfo> {
    let _guard = install_lock().lock().await;
    let available = yt_dlp_path
        .and_then(resolve_deno_path_for_ytdlp)
        .or_else(resolve_deno_path);
    if let Some(path) = available
        && let Ok(version) = read_version(&path).await
        && is_supported_version(&version)
    {
        return Ok(RuntimeInfo {
            managed: managed_path().is_ok_and(|managed| managed == path),
            supported: true,
            path,
            version,
        });
    }
    install_latest().await
}

/// 安装或修复 MageKit 管理的 Deno 到最新官方版本。
pub async fn update() -> ToolManagerResult<RuntimeInfo> {
    let _guard = install_lock().lock().await;
    install_latest().await
}

async fn install_latest() -> ToolManagerResult<RuntimeInfo> {
    let target = supported_target()?;
    let directory = install_dir()?;
    tokio::fs::create_dir_all(&directory).await?;

    let client = reqwest::Client::builder()
        .user_agent("MageKit/1.0")
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(180))
        .build()?;

    let release = client
        .get(LATEST_VERSION_URL)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let tag = version_tag(&release)?;
    let file_name = format!("deno-{target}.zip");
    let base_url = format!("https://github.com/denoland/deno/releases/download/{tag}/{file_name}");

    let expected_hash = client
        .get(format!("{base_url}.sha256sum"))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?
        .split_whitespace()
        .next()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| ToolManagerError::internal("Invalid Deno SHA-256 sidecar"))?;

    let response = client.get(&base_url).send().await?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_ARCHIVE_SIZE as u64)
    {
        return Err(ToolManagerError::internal(
            "Deno release archive exceeds the size limit",
        ));
    }
    let archive = response.bytes().await?;
    if archive.len() > MAX_ARCHIVE_SIZE {
        return Err(ToolManagerError::internal(
            "Deno release archive exceeds the size limit",
        ));
    }
    let actual_hash = format!("{:x}", Sha256::digest(&archive));
    if actual_hash != expected_hash {
        return Err(ToolManagerError::internal(
            "Deno release archive SHA-256 does not match the official checksum",
        ));
    }

    let stage = tempfile::Builder::new()
        .prefix(".deno-stage-")
        .tempdir_in(&directory)
        .map_err(|error| ToolManagerError::internal(error.to_string()))?;
    let archive = archive.to_vec();
    let staged_path = stage.path().join(executable_name());
    let extract_path = staged_path.clone();
    let archive_name = executable_name().to_owned();
    tokio::task::spawn_blocking(move || -> ToolManagerResult<()> {
        let mut archive = zip::ZipArchive::new(Cursor::new(archive))
            .map_err(|error| ToolManagerError::internal(format!("Invalid Deno ZIP: {error}")))?;
        let mut executable = archive.by_name(&archive_name).map_err(|error| {
            ToolManagerError::internal(format!("Deno executable missing from ZIP: {error}"))
        })?;
        if executable.size() > MAX_RUNTIME_SIZE {
            return Err(ToolManagerError::internal(
                "Deno executable exceeds the size limit",
            ));
        }
        let mut output = std::fs::File::create(extract_path)?;
        std::io::copy(&mut executable, &mut output)?;
        output.flush()?;
        output.sync_all()?;
        Ok(())
    })
    .await
    .map_err(|error| {
        ToolManagerError::internal(format!("Deno extraction task failed: {error}"))
    })??;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = tokio::fs::metadata(&staged_path).await?.permissions();
        permissions.set_mode(0o755);
        tokio::fs::set_permissions(&staged_path, permissions).await?;
    }

    let installed_version = read_version(&staged_path).await?;
    let released_version = tag.trim_start_matches('v');
    if installed_version != released_version || !is_supported_version(&installed_version) {
        return Err(ToolManagerError::internal(format!(
            "Downloaded Deno version {installed_version} does not match release {released_version}"
        )));
    }

    let final_path = directory.join(executable_name());
    let backup_path = directory.join(format!(".deno-backup-{}", uuid::Uuid::new_v4()));
    let had_previous = final_path.exists();
    if had_previous {
        tokio::fs::rename(&final_path, &backup_path).await?;
    }
    if let Err(error) = tokio::fs::rename(&staged_path, &final_path).await {
        if had_previous {
            let _ = tokio::fs::rename(&backup_path, &final_path).await;
        }
        return Err(ToolManagerError::file_operation_failed(
            "publish Deno runtime",
            error.to_string(),
        ));
    }
    if had_previous {
        let _ = tokio::fs::remove_file(&backup_path).await;
    }

    tracing::info!("✅ Installed Deno {installed_version} at {:?}", final_path);
    Ok(RuntimeInfo {
        version: installed_version,
        path: final_path,
        managed: true,
        supported: true,
    })
}
