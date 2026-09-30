//! yt-dlp YouTube EJS 使用的 Deno runtime 管理。

use crate::error::{ToolManagerError, ToolManagerResult};
use magekit_shared::{
    create_tokio_command, get_tools_dir, resolve_deno_path, resolve_deno_path_for_ytdlp,
};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::Mutex;

const LATEST_VERSION_URL: &str = "https://dl.deno.land/release-latest.txt";
const MAX_VERSION_SIZE: usize = 128;
const MAX_CHECKSUM_SIZE: usize = 4096;
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
    let version = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.split(['-', '+']).next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(version)
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

/// 官方 Unix sidecar 来自 shasum；Windows 则来自 Get-FileHash | Format-List。
/// 严格校验算法、文件名与唯一哈希，不能从任意响应（例如 HTML）中搜寻哈希。
fn parse_checksum(sidecar: &[u8], file_name: &str) -> ToolManagerResult<String> {
    let invalid = || {
        ToolManagerError::internal(format!(
            "Invalid Deno SHA-256 sidecar for {file_name}: expected an official shasum or PowerShell checksum"
        ))
    };
    if sidecar.len() > MAX_CHECKSUM_SIZE {
        return Err(invalid());
    }
    let text = std::str::from_utf8(sidecar).map_err(|_| invalid())?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let hash = match lines.as_slice() {
        [line] => {
            let mut fields = line.split_ascii_whitespace();
            let hash = fields.next().ok_or_else(invalid)?;
            let name = fields.next().ok_or_else(invalid)?;
            if name.strip_prefix('*').unwrap_or(name) != file_name || fields.next().is_some() {
                return Err(invalid());
            }
            hash
        }
        [_, _, _] => {
            let mut algorithm = None;
            let mut hash = None;
            let mut path = None;
            for line in lines {
                let (key, value) = line.split_once(':').ok_or_else(invalid)?;
                let field = match key.trim() {
                    "Algorithm" => &mut algorithm,
                    "Hash" => &mut hash,
                    "Path" => &mut path,
                    _ => return Err(invalid()),
                };
                if field.replace(value.trim()).is_some() {
                    return Err(invalid());
                }
            }
            if algorithm != Some("SHA256")
                || path.and_then(|path| path.rsplit(['/', '\\']).next()) != Some(file_name)
            {
                return Err(invalid());
            }
            hash.ok_or_else(invalid)?
        }
        _ => return Err(invalid()),
    };
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    Ok(hash.to_ascii_lowercase())
}

