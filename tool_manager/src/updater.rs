use crate::error::{ToolManagerError, ToolManagerResult};
use crate::storage::ToolStorage;
use futures_util::StreamExt;
use magekit_shared::{create_tokio_command, get_temp_dir, ToolType, UpdateChannel};
use serde::Deserialize;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;

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
        if self.storage.is_tool_installed(ToolType::YtDlp).await {
            tracing::info!("yt-dlp already installed");
            return Ok(());
        }

        tracing::info!("Installing yt-dlp...");
        self.download_yt_dlp_with_progress(channel, progress_callback)
            .await
    }

    /// 检查并下载ffmpeg
    pub async fn ensure_ffmpeg(&self) -> ToolManagerResult<()> {
        if self.storage.is_tool_installed(ToolType::Ffmpeg).await {
            tracing::info!("ffmpeg already available");
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

        // 检查yt-dlp更新
        if let Ok(Some(current_version)) = self.storage.get_tool_version(ToolType::YtDlp).await {
            if let Some(latest_version) = self.get_latest_yt_dlp_version(channel).await? {
                if self.is_version_newer(&latest_version, &current_version) {
                    update_info.yt_dlp_update = Some(ToolUpdate {
                        current: current_version,
                        latest: latest_version,
                    });
                }
            }
        } else {
            // 工具未安装，需要下载
            update_info.yt_dlp_update = Some(ToolUpdate {
                current: "Not installed".to_string(),
                latest: "Latest".to_string(),
            });
        }

        // 检查ffmpeg更新（简化实现）
        if !self.storage.is_tool_installed(ToolType::Ffmpeg).await {
            update_info.ffmpeg_update = Some(ToolUpdate {
                current: "Not installed".to_string(),
                latest: "Latest".to_string(),
            });
        }

        Ok(update_info)
    }

    /// 下载yt-dlp (无进度回调)
    async fn download_yt_dlp(&self, channel: UpdateChannel) -> ToolManagerResult<()> {
        self.download_yt_dlp_with_progress(channel, None).await
    }

    /// 下载yt-dlp (带进度回调)
    async fn download_yt_dlp_with_progress(
        &self,
        _channel: UpdateChannel,
        progress_callback: Option<ProgressCallback>,
    ) -> ToolManagerResult<()> {
        // 直接使用最新版本的下载 URL（避免 GitHub API 限流）
        let download_url = self.get_direct_yt_dlp_url();
        let temp_dir = get_temp_dir().map_err(|e| ToolManagerError::internal(e.to_string()))?;
        let temp_file = temp_dir.join("yt-dlp");

        tracing::info!("Downloading yt-dlp from: {}", download_url);

        // 创建带 User-Agent 的客户端
        let client = reqwest::Client::builder()
            .user_agent("MageKit/1.0")
            .build()
            .map_err(|e| {
                ToolManagerError::internal(format!("Failed to create HTTP client: {}", e))
            })?;

        // 发送请求
        let response = client
            .get(&download_url)
            .send()
            .await
            .map_err(ToolManagerError::Network)?;

        if !response.status().is_success() {
            return Err(ToolManagerError::installation_failed(
                "yt-dlp",
                format!(
                    "HTTP {}: {}",
                    response.status(),
                    response.status().canonical_reason().unwrap_or("Unknown")
                ),
            ));
        }

        // 获取文件总大小
        let total_size = response.content_length().unwrap_or(0);
        tracing::info!("Total size: {} bytes", total_size);

        // 创建临时文件
        let mut file = tokio::fs::File::create(&temp_file).await.map_err(|e| {
            ToolManagerError::file_operation_failed("create temp file", e.to_string())
        })?;

        // 流式下载并报告进度
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();
        let mut last_progress_time = std::time::Instant::now();
        let mut last_downloaded: u64 = 0;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(ToolManagerError::Network)?;
            file.write_all(&chunk).await.map_err(|e| {
                ToolManagerError::file_operation_failed("write chunk", e.to_string())
            })?;

            downloaded += chunk.len() as u64;

            // 每 100ms 更新一次进度
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(last_progress_time);
            if elapsed.as_millis() >= 100 {
                let speed = if elapsed.as_secs_f64() > 0.0 {
                    ((downloaded - last_downloaded) as f64 / elapsed.as_secs_f64()) as u64
                } else {
                    0
                };

                if let Some(ref callback) = progress_callback {
                    callback(downloaded, total_size, speed);
                }

                last_progress_time = now;
                last_downloaded = downloaded;
            }
        }

        // 下载完成，通知 UI 进入安装阶段
        // 使用 speed = u64::MAX 作为特殊标记，表示进入安装阶段
        if let Some(ref callback) = progress_callback {
            callback(downloaded, total_size, u64::MAX);
        }

        tracing::info!("Downloaded {} bytes, starting installation...", downloaded);

        // 刷新文件
        file.flush()
            .await
            .map_err(|e| ToolManagerError::file_operation_failed("flush file", e.to_string()))?;
        drop(file);

        // 设置执行权限（Unix系统）
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&temp_file)
                .await
                .map_err(|e| {
                    ToolManagerError::file_operation_failed("get file metadata", e.to_string())
                })?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&temp_file, perms).await.map_err(|e| {
                ToolManagerError::file_operation_failed("set file permissions", e.to_string())
            })?;
        }

        // 移动到最终位置
        let final_path = self.storage.get_tool_path(ToolType::YtDlp);
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                ToolManagerError::file_operation_failed("create tools directory", e.to_string())
            })?;
        }

        fs::rename(&temp_file, &final_path).await.map_err(|e| {
            ToolManagerError::file_operation_failed("move yt-dlp to final location", e.to_string())
        })?;

        tracing::info!("yt-dlp installed successfully at: {:?}", final_path);
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

    /// 获取 yt-dlp 直接下载 URL (避免 GitHub API 限流)
    fn get_direct_yt_dlp_url(&self) -> String {
        // 使用 GitHub releases 的直接下载链接
        // macOS: yt-dlp_macos (通用二进制，支持 Intel 和 Apple Silicon)
        // Linux: yt-dlp
        // Windows: yt-dlp.exe

        #[cfg(target_os = "macos")]
        {
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos".to_string()
        }
        #[cfg(target_os = "linux")]
        {
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp".to_string()
        }
        #[cfg(target_os = "windows")]
        {
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe".to_string()
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp".to_string()
        }
    }

    /// 获取yt-dlp下载URL
    #[allow(dead_code)]
    async fn get_yt_dlp_download_url(&self, channel: UpdateChannel) -> ToolManagerResult<String> {
        match channel {
            UpdateChannel::Stable => {
                // 获取最新稳定版
                let api_url = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";
                self.get_yt_dlp_release_url(api_url).await
            }
            UpdateChannel::Nightly => {
                // 获取最新预发布版
                let api_url = "https://api.github.com/repos/yt-dlp/yt-dlp/releases";
                self.get_yt_dlp_nightly_url(api_url).await
            }
            UpdateChannel::Custom(_) => {
                // 自定义版本暂时不支持
                Err(ToolManagerError::config(
                    "Custom update channel not yet supported".to_string(),
                ))
            }
        }
    }

    /// 获取yt-dlp最新版本信息
    async fn get_latest_yt_dlp_version(
        &self,
        _channel: UpdateChannel,
    ) -> ToolManagerResult<Option<String>> {
        // 简化实现，返回固定版本
        Ok(Some("2023.07.06".to_string()))
    }

    /// 从GitHub API获取发布下载URL
    async fn get_yt_dlp_release_url(&self, api_url: &str) -> ToolManagerResult<String> {
        let response = reqwest::get(api_url)
            .await
            .map_err(|e| ToolManagerError::process_failed("GitHub API request", e.to_string()))?;

        if !response.status().is_success() {
            return Err(ToolManagerError::internal(format!(
                "GitHub API request failed: {}",
                response.status()
            )));
        }

        let release_info: GitHubRelease = response
            .json()
            .await
            .map_err(|e| ToolManagerError::internal(format!("Failed to parse JSON: {}", e)))?;

        // 查找适合当前平台的二进制文件
        let binary_name = if cfg!(windows) {
            "yt-dlp.exe"
        } else {
            "yt-dlp"
        };

        for asset in release_info.assets {
            if asset.name.contains(binary_name) {
                return Ok(asset.browser_download_url);
            }
        }

        Err(ToolManagerError::installation_failed(
            "yt-dlp",
            "No suitable binary found for current platform".to_string(),
        ))
    }

    /// 获取夜间构建版本URL
    async fn get_yt_dlp_nightly_url(&self, _api_url: &str) -> ToolManagerResult<String> {
        // 夜间构建通常在不同的URL
        Ok("https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest".to_string())
    }

    /// 比较版本号
    fn is_version_newer(&self, latest: &str, current: &str) -> bool {
        // 简化的版本比较，实际应用中需要更复杂的语义版本比较
        latest != current
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

/// GitHub API返回的发布信息
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}
