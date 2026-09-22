//! An application-owned Python runtime. Published environments are never modified in place.
use crate::error::{RecorderError, RecorderResult};
use serde::{Deserialize, Serialize};
use std::{path::{Path, PathBuf}, process::Stdio, sync::OnceLock, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command, sync::Mutex};

pub const STREAMLINK_VERSION: &str = "8.6.1";
pub const UV_VERSION: &str = "0.12.17";
const UV_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/magekit-uv.bin"));
static INSTALL_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub environment: String,
    pub streamlink_version: String,
    pub uv_version: String,
    pub python: PathBuf,
}

fn root() -> RecorderResult<PathBuf> {
    magekit_shared::utils::get_tools_dir()
        .map(|p| p.join("streamlink"))
        .map_err(|e| RecorderError::ConfigError(e.to_string()))
}

fn python_path(env: &Path) -> PathBuf {
    env.join(if cfg!(windows) { "Scripts/python.exe" } else { "bin/python" })
}

/// Does not access the network or install anything. Incomplete installs are not active.
pub async fn status() -> RecorderResult<Option<RuntimeInfo>> {
    let root = root()?;
    let mut records = match tokio::fs::read_dir(root.join("activations")).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut paths = Vec::new();
    while let Some(entry) = records.next_entry().await? {
        if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
            paths.push(entry.path());
        }
    }
    paths.sort();
    for path in paths.into_iter().rev() {
        let Ok(bytes) = tokio::fs::read(path).await else { continue };
        let Ok(mut info) = serde_json::from_slice::<RuntimeInfo>(&bytes) else { continue };
        // Never execute a path supplied by a manifest outside our own environment directory.
        if !info.environment.starts_with("env-")
            || !info.environment.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            continue;
        }
        info.python = python_path(&root.join(&info.environment));
        if info.python.is_file() { return Ok(Some(info)); }
    }
    Ok(None)
}

async fn uv_path(root: &Path) -> RecorderResult<PathBuf> {
    // Explicit developer override; release builds normally use the embedded executable.
    if let Some(path) = std::env::var_os("MAGEKIT_UV_PATH") {
        let path = PathBuf::from(path);
        if !path.is_file() {
            return Err(RecorderError::ConfigError("MAGEKIT_UV_PATH is not a file".into()));
        }
        return Ok(path);
    }
    if UV_BYTES.is_empty() {
        let name = if cfg!(windows) { "uv.exe" } else { "uv" };
        if let Some(paths) = std::env::var_os("PATH") {
            for directory in std::env::split_paths(&paths) {
                let candidate = directory.join(name);
                if candidate.is_file() { return Ok(candidate); }
            }
        }
        return Err(RecorderError::ConfigError(
            "开发构建未内嵌 uv；请设置 MAGEKIT_UV_PATH，或将 uv 加入 PATH".into()
        ));
    }
    let dir = root.join(format!("uv-{UV_VERSION}-{}-{}", std::env::consts::OS, std::env::consts::ARCH));
    tokio::fs::create_dir_all(&dir).await?;
    tokio::fs::write(dir.join("LICENSE-MIT.txt"), include_str!("../licenses/uv-MIT.txt")).await?;
    let executable = dir.join(if cfg!(windows) { "uv.exe" } else { "uv" });
    if tokio::fs::read(&executable).await.ok().as_deref() == Some(UV_BYTES) {
        return Ok(executable);
    }
    // Write to a unique temporary file so another process never sees half an executable.
    let staged = dir.join(format!("{}.tmp", uuid::Uuid::new_v4()));
    tokio::fs::write(&staged, UV_BYTES).await?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o700)).await?;
    }
    if let Err(e) = tokio::fs::rename(&staged, &executable).await {
        let _ = tokio::fs::remove_file(&staged).await;
        if tokio::fs::read(&executable).await.ok().as_deref() != Some(UV_BYTES) {
            return Err(e.into());
        }
    }
    Ok(executable)
}

fn uv_command(uv: &Path, root: &Path) -> Command {
    let mut cmd = magekit_shared::create_tokio_command(uv);
    cmd.args(["--no-config", "--no-progress"])
        .env("UV_CACHE_DIR", root.join("cache"))
        .env("UV_PYTHON_INSTALL_DIR", root.join("python"))
        .env("UV_PYTHON_INSTALL_REGISTRY", "false")
        .env("UV_PYTHON_INSTALL_BIN", "false")
        .env("UV_PYTHON_PREFERENCE", "only-managed")
        .stdin(Stdio::null()).kill_on_drop(true);
    cmd
}

