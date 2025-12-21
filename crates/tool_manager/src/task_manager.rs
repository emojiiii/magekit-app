use crate::download_adapter::TaskProgressCallback;
use crate::downloader::{DownloadProgress, VideoDownloader};
use crate::error::{DownloadError, DownloadResult, ToolManagerResult};
use crate::storage::ToolStorage;
use crate::task_persistence::TaskPersistence;
use crate::task_queue::{QueueStats, QueuedTask, TaskPriority, TaskQueue};
use crate::{config::ConfigManager, updater::UpdateInfo};
use download::{DownloadClient, DownloadStrategy};
use magekit_shared::{
    DownloadOptions, PlatformCookie, TaskId, TaskState, TaskStatus, TaskUpdate, VideoInfo,
};
use magekit_shared::{UpdateChannel, generate_output_path, normalize_url};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, Semaphore, broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// 工具管理器
pub struct ToolManager {
    /// 工具存储管理器 (公开以供外部访问)
    pub storage: ToolStorage,
    /// 视频下载器（仅用于获取视频信息）
    downloader: VideoDownloader,
    /// 新的下载客户端（用于实际下载）
    download_client: Arc<DownloadClient>,
    config_manager: ConfigManager,
    tasks: Arc<RwLock<HashMap<TaskId, TaskHandle>>>,
    /// 事件广播发送器（用于向所有订阅者广播事件）
    event_tx: broadcast::Sender<ToolManagerEvent>,
    /// 任务队列
    task_queue: Arc<TaskQueue>,
    /// 下载并发控制（避免同时启动过多下载导致 UI 卡顿/状态延迟）
    download_semaphore: Arc<Semaphore>,
    /// 任务持久化
    persistence: Arc<Mutex<TaskPersistence>>,
    /// 默认最大重试次数
    max_retries: u32,
    /// 速度限制 (bytes/s)
    speed_limit: Arc<RwLock<Option<u64>>>,
}

/// 任务句柄
pub struct TaskHandle {
    pub status: TaskStatus,
    pub cancel_tx: Option<mpsc::Sender<()>>,
    /// 重试次数
    pub retry_count: u32,
    /// 最大重试次数
    pub max_retries: u32,
    /// 原始下载选项（用于恢复下载）
    pub options: Option<DownloadOptions>,
    /// 原始 cookies（用于恢复下载）
    pub cookies: Option<Vec<PlatformCookie>>,
}

/// 工具管理器事件
#[derive(Debug, Clone)]
pub enum ToolManagerEvent {
    TaskUpdate(TaskUpdate),
    ToolUpdate(UpdateInfo),
    Error(String),
    /// 队列事件
    QueueEvent(QueueEventInfo),
}

/// 队列事件信息
#[derive(Debug, Clone)]
pub struct QueueEventInfo {
    pub event_type: String,
    pub task_id: Option<TaskId>,
    pub message: Option<String>,
}

impl ToolManager {
    /// 创建新的工具管理器 (同步版本)
    pub fn new_sync() -> ToolManagerResult<Self> {
        let storage = ToolStorage::new_sync()?;

        // 获取工具路径
        let yt_dlp_path = storage.get_tool_path(magekit_shared::ToolType::YtDlp);
        // 优先全局解析器，再兜底存储路径
        let ffmpeg_path = magekit_shared::resolve_ffmpeg_path().or_else(|| {
            let p = storage.get_tool_path(magekit_shared::ToolType::Ffmpeg);
            if p.exists() { Some(p) } else { None }
        });

        // 创建旧的下载器（仅用于获取视频信息）
        let downloader = VideoDownloader::new(yt_dlp_path.clone(), ffmpeg_path.clone());

        // 创建新的下载客户端（用于实际下载）
        // 工具路径优先级：应用内 tools 目录 / ToolStorage → 系统 PATH。
        // 注意：不要把“尚未安装”的 storage 路径强行传给 DownloadClient，否则会导致 spawn 直接失败。
        // 🔧 使用 resolve_*_path() 获取工具路径，确保优先使用应用内工具
        let ytdlp_for_client = magekit_shared::resolve_yt_dlp_path().or_else(|| {
            let p = storage.get_tool_path(magekit_shared::ToolType::YtDlp);
            if p.exists() { Some(p) } else { None }
        });
        let ffmpeg_for_client =
            magekit_shared::resolve_ffmpeg_path().or_else(|| ffmpeg_path.clone());

        tracing::info!("🔧 下载客户端工具路径:");
        tracing::info!("  ├─ yt-dlp: {:?}", ytdlp_for_client);
        tracing::info!("  └─ ffmpeg: {:?}", ffmpeg_for_client);

        let download_client = Arc::new(DownloadClient::with_tools(
            ytdlp_for_client,
            ffmpeg_for_client,
        ));

        let config_manager = ConfigManager::new_sync()?;

        // 创建广播通道（容量 1000）
        let (event_tx, _) = broadcast::channel(1000);

        // 创建任务队列（默认3个并发）
        let max_concurrent = config_manager
            .config()
            .download_defaults
            .max_concurrent_downloads;
        let (task_queue, _queue_rx) = TaskQueue::new(max_concurrent);
        let download_semaphore = Arc::new(Semaphore::new(max_concurrent));

        // 创建任务持久化
        let persistence = TaskPersistence::new().unwrap_or_default();

        // 获取最大重试次数
        let max_retries = config_manager.config().download_defaults.retry_times;

        Ok(Self {
            storage,
            downloader,
            download_client,
            config_manager,
            tasks: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            task_queue: Arc::new(task_queue),
            download_semaphore,
            persistence: Arc::new(Mutex::new(persistence)),
            max_retries,
            speed_limit: Arc::new(RwLock::new(None)),
        })
    }

    /// 创建新的工具管理器 (异步版本，保持兼容)
    pub async fn new() -> ToolManagerResult<Self> {
        Self::new_sync()
    }

    /// 订阅事件流
    ///
    /// 返回一个 broadcast::Receiver，可以接收所有任务更新事件。
    /// 多个订阅者可以同时接收同一事件。
    pub fn subscribe(&self) -> broadcast::Receiver<ToolManagerEvent> {
        self.event_tx.subscribe()
    }

    /// 确保所有工具都已安装
    pub async fn ensure_tools(&self, channel: UpdateChannel) -> ToolManagerResult<()> {
        let updater = crate::updater::ToolUpdater::new(self.storage.clone());
        updater.ensure_all_tools(channel).await
    }

    /// 检查工具更新
    pub async fn check_for_updates(&self, channel: UpdateChannel) -> ToolManagerResult<UpdateInfo> {
        let updater = crate::updater::ToolUpdater::new(self.storage.clone());
        updater.check_for_updates(channel).await
    }

    /// 获取视频信息
    ///
    /// # 参数
    /// - `url`: 视频 URL
    /// - `cookies`: 可选的平台 Cookie 列表
    pub async fn get_video_info(
        &self,
        url: &str,
        cookies: Option<&[magekit_shared::PlatformCookie]>,
    ) -> DownloadResult<VideoInfo> {
        self.downloader.get_video_info(url, cookies).await
    }

    /// 获取频道/作者的所有视频列表
    ///
    /// # 参数
    /// - `url`: 频道或播放列表 URL
    /// - `cookies`: 可选的平台 Cookie 列表
    pub async fn get_channel_videos(
        &self,
        url: &str,
        cookies: Option<&[magekit_shared::PlatformCookie]>,
    ) -> DownloadResult<magekit_shared::ChannelInfo> {
        self.downloader.get_channel_videos(url, cookies).await
    }

    /// 分页获取频道/作者作品列表。
    pub async fn get_channel_videos_page(
        &self,
        url: &str,
        cursor: Option<i64>,
        count: usize,
        cookies: Option<&[magekit_shared::PlatformCookie]>,
    ) -> DownloadResult<magekit_shared::ChannelPageResult> {
        self.downloader
            .get_channel_videos_page(url, cursor, count, cookies)
            .await
    }

    /// 开始下载任务
    ///
    /// # 参数
    /// - `url`: 视频 URL
    /// - `options`: 下载选项
    /// - `cookies`: 可选的平台 Cookie 列表
    pub async fn start_download(
        &self,
        url: &str,
        options: DownloadOptions,
        cookies: Option<&[PlatformCookie]>,
    ) -> DownloadResult<TaskId> {
        self.start_download_with_info(url, options, cookies, None)
            .await
    }

