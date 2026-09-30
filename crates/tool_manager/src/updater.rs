use crate::error::{ToolManagerError, ToolManagerResult};
use crate::storage::ToolStorage;
use futures_util::StreamExt;
use magekit_shared::{
    ToolType, UpdateChannel, create_tokio_command, resolve_ffmpeg_path, resolve_yt_dlp_path,
};
use sha2::{Digest, Sha256};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncWriteExt;

const MAX_YTDLP_SIZE: u64 = 128 * 1024 * 1024;
const MAX_CHECKSUM_SIZE: usize = 64 * 1024;
static YTDLP_INSTALL_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn release_repository(channel: &UpdateChannel) -> ToolManagerResult<&'static str> {
    match channel {
        UpdateChannel::Stable => Ok("yt-dlp/yt-dlp"),
        UpdateChannel::Nightly => Ok("yt-dlp/yt-dlp-nightly-builds"),
        UpdateChannel::Custom(_) => Err(ToolManagerError::config(
            "Custom update channel is not supported",
        )),
    }
}

fn asset_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        "yt-dlp_arm64.exe"
    } else if cfg!(all(windows, target_arch = "x86")) {
        "yt-dlp_x86.exe"
    } else if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

fn checksum_for_asset(manifest: &str, asset: &str) -> ToolManagerResult<String> {
    let mut matches = manifest.lines().filter_map(|line| {
        let mut fields = line.split_whitespace();
        let hash = fields.next()?;
        let name = fields.next()?.trim_start_matches('*');
        (name == asset
            && fields.next().is_none()
            && hash.len() == 64
            && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| hash.to_ascii_lowercase())
    });
    let hash = matches.next().ok_or_else(|| {
        ToolManagerError::installation_failed(
            "yt-dlp",
            "Official SHA-256 checksum is missing or invalid",
        )
    })?;
    if matches.next().is_some() {
        return Err(ToolManagerError::installation_failed(
            "yt-dlp",
            "Duplicate SHA-256 entries",
        ));
    }
    Ok(hash)
}

/// 下载进度回调类型
pub type ProgressCallback = Arc<dyn Fn(u64, u64, u64) + Send + Sync>;

/// 工具更新器
pub struct ToolUpdater {
    storage: ToolStorage,
}

impl ToolUpdater {
    /// 创建新的工具更新器
    pub fn new(storage: ToolStorage) -> Self {
        Self { storage }
    }

    /// 检查并下载yt-dlp
    pub async fn ensure_yt_dlp(&self, channel: UpdateChannel) -> ToolManagerResult<()> {
        self.ensure_yt_dlp_with_progress(channel, None).await
    }

    /// 检查并下载yt-dlp (带进度回调)
    pub async fn ensure_yt_dlp_with_progress(
        &self,
        channel: UpdateChannel,
        progress_callback: Option<ProgressCallback>,
    ) -> ToolManagerResult<()> {
        let has_progress = progress_callback.is_some();

        // 已安装且已是最新版本时直接返回；否则执行覆盖式更新。
        if self.storage.is_tool_installed(ToolType::YtDlp).await {
            let current = self
                .storage
                .get_tool_version(ToolType::YtDlp)
                .await
                .ok()
                .flatten();
            let latest = match self.get_latest_yt_dlp_version(channel.clone()).await {
                Ok(v) => v,
                Err(e) => {
                    // 无法获取最新版本：非交互场景不阻断；交互场景允许继续走覆盖式下载。
                    tracing::warn!("yt-dlp version check failed: {}", e);
                    if !has_progress {
                        return Ok(());
                    }
                    None
                }
            };

            // 已安装但无法解析版本 / 无法获取最新版本：非交互场景不强行更新，避免“每次确保工具都重下”。
            let (Some(current), Some(latest)) = (current.as_deref(), latest.as_deref()) else {
                if !has_progress {
                    tracing::info!(
                        "yt-dlp already installed (skip update check due to missing version)"
                    );
                    return Ok(());
                }
                // 交互场景（UI 点击“更新/安装”）允许继续走覆盖式下载
                tracing::info!("yt-dlp already installed (force reinstall due to missing version)");
                return self
                    .download_yt_dlp_with_progress(channel, progress_callback)
                    .await;
            };

            if !self.is_version_newer(latest, current) {
                tracing::info!("yt-dlp already up-to-date: {}", current);
                return Ok(());
            }
            tracing::info!("yt-dlp update available: {} -> {}", current, latest);
        }

        tracing::info!("Installing/updating yt-dlp...");
        self.download_yt_dlp_with_progress(channel, progress_callback)
            .await
    }