async fn checked(mut cmd: Command, label: &str) -> RecorderResult<Vec<u8>> {
    let output = tokio::time::timeout(Duration::from_secs(600), cmd.output()).await
        .map_err(|_| RecorderError::ConfigError(format!("{label}: timed out")))??;
    if !output.status.success() {
        // Do not forward subprocess output: index/proxy credentials may be included there.
        return Err(RecorderError::ConfigError(format!(
            "{label} failed (exit {:?}); check network/proxy and available disk space", output.status.code()
        )));
    }
    Ok(output.stdout)
}

/// First installation pins Streamlink. No network requests after a usable install exists.
pub async fn ensure() -> RecorderResult<RuntimeInfo> { install(false).await }

/// Resolve the newest compatible 8.x release in a NEW environment, then publish it.
/// Existing recording/probe processes retain their previous Python path.
pub async fn update() -> RecorderResult<RuntimeInfo> { install(true).await }

async fn install(upgrade: bool) -> RecorderResult<RuntimeInfo> {
    let _guard = INSTALL_LOCK.get_or_init(|| Mutex::new(())).lock().await;
    if !upgrade {
        if let Some(info) = status().await? { return Ok(info); }
    }
    let root = root()?;
    tokio::fs::create_dir_all(root.join("activations")).await?;
    let uv = uv_path(&root).await?;
    let environment = format!("env-{}", uuid::Uuid::new_v4());
    let env_dir = root.join(&environment);
    let result = async {
        let mut create = uv_command(&uv, &root);
        create.args(["venv", "--python", "3.12", "--managed-python"]).arg(&env_dir);
        checked(create, "Create managed Python 3.12 environment").await?;
        let python = python_path(&env_dir);
        let mut pip = uv_command(&uv, &root);
        let requirement = if upgrade { format!("streamlink>={STREAMLINK_VERSION},<9") }
                          else { format!("streamlink=={STREAMLINK_VERSION}") };
        pip.args(["pip", "install", "--python"]).arg(&python)
            .args(["--index-url", "https://pypi.org/simple"])
            .arg(requirement);
        checked(pip, "Install Streamlink").await?;
        let mut smoke = magekit_shared::create_tokio_command(&python);
        smoke.args(["-I", "-c", "import streamlink; from streamlink import Streamlink; Streamlink(); print(streamlink.__version__)"])
            .kill_on_drop(true);
        let version = String::from_utf8_lossy(&checked(smoke, "Streamlink import check").await?).trim().to_owned();
        if !version.starts_with("8.") {
            return Err(RecorderError::ConfigError("Unsupported Streamlink major version".into()));
        }
        let mut uv_version_cmd = uv_command(&uv, &root);
        uv_version_cmd.arg("--version");
        let uv_version = String::from_utf8_lossy(&checked(uv_version_cmd, "uv version check").await?).trim().to_owned();
        let info = RuntimeInfo { environment, streamlink_version: version, uv_version, python };
        // Append-only activation records avoid Windows rename-over-existing problems.
        // The environment itself is NOT renamed (Python venvs contain absolute paths).
        let id = format!("{:020}-{}", chrono::Utc::now().timestamp_micros(), uuid::Uuid::new_v4());
        let tmp = root.join("activations").join(format!("{id}.tmp"));
        let published = tmp.with_extension("json");
        let mut file = tokio::fs::OpenOptions::new().write(true).create_new(true).open(&tmp).await?;
        file.write_all(&serde_json::to_vec_pretty(&info)?).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(tmp, published).await?;
        Ok(info)
    }.await;
    if result.is_err() { let _ = tokio::fs::remove_dir_all(&env_dir).await; }
    result
}

pub(crate) fn worker(info: &RuntimeInfo) -> Command {
    let mut cmd = magekit_shared::create_tokio_command(&info.python);
    cmd.args(["-I", "-u", "-c", include_str!("streamlink_worker.py")])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)] { cmd.process_group(0); }
    cmd
}
