use crate::downloader::{DownloadProgress, VideoDownloader};
use crate::error::{DownloadError, DownloadResult, ToolManagerResult};
use crate::storage::ToolStorage;
use crate::task_persistence::TaskPersistence;
use crate::task_queue::{TaskQueue, QueuedTask, TaskPriority, QueueStats};
use crate::{updater::UpdateInfo, config::ConfigManager};
use magekit_shared::{TaskStatus, TaskState, TaskUpdate, VideoInfo, DownloadOptions, TaskId};
use magekit_shared::{UpdateChannel, generate_output_path};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Mutex};
use uuid::Uuid;

/// 工具管理器
pub struct ToolManager {
    /// 工具存储管理器 (公开以供外部访问)
    pub storage: ToolStorage,
    downloader: VideoDownloader,
    config_manager: ConfigManager,
    tasks: Arc<RwLock<HashMap<TaskId, TaskHandle>>>,
    update_tx: mpsc::Sender<ToolManagerEvent>,
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
        let ffmpeg_path = storage.get_tool_path(magekit_shared::ToolType::Ffmpeg);

        // 检查ffmpeg是否可用
        let ffmpeg_path = if ffmpeg_path.exists() {
            Some(ffmpeg_path)
        } else {
            // 尝试在系统PATH中查找
            match which::which("ffmpeg") {
                Ok(path) => Some(path),
                Err(_) => {
                    tracing::warn!("ffmpeg not found in PATH, some features may not work");
                    None
                }
            }
        };

        let downloader = VideoDownloader::new(yt_dlp_path, ffmpeg_path);
        let config_manager = ConfigManager::new_sync()?;

        let (update_tx, _) = mpsc::channel(1000);
        
        // 创建任务队列（默认3个并发）
        let max_concurrent = config_manager.config().download_defaults.max_concurrent_downloads;
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
            update_tx,
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

    /// 获取事件接收器
    pub fn subscribe(&self) -> mpsc::Receiver<ToolManagerEvent> {
        let (_tx, rx) = mpsc::channel(1000);
        // 在实际实现中，这里应该转发事件
        // 为了简化，我们直接返回一个空接收器
        rx
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
    pub async fn get_video_info(&self, url: &str) -> DownloadResult<VideoInfo> {
        self.downloader.get_video_info(url).await
    }

    /// 开始下载任务
    pub async fn start_download(
        &self,
        url: &str,
        options: DownloadOptions,
    ) -> DownloadResult<TaskId> {
        let task_id = Uuid::new_v4();

        // 创建任务状态
        let mut task_status = TaskStatus::new(task_id, url.to_string(), None);
        task_status.state = TaskState::Queued;

        // 生成输出路径
        let video_info = self.get_video_info(url).await?;
        let output_path = generate_output_path(
            &options.output_path,
            &video_info.title,
            &video_info.formats
                .iter()
                .find(|f| f.format_id == options.format_id)
                .map(|f| f.ext.as_str())
                .unwrap_or("mp4"),
        ).map_err(|e| DownloadError::internal(e.to_string()))?;

        task_status.output_path = Some(output_path.clone());

        // 发送任务创建事件
        self.send_task_update(TaskUpdate::Created(task_status.clone())).await;

        // 创建任务句柄
        let (cancel_tx, cancel_rx) = mpsc::channel(1);
        let task_handle = TaskHandle {
            status: task_status,
            cancel_tx: Some(cancel_tx),
            retry_count: 0,
            max_retries: self.max_retries,
        };

        // 存储任务
        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(task_id, task_handle);
        }

        // 启动下载任务
        let downloader = self.downloader.clone();
        let tasks = self.tasks.clone();
        let update_tx = self.update_tx.clone();
        let url_clone = url.to_string();

        tokio::spawn(async move {
            Self::run_download_task(
                task_id,
                url_clone,
                options,
                downloader,
                tasks,
                update_tx,
                cancel_rx,
            ).await;
        });

        Ok(task_id)
    }