    /// 检查并下载ffmpeg
    pub async fn ensure_ffmpeg(&self) -> ToolManagerResult<()> {
        // 只要能解析到可执行文件（应用内 tools 或系统 PATH）即可视为可用，避免反复执行 brew/apt。
        if resolve_ffmpeg_path().is_some() {
            tracing::info!("ffmpeg already available (tools dir or system PATH)");
            return Ok(());
        }

        tracing::info!("Installing ffmpeg...");
        self.download_ffmpeg().await
    }

    /// 确保所有工具都已安装
    pub async fn ensure_all_tools(&self, channel: UpdateChannel) -> ToolManagerResult<()> {
        self.ensure_yt_dlp(channel).await?;
        self.ensure_ffmpeg().await?;
        Ok(())
    }

    /// 检查工具更新
    pub async fn check_for_updates(&self, channel: UpdateChannel) -> ToolManagerResult<UpdateInfo> {
        let mut update_info = UpdateInfo::new();

        // 检查 yt-dlp 更新（优先应用内 tools，其次系统 PATH）
        let current_yt_dlp = self.get_installed_tool_version(ToolType::YtDlp).await?;
        let latest_yt_dlp = self.get_latest_yt_dlp_version(channel).await?;
        if let (Some(current), Some(latest)) = (current_yt_dlp, latest_yt_dlp) {
            if self.is_version_newer(&latest, &current) {
                update_info.yt_dlp_update = Some(ToolUpdate { current, latest });
            }
        }

        // 检查 ffmpeg（目前仅提示“未安装”）
        // ffmpeg 更新：暂不支持可靠的“最新版本”判断；因此这里只做占位，不提示更新。

        Ok(update_info)
    }

    /// 下载yt-dlp (无进度回调)
    async fn download_yt_dlp(&self, channel: UpdateChannel) -> ToolManagerResult<()> {
        self.download_yt_dlp_with_progress(channel, None).await
    }

    /// 验证同一发布版本的 SHA-256 后再原子替换，失败不破坏现有工具。
    async fn download_yt_dlp_with_progress(
        &self,
        channel: UpdateChannel,
        progress_callback: Option<ProgressCallback>,
    ) -> ToolManagerResult<()> {
        let _guard = YTDLP_INSTALL_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        let repository = release_repository(&channel)?;
        let version = self
            .get_latest_yt_dlp_version(channel)
            .await?
            .ok_or_else(|| {
                ToolManagerError::installation_failed("yt-dlp", "Release version is unavailable")
            })?;
        let asset = asset_name();
        let release_base = format!("https://github.com/{repository}/releases/download/{version}");
        let client = reqwest::Client::builder()
            .user_agent("MageKit/1.0")
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(180))
            .build()?;

        let mut checksums = client
            .get(format!("{release_base}/SHA2-256SUMS"))
            .send()
            .await?
            .error_for_status()?;
        let mut manifest = Vec::new();
        while let Some(chunk) = checksums.chunk().await? {
            if manifest.len().saturating_add(chunk.len()) > MAX_CHECKSUM_SIZE {
                return Err(ToolManagerError::installation_failed(
                    "yt-dlp",
                    "Checksum manifest exceeds size limit",
                ));
            }
            manifest.extend_from_slice(&chunk);
        }
        let manifest = std::str::from_utf8(&manifest).map_err(|_| {
            ToolManagerError::installation_failed("yt-dlp", "Invalid checksum manifest encoding")
        })?;
        let expected_hash = checksum_for_asset(manifest, asset)?;