    /// 开始下载任务（带视频信息）
    ///
    /// # 参数
    /// - `url`: 视频 URL
    /// - `options`: 下载选项
    /// - `cookies`: 可选的平台 Cookie 列表
    /// - `video_info`: 可选的视频信息，如果提供则不再获取
    pub async fn start_download_with_info(
        &self,
        url: &str,
        mut options: DownloadOptions,
        cookies: Option<&[PlatformCookie]>,
        video_info: Option<VideoInfo>,
    ) -> DownloadResult<TaskId> {
        // 🔧 规范化 URL（确保有协议前缀）
        let normalized_url = normalize_url(url);
        if normalized_url != url {
            tracing::info!("🔧 URL 规范化: {} -> {}", url, normalized_url);
        }

        // 每次下载都创建新任务（使用新的 UUID）
        let task_id = Uuid::new_v4();

        // 创建任务状态（标题优先使用 options.task_title）
        let mut task_status =
            TaskStatus::new(task_id, normalized_url.clone(), options.task_title.clone());
        task_status.state = TaskState::Queued;

        // 获取视频信息（如果未提供）
        let video_info = match video_info {
            Some(info) => info,
            None => self.get_video_info(&normalized_url, cookies).await?,
        };

        // 如果解析器提供了直链，且用户选择的格式对应直链，则优先走直链下载（避免 yt-dlp）
        if options.download_url.is_none() && options.ffmpeg_url.is_none() {
            if let Some(url) = video_info
                .formats
                .iter()
                .find(|f| f.format_id == options.format_id)
                .and_then(|f| f.download_url.clone())
            {
                options.download_url = Some(url);
            } else if let Some(best) = select_best_direct_format(&video_info.formats) {
                // UI 可能传入 yt-dlp 风格的 format 字符串；如果解析器已提供直链格式，
                // 则直接选取“最高分辨率”的直链格式作为兜底，避免走 yt-dlp。
                options.format_id = best.format_id.clone();
                options.download_url = best.download_url.clone();
            }
        }
        if task_status.title.is_none() {
            task_status.title = Some(video_info.title.clone());
        }
        let title_for_path = task_status
            .title
            .clone()
            .unwrap_or_else(|| video_info.title.clone());

        let output_path = generate_output_path(
            &options.output_path,
            &title_for_path,
            &video_info
                .formats
                .iter()
                .find(|f| f.format_id == options.format_id)
                .map(|f| f.ext.as_str())
                .unwrap_or("mp4"),
        )
        .map_err(|e| DownloadError::internal(e.to_string()))?;

        task_status.output_path = Some(output_path.clone());

        // 创建任务句柄
        let (cancel_tx, cancel_rx) = mpsc::channel(1);
        let cookies_owned: Option<Vec<PlatformCookie>> = cookies.map(|c| c.to_vec());
        let task_handle = TaskHandle {
            status: task_status.clone(),
            cancel_tx: Some(cancel_tx),
            retry_count: 0,
            max_retries: self.max_retries,
            options: Some(options.clone()),
            cookies: cookies_owned.clone(),
        };

        // 存储任务
        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(task_id, task_handle);
        }

        // 持久化任务（包含下载选项和 cookies，以便恢复）
        {
            let mut persistence = self.persistence.lock().await;
            let _ = persistence.add_task(
                task_status.clone(),
                self.max_retries,
                Some(options.clone()),
                cookies_owned.clone(),
            );
        }

        // 发送任务创建事件
        self.broadcast_event(ToolManagerEvent::TaskUpdate(TaskUpdate::Created(
            task_status,
        )));

        // 启动下载任务（使用新架构 + 并发控制）
        let download_client = self.download_client.clone();
        let download_semaphore = self.download_semaphore.clone();
        let tasks = self.tasks.clone();
        let event_tx = self.event_tx.clone();
        let persistence = self.persistence.clone();
        let url_clone = url.to_string();

        tokio::spawn(async move {
            let _permit = match download_semaphore.acquire_owned().await {
                Ok(p) => p,
                Err(_) => return,
            };

            // 如果任务在排队期间已被取消，则不再启动下载
            let cancelled = {
                let guard = tasks.read().await;
                matches!(
                    guard.get(&task_id).map(|t| &t.status.state),
                    Some(TaskState::Cancelled)
                )
            };
            if cancelled {
                return;
            }
            Self::run_download_task_new(
                task_id,
                url_clone,
                options,
                download_client,
                tasks,
                event_tx,
                persistence,
                cancel_rx,
                cookies_owned,
            )
            .await;
        });