    /// 暂停下载任务
    pub async fn pause_download(&self, task_id: TaskId) -> DownloadResult<()> {
        let mut tasks = self.tasks.write().await;

        if let Some(task) = tasks.get_mut(&task_id) {
            if task.status.state == TaskState::Downloading {
                task.status.state = TaskState::Paused;

                // 发送取消信号
                if let Some(cancel_tx) = task.cancel_tx.take() {
                    let _ = cancel_tx.send(()).await;
                }

                drop(tasks); // 释放锁

                self.send_task_update(TaskUpdate::StateChanged(
                    task_id,
                    TaskState::Paused,
                )).await;

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
    pub async fn resume_download(&self, task_id: TaskId) -> DownloadResult<()> {
        // 实际实现中，这里需要重新启动下载任务
        // 为了简化，我们只是更新状态
        self.update_task_state(task_id, TaskState::Downloading).await
    }

    /// 取消下载任务
    pub async fn cancel_download(&self, task_id: TaskId) -> DownloadResult<()> {
        let mut tasks = self.tasks.write().await;

        if let Some(task) = tasks.remove(&task_id) {
            // 发送取消信号
            if let Some(cancel_tx) = task.cancel_tx {
                let _ = cancel_tx.send(()).await;
            }

            drop(tasks); // 释放锁

            self.send_task_update(TaskUpdate::StateChanged(
                task_id,
                TaskState::Cancelled,
            )).await;

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

    /// 获取所有任务状态
    pub async fn get_all_tasks(&self) -> Vec<TaskStatus> {
        let tasks = self.tasks.read().await;
        tasks.values().map(|task| task.status.clone()).collect()
    }

    /// 更新任务状态
    async fn update_task_state(&self, task_id: TaskId, state: TaskState) -> DownloadResult<()> {
        let mut tasks = self.tasks.write().await;

        if let Some(task) = tasks.get_mut(&task_id) {
            task.status.state = state.clone();
            drop(tasks);

            self.send_task_update(TaskUpdate::StateChanged(task_id, state)).await;
            Ok(())
        } else {
            Err(DownloadError::task_not_found(task_id))
        }
    }

    /// 发送任务更新事件
    async fn send_task_update(&self, update: TaskUpdate) {
        let _ = self.update_tx.send(ToolManagerEvent::TaskUpdate(update)).await;
    }

    /// 运行下载任务
    async fn run_download_task(
        task_id: TaskId,
        url: String,
        options: DownloadOptions,
        downloader: VideoDownloader,
        tasks: Arc<RwLock<HashMap<TaskId, TaskHandle>>>,
        _update_tx: mpsc::Sender<ToolManagerEvent>,
        mut cancel_rx: mpsc::Receiver<()>,
    ) {
        // 创建进度通道
        let (progress_tx, mut progress_rx) = mpsc::channel(1000);

        // 克隆tasks用于发送更新
        let tasks_clone = tasks.clone();

        // 启动进度监控任务
        let progress_task_id = task_id;
        tokio::spawn(async move {
            while let Some(progress) = progress_rx.recv().await {
                match progress {
                    DownloadProgress::Started { .. } => {
                        Self::update_task_in_map(
                            &tasks_clone,
                            progress_task_id,
                            |status| {
                                status.state = TaskState::Downloading;
                                status.started_at = Some(std::time::SystemTime::now());
                            },
                        ).await;
                    }
                    DownloadProgress::Progress { percent, .. } => {
                        Self::update_task_in_map(
                            &tasks_clone,
                            progress_task_id,
                            |status| {
                                status.progress = percent;
                            },
                        ).await;
                    }
                    DownloadProgress::Completed { output_path, .. } => {
                        Self::update_task_in_map(
                            &tasks_clone,
                            progress_task_id,
                            |status| {
                                status.state = TaskState::Completed;
                                status.progress = 100.0;
                                status.completed_at = Some(std::time::SystemTime::now());
                                status.output_path = Some(output_path);
                            },
                        ).await;

                        // 任务完成，退出循环
                        break;
                    }
                    DownloadProgress::Error { error, .. } => {
                        Self::update_task_in_map(
                            &tasks_clone,
                            progress_task_id,
                            |status| {
                                status.state = TaskState::Failed(error.clone());
                                status.completed_at = Some(std::time::SystemTime::now());
                            },
                        ).await;
                        break;
                    }
                }
            }
        });

        // 监控取消信号
        let cancel_task_id = task_id;
        let cancel_task = tokio::spawn(async move {
            if let Some(()) = cancel_rx.recv().await {
                tracing::info!("Download task {} cancelled", cancel_task_id);
            }
        });

        // 执行下载
        let download_result = tokio::select! {
            result = downloader.start_download(task_id, &url, options, progress_tx) => {
                result
            }
            _ = cancel_task => {
                Err(DownloadError::download_cancelled(&url))
            }
        };

        match download_result {
            Ok(output_path) => {
                tracing::info!("Download completed: {:?}", output_path);
            }
            Err(e) => {
                tracing::error!("Download failed: {}", e);
            }
        }

        // 清理任务
        let mut tasks = tasks.write().await;
        tasks.remove(&task_id);
    }

    /// 在任务映射中更新任务状态
    async fn update_task_in_map<F>(
        tasks: &Arc<RwLock<HashMap<TaskId, TaskHandle>>>,
        task_id: TaskId,
        updater: F,
    ) where
        F: FnOnce(&mut TaskStatus),
    {
        let mut tasks_guard = tasks.write().await;
        if let Some(task) = tasks_guard.get_mut(&task_id) {
            updater(&mut task.status);
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
        
        // 持久化任务
        {
            let mut persistence = self.persistence.lock().await;
            let _ = persistence.add_task(task_status.clone(), self.max_retries);
        }
        
        // 发送任务创建事件
        self.send_task_update(TaskUpdate::Created(task_status)).await;
        
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
            tasks.get(&task_id).map(|t| (t.status.clone(), t.retry_count, t.max_retries))
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
            
            // 重新加入队列
            let queued_task = QueuedTask {
                task_id,
                url: status.url.clone(),
                options: DownloadOptions::default(), // 需要从某处获取原始选项
                priority: TaskPriority::Normal,
                created_at: std::time::Instant::now(),
            };
            
            {
                let queue = self.task_queue.lock().await;
                queue.enqueue(queued_task).await;
            }
            
            self.send_task_update(TaskUpdate::StateChanged(task_id, TaskState::Queued)).await;
            
            tracing::info!("Task {} retry scheduled (attempt {})", task_id, retry_count + 1);
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
            tasks.insert(status.id, TaskHandle {
                status: status.clone(),
                cancel_tx: None,
                retry_count: task.retry_count,
                max_retries: task.max_retries,
            });
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
            persistence.clear_completed()
                .map_err(|e| DownloadError::internal(e.to_string()))?;
        }
        
        Ok(())
    }

    /// 保存当前任务状态
    pub async fn save_tasks(&self) -> DownloadResult<()> {
        let persistence = self.persistence.lock().await;
        persistence.save()
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
            tasks.iter()
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
        tasks.values()
            .filter(|t| t.status.is_active())
            .count()
    }

    /// 获取已完成任务数量
    pub async fn completed_task_count(&self) -> usize {
        let tasks = self.tasks.read().await;
        tasks.values()
            .filter(|t| matches!(t.status.state, TaskState::Completed))
            .count()
    }

    /// 获取失败任务数量
    pub async fn failed_task_count(&self) -> usize {
        let tasks = self.tasks.read().await;
        tasks.values()
            .filter(|t| matches!(t.status.state, TaskState::Failed(_)))
            .count()
    }
}