        let response = client
            .get(format!("{release_base}/{asset}"))
            .send()
            .await?
            .error_for_status()?;
        let total_size = response.content_length().unwrap_or(0);
        if total_size > MAX_YTDLP_SIZE {
            return Err(ToolManagerError::installation_failed(
                "yt-dlp",
                "Release asset exceeds size limit",
            ));
        }
        fs::create_dir_all(self.storage.tools_dir()).await?;
        // 唯一、受保护、同文件系统的暂存文件，避免共享 /tmp 文件竞态及跨设备 rename。
        let staged = tempfile::Builder::new()
            .prefix(".yt-dlp-stage-")
            .tempfile_in(self.storage.tools_dir())?;
        let mut file = tokio::fs::File::from_std(staged.reopen()?);
        let mut hasher = Sha256::new();
        let mut downloaded = 0u64;
        let mut stream = response.bytes_stream();
        let mut last_progress_time = std::time::Instant::now();
        let mut last_downloaded = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            if downloaded > MAX_YTDLP_SIZE {
                return Err(ToolManagerError::installation_failed(
                    "yt-dlp",
                    "Release asset exceeds size limit",
                ));
            }
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            let elapsed = last_progress_time.elapsed();
            if elapsed >= Duration::from_millis(100) {
                let speed = ((downloaded - last_downloaded) as f64 / elapsed.as_secs_f64()) as u64;
                if let Some(callback) = &progress_callback {
                    callback(downloaded, total_size, speed);
                }
                last_progress_time = std::time::Instant::now();
                last_downloaded = downloaded;
            }
        }
        if downloaded == 0 || format!("{:x}", hasher.finalize()) != expected_hash {
            return Err(ToolManagerError::installation_failed(
                "yt-dlp",
                "Release asset SHA-256 verification failed",
            ));
        }
        file.flush().await?;
        file.sync_all().await?;
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(staged.path(), std::fs::Permissions::from_mode(0o755)).await?;
        }
        if let Some(callback) = &progress_callback {
            callback(downloaded, total_size, u64::MAX);
        }
        let final_path = self.storage.get_tool_path(ToolType::YtDlp);
        staged.persist(&final_path).map_err(|error| {
            ToolManagerError::file_operation_failed(
                "publish verified yt-dlp",
                error.error.to_string(),
            )
        })?;
        tracing::info!("✅ Verified yt-dlp {version} installed at {:?}", final_path);
        Ok(())
    }

    /// 下载ffmpeg
    async fn download_ffmpeg(&self) -> ToolManagerResult<()> {
        // 这里实现一个简化版本，实际应用中可能需要根据平台下载不同的二进制
        // 或者使用系统的包管理器

        #[cfg(windows)]
        {
            self.download_ffmpeg_windows().await
        }
        #[cfg(target_os = "macos")]
        {
            self.download_ffmpeg_macos().await
        }
        #[cfg(target_os = "linux")]
        {
            self.download_ffmpeg_linux().await
        }
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            Err(ToolManagerError::unsupported_platform(std::env::consts::OS))
        }
    }

    #[cfg(windows)]
    async fn download_ffmpeg_windows(&self) -> ToolManagerResult<()> {
        // Windows版本的ffmpeg下载实现
        // 这里可以下载预编译的Windows二进制或使用winget/chocolatey
        tracing::info!(
            "Please install ffmpeg manually on Windows or use winget: winget install Gyan.FFmpeg"
        );
        Ok(())
    }

    #[cfg(target_os = "macos")]
    async fn download_ffmpeg_macos(&self) -> ToolManagerResult<()> {
        // macOS版本可以使用brew安装
        let output = create_tokio_command("brew")
            .arg("install")
            .arg("ffmpeg")
            .output()
            .await
            .map_err(|e| ToolManagerError::process_failed("brew install ffmpeg", e.to_string()))?;

        if output.status.success() {
            tracing::info!("ffmpeg installed via brew");
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ToolManagerError::installation_failed(
                "ffmpeg",
                stderr.to_string(),
            ))
        }
    }

    #[cfg(target_os = "linux")]
    async fn download_ffmpeg_linux(&self) -> ToolManagerResult<()> {
        // Linux版本可以使用apt/yum等包管理器
        let output = create_tokio_command("apt")
            .arg("install")
            .arg("-y")
            .arg("ffmpeg")
            .output()
            .await
            .map_err(|e| ToolManagerError::process_failed("apt install ffmpeg", e.to_string()))?;

        if output.status.success() {
            tracing::info!("ffmpeg installed via apt");
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ToolManagerError::installation_failed(
                "ffmpeg",
                stderr.to_string(),
            ))
        }
    }

    /// 从官方 release 重定向读取版本，并绑定准确仓库及安全 tag。
    async fn get_latest_yt_dlp_version(
        &self,
        channel: UpdateChannel,
    ) -> ToolManagerResult<Option<String>> {
        let repository = release_repository(&channel)?;
        let client = reqwest::Client::builder()
            .user_agent("MageKit/1.0")
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(30))
            .build()?;
        let response = client
            .get(format!("https://github.com/{repository}/releases/latest"))
            .send()
            .await?
            .error_for_status()?;
        let url = response.url();
        let prefix = format!("/{repository}/releases/tag/");
        let tag = url.path().strip_prefix(&prefix).filter(|tag| {
            !tag.is_empty()
                && tag
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        });
        if url.scheme() != "https" || url.host_str() != Some("github.com") || tag.is_none() {
            return Err(ToolManagerError::version_check_failed(
                "yt-dlp",
                "Invalid official release redirect",
            ));
        }
        Ok(tag.map(str::to_owned))
    }

    /// 比较版本号
    fn is_version_newer(&self, latest: &str, current: &str) -> bool {
        fn parse(v: &str) -> Option<Vec<u32>> {
            let mut out = Vec::new();
            for part in v.split('.') {
                let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
                if digits.is_empty() {
                    return None;
                }
                out.push(digits.parse::<u32>().ok()?);
            }
            Some(out)
        }

        match (parse(latest), parse(current)) {
            (Some(a), Some(b)) => a > b,
            _ => latest != current,
        }
    }

    async fn get_installed_tool_version(
        &self,
        tool_type: ToolType,
    ) -> ToolManagerResult<Option<String>> {
        // 1) 优先应用内 tools 目录
        if let Ok(Some(v)) = self.storage.get_tool_version(tool_type).await {
            return Ok(Some(v));
        }

        // 2) 退回系统 PATH
        let path = match tool_type {
            ToolType::YtDlp => resolve_yt_dlp_path(),
            ToolType::Ffmpeg => resolve_ffmpeg_path(),
        };
        let Some(path) = path else { return Ok(None) };

        self.get_tool_version_from_path(tool_type, &path).await
    }

    async fn get_tool_version_from_path(
        &self,
        tool_type: ToolType,
        path: &std::path::Path,
    ) -> ToolManagerResult<Option<String>> {
        let mut cmd = create_tokio_command(path);
        match tool_type {
            ToolType::YtDlp => {
                cmd.arg("--version");
            }
            ToolType::Ffmpeg => {
                cmd.arg("-version");
            }
        }

        let output = cmd.output().await.map_err(|e| {
            ToolManagerError::process_failed(path.display().to_string(), e.to_string())
        })?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(match tool_type {
            ToolType::YtDlp => stdout
                .lines()
                .next()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
            ToolType::Ffmpeg => stdout
                .lines()
                .find(|line| line.contains("ffmpeg version"))
                .and_then(|line| line.split("version").nth(1))
                .and_then(|rest| rest.split_whitespace().next())
                .map(|v| v.to_string()),
        })
    }
}