        Ok(task_id)
    }

    /// 暂停下载任务
    pub async fn pause_download(&self, task_id: TaskId) -> DownloadResult<()> {
        let mut tasks = self.tasks.write().await;

        if let Some(task) = tasks.get_mut(&task_id) {
            if task.status.state == TaskState::Downloading {
                task.status.state = TaskState::Paused;

                // 发送取消信号来停止下载进程
                if let Some(cancel_tx) = task.cancel_tx.take() {
                    let _ = cancel_tx.send(()).await;
                }

                // 持久化暂停状态
                let status = task.status.clone();
                drop(tasks); // 释放锁

                {
                    let mut persistence = self.persistence.lock().await;
                    let _ = persistence.update_task_status(task_id, status);
                }

                // 广播状态变更
                self.broadcast_event(ToolManagerEvent::TaskUpdate(TaskUpdate::StateChanged(
                    task_id,
                    TaskState::Paused,
                )));

                tracing::info!("⏸️ 任务已暂停: {}", task_id);
                Ok(())
            } else {
                Err(DownloadError::task_operation_failed(
                    task_id,
                    "pause",
                    "Task is not in downloading state".to_string(),
                ))
            }
        } else {
            Err(DownloadError::task_not_found(task_id))
        }
    }

    /// 恢复下载任务
    ///
    /// 真正重新启动下载任务（而不是仅更新状态）
    /// 支持从内存和持久化存储中恢复任务
    pub async fn resume_download(&self, task_id: TaskId) -> DownloadResult<()> {
        // 首先尝试从内存中获取任务信息
        let task_info = {
            let tasks = self.tasks.read().await;
            tasks
                .get(&task_id)
                .map(|t| (t.status.clone(), t.options.clone(), t.cookies.clone()))
        };

        // 如果内存中没有，尝试从持久化存储中获取
        let (mut status, options, cookies) = match task_info {
            Some(info) => info,
            None => {
                // 从持久化存储中获取
                let persistence = self.persistence.lock().await;
                let persisted_task = persistence
                    .get_task(task_id)
                    .ok_or_else(|| DownloadError::task_not_found(task_id))?;
                (
                    persisted_task.status.clone(),
                    persisted_task.options.clone(),
                    persisted_task.cookies.clone(),
                )
            }
        };

        // 检查状态 - 允许恢复 Downloading、Paused 和 Failed 状态的任务
        if !matches!(
            status.state,
            TaskState::Downloading | TaskState::Paused | TaskState::Failed(_)
        ) {
            return Err(DownloadError::task_operation_failed(
                task_id,
                "resume",
                format!("Task is in {:?} state, cannot resume", status.state),
            ));
        }

        // 获取下载选项
        let options = options.ok_or_else(|| {
            DownloadError::task_operation_failed(
                task_id,
                "resume",
                "No download options saved for this task".to_string(),
            )
        })?;

        // 更新状态为 Downloading
        status.state = TaskState::Downloading;

        // 重新创建取消通道
        let (cancel_tx, cancel_rx) = mpsc::channel(1);

        // 创建或更新内存中的任务句柄
        {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(&task_id) {
                task.status.state = TaskState::Downloading;
                task.cancel_tx = Some(cancel_tx);
            } else {
                // 任务不在内存中，需要创建新的任务句柄
                let task_handle = TaskHandle {
                    status: status.clone(),
                    cancel_tx: Some(cancel_tx),
                    retry_count: 0,
                    max_retries: self.max_retries,
                    options: Some(options.clone()),
                    cookies: cookies.clone(),
                };
                tasks.insert(task_id, task_handle);
            }
        }

        // 持久化状态
        {
            let mut persistence = self.persistence.lock().await;
            let _ = persistence.update_task_status(task_id, status.clone());
        }

        // 广播状态变更
        self.broadcast_event(ToolManagerEvent::TaskUpdate(TaskUpdate::StateChanged(
            task_id,
            TaskState::Downloading,
        )));

        // 重新启动下载任务（使用新架构 + 并发控制）
        let download_client = self.download_client.clone();
        let download_semaphore = self.download_semaphore.clone();
        let tasks = self.tasks.clone();
        let event_tx = self.event_tx.clone();
        let persistence = self.persistence.clone();
        let url = status.url.clone();

        tokio::spawn(async move {
            let _permit = match download_semaphore.acquire_owned().await {
                Ok(p) => p,
                Err(_) => return,
            };

            let cancelled = {
                let guard = tasks.read().await;
                matches!(
                    guard.get(&task_id).map(|t| &t.status.state),
                    Some(TaskState::Cancelled)
                )
            };
            if cancelled {
                return;
            }
            Self::run_download_task_new(
                task_id,
                url,
                options,
                download_client,
                tasks,
                event_tx,
                persistence,
                cancel_rx,
                cookies,
            )
            .await;
        });

        tracing::info!("▶️ 任务已恢复: {}", task_id);
        Ok(())
    }

    /// 取消下载任务
    pub async fn cancel_download(&self, task_id: TaskId) -> DownloadResult<()> {
        let mut tasks = self.tasks.write().await;

        if let Some(task) = tasks.get_mut(&task_id) {
            // 发送取消信号
            if let Some(cancel_tx) = task.cancel_tx.take() {
                let _ = cancel_tx.send(()).await;
            }

            task.status.state = TaskState::Cancelled;
            task.status.completed_at = Some(std::time::SystemTime::now());
            let status = task.status.clone();

            drop(tasks); // 释放锁

            // 持久化取消状态
            {
                let mut persistence = self.persistence.lock().await;
                let _ = persistence.update_task_status(task_id, status);
            }

            // 广播状态变更
            self.broadcast_event(ToolManagerEvent::TaskUpdate(TaskUpdate::StateChanged(
                task_id,
                TaskState::Cancelled,
            )));

            tracing::info!("🛑 任务已取消: {}", task_id);
            Ok(())
        } else {
            drop(tasks); // 释放锁

            // 任务在内存中不存在，尝试从持久化存储中删除
            tracing::warn!("⚠️ 任务 {} 在内存中不存在，尝试从持久化存储中删除", task_id);
            let mut persistence = self.persistence.lock().await;
            let _ = persistence.remove_task(task_id);
            let _ = persistence.save();

            // 不返回错误，允许后续删除操作继续
            Ok(())
        }
    }

    /// 获取任务状态
    pub async fn get_task_status(&self, task_id: TaskId) -> Option<TaskStatus> {
        let tasks = self.tasks.read().await;
        tasks.get(&task_id).map(|task| task.status.clone())
    }

    /// 获取所有任务状态（包括持久化的任务）
    ///
    /// 按 URL 去重，保留最新的任务（基于创建时间）
    pub async fn get_all_tasks(&self) -> Vec<TaskStatus> {
        let mut all_tasks: HashMap<TaskId, TaskStatus> = HashMap::new();

        // 1. 先从持久化存储加载任务
        let persistence = self.persistence.lock().await;
        for persisted_task in persistence.get_all_tasks() {
            all_tasks.insert(persisted_task.status.id, persisted_task.status.clone());
        }
        drop(persistence);

        // 2. 再从内存中加载正在运行的任务（覆盖持久化的旧状态）
        let tasks = self.tasks.read().await;
        for (task_id, task_handle) in tasks.iter() {
            all_tasks.insert(*task_id, task_handle.status.clone());
        }

        // 3. 按 URL 去重，保留最新的任务
        let mut url_to_task: HashMap<String, TaskStatus> = HashMap::new();
        for task in all_tasks.into_values() {
            let url = task.url.clone();
            if let Some(existing) = url_to_task.get(&url) {
                // 比较创建时间，保留更新的
                if task.created_at > existing.created_at {
                    url_to_task.insert(url, task);
                }
            } else {
                url_to_task.insert(url, task);
            }
        }

        url_to_task.into_values().collect()
    }

    /// 更新任务状态
    async fn update_task_state(&self, task_id: TaskId, state: TaskState) -> DownloadResult<()> {
        let mut tasks = self.tasks.write().await;

        if let Some(task) = tasks.get_mut(&task_id) {
            task.status.state = state.clone();

            // 同步保存到持久化存储
            let status = task.status.clone();
            drop(tasks);

            let _ = self.save_task_status(&status).await;

            // 广播状态变更
            self.broadcast_event(ToolManagerEvent::TaskUpdate(TaskUpdate::StateChanged(
                task_id, state,
            )));
            Ok(())
        } else {
            Err(DownloadError::task_not_found(task_id))
        }
    }

    /// 保存任务状态到持久化存储
    pub async fn save_task_status(&self, status: &TaskStatus) -> DownloadResult<()> {
        let mut persistence = self.persistence.lock().await;
        persistence
            .update_task_status(status.id, status.clone())
            .map_err(|e| DownloadError::internal(e.to_string()))?;
        Ok(())
    }

    /// 从持久化存储删除任务
    pub async fn delete_task_status(&self, task_id: TaskId) -> DownloadResult<()> {
        // 尝试清理临时文件/缓存（不删除最终输出文件）
        let mut task_url = String::new();
        let mut output_path = None;

        {
            let tasks = self.tasks.read().await;
            if let Some(t) = tasks.get(&task_id) {
                task_url = t.status.url.clone();
                output_path = t.status.output_path.clone();
            }
        }

        if output_path.is_none() {
            let persistence = self.persistence.lock().await;
            if let Some(t) = persistence.get_task(task_id) {
                task_url = t.status.url.clone();
                output_path = t.status.output_path.clone();
            }
        }

        if let Some(output_path) = output_path {
            // 直链/合并临时文件：<filename>.part
            let part_path = download::utils::part_path_for_output(&output_path);
            let _ = tokio::fs::remove_file(&part_path).await;

            // HLS 缓存目录（新路径）
            if let Ok(url) = task_url.parse::<url::Url>() {
                let cache_dir = download::utils::hls_cache_dir(&output_path, &url);
                let _ = tokio::fs::remove_dir_all(&cache_dir).await;
            }

            // 向后兼容：历史版本可能使用 `<filename>.parts/`
            if let Some(file_name) = output_path.file_name().and_then(|s| s.to_str()) {
                let legacy = output_path.with_file_name(format!("{}.parts", file_name));
                let _ = tokio::fs::remove_dir_all(&legacy).await;
            }
        }

        // 从内存中移除
        {
            let mut tasks = self.tasks.write().await;
            tasks.remove(&task_id);
        }

        // 从持久化存储删除
        let mut persistence = self.persistence.lock().await;
        persistence
            .remove_task(task_id)
            .map_err(|e| DownloadError::internal(e.to_string()))?;
        Ok(())
    }

    /// 广播事件到所有订阅者
    fn broadcast_event(&self, event: ToolManagerEvent) {
        // broadcast::send 返回发送给的接收者数量，即使没有接收者也不会阻塞
        let _ = self.event_tx.send(event);
    }

    /// 运行下载任务
    /// 运行下载任务（新架构 - 使用 DownloadClient）
    async fn run_download_task_new(
        task_id: TaskId,
        url: String,
        options: DownloadOptions,
        download_client: Arc<DownloadClient>,
        tasks: Arc<RwLock<HashMap<TaskId, TaskHandle>>>,
        event_tx: broadcast::Sender<ToolManagerEvent>,
        persistence: Arc<Mutex<TaskPersistence>>,
        mut cancel_rx: mpsc::Receiver<()>,
        cookies: Option<Vec<PlatformCookie>>,
    ) {
        tracing::info!("🚀 启动新下载任务 {} - {}", task_id, url);

        // 创建进度通道
        let (progress_tx, mut progress_rx) = mpsc::channel::<TaskUpdate>(1000);

        // 创建回调
        let callback = TaskProgressCallback::new(task_id, progress_tx.clone());

        // 创建取消令牌
        let cancel_token = CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();

        // 启动取消监听
        tokio::spawn(async move {
            let _ = cancel_rx.recv().await;
            tracing::info!("⏹️ 收到取消信号，任务 {}", task_id);
            cancel_token_clone.cancel();
        });

        // 克隆用于进度监控
        let tasks_for_progress = tasks.clone();
        let event_tx_for_progress = event_tx.clone();
        let persistence_for_progress = persistence.clone();

        // 启动进度监控任务
        let progress_handle = tokio::spawn(async move {
            while let Some(update) = progress_rx.recv().await {
                match &update {
                    TaskUpdate::Progress(id, progress, downloaded, total, speed, eta) => {
                        // 更新任务状态
                        let status = Self::update_task_in_map(&tasks_for_progress, *id, |status| {
                            // 避免 pause/cancel 后仍被“余震进度”拉回 Downloading，导致 UI 状态错乱
                            if matches!(status.state, TaskState::Paused | TaskState::Cancelled) {
                                return;
                            }
                            status.state = TaskState::Downloading;
                            status.progress = *progress;
                            status.downloaded_bytes = *downloaded;
                            status.total_bytes = *total;
                            status.speed = *speed;
                            status.eta = *eta;
                        })
                        .await;

                        // 若任务已暂停/取消，则不再广播进度，减少事件风暴与 UI 抖动
                        if matches!(
                            status.as_ref().map(|s| &s.state),
                            Some(TaskState::Paused | TaskState::Cancelled)
                        ) {
                            continue;
                        }

                        // 广播进度更新
                        let _ = event_tx_for_progress.send(ToolManagerEvent::TaskUpdate(update));
                    }
                    TaskUpdate::Completed(id, output_path) => {
                        // 更新任务状态为完成
                        let status = Self::update_task_in_map(&tasks_for_progress, *id, |status| {
                            status.state = TaskState::Completed;
                            status.progress = 1.0;
                            status.completed_at = Some(std::time::SystemTime::now());
                            status.output_path = Some(output_path.clone());
                        })
                        .await;

                        // 持久化
                        if let Some(status) = status {
                            let mut p = persistence_for_progress.lock().await;
                            let _ = p.update_task_status(*id, status);
                        }

                        // 广播完成事件
                        let _ = event_tx_for_progress.send(ToolManagerEvent::TaskUpdate(update));
                        break;
                    }
                    TaskUpdate::Failed(id, error) => {
                        // 更新任务状态为失败
                        let status = Self::update_task_in_map(&tasks_for_progress, *id, |status| {
                            status.state = TaskState::Failed(error.clone());
                            status.completed_at = Some(std::time::SystemTime::now());
                        })
                        .await;

                        // 持久化
                        if let Some(status) = status {
                            let mut p = persistence_for_progress.lock().await;
                            let _ = p.update_task_status(*id, status);
                        }

                        // 广播失败事件
                        let _ = event_tx_for_progress.send(ToolManagerEvent::TaskUpdate(update));
                        break;
                    }
                    _ => {}
                }
            }
        });

        // 更新任务状态为下载中
        Self::update_task_in_map(&tasks, task_id, |status| {
            status.state = TaskState::Downloading;
            status.started_at = Some(std::time::SystemTime::now());
        })
        .await;

        // 广播状态变更
        let _ = event_tx.send(ToolManagerEvent::TaskUpdate(TaskUpdate::StateChanged(
            task_id,
            TaskState::Downloading,
        )));

        // 构建下载请求
        let parsed_url = match url.parse::<url::Url>() {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("❌ 无效的 URL: {}", e);
                let _ = event_tx.send(ToolManagerEvent::TaskUpdate(TaskUpdate::Failed(
                    task_id,
                    format!("无效的 URL: {}", e),
                )));
                return;
            }
        };

        // 解析 ffmpeg_args，提取 headers 和其他参数
        let mut headers_map = std::collections::HashMap::new();
        let mut other_ffmpeg_args = Vec::new();
        let mut i = 0;
        while i < options.ffmpeg_args.len() {
            if options.ffmpeg_args[i] == "-headers" && i + 1 < options.ffmpeg_args.len() {
                // 找到 -headers 参数，解析下一个参数（headers 字符串）
                let headers_str = &options.ffmpeg_args[i + 1];
                tracing::info!("📝 解析 ffmpeg headers: {}", headers_str);

                // 解析 headers 字符串（格式：Key: Value\r\nKey2: Value2\r\n）
                for line in headers_str.split("\r\n") {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if let Some(pos) = line.find(':') {
                        let key = line[..pos].trim().to_string();
                        let value = line[pos + 1..].trim().to_string();
                        headers_map.insert(key, value);
                    }
                }
                tracing::info!("✅ 解析得到 {} 个 headers", headers_map.len());
                i += 2; // 跳过 -headers 和 headers 字符串
            } else {
                // 其他参数保留
                other_ffmpeg_args.push(options.ffmpeg_args[i].clone());
                i += 1;
            }
        }

        // 构建 tool_args（传递非 headers 的 ffmpeg 参数）
        let mut tool_args = std::collections::HashMap::new();
        if !other_ffmpeg_args.is_empty() {
            // key 必须是 "ffmpeg"（参见 crates/download/src/downloader/ffmpeg.rs:59）
            tool_args.insert("ffmpeg".to_string(), other_ffmpeg_args.clone());
            tracing::debug!("📝 传递其他 ffmpeg 参数: {:?}", other_ffmpeg_args);
        }

        // 🔧 仅在会走 yt-dlp（Auto）时传递 format_id，避免 Direct/ffmpeg 场景产生误导日志
        if options.download_url.is_none() && options.ffmpeg_url.is_none() && !options.format_id.is_empty() {
            let ytdlp_args = vec!["-f".to_string(), options.format_id.clone()];
            tool_args.insert("ytdlp".to_string(), ytdlp_args.clone());
            tracing::info!("📝 传递 yt-dlp 格式参数: {:?}", ytdlp_args);
        }

        // Direct/ffmpeg 策略时，下载目标应使用解析得到的真实资源 URL，而不是用户输入的页面/短链。
        // 否则会把 HTML/跳转页落盘成 .mp4（表现为 moov atom not found / 无法播放）。
        let download_target_url = if let Some(stream_url) = options.ffmpeg_url.as_deref() {
            stream_url
        } else if let Some(direct_url) = options.download_url.as_deref() {
            direct_url
        } else {
            parsed_url.as_str()
        };

        let parsed_download_url: url::Url = match download_target_url.parse() {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("❌ 无法解析下载 URL: {} ({})", download_target_url, e);
                let _ = event_tx.send(ToolManagerEvent::TaskUpdate(TaskUpdate::Failed(
                    task_id,
                    format!("无效的下载 URL: {}", e),
                )));
                return;
            }
        };

        let platform_hint = platform_hint_from_url(&url);

        // 对 Douyin 直链资源补齐必要的默认 headers，避免服务端返回 HTML/JSON。
        if platform_hint == Some("douyin") {
            headers_map.entry("User-Agent".to_string()).or_insert_with(|| {
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.212 Safari/537.36".to_string()
            });
            headers_map
                .entry("Referer".to_string())
                .or_insert_with(|| "https://www.douyin.com/".to_string());
            headers_map
                .entry("Origin".to_string())
                .or_insert_with(|| "https://www.douyin.com".to_string());
        }

        let request = download::DownloadRequest {
            url: parsed_download_url,
            output: download::DownloadOutput {
                directory: options.output_path.clone(),
                template: options.output_template.clone(),
                // 🔧 从 TaskStatus 获取完整输出路径
                full_path: {
                    let tasks_guard = tasks.read().await;
                    tasks_guard
                        .get(&task_id)
                        .and_then(|handle| handle.status.output_path.clone())
                },
            },
            strategy: if let Some(stream_url) = options.ffmpeg_url.as_deref() {
                // m3u8/mpd 默认走 hls_dash（分段缓存 + 断点续传），其它仍走 ffmpeg
                if is_streaming_resource(stream_url) {
                    DownloadStrategy::HlsDash
                } else {
                    DownloadStrategy::Ffmpeg
                }
            } else if options.download_url.is_some() {
                DownloadStrategy::Direct
            } else {
                DownloadStrategy::Auto
            },
            extra: download::DownloadExtra {
                headers: headers_map, // ✅ 传递解析后的 headers
                cookie: magekit_extractor::cookies::build_cookie_header(
                    platform_hint,
                    cookies.as_deref(),
                ),
                query: Vec::new(),
                tool_args, // ✅ 传递其他 ffmpeg 参数
            },
            timeout: None,
            bandwidth_limit: None,
            retries: download::RetryPolicy::default(),
            resume: true,
        };

        // 🔧 添加详细的请求信息日志
        tracing::info!("📦 准备执行下载:");
        tracing::info!("  ├─ URL: {}", request.url);
        tracing::info!("  ├─ 策略: {:?}", request.strategy);
        tracing::info!("  ├─ full_path: {:?}", request.output.full_path);
        tracing::info!("  ├─ directory: {:?}", request.output.directory);
        tracing::info!("  ├─ template: {:?}", request.output.template);
        tracing::info!("  ├─ headers 数量: {}", request.extra.headers.len());
        tracing::info!("  └─ tool_args 数量: {}", request.extra.tool_args.len());

        // 执行下载
        tracing::info!("🚀 调用 download_client.download()...");
        let result = download_client
            .download(request, &callback, cancel_token)
            .await;
        tracing::info!("✅ download_client.download() 返回");

        // 等待进度监控完成
        let _ = progress_handle.await;

        // 处理结果
        match result {
            Ok(outcome) => {
                tracing::info!("✅ 任务 {} 下载成功: {:?}", task_id, outcome.output_path);
            }
            Err(e) => {
                // 取消（暂停/取消按钮）属于“控制流”，不应被视为失败。
                if matches!(e, download::DownloadError::Canceled) {
                    let should_ignore = {
                        let guard = tasks.read().await;
                        matches!(
                            guard.get(&task_id).map(|t| &t.status.state),
                            Some(TaskState::Paused | TaskState::Cancelled)
                        )
                    };
                    if should_ignore {
                        tracing::info!("⏸️ 任务 {} 已暂停/取消，忽略取消错误", task_id);
                        return;
                    }
                }

                tracing::error!("❌ 任务 {} 下载失败: {}", task_id, e);

                // 更新任务状态为失败
                Self::update_task_in_map(&tasks, task_id, |status| {
                    status.state = TaskState::Failed(e.to_string());
                    status.completed_at = Some(std::time::SystemTime::now());
                })
                .await;

                // 广播失败事件
                let _ = event_tx.send(ToolManagerEvent::TaskUpdate(TaskUpdate::Failed(
                    task_id,
                    e.to_string(),
                )));
            }
        }
    }

    /// 运行下载任务（旧架构 - 已废弃，保留用于兼容）
    #[allow(dead_code)]
    async fn run_download_task(
        task_id: TaskId,
        url: String,
        options: DownloadOptions,
        downloader: VideoDownloader,
        tasks: Arc<RwLock<HashMap<TaskId, TaskHandle>>>,
        event_tx: broadcast::Sender<ToolManagerEvent>,
        persistence: Arc<Mutex<TaskPersistence>>,
        mut cancel_rx: mpsc::Receiver<()>,
        cookies: Option<Vec<PlatformCookie>>,
    ) {
        // 创建进度通道
        let (progress_tx, mut progress_rx) = mpsc::channel(1000);

        // 克隆用于进度监控
        let tasks_for_progress = tasks.clone();
        let event_tx_for_progress = event_tx.clone();
        let persistence_for_progress = persistence.clone();

        // 启动进度监控任务
        let progress_handle = tokio::spawn(async move {
            while let Some(progress) = progress_rx.recv().await {
                match progress {
                    DownloadProgress::Started { .. } => {
                        let status =
                            Self::update_task_in_map(&tasks_for_progress, task_id, |status| {
                                status.state = TaskState::Downloading;
                                status.started_at = Some(std::time::SystemTime::now());
                            })
                            .await;

                        // 广播状态变更
                        let _ = event_tx_for_progress.send(ToolManagerEvent::TaskUpdate(
                            TaskUpdate::StateChanged(task_id, TaskState::Downloading),
                        ));

                        // 持久化
                        if let Some(status) = status {
                            let mut p = persistence_for_progress.lock().await;
                            let _ = p.update_task_status(task_id, status);
                        }
                    }
                    DownloadProgress::Progress {
                        percent,
                        speed,
                        eta,
                        downloaded_bytes,
                        total_bytes,
                        ..
                    } => {
                        let status =
                            Self::update_task_in_map(&tasks_for_progress, task_id, |status| {
                                status.progress = percent / 100.0; // 转换为 0-1 范围
                                if let Some(s) = speed {
                                    status.speed = Some(s);
                                }
                                status.eta = eta;
                                if let Some(d) = downloaded_bytes {
                                    status.downloaded_bytes = d;
                                }
                                if let Some(t) = total_bytes {
                                    status.total_bytes = Some(t);
                                }
                            })
                            .await;

                        // 广播进度更新
                        if let Some(s) = &status {
                            let _ = event_tx_for_progress.send(ToolManagerEvent::TaskUpdate(
                                TaskUpdate::Progress(
                                    task_id,
                                    s.progress,
                                    s.downloaded_bytes,
                                    s.total_bytes,
                                    s.speed,
                                    s.eta,
                                ),
                            ));
                        }
                    }
                    DownloadProgress::Completed { output_path, .. } => {
                        let status =
                            Self::update_task_in_map(&tasks_for_progress, task_id, |status| {
                                status.state = TaskState::Completed;
                                status.progress = 1.0;
                                status.completed_at = Some(std::time::SystemTime::now());
                                status.output_path = Some(output_path.clone());

                                // 获取文件大小
                                if let Ok(metadata) = std::fs::metadata(&output_path) {
                                    status.total_bytes = Some(metadata.len());
                                    status.downloaded_bytes = metadata.len();
                                }
                            })
                            .await;

                        // 广播完成事件
                        let _ = event_tx_for_progress.send(ToolManagerEvent::TaskUpdate(
                            TaskUpdate::Completed(task_id, output_path),
                        ));

                        // 持久化
                        if let Some(status) = status {
                            let mut p = persistence_for_progress.lock().await;
                            let _ = p.update_task_status(task_id, status);
                        }

                        tracing::info!("✅ 下载完成: {}", task_id);
                        break;
                    }
                    DownloadProgress::Error { error, .. } => {
                        let status =
                            Self::update_task_in_map(&tasks_for_progress, task_id, |status| {
                                status.state = TaskState::Failed(error.clone());
                                status.completed_at = Some(std::time::SystemTime::now());
                            })
                            .await;

                        // 广播失败事件
                        let _ = event_tx_for_progress.send(ToolManagerEvent::TaskUpdate(
                            TaskUpdate::Failed(task_id, error.clone()),
                        ));

                        // 持久化
                        if let Some(status) = status {
                            let mut p = persistence_for_progress.lock().await;
                            let _ = p.update_task_status(task_id, status);
                        }

                        tracing::error!("❌ 下载失败: {} - {}", task_id, error);
                        break;
                    }
                }
            }
        });

        // 执行下载，同时监听取消信号
        let download_result = tokio::select! {
            result = downloader.start_download(task_id, &url, options, progress_tx, cookies.as_deref()) => {
                result
            }
            _ = cancel_rx.recv() => {
                tracing::info!("🛑 下载任务收到取消信号: {}", task_id);
                Err(DownloadError::download_cancelled(&url))
            }
        };

        // 等待进度监控任务完成
        let _ = progress_handle.await;

        // 处理下载结果（如果进度监控没有处理）
        match &download_result {
            Ok(output_path) => {
                tracing::info!("✅ 下载任务完成: {} -> {:?}", task_id, output_path);
            }
            Err(e) => {
                let error_msg = e.to_string();
                // 检查是否是取消导致的
                if error_msg.contains("cancelled") || error_msg.contains("取消") {
                    // 取消状态已经在 cancel_download 中处理
                    tracing::info!("🛑 下载任务已取消: {}", task_id);
                } else {
                    tracing::error!("❌ 下载任务失败: {} - {}", task_id, e);
                }
            }
        }
    }

    /// 在任务映射中更新任务状态，返回更新后的状态副本
    async fn update_task_in_map<F>(
        tasks: &Arc<RwLock<HashMap<TaskId, TaskHandle>>>,
        task_id: TaskId,
        updater: F,
    ) -> Option<TaskStatus>
    where
        F: FnOnce(&mut TaskStatus),
    {
        let mut tasks_guard = tasks.write().await;
        if let Some(task) = tasks_guard.get_mut(&task_id) {
            updater(&mut task.status);
            Some(task.status.clone())
        } else {
            None
        }
    }

    /// 获取配置管理器
    pub fn config_manager(&self) -> &ConfigManager {
        &self.config_manager
    }

    /// 获取工具存储
    pub fn storage(&self) -> &ToolStorage {
        &self.storage
    }

    /// 获取工具更新器
    pub fn updater(&self) -> crate::updater::ToolUpdater {
        crate::updater::ToolUpdater::new(self.storage.clone())
    }

    // ==================== 新增：队列管理方法 ====================

    /// 将任务添加到队列（带优先级）
    pub async fn enqueue_download(
        &self,
        url: &str,
        options: DownloadOptions,
        priority: TaskPriority,
    ) -> DownloadResult<TaskId> {
        let task_id = Uuid::new_v4();

        // 创建队列任务
        let queued_task = QueuedTask {
            task_id,
            url: url.to_string(),
            options: options.clone(),
            priority,
            created_at: std::time::Instant::now(),
        };

        // 创建任务状态
        let task_status = TaskStatus::new(task_id, url.to_string(), None);

        // 添加到队列
        self.task_queue.enqueue(queued_task).await;

        // 持久化任务（包含下载选项，以便恢复）
        {
            let mut persistence = self.persistence.lock().await;
            let _ = persistence.add_task(
                task_status.clone(),
                self.max_retries,
                Some(options.clone()),
                None, // 队列任务暂不支持 cookies
            );
        }

        // 广播任务创建事件
        self.broadcast_event(ToolManagerEvent::TaskUpdate(TaskUpdate::Created(
            task_status,
        )));

        tracing::info!("Task {} enqueued with priority {:?}", task_id, priority);
        Ok(task_id)
    }

    /// 获取队列统计信息
    pub async fn get_queue_stats(&self) -> QueueStats {
        self.task_queue.stats().await
    }

    /// 暂停任务队列
    pub async fn pause_queue(&self) {
        self.task_queue.pause().await;
    }

    /// 恢复任务队列
    pub async fn resume_queue(&self) {
        self.task_queue.resume().await;
    }

    /// 清空任务队列
    pub async fn clear_queue(&self) {
        self.task_queue.clear().await;
    }

    /// 设置任务优先级
    pub async fn set_task_priority(&self, task_id: TaskId, priority: TaskPriority) -> bool {
        self.task_queue.set_priority(task_id, priority).await
    }

    // ==================== 新增：重试机制 ====================

    /// 重试失败的任务
    pub async fn retry_task(&self, task_id: TaskId) -> DownloadResult<()> {
        // 检查任务是否存在且失败
        let task_info = {
            let tasks = self.tasks.read().await;
            tasks
                .get(&task_id)
                .map(|t| (t.status.clone(), t.retry_count, t.max_retries))
        };

        if let Some((status, retry_count, max_retries)) = task_info {
            if !matches!(status.state, TaskState::Failed(_)) {
                return Err(DownloadError::task_operation_failed(
                    task_id,
                    "retry",
                    "Task is not in failed state".to_string(),
                ));
            }

            if retry_count >= max_retries {
                return Err(DownloadError::task_operation_failed(
                    task_id,
                    "retry",
                    format!("Max retries ({}) exceeded", max_retries),
                ));
            }

            // 更新重试次数
            {
                let mut tasks = self.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.retry_count += 1;
                    task.status.state = TaskState::Queued;
                }
            }

            // 更新持久化
            {
                let mut persistence = self.persistence.lock().await;
                let _ = persistence.increment_retry(task_id);
            }

            // 获取原始选项
            let original_options = {
                let tasks = self.tasks.read().await;
                tasks.get(&task_id).and_then(|t| t.options.clone())
            }
            .unwrap_or_default();

            // 重新加入队列
            let queued_task = QueuedTask {
                task_id,
                url: status.url.clone(),
                options: original_options,
                priority: TaskPriority::Normal,
                created_at: std::time::Instant::now(),
            };

            {
                self.task_queue.enqueue(queued_task).await;
            }

            // 广播状态变更
            self.broadcast_event(ToolManagerEvent::TaskUpdate(TaskUpdate::StateChanged(
                task_id,
                TaskState::Queued,
            )));

            tracing::info!(
                "Task {} retry scheduled (attempt {})",
                task_id,
                retry_count + 1
            );
            Ok(())
        } else {
            Err(DownloadError::task_not_found(task_id))
        }
    }

    /// 自动重试失败的任务（如果在重试限制内）
    pub async fn auto_retry_if_possible(&self, task_id: TaskId) -> bool {
        let can_retry = {
            let persistence = self.persistence.lock().await;
            persistence.can_retry(task_id)
        };

        if can_retry {
            match self.retry_task(task_id).await {
                Ok(_) => true,
                Err(e) => {
                    tracing::warn!("Auto retry failed for task {}: {}", task_id, e);
                    false
                }
            }
        } else {
            false
        }
    }

    // ==================== 新增：速度限制 ====================

    /// 设置下载速度限制
    pub async fn set_speed_limit(&self, bytes_per_second: Option<u64>) {
        *self.speed_limit.write().await = bytes_per_second;

        if let Some(limit) = bytes_per_second {
            tracing::info!("Speed limit set to {} bytes/s", limit);
        } else {
            tracing::info!("Speed limit removed");
        }
    }

    /// 获取当前速度限制
    pub async fn get_speed_limit(&self) -> Option<u64> {
        *self.speed_limit.read().await
    }

    // ==================== 新增：持久化操作 ====================

    /// 恢复未完成的任务
    pub async fn restore_tasks(&self) -> Vec<TaskId> {
        let persistence = self.persistence.lock().await;
        let resumable = persistence.get_resumable_tasks();

        let mut restored = Vec::new();
        for task in resumable {
            let status = task.status.clone();
            restored.push(status.id);

            // 将任务添加到内存中的任务列表，保留 options 和 cookies
            let mut tasks = self.tasks.write().await;
            tasks.insert(
                status.id,
                TaskHandle {
                    status: status.clone(),
                    cancel_tx: None,
                    retry_count: task.retry_count,
                    max_retries: task.max_retries,
                    options: task.options.clone(), // ✅ 恢复 options
                    cookies: task.cookies.clone(), // ✅ 恢复 cookies
                },
            );
        }

        tracing::info!("Restored {} tasks from persistence", restored.len());
        restored
    }

    /// 清除已完成的任务（从持久化存储）
    pub async fn clear_completed_tasks(&self) -> DownloadResult<()> {
        // 从内存中移除
        {
            let mut tasks = self.tasks.write().await;
            tasks.retain(|_, task| !task.status.is_finished());
        }

        // 从持久化存储中移除
        {
            let mut persistence = self.persistence.lock().await;
            persistence
                .clear_completed()
                .map_err(|e| DownloadError::internal(e.to_string()))?;
        }

        Ok(())
    }

    /// 保存当前任务状态
    pub async fn save_tasks(&self) -> DownloadResult<()> {
        let persistence = self.persistence.lock().await;
        persistence
            .save()
            .map_err(|e| DownloadError::internal(e.to_string()))
    }

    // ==================== 新增：批量操作 ====================

    /// 批量暂停任务
    pub async fn pause_all(&self) {
        let task_ids: Vec<TaskId> = {
            let tasks = self.tasks.read().await;
            tasks.keys().cloned().collect()
        };

        for task_id in task_ids {
            let _ = self.pause_download(task_id).await;
        }
    }

    /// 批量恢复任务
    pub async fn resume_all(&self) {
        let task_ids: Vec<TaskId> = {
            let tasks = self.tasks.read().await;
            tasks
                .iter()
                .filter(|(_, t)| matches!(t.status.state, TaskState::Paused))
                .map(|(id, _)| *id)
                .collect()
        };

        for task_id in task_ids {
            let _ = self.resume_download(task_id).await;
        }
    }

    /// 批量取消任务
    pub async fn cancel_all(&self) {
        let task_ids: Vec<TaskId> = {
            let tasks = self.tasks.read().await;
            tasks.keys().cloned().collect()
        };

        for task_id in task_ids {
            let _ = self.cancel_download(task_id).await;
        }
    }

    /// 获取活跃任务数量
    pub async fn active_task_count(&self) -> usize {
        let tasks = self.tasks.read().await;
        tasks.values().filter(|t| t.status.is_active()).count()
    }

    /// 获取已完成任务数量
    pub async fn completed_task_count(&self) -> usize {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|t| matches!(t.status.state, TaskState::Completed))
            .count()
    }

    /// 获取失败任务数量
    pub async fn failed_task_count(&self) -> usize {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|t| matches!(t.status.state, TaskState::Failed(_)))
            .count()
    }

    // ==================== 新增：快速入队方法 ====================

    /// 快速入队下载任务（立即返回，异步启动下载）
    ///
    /// 与 start_download 的区别：
    /// - 立即创建任务并返回 ID，不等待视频信息
    /// - 在后台异步获取信息并启动下载
    /// - 标题优先使用 URL 文件名，而不是视频标题
    /// - 自动识别资源类型，直接资源跳过 extractor
    pub async fn quick_enqueue_download(
        &self,
        url: &str,
        mut options: DownloadOptions,
        cookies: Option<&[PlatformCookie]>,
    ) -> DownloadResult<TaskId> {
        // 添加调试日志
        tracing::debug!(
            "📝 quick_enqueue_download 入口: url={}, ffmpeg_args长度={}",
            url,
            options.ffmpeg_args.len()
        );

        // 创建任务 ID
        let task_id = Uuid::new_v4();

        // 从 URL 提取标题
        let title = extract_title_from_url(url);

        // 检测资源类型（用于直链/流媒体的自动策略）
        let is_direct = is_direct_resource(url);
        let is_streaming = is_streaming_resource(url);

        // 自动设置 download_url 或 ffmpeg_url（如果用户没有手动设置）
        if options.download_url.is_none() && options.ffmpeg_url.is_none() {
            if is_streaming {
                options.ffmpeg_url = Some(url.to_string());
                tracing::debug!(
                    "🎞️ 自动设置 ffmpeg_url 用于流媒体，ffmpeg_args长度={}",
                    options.ffmpeg_args.len()
                );
            } else if is_direct {
                options.download_url = Some(url.to_string());
                tracing::debug!("📦 自动设置 download_url 用于直接资源");
            }
        }

        // 创建任务状态
        let mut task_status = TaskStatus::new(task_id, url.to_string(), Some(title.clone()));
        task_status.state = TaskState::Queued;

        // 直链/流媒体：生成固定输出路径；yt-dlp：交给模板生成（保证 ext/容器匹配）
        if options.download_url.is_some() || options.ffmpeg_url.is_some() {
            // 生成初步输出路径（使用 URL 文件名）
            // 🔧 关键：从 URL 提取扩展名，但流媒体格式强制转换为 mp4
            let ext = url::Url::parse(url)
                .ok()
                .and_then(|u| {
                    u.path_segments().and_then(|s| s.last()).and_then(|name| {
                        // 去除查询参数
                        let name_without_query = name.split('?').next().unwrap_or(name);
                        name_without_query
                            .rsplit_once('.')
                            .map(|(_, e)| e.to_string())
                    })
                })
                .map(|e| {
                    // 🔧 流媒体格式转换为 mp4
                    let e_lower = e.to_lowercase();
                    if e_lower == "m3u8" || e_lower == "m3u" || e_lower == "mpd" || e_lower == "ts"
                    {
                        "mp4".to_string()
                    } else {
                        e
                    }
                })
                .unwrap_or_else(|| "mp4".to_string());

            let output_path = generate_output_path(&options.output_path, &title, &ext)
                .map_err(|e| DownloadError::internal(e.to_string()))?;
            task_status.output_path = Some(output_path);

            // 🔧 直链/流媒体使用固定路径，避免库层二次改名
            options.output_template = None;
        } else if options.output_template.is_none() {
            options.output_template = Some("%(title)s.%(ext)s".to_string());
        }

        options.task_title = Some(title.clone());

        // 创建任务句柄
        let (cancel_tx, cancel_rx) = mpsc::channel(1);
        let cookies_owned: Option<Vec<PlatformCookie>> = cookies.map(|c| c.to_vec());
        let task_handle = TaskHandle {
            status: task_status.clone(),
            cancel_tx: Some(cancel_tx),
            retry_count: 0,
            max_retries: self.max_retries,
            options: Some(options.clone()),
            cookies: cookies_owned.clone(),
        };

        // 存储任务
        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(task_id, task_handle);
        }

        // 持久化任务
        {
            let mut persistence = self.persistence.lock().await;
            let _ = persistence.add_task(
                task_status.clone(),
                self.max_retries,
                Some(options.clone()),
                cookies_owned.clone(),
            );
        }

        // 发送任务创建事件
        self.broadcast_event(ToolManagerEvent::TaskUpdate(TaskUpdate::Created(
            task_status,
        )));

        tracing::info!("📝 快速入队任务: {} - {}", task_id, title);

        // 在后台异步启动下载
        let download_client = self.download_client.clone();
        let download_semaphore = self.download_semaphore.clone();
        let tasks = self.tasks.clone();
        let event_tx = self.event_tx.clone();
        let persistence = self.persistence.clone();
        let url_clone = url.to_string();

        tokio::spawn(async move {
            let _permit = match download_semaphore.acquire_owned().await {
                Ok(p) => p,
                Err(_) => return,
            };

            let cancelled = {
                let guard = tasks.read().await;
                matches!(
                    guard.get(&task_id).map(|t| &t.status.state),
                    Some(TaskState::Cancelled)
                )
            };
            if cancelled {
                return;
            }
            // 检测资源类型和下载策略
            let is_direct = is_direct_resource(&url_clone);
            let is_streaming = is_streaming_resource(&url_clone);
            let has_ffmpeg_url = options.ffmpeg_url.is_some();
            let has_download_url = options.download_url.is_some();

            // 决定使用哪种下载方式
            // 优先级：
            // 1. 用户明确指定了 download_url → 直接下载
            // 2. 用户明确指定了 ffmpeg_url → ffmpeg
            // 3. 自动检测：is_streaming → ffmpeg
            // 4. 自动检测：is_direct → 直接下载
            // 5. 其他 → extractor（yt-dlp）

            // 自动设置 download_url 或 ffmpeg_url（如果用户没有手动设置）
            let mut options = options;
            if !has_download_url && !has_ffmpeg_url {
                if is_streaming {
                    // 流媒体自动设置 ffmpeg_url
                    options.ffmpeg_url = Some(url_clone.clone());
                    tracing::debug!(
                        "🔧 自动设置 ffmpeg_url 用于流媒体，ffmpeg_args长度={}",
                        options.ffmpeg_args.len()
                    );
                } else if is_direct {
                    // 直接资源自动设置 download_url
                    options.download_url = Some(url_clone.clone());
                    tracing::debug!("🔧 自动设置 download_url 用于直接资源");
                }
            }

            let use_direct_download = if has_download_url || options.download_url.is_some() {
                true
            } else if has_ffmpeg_url || options.ffmpeg_url.is_some() {
                false
            } else {
                is_direct && !is_streaming
            };

            let use_ffmpeg = if has_ffmpeg_url || options.ffmpeg_url.is_some() {
                true
            } else if has_download_url || options.download_url.is_some() {
                false
            } else {
                is_streaming
            };

            if use_direct_download {
                // 直接下载资源，使用新架构
                tracing::info!("🔗 使用直接下载: {}", url_clone);
                Self::run_download_task_new(
                    task_id,
                    url_clone,
                    options,
                    download_client,
                    tasks,
                    event_tx,
                    persistence,
                    cancel_rx,
                    cookies_owned,
                )
                .await;
            } else if use_ffmpeg {
                // 流媒体使用 ffmpeg
                tracing::info!("📺 使用 ffmpeg 拉流: {}", url_clone);
                Self::run_download_task_new(
                    task_id,
                    url_clone,
                    options,
                    download_client,
                    tasks,
                    event_tx,
                    persistence,
                    cancel_rx,
                    cookies_owned,
                )
                .await;
            } else {
                // 需要 yt-dlp 的资源（YouTube、Bilibili 等），统一走 download 库
                tracing::info!("🎬 使用 yt-dlp（download 库）: {}", url_clone);
                Self::run_download_task_new(
                    task_id,
                    url_clone,
                    options,
                    download_client,
                    tasks,
                    event_tx,
                    persistence,
                    cancel_rx,
                    cookies_owned,
                )
                .await;
            }
        });

        Ok(task_id)
    }
}

