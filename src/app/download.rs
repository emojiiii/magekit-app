//! 视频下载功能
//!
//! GUI 层只负责调度，核心逻辑全部由 tool_manager 处理
//! 本模块提供便捷的同步包装方法，供 UI 层调用

use anyhow::Result;
use magekit_shared::{DownloadOptions, TaskId, VideoInfo};
use std::path::PathBuf;

use super::state::AppState;
use super::types::DownloadVideoOptions;

impl AppState {
    /// 仅接受完整的 HTTP(S) 媒体地址，避免把分享文案、文件路径或凭据作为请求发送。
    pub fn validate_media_url(input: &str) -> Result<String> {
        let value = input.trim();
        let valid = url::Url::parse(value).ok().filter(|url| {
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && !value.chars().any(char::is_whitespace)
        });
        if valid.is_none() {
            anyhow::bail!(crate::i18n::tr("请输入有效的 HTTP 或 HTTPS 链接"));
        }
        Ok(value.to_string())
    }

    /// 获取视频信息（在后台线程中运行）
    ///
    /// 调用 tool_manager 的 get_video_info，所有解析逻辑由 extractor 处理
    pub fn get_video_info_in_background(
        &self,
        url: String,
    ) -> std::thread::JoinHandle<Result<VideoInfo>> {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();
        let config = self.config();
        let cookies = config.advanced.cookies.clone();

        std::thread::spawn(move || {
            runtime.block_on(async {
                let cookies_opt = if cookies.is_empty() {
                    None
                } else {
                    Some(cookies.as_slice())
                };
                tool_manager
                    .get_video_info(&url, cookies_opt)
                    .await
                    .map_err(|e| {
                        anyhow::anyhow!(crate::i18n::format(
                            "获取视频信息失败: {}",
                            &[format!("{}", e)]
                        ))
                    })
            })
        })
    }

    /// 开始下载任务（在后台线程中运行）
    ///
    /// 委托给 ToolManager 处理，返回任务 ID
    /// 任务状态通过事件自动更新到 AppState.tasks
    pub fn start_download_in_background(
        &self,
        url: String,
        output_dir: PathBuf,
        format_id: String,
        options: DownloadVideoOptions,
    ) -> std::thread::JoinHandle<Result<TaskId>> {
        self.start_download_in_background_with_info(url, output_dir, format_id, options, None)
    }

    /// 开始下载任务（在后台线程中运行，带视频信息）
    ///
    /// 委托给 ToolManager 处理，返回任务 ID
    /// 任务状态通过事件自动更新到 AppState.tasks
    /// 如果提供了 video_info，则不会重新获取视频信息，可以加快任务创建速度
    pub fn start_download_in_background_with_info(
        &self,
        url: String,
        output_dir: PathBuf,
        format_id: String,
        options: DownloadVideoOptions,
        video_info: Option<VideoInfo>,
    ) -> std::thread::JoinHandle<Result<TaskId>> {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();
        let config = self.config();
        let cookies = config.advanced.cookies.clone();

        std::thread::spawn(move || {
            runtime.block_on(async {
                let cookies_opt = if cookies.is_empty() {
                    None
                } else {
                    Some(cookies.as_slice())
                };

                // 构建下载选项
                let download_options = DownloadOptions {
                    output_path: output_dir,
                    format_id,
                    embed_metadata: options.embed_metadata,
                    embed_thumbnail: options.embed_thumbnail,
                    extract_audio: options.audio_only,
                    audio_format: if options.audio_only {
                        Some("mp3".to_string())
                    } else {
                        None
                    },
                    write_subs: options.download_subtitles,
                    write_auto_subs: options.download_subtitles,
                    ..Default::default()
                };

                tracing::info!("🚀 发起下载请求: {}", url);

                // 调用 tool_manager 开始下载（传递已有的视频信息以避免重复获取）
                let task_id = tool_manager
                    .start_download_with_info(&url, download_options, cookies_opt, video_info)
                    .await
                    .map_err(|e| {
                        anyhow::anyhow!(crate::i18n::format(
                            "开始下载失败: {}",
                            &[format!("{}", e)]
                        ))
                    })?;

                tracing::info!("✅ 下载任务已创建: {}", task_id);
                Ok(task_id)
            })
        })
    }

    /// 任务操作返回可等待的结果；UI 只在成功后更新状态，失败保留原记录。
    pub fn pause_download_sync(&self, task_id: TaskId) -> tokio::task::JoinHandle<Result<()>> {
        let manager = self.tool_manager.clone();
        let tasks = self.tasks.clone();
        self.runtime.spawn(async move {
            manager.pause_download(task_id).await?;
            if let Some(status) = manager.get_task_status(task_id).await {
                tasks.write().await.insert(task_id, status);
            }
            Ok(())
        })
    }

    pub fn resume_download_sync(&self, task_id: TaskId) -> tokio::task::JoinHandle<Result<()>> {
        let manager = self.tool_manager.clone();
        let tasks = self.tasks.clone();
        self.runtime.spawn(async move {
            manager.resume_download(task_id).await?;
            if let Some(status) = manager.get_task_status(task_id).await {
                tasks.write().await.insert(task_id, status);
            }
            Ok(())
        })
    }

    pub fn cancel_download_sync(&self, task_id: TaskId) -> tokio::task::JoinHandle<Result<()>> {
        let manager = self.tool_manager.clone();
        let tasks = self.tasks.clone();
        self.runtime.spawn(async move {
            manager.cancel_download(task_id).await?;
            if let Some(status) = manager.get_task_status(task_id).await {
                tasks.write().await.insert(task_id, status);
            }
            Ok(())
        })
    }

    pub fn delete_task_sync(&self, task_id: TaskId) -> tokio::task::JoinHandle<Result<()>> {
        let manager = self.tool_manager.clone();
        let tasks = self.tasks.clone();
        self.runtime.spawn(async move {
            // 已结束任务不应先被改成“已取消”；删除失败时仍能展示原始状态。
            if manager
                .get_task_status(task_id)
                .await
                .is_some_and(|task| task.is_active())
            {
                manager.cancel_download(task_id).await?;
            }
            manager.delete_task_status(task_id).await?;
            tasks.write().await.remove(&task_id);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AppState;

    #[test]
    fn validates_and_trims_media_urls_without_rewriting_queries() {
        assert_eq!(
            AppState::validate_media_url("  https://example.com/watch?v=one&list=two  ").unwrap(),
            "https://example.com/watch?v=one&list=two"
        );
        assert!(AppState::validate_media_url("http://localhost:8080/media.mp4").is_ok());
    }

    #[test]
    fn rejects_missing_host_unsupported_schemes_share_text_and_credentials() {
        for value in [
            "",
            "example.com",
            "file:///tmp/video.mp4",
            "javascript:alert(1)",
            "https://",
            "video https://example.com",
            "https://example.com/a b",
            "https://user:password@example.com/video",
        ] {
            assert!(
                AppState::validate_media_url(value).is_err(),
                "accepted {value}"
            );
        }
    }
}