/// 工具更新信息
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub yt_dlp_update: Option<ToolUpdate>,
    pub ffmpeg_update: Option<ToolUpdate>,
}

impl UpdateInfo {
    pub fn new() -> Self {
        Self {
            yt_dlp_update: None,
            ffmpeg_update: None,
        }
    }

    pub fn has_updates(&self) -> bool {
        self.yt_dlp_update.is_some() || self.ffmpeg_update.is_some()
    }
}

/// 工具更新信息
#[derive(Debug, Clone)]
pub struct ToolUpdate {
    pub current: String,
    pub latest: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_selects_exact_asset_and_rejects_invalid_manifests() {
        let hash = "a".repeat(64);
        let manifest = format!("{hash}  yt-dlp.exe\n{hash} *yt-dlp\n");
        assert_eq!(checksum_for_asset(&manifest, "yt-dlp").unwrap(), hash);
        assert!(checksum_for_asset(&manifest, "yt-dlp_macos").is_err());
        assert!(checksum_for_asset("invalid yt-dlp", "yt-dlp").is_err());
        assert!(checksum_for_asset(&format!("{hash} yt-dlp\n{hash} yt-dlp"), "yt-dlp").is_err());
        assert!(checksum_for_asset(&format!("{hash} yt-dlp.sig"), "yt-dlp").is_err());
    }

    #[test]
    fn release_channels_never_silently_install_stable() {
        assert_eq!(
            release_repository(&UpdateChannel::Stable).unwrap(),
            "yt-dlp/yt-dlp"
        );
        assert_eq!(
            release_repository(&UpdateChannel::Nightly).unwrap(),
            "yt-dlp/yt-dlp-nightly-builds"
        );
        assert!(release_repository(&UpdateChannel::Custom("attacker".into())).is_err());
    }
}