// ==================== 辅助函数 ====================

/// 从 URL 提取文件名作为标题
fn extract_title_from_url(url: &str) -> String {
    use url::Url;

    if let Ok(parsed_url) = Url::parse(url) {
        // 获取路径的最后一部分
        if let Some(segments) = parsed_url.path_segments() {
            if let Some(last_segment) = segments.last() {
                if !last_segment.is_empty() {
                    // 去除扩展名
                    let name_without_ext = last_segment
                        .rsplit_once('.')
                        .map(|(name, _)| name)
                        .unwrap_or(last_segment);

                    // URL 解码
                    if let Ok(decoded) = urlencoding::decode(name_without_ext) {
                        let decoded_str = decoded.to_string();
                        // 限制长度
                        if decoded_str.len() > 100 {
                            return format!("{}...", &decoded_str[..97]);
                        }
                        return decoded_str;
                    }

                    return name_without_ext.to_string();
                }
            }
        }

        // 如果路径为空，使用域名
        if let Some(host) = parsed_url.host_str() {
            return host.to_string();
        }
    }

    // 降级方案：使用 URL 的 hash
    format!("download_{}", &url.chars().take(20).collect::<String>())
}

/// 从 URL 猜测平台名（用于 cookie 选择）
///
/// 注意：这里仅用于“启用 cookies 时的粗粒度匹配”，不追求 100% 精确。
fn platform_hint_from_url(url: &str) -> Option<&'static str> {
    let url = url.to_lowercase();
    if url.contains("bilibili.com") || url.contains("b23.tv") {
        Some("bilibili")
    } else if url.contains("youtube.com") || url.contains("youtu.be") {
        Some("youtube")
    } else if url.contains("twitter.com") || url.contains("x.com") {
        Some("twitter")
    } else if url.contains("instagram.com") {
        Some("instagram")
    } else if url.contains("tiktok.com") {
        Some("tiktok")
    } else if url.contains("douyin.com") {
        Some("douyin")
    } else if url.contains("weibo.com") {
        Some("weibo")
    } else if url.contains("xiaohongshu.com") || url.contains("xhs.link") {
        Some("xiaohongshu")
    } else {
        None
    }
}

