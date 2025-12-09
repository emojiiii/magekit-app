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
                    .map_err(|e| anyhow::anyhow!("获取视频信息失败: {}", e))
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
                    .map_err(|e| anyhow::anyhow!("开始下载失败: {}", e))?;

                tracing::info!("✅ 下载任务已创建: {}", task_id);
                Ok(task_id)
            })
        })
    }

    /// 暂停下载任务（同步包装）
    pub fn pause_download_sync(&self, task_id: TaskId) {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();

        tracing::info!("⏸️ 暂停下载任务: {}", task_id);

        runtime.spawn(async move {
            if let Err(e) = tool_manager.pause_download(task_id).await {
                tracing::error!("❌ 暂停任务失败: {}", e);
            }
        });
    }

    /// 恢复下载任务（同步包装）
    pub fn resume_download_sync(&self, task_id: TaskId) {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();

        tracing::info!("▶️ 恢复下载任务: {}", task_id);

        runtime.spawn(async move {
            if let Err(e) = tool_manager.resume_download(task_id).await {
                tracing::error!("❌ 恢复任务失败: {}", e);
            }
        });
    }

    /// 取消下载任务（同步包装）
    pub fn cancel_download_sync(&self, task_id: TaskId) {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();

        tracing::info!("🛑 取消下载任务: {}", task_id);

        runtime.spawn(async move {
            if let Err(e) = tool_manager.cancel_download(task_id).await {
                tracing::error!("❌ 取消任务失败: {}", e);
            }
        });
    }

    /// 删除任务（同步包装）
    pub fn delete_task_sync(&self, task_id: TaskId) {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();
        let tasks = self.tasks.clone();

        tracing::info!("🗑️ 删除任务: {}", task_id);

        runtime.spawn(async move {
            // 先取消（如果正在运行）
            let _ = tool_manager.cancel_download(task_id).await;

            // 从持久化存储删除
            if let Err(e) = tool_manager.delete_task_status(task_id).await {
                tracing::error!("❌ 删除任务失败: {}", e);
            }

            // 从本地缓存删除
            {
                let mut tasks = tasks.write().await;
                tasks.remove(&task_id);
            }
        });
    }
}
