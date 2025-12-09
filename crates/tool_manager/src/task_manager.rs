use crate::downloader::{DownloadProgress, VideoDownloader};
use crate::error::{DownloadError, DownloadResult, ToolManagerResult};
use crate::storage::ToolStorage;
use crate::task_persistence::TaskPersistence;
use crate::task_queue::{QueueStats, QueuedTask, TaskPriority, TaskQueue};
use crate::{config::ConfigManager, updater::UpdateInfo};
use magekit_shared::{DownloadOptions, PlatformCookie, TaskId, TaskState, TaskStatus, TaskUpdate, VideoInfo};
use magekit_shared::{UpdateChannel, generate_output_path};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};
use uuid::Uuid;

/// 工具管理器
pub struct ToolManager {
    /// 工具存储管理器 (公开以供外部访问)
    pub storage: ToolStorage,
    downloader: VideoDownloader,
    config_manager: ConfigManager,
    tasks: Arc<RwLock<HashMap<TaskId, TaskHandle>>>,
    /// 事件广播发送器（用于向所有订阅者广播事件）
    event_tx: broadcast::Sender<ToolManagerEvent>,
    /// 任务队列
    task_queue: Arc<Mutex<TaskQueue>>,
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
        let ffmpeg_path = magekit_shared::resolve_ffmpeg_path()
            .or_else(|| {
                let p = storage.get_tool_path(magekit_shared::ToolType::Ffmpeg);
                if p.exists() { Some(p) } else { None }
            });

        let downloader = VideoDownloader::new(yt_dlp_path, ffmpeg_path);
        let config_manager = ConfigManager::new_sync()?;

        // 创建广播通道（容量 1000）
        let (event_tx, _) = broadcast::channel(1000);

        // 创建任务队列（默认3个并发）
        let max_concurrent = config_manager
            .config()
            .download_defaults
            .max_concurrent_downloads;
        let (task_queue, _queue_rx) = TaskQueue::new(max_concurrent);

        // 创建任务持久化
        let persistence = TaskPersistence::new().unwrap_or_default();

        // 获取最大重试次数
        let max_retries = config_manager.config().download_defaults.retry_times;