/// 检测是否为直接下载资源（图片、视频文件等）
/// 注意：m3u8 等流媒体需要用 ffmpeg，不是直接下载
fn is_direct_resource(url: &str) -> bool {
    let direct_exts = [
        // 图片
        "jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "ico",
        // 视频文件（非流媒体）
        "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", // 音频
        "mp3", "wav", "flac", "aac", "ogg", "m4a",
        "wma",
        // 注意：m3u8, m3u, ts, mpd 等流媒体需要 ffmpeg，不在这里
    ];

    if let Ok(parsed_url) = url::Url::parse(url) {
        if let Some(path) = parsed_url.path_segments() {
            if let Some(last) = path.last() {
                if let Some(ext) = last.rsplit_once('.').map(|(_, e)| e.to_lowercase()) {
                    return direct_exts.contains(&ext.as_str());
                }
            }
        }
    }

    false
}

fn select_best_direct_format(formats: &[magekit_shared::VideoFormat]) -> Option<&magekit_shared::VideoFormat> {
    formats
        .iter()
        .filter(|f| f.download_url.is_some())
        .max_by_key(|f| {
            (
                parse_height_from_resolution(f.resolution.as_deref()).unwrap_or(0),
                codec_preference_score(f),
            )
        })
}

fn codec_preference_score(format: &magekit_shared::VideoFormat) -> u8 {
    // 经验优先级：h264（通用兼容） > 未知 > h265/hevc（Windows/部分播放器可能缺 codec）
    let mut s = format.format_id.to_lowercase();
    if let Some(q) = format.quality.as_deref() {
        s.push(' ');
        s.push_str(&q.to_lowercase());
    }
    if let Some(v) = format.vcodec.as_deref() {
        s.push(' ');
        s.push_str(&v.to_lowercase());
    }

    if s.contains("h264") || s.contains("avc") {
        2
    } else if s.contains("h265") || s.contains("hevc") {
        0
    } else {
        1
    }
}