/// 在读取过程中限制响应体大小，也覆盖没有 Content-Length 的分块响应。
async fn download_bounded(
    client: &reqwest::Client,
    url: &str,
    limit: usize,
    description: &str,
) -> ToolManagerResult<Vec<u8>> {
    let too_large = || ToolManagerError::internal(format!("{description} exceeds the size limit"));
    let mut response = client.get(url).send().await?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err(too_large());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if chunk.len() > limit - body.len() {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

async fn read_version(path: &Path) -> ToolManagerResult<String> {
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        create_tokio_command(path)
            .arg("--version")
            .kill_on_drop(true)
            .output(),
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

    let release = download_bounded(
        &client,
        LATEST_VERSION_URL,
        MAX_VERSION_SIZE,
        "Deno release version",
    )
    .await?;
    let release = std::str::from_utf8(&release)
        .map_err(|_| ToolManagerError::internal("Invalid Deno release version encoding"))?;
    let tag = version_tag(release)?;
    let file_name = format!("deno-{target}.zip");
    let base_url = format!("https://github.com/denoland/deno/releases/download/{tag}/{file_name}");

    let sidecar = download_bounded(
        &client,
        &format!("{base_url}.sha256sum"),
        MAX_CHECKSUM_SIZE,
        "Deno SHA-256 sidecar",
    )
    .await?;
    let expected_hash = parse_checksum(&sidecar, &file_name)?;

    let archive =
        download_bounded(&client, &base_url, MAX_ARCHIVE_SIZE, "Deno release archive").await?;
    install_archive(&directory, &tag, archive, &expected_hash).await
}

async fn install_archive(
    directory: &Path,
    tag: &str,
    archive: Vec<u8>,
    expected_hash: &str,
) -> ToolManagerResult<RuntimeInfo> {
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
        .tempdir_in(directory)
        .map_err(|error| ToolManagerError::internal(error.to_string()))?;
    let staged_path = stage.path().join(executable_name());
    let extract_path = staged_path.clone();
    let archive_name = executable_name().to_owned();
    tokio::task::spawn_blocking(move || -> ToolManagerResult<()> {
        let mut archive = zip::ZipArchive::new(Cursor::new(archive))
            .map_err(|error| ToolManagerError::internal(format!("Invalid Deno ZIP: {error}")))?;
        let executable = archive.by_name(&archive_name).map_err(|error| {
            ToolManagerError::internal(format!("Deno executable missing from ZIP: {error}"))
        })?;
        let expected_size = executable.size();
        if expected_size > MAX_RUNTIME_SIZE {
            return Err(ToolManagerError::internal(
                "Deno executable exceeds the size limit",
            ));
        }
        let mut output = std::fs::File::create(extract_path)?;
        let written = std::io::copy(&mut executable.take(MAX_RUNTIME_SIZE + 1), &mut output)?;
        if written > MAX_RUNTIME_SIZE || written != expected_size {
            return Err(ToolManagerError::internal("Invalid Deno executable size"));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const WINDOWS_ARCHIVE: &str = "deno-x86_64-pc-windows-msvc.zip";
    const WINDOWS_HASH: &str = "a0c3101b4158d1dfb7d6a78a7bf0f3de80c96bb423c152beec8beb22786f2238";
    // 官方 v2.9.7 的原始 sidecar，保留 PowerShell 输出的空行、CRLF 与构建机路径。
    const WINDOWS_SIDECAR: &str = concat!(
        "\r\nAlgorithm : SHA256\r\n",
        "Hash      : A0C3101B4158D1DFB7D6A78A7BF0F3DE80C96BB423C152BEEC8BEB22786F2238\r\n",
        "Path      : C:\\a\\deno\\deno\\target\\release\\deno-x86_64-pc-windows-msvc.zip\r\n\r\n",
    );

    #[test]
    fn official_windows_checksums_support_both_architectures() {
        assert_eq!(
            parse_checksum(WINDOWS_SIDECAR.as_bytes(), WINDOWS_ARCHIVE).unwrap(),
            WINDOWS_HASH
        );
        let arm = WINDOWS_SIDECAR.replace("x86_64", "aarch64").replace(
            &WINDOWS_HASH.to_ascii_uppercase(),
            "C4C4AC8BFDAA37814BDA5C05FC9CDF2154904E2EF8277673A30BDCEAAA649807",
        );
        assert_eq!(
            parse_checksum(arm.as_bytes(), "deno-aarch64-pc-windows-msvc.zip").unwrap(),
            "c4c4ac8bfdaa37814bda5c05fc9cdf2154904e2ef8277673a30bdceaaa649807"
        );
    }

    #[test]
    fn official_unix_checksums_bind_hash_to_archive() {
        for (target, hash) in [
            (
                "x86_64-unknown-linux-gnu",
                "c6527f24f4b16031d3ae4fa9f658d5f11534c8d84ce7dc8502420280919c3490",
            ),
            (
                "x86_64-apple-darwin",
                "95daaff11c116a52ad54785e7914c8e9c9cdcaba793c5ed929c74ca2d8e6259a",
            ),
            (
                "aarch64-apple-darwin",
                "5cd46d6268f6f78f5d88bdc7159d20bd44cdaa4b3303474839f87ec6fe7ae25c",
            ),
        ] {
            let name = format!("deno-{target}.zip");
            assert_eq!(
                parse_checksum(format!("{hash}  {name}\n").as_bytes(), &name).unwrap(),
                hash
            );
        }
    }

    #[test]
    fn checksum_allows_one_utf8_bom_and_shasum_binary_marker() {
        for sidecar in [
            format!("\u{feff}{WINDOWS_SIDECAR}"),
            format!("\u{feff}{WINDOWS_HASH} *{WINDOWS_ARCHIVE}\r\n"),
        ] {
            assert_eq!(
                parse_checksum(sidecar.as_bytes(), WINDOWS_ARCHIVE).unwrap(),
                WINDOWS_HASH
            );
        }
    }

    #[test]
    fn checksum_rejects_wrong_algorithm_filename_or_ambiguous_fields() {
        for sidecar in [
            WINDOWS_SIDECAR.replace("SHA256", "SHA512"),
            WINDOWS_SIDECAR.replace("x86_64", "aarch64"),
            WINDOWS_SIDECAR.replace("Hash      :", "Algorithm :"),
            format!("{WINDOWS_SIDECAR}Hash : {WINDOWS_HASH}\r\n"),
            WINDOWS_SIDECAR.replace("Path      :", "Unexpected :"),
            format!("{WINDOWS_HASH} denort-x86_64-pc-windows-msvc.zip"),
            format!("{WINDOWS_HASH} {WINDOWS_ARCHIVE}\n{WINDOWS_HASH} {WINDOWS_ARCHIVE}"),
            format!("{WINDOWS_HASH} {WINDOWS_ARCHIVE} extra"),
        ] {
            assert!(
                parse_checksum(sidecar.as_bytes(), WINDOWS_ARCHIVE).is_err(),
                "{sidecar:?}"
            );
        }
    }

    #[test]
    fn checksum_rejects_malformed_unrecognized_and_oversized_responses() {
        for sidecar in [
            String::new(),
            WINDOWS_HASH.to_owned(),
            format!("<html>{WINDOWS_HASH} {WINDOWS_ARCHIVE}</html>"),
            format!("SHA256 ({WINDOWS_ARCHIVE}) = {WINDOWS_HASH}"),
            format!("{}  {WINDOWS_ARCHIVE}", "z".repeat(64)),
            format!("{}  {WINDOWS_ARCHIVE}", "0".repeat(63)),
            format!("{}  {WINDOWS_ARCHIVE}", "0".repeat(65)),
            format!("\u{feff}\u{feff}{WINDOWS_SIDECAR}"),
            format!("{WINDOWS_SIDECAR}{}", " ".repeat(MAX_CHECKSUM_SIZE)),
        ] {
            assert!(parse_checksum(sidecar.as_bytes(), WINDOWS_ARCHIVE).is_err());
        }
        assert!(parse_checksum(&[0xff, 0xfe, 0], WINDOWS_ARCHIVE).is_err());
    }

    #[test]
    fn release_version_requires_exactly_three_numeric_components() {
        assert_eq!(version_tag(" v2.9.7\n").unwrap(), "v2.9.7");
        assert_eq!(version_tag("2.3.0").unwrap(), "v2.3.0");
        assert!(is_supported_version("2.3.0"));
        assert!(!is_supported_version("2.2.9"));
        for value in [
            "2.9",
            "2.9.7.1",
            "2.9.7.",
            "2.9.7-rc.1",
            "2.9.7/../x",
            "2..7",
            "garbage",
        ] {
            assert!(version_tag(value).is_err(), "{value}");
        }
    }

    async fn serve_response(response: &'static [u8]) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/checksum", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            socket.read(&mut request).await.unwrap();
            socket.write_all(response).await.unwrap();
        });
        (url, server)
    }

    #[tokio::test]
    async fn download_bounds_declared_chunked_and_unknown_length_bodies() {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        for response in [
            &b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\n123456"[..],
            &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\n123\r\n3\r\n456\r\n0\r\n\r\n"[..],
            &b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n123456"[..],
        ] {
            let (url, server) = serve_response(response).await;
            let error = download_bounded(&client, &url, 5, "Deno test response").await.unwrap_err();
            assert!(error.to_string().contains("exceeds the size limit"), "{error}");
            server.await.unwrap();
        }
        let (url, server) = serve_response(
            b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n12345",
        )
        .await;
        assert_eq!(
            download_bounded(&client, &url, 5, "Deno test response")
                .await
                .unwrap(),
            b"12345"
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn download_rejects_http_errors_and_truncated_bodies() {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        for response in [
            &b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"[..],
            &b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\n123"[..],
        ] {
            let (url, server) = serve_response(response).await;
            assert!(
                download_bounded(&client, &url, 10, "Deno test response")
                    .await
                    .is_err()
            );
            server.await.unwrap();
        }
    }

    fn zip_with_file(name: &str, content: &[u8]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(content).unwrap();
        zip.finish().unwrap().into_inner()
    }

    async fn assert_install_failure_preserves_runtime(archive: Vec<u8>, hash: &str, error: &str) {
        let directory = tempfile::tempdir().unwrap();
        let previous = directory.path().join(executable_name());
        std::fs::write(&previous, b"previous working runtime").unwrap();
        let result = install_archive(directory.path(), "v2.9.7", archive, hash)
            .await
            .unwrap_err();
        assert!(result.to_string().contains(error), "{result}");
        assert_eq!(
            std::fs::read(&previous).unwrap(),
            b"previous working runtime"
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn checksum_failure_preserves_installed_runtime() {
        let archive = zip_with_file(executable_name(), b"unverified executable");
        assert_install_failure_preserves_runtime(
            archive,
            &"0".repeat(64),
            "SHA-256 does not match",
        )
        .await;
    }

    #[tokio::test]
    async fn malformed_archives_preserve_installed_runtime() {
        for (archive, error) in [
            (b"not a ZIP file".to_vec(), "Invalid Deno ZIP"),
            (
                zip_with_file("unexpected/deno", b"wrong member"),
                "Deno executable missing",
            ),
        ] {
            let hash = format!("{:x}", Sha256::digest(&archive));
            assert_install_failure_preserves_runtime(archive, &hash, error).await;
        }
    }

    #[tokio::test]
    async fn oversized_zip_member_preserves_installed_runtime() {
        let mut archive = zip_with_file(executable_name(), b"small fixture");
        let directory = archive
            .windows(4)
            .position(|bytes| bytes == b"PK\x01\x02")
            .unwrap();
        archive[directory + 24..directory + 28]
            .copy_from_slice(&((MAX_RUNTIME_SIZE + 1) as u32).to_le_bytes());
        let hash = format!("{:x}", Sha256::digest(&archive));
        assert_install_failure_preserves_runtime(
            archive,
            &hash,
            "Deno executable exceeds the size limit",
        )
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn staged_version_mismatch_preserves_installed_runtime() {
        let archive = zip_with_file("deno", b"#!/bin/sh\nprintf 'deno 2.9.6\\n'\n");
        let hash = format!("{:x}", Sha256::digest(&archive));
        assert_install_failure_preserves_runtime(archive, &hash, "does not match release 2.9.7")
            .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn verified_runtime_replaces_previous_installation() {
        let directory = tempfile::tempdir().unwrap();
        let previous = directory.path().join("deno");
        std::fs::write(&previous, b"previous runtime").unwrap();
        let content = b"#!/bin/sh\nprintf 'deno 2.9.7\\nv8 fixture\\ntypescript fixture\\n'\n";
        let archive = zip_with_file("deno", content);
        let hash = format!("{:x}", Sha256::digest(&archive));
        let installed = install_archive(directory.path(), "v2.9.7", archive, &hash)
            .await
            .unwrap();
        assert_eq!(installed.path, previous);
        assert_eq!(installed.version, "2.9.7");
        assert!(installed.supported && installed.managed);
        assert_eq!(std::fs::read(&previous).unwrap(), content);
        assert_eq!(read_version(&previous).await.unwrap(), "2.9.7");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    // 手动验证真实官方资源；普通 CI 不依赖外网，也不修改用户的工具目录。
    #[tokio::test]
    #[ignore = "downloads and executes the official Deno v2.9.7 release in a temporary directory"]
    async fn live_official_release_installs_in_temporary_directory() {
        let directory = tempfile::tempdir().unwrap();
        let client = reqwest::Client::builder()
            .user_agent("MageKit/1.0")
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(180))
            .build()
            .unwrap();
        let file_name = format!("deno-{}.zip", supported_target().unwrap());
        let url = format!("https://github.com/denoland/deno/releases/download/v2.9.7/{file_name}");
        let sidecar = download_bounded(
            &client,
            &format!("{url}.sha256sum"),
            MAX_CHECKSUM_SIZE,
            "Deno SHA-256 sidecar",
        )
        .await
        .unwrap();
        let hash = parse_checksum(&sidecar, &file_name).unwrap();
        let archive = download_bounded(&client, &url, MAX_ARCHIVE_SIZE, "Deno release archive")
            .await
            .unwrap();
        let installed = install_archive(directory.path(), "v2.9.7", archive, &hash)
            .await
            .unwrap();
        assert_eq!(installed.version, "2.9.7");
        assert_eq!(read_version(&installed.path).await.unwrap(), "2.9.7");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