        Ok(Self {
            storage,
            downloader,
            config_manager,
            tasks: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            task_queue: Arc::new(Mutex::new(task_queue)),
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
        self.start_download_with_info(url, options, cookies, None).await
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
        options: DownloadOptions,
        cookies: Option<&[PlatformCookie]>,
        video_info: Option<VideoInfo>,
    ) -> DownloadResult<TaskId> {
        // 每次下载都创建新任务（使用新的 UUID）
        let task_id = Uuid::new_v4();

        // 创建任务状态
        let mut task_status = TaskStatus::new(task_id, url.to_string(), None);
        task_status.state = TaskState::Queued;

        // 获取视频信息（如果未提供）
        let video_info = match video_info {
            Some(info) => info,
            None => self.get_video_info(url, cookies).await?,
        };
        task_status.title = Some(video_info.title.clone());
        
        let output_path = generate_output_path(
            &options.output_path,
            &video_info.title,
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
        self.broadcast_event(ToolManagerEvent::TaskUpdate(TaskUpdate::Created(task_status)));

        // 启动下载任务
        let downloader = self.downloader.clone();
        let tasks = self.tasks.clone();
        let event_tx = self.event_tx.clone();
        let persistence = self.persistence.clone();
        let url_clone = url.to_string();

        tokio::spawn(async move {
            Self::run_download_task(
                task_id,
                url_clone,
                options,
                downloader,
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
                self.broadcast_event(ToolManagerEvent::TaskUpdate(
                    TaskUpdate::StateChanged(task_id, TaskState::Paused)
                ));

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
            tasks.get(&task_id).map(|t| {
                (t.status.clone(), t.options.clone(), t.cookies.clone())
            })
        };

        // 如果内存中没有，尝试从持久化存储中获取
        let (mut status, options, cookies) = match task_info {
            Some(info) => info,
            None => {
                // 从持久化存储中获取
                let persistence = self.persistence.lock().await;
                let persisted_task = persistence.get_task(task_id)
                    .ok_or_else(|| DownloadError::task_not_found(task_id))?;
                (
                    persisted_task.status.clone(),
                    persisted_task.options.clone(),
                    persisted_task.cookies.clone(),
                )
            }
        };

        // 检查状态 - 允许恢复 Downloading、Paused 和 Failed 状态的任务
        if !matches!(status.state, TaskState::Downloading | TaskState::Paused | TaskState::Failed(_)) {
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
        self.broadcast_event(ToolManagerEvent::TaskUpdate(
            TaskUpdate::StateChanged(task_id, TaskState::Downloading)
        ));

        // 重新启动下载任务
        let downloader = self.downloader.clone();
        let tasks = self.tasks.clone();
        let event_tx = self.event_tx.clone();
        let persistence = self.persistence.clone();
        let url = status.url.clone();

        tokio::spawn(async move {
            Self::run_download_task(
                task_id,
                url,
                options,
                downloader,
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
            self.broadcast_event(ToolManagerEvent::TaskUpdate(
                TaskUpdate::StateChanged(task_id, TaskState::Cancelled)
            ));

            tracing::info!("🛑 任务已取消: {}", task_id);
            Ok(())
        } else {
            Err(DownloadError::task_not_found(task_id))
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
            self.broadcast_event(ToolManagerEvent::TaskUpdate(
                TaskUpdate::StateChanged(task_id, state)
            ));
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
                        let status = Self::update_task_in_map(&tasks_for_progress, task_id, |status| {
                            status.state = TaskState::Downloading;
                            status.started_at = Some(std::time::SystemTime::now());
                        }).await;
                        
                        // 广播状态变更
                        let _ = event_tx_for_progress.send(ToolManagerEvent::TaskUpdate(
                            TaskUpdate::StateChanged(task_id, TaskState::Downloading)
                        ));
                        
                        // 持久化
                        if let Some(status) = status {
                            let mut p = persistence_for_progress.lock().await;
                            let _ = p.update_task_status(task_id, status);
                        }
                    }
                    DownloadProgress::Progress { percent, speed, eta, downloaded_bytes, total_bytes, .. } => {
                        let status = Self::update_task_in_map(&tasks_for_progress, task_id, |status| {
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
                        }).await;
                        
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
                                )
                            ));
                        }
                    }
                    DownloadProgress::Completed { output_path, .. } => {
                        let status = Self::update_task_in_map(&tasks_for_progress, task_id, |status| {
                            status.state = TaskState::Completed;
                            status.progress = 1.0;
                            status.completed_at = Some(std::time::SystemTime::now());
                            status.output_path = Some(output_path.clone());
                            
                            // 获取文件大小
                            if let Ok(metadata) = std::fs::metadata(&output_path) {
                                status.total_bytes = Some(metadata.len());
                                status.downloaded_bytes = metadata.len();
                            }
                        }).await;
                        
                        // 广播完成事件
                        let _ = event_tx_for_progress.send(ToolManagerEvent::TaskUpdate(
                            TaskUpdate::Completed(task_id, output_path)
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
                        let status = Self::update_task_in_map(&tasks_for_progress, task_id, |status| {
                            status.state = TaskState::Failed(error.clone());
                            status.completed_at = Some(std::time::SystemTime::now());
                        }).await;
                        
                        // 广播失败事件
                        let _ = event_tx_for_progress.send(ToolManagerEvent::TaskUpdate(
                            TaskUpdate::Failed(task_id, error.clone())
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
        {
            let queue = self.task_queue.lock().await;
            queue.enqueue(queued_task).await;
        }

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
        self.broadcast_event(ToolManagerEvent::TaskUpdate(TaskUpdate::Created(task_status)));

        tracing::info!("Task {} enqueued with priority {:?}", task_id, priority);
        Ok(task_id)
    }

    /// 获取队列统计信息
    pub async fn get_queue_stats(&self) -> QueueStats {
        let queue = self.task_queue.lock().await;
        queue.stats().await
    }

    /// 暂停任务队列
    pub async fn pause_queue(&self) {
        let queue = self.task_queue.lock().await;
        queue.pause().await;
    }

    /// 恢复任务队列
    pub async fn resume_queue(&self) {
        let queue = self.task_queue.lock().await;
        queue.resume().await;
    }

    /// 清空任务队列
    pub async fn clear_queue(&self) {
        let queue = self.task_queue.lock().await;
        queue.clear().await;
    }

    /// 设置任务优先级
    pub async fn set_task_priority(&self, task_id: TaskId, priority: TaskPriority) -> bool {
        let queue = self.task_queue.lock().await;
        queue.set_priority(task_id, priority).await
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
            }.unwrap_or_default();

            // 重新加入队列
            let queued_task = QueuedTask {
                task_id,
                url: status.url.clone(),
                options: original_options,
                priority: TaskPriority::Normal,
                created_at: std::time::Instant::now(),
            };

            {
                let queue = self.task_queue.lock().await;
                queue.enqueue(queued_task).await;
            }

            // 广播状态变更
            self.broadcast_event(ToolManagerEvent::TaskUpdate(
                TaskUpdate::StateChanged(task_id, TaskState::Queued)
            ));

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

            // 将任务添加到内存中的任务列表
            let mut tasks = self.tasks.write().await;
            tasks.insert(
                status.id,
                TaskHandle {
                    status: status.clone(),
                    cancel_tx: None,
                    retry_count: task.retry_count,
                    max_retries: task.max_retries,
                    options: None,
                    cookies: None,
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
}