fn parse_height_from_resolution(resolution: Option<&str>) -> Option<u32> {
    let res = resolution?;
    // 常见：1920x1080
    if let Some(h) = res.split('x').last().and_then(|h| h.parse::<u32>().ok()) {
        return Some(h);
    }
    // 兜底：1080p
    if let Some(stripped) = res.strip_suffix('p') {
        return stripped.parse::<u32>().ok();
    }
    None
}

/// 检测是否为流媒体资源（需要 ffmpeg 下载）
fn is_streaming_resource(url: &str) -> bool {
    let streaming_exts = ["m3u8", "m3u", "ts", "mpd"];

    // 先检查扩展名
    if let Ok(parsed_url) = url::Url::parse(url) {
        if let Some(path) = parsed_url.path_segments() {
            if let Some(last) = path.last() {
                // 去除查询参数后再检查扩展名
                let path_without_query = last.split('?').next().unwrap_or(last);
                if let Some(ext) = path_without_query
                    .rsplit_once('.')
                    .map(|(_, e)| e.to_lowercase())
                {
                    if streaming_exts.contains(&ext.as_str()) {
                        return true;
                    }
                }
            }
        }
    }

    // 特殊处理：URL 中包含流媒体特征（处理特殊格式的 URL）
    url.contains(".m3u8") || url.contains(".m3u") || url.contains(".mpd")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use magekit_shared::VideoFormat;

    #[test]
    fn test_parse_ffmpeg_headers() {
        // 模拟 capture 页面构造的 ffmpeg_args
        let mut ffmpeg_args = Vec::new();
        ffmpeg_args.push("-headers".to_string());
        ffmpeg_args.push("User-Agent: Mozilla/5.0\r\nReferer: https://example.com/\r\nOrigin: https://example.com\r\n".to_string());
        ffmpeg_args.push("-c".to_string());
        ffmpeg_args.push("copy".to_string());

        // 解析 headers（复制自 task_manager.rs 中的逻辑）
        let mut headers_map = HashMap::new();
        let mut other_args = Vec::new();
        let mut i = 0;
        while i < ffmpeg_args.len() {
            if ffmpeg_args[i] == "-headers" && i + 1 < ffmpeg_args.len() {
                let headers_str = &ffmpeg_args[i + 1];
                for line in headers_str.split("\r\n") {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if let Some(pos) = line.find(':') {
                        let key = line[..pos].trim().to_string();
                        let value = line[pos + 1..].trim().to_string();
                        headers_map.insert(key, value);
                    }
                }
                i += 2;
            } else {
                other_args.push(ffmpeg_args[i].clone());
                i += 1;
            }
        }

        // 验证结果
        assert_eq!(headers_map.len(), 3);
        assert_eq!(
            headers_map.get("User-Agent"),
            Some(&"Mozilla/5.0".to_string())
        );
        assert_eq!(
            headers_map.get("Referer"),
            Some(&"https://example.com/".to_string())
        );
        assert_eq!(
            headers_map.get("Origin"),
            Some(&"https://example.com".to_string())
        );
        assert_eq!(other_args, vec!["-c".to_string(), "copy".to_string()]);
    }

    #[test]
    fn test_parse_empty_ffmpeg_args() {
        let ffmpeg_args: Vec<String> = Vec::new();

        let mut headers_map = HashMap::new();
        let mut other_args = Vec::new();
        let mut i = 0;
        while i < ffmpeg_args.len() {
            if ffmpeg_args[i] == "-headers" && i + 1 < ffmpeg_args.len() {
                let headers_str = &ffmpeg_args[i + 1];
                for line in headers_str.split("\r\n") {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if let Some(pos) = line.find(':') {
                        let key = line[..pos].trim().to_string();
                        let value = line[pos + 1..].trim().to_string();
                        headers_map.insert(key, value);
                    }
                }
                i += 2;
            } else {
                other_args.push(ffmpeg_args[i].clone());
                i += 1;
            }
        }

        assert_eq!(headers_map.len(), 0);
        assert_eq!(other_args.len(), 0);
    }

    #[test]
    fn test_parse_ffmpeg_args_without_headers() {
        let mut ffmpeg_args = Vec::new();
        ffmpeg_args.push("-c".to_string());
        ffmpeg_args.push("copy".to_string());
        ffmpeg_args.push("-f".to_string());
        ffmpeg_args.push("mp4".to_string());

        let mut headers_map = HashMap::new();
        let mut other_args = Vec::new();
        let mut i = 0;
        while i < ffmpeg_args.len() {
            if ffmpeg_args[i] == "-headers" && i + 1 < ffmpeg_args.len() {
                let headers_str = &ffmpeg_args[i + 1];
                for line in headers_str.split("\r\n") {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if let Some(pos) = line.find(':') {
                        let key = line[..pos].trim().to_string();
                        let value = line[pos + 1..].trim().to_string();
                        headers_map.insert(key, value);
                    }
                }
                i += 2;
            } else {
                other_args.push(ffmpeg_args[i].clone());
                i += 1;
            }
        }

        assert_eq!(headers_map.len(), 0);
        assert_eq!(other_args.len(), 4);
        assert_eq!(other_args, vec!["-c", "copy", "-f", "mp4"]);
    }

    #[test]
    fn test_select_best_direct_format_prefers_highest_height() {
        let formats = vec![
            VideoFormat {
                format_id: "720p".to_string(),
                ext: "mp4".to_string(),
                resolution: Some("1280x720".to_string()),
                fps: None,
                filesize: None,
                vcodec: None,
                acodec: None,
                quality: None,
                download_url: Some("https://example.com/720.mp4".to_string()),
            },
            VideoFormat {
                format_id: "1080p".to_string(),
                ext: "mp4".to_string(),
                resolution: Some("1920x1080".to_string()),
                fps: None,
                filesize: None,
                vcodec: None,
                acodec: None,
                quality: None,
                download_url: Some("https://example.com/1080.mp4".to_string()),
            },
            // 即使更高分辨率，但没有直链也不应被选中
            VideoFormat {
                format_id: "4k".to_string(),
                ext: "mp4".to_string(),
                resolution: Some("3840x2160".to_string()),
                fps: None,
                filesize: None,
                vcodec: None,
                acodec: None,
                quality: None,
                download_url: None,
            },
        ];

        let best = super::select_best_direct_format(&formats).expect("best format");
        assert_eq!(best.format_id, "1080p");
        assert_eq!(best.download_url.as_deref(), Some("https://example.com/1080.mp4"));
    }

    #[test]
    fn test_select_best_direct_format_prefers_h264_when_height_equal() {
        let formats = vec![
            VideoFormat {
                format_id: "1080p_h265".to_string(),
                ext: "mp4".to_string(),
                resolution: Some("1920x1080".to_string()),
                fps: None,
                filesize: None,
                vcodec: Some("h265".to_string()),
                acodec: None,
                quality: None,
                download_url: Some("https://example.com/h265.mp4".to_string()),
            },
            VideoFormat {
                format_id: "1080p_h264".to_string(),
                ext: "mp4".to_string(),
                resolution: Some("1920x1080".to_string()),
                fps: None,
                filesize: None,
                vcodec: Some("h264".to_string()),
                acodec: None,
                quality: None,
                download_url: Some("https://example.com/h264.mp4".to_string()),
            },
        ];

        let best = super::select_best_direct_format(&formats).expect("best format");
        assert_eq!(best.download_url.as_deref(), Some("https://example.com/h264.mp4"));
    }

    #[test]
    fn test_parse_height_from_resolution_supports_common_shapes() {
        assert_eq!(super::parse_height_from_resolution(Some("1920x1080")), Some(1080));
        assert_eq!(super::parse_height_from_resolution(Some("1080p")), Some(1080));
        assert_eq!(super::parse_height_from_resolution(Some("bad")), None);
        assert_eq!(super::parse_height_from_resolution(None), None);
    }
}
