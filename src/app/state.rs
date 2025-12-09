//! 应用程序核心状态
//!
//! 定义 AppState 结构体和基础方法
//!
//! 设计原则：
//! - AppState 作为 GUI 层与 ToolManager 的桥梁
//! - 任务管理完全委托给 ToolManager
//! - 通过事件订阅获取任务状态更新
//! - GUI 层只负责渲染，不直接管理下载逻辑

use anyhow::Result;
use gpui::Global;
use magekit_shared::{AppConfig, DownloadOptions, TaskId, TaskState, TaskStatus, TaskUpdate};
use magekit_shared::{load_app_config_or_default, save_app_config};
use magekit_tool_manager::{ToolManager, ToolManagerEvent};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::{RwLock, broadcast, mpsc};

use super::types::AppEvent;

/// Global 包装器，用于在 GPUI 上下文中共享 AppState
pub struct GlobalAppState(pub Arc<AppState>);

impl Global for GlobalAppState {}

/// 应用程序主状态
///
/// 职责：
/// - 持有 ToolManager 实例
/// - 管理应用配置
/// - 维护任务状态的只读视图（由 ToolManager 事件驱动更新）
/// - 提供 GUI 层访问接口
pub struct AppState {
    /// 工具管理器（所有下载逻辑的唯一入口）
    pub tool_manager: Arc<ToolManager>,
    /// 应用配置
    pub config: Arc<RwLock<AppConfig>>,
    /// 任务状态缓存（由事件驱动更新，GUI 只读访问）
    pub tasks: Arc<RwLock<HashMap<TaskId, TaskStatus>>>,
    /// 应用事件发送器（用于通知 UI）
    pub event_tx: mpsc::Sender<AppEvent>,
    /// 应用事件接收器
    pub event_rx: mpsc::Receiver<AppEvent>,
    /// Tokio 运行时
    pub runtime: Arc<Runtime>,
}

impl AppState {
    /// 创建新的应用状态 (同步版本)
    pub fn new_sync() -> Result<Self> {
        // 创建 Tokio 运行时
        let runtime =
            Runtime::new().map_err(|e| anyhow::anyhow!("创建 Tokio 运行时失败: {}", e))?;
        let runtime = Arc::new(runtime);

        // 创建应用事件通道
        let (event_tx, event_rx) = mpsc::channel(1000);

        // 从文件加载配置
        let config = load_app_config_or_default();
        tracing::info!(
            "Loaded config: download_path={:?}",
            config.download.default_output_path
        );
        let config = Arc::new(RwLock::new(config));

        // 创建工具管理器
        let tool_manager = Arc::new(ToolManager::new_sync()?);

        // 初始化任务列表
        let tasks = Arc::new(RwLock::new(HashMap::new()));

        // 订阅 ToolManager 事件并转发到应用事件
        let tool_manager_rx = tool_manager.subscribe();
        let event_tx_clone = event_tx.clone();
        let tasks_clone = tasks.clone();

        runtime.spawn(async move {
            Self::event_listener_loop(tool_manager_rx, event_tx_clone, tasks_clone).await;
        });

        // 加载持久化的任务
        let tool_manager_clone = tool_manager.clone();
        let tasks_clone = tasks.clone();
        runtime.block_on(async move {
            let restored_tasks = tool_manager_clone.get_all_tasks().await;
            if !restored_tasks.is_empty() {
                tracing::info!("📦 恢复 {} 个持久化任务", restored_tasks.len());
                let mut tasks_to_resume = Vec::new();

                {
                    let mut tasks_map = tasks_clone.write().await;
                    for task_status in restored_tasks {
                        tracing::debug!(
                            "  - {} ({:?})",
                            task_status.title.as_deref().unwrap_or("Unknown"),
                            task_status.state
                        );

                        // 记录需要恢复的任务（下载中或排队中的任务）
                        if matches!(
                            task_status.state,
                            TaskState::Downloading | TaskState::Queued
                        ) {
                            tasks_to_resume.push(task_status.id);
                        }

                        tasks_map.insert(task_status.id, task_status);
                    }
                }

                // 自动恢复下载中的任务
                if !tasks_to_resume.is_empty() {
                    tracing::info!("🔄 自动恢复 {} 个下载任务", tasks_to_resume.len());
                    for task_id in tasks_to_resume {
                        if let Err(e) = tool_manager_clone.resume_download(task_id).await {
                            tracing::warn!("⚠️ 恢复任务 {} 失败: {}", task_id, e);
                        }
                    }
                }
            }
        });

        Ok(Self {
            tool_manager,
            config,
            tasks,
            event_rx,
            event_tx,
            runtime,
        })
    }

    /// 事件监听循环
    ///
    /// 监听 ToolManager 的事件，更新本地任务缓存，并转发到应用事件
    async fn event_listener_loop(
        mut tool_manager_rx: broadcast::Receiver<ToolManagerEvent>,
        event_tx: mpsc::Sender<AppEvent>,
        tasks: Arc<RwLock<HashMap<TaskId, TaskStatus>>>,
    ) {
        tracing::info!("🎧 开始监听 ToolManager 事件");

        loop {
            match tool_manager_rx.recv().await {
                Ok(event) => {
                    match &event {
                        ToolManagerEvent::TaskUpdate(update) => {
                            // 更新本地任务缓存
                            Self::apply_task_update(&tasks, update).await;

                            // 转发到应用事件
                            let _ = event_tx.send(AppEvent::TaskUpdate(update.clone())).await;
                        }
                        ToolManagerEvent::ToolUpdate(info) => {
                            let _ = event_tx.send(AppEvent::ToolUpdate(info.clone())).await;
                        }
                        ToolManagerEvent::Error(msg) => {
                            tracing::error!("ToolManager 错误: {}", msg);
                        }
                        ToolManagerEvent::QueueEvent(info) => {
                            tracing::debug!("队列事件: {:?}", info);
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("事件监听器落后 {} 条消息", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("ToolManager 事件通道已关闭");
                    break;
                }
            }
        }
    }

    /// 应用任务更新到本地缓存
    async fn apply_task_update(
        tasks: &Arc<RwLock<HashMap<TaskId, TaskStatus>>>,
        update: &TaskUpdate,
    ) {
        let mut tasks = tasks.write().await;

        match update {
            TaskUpdate::Created(status) => {
                tracing::info!(
                    "📝 新任务创建: {} - {}",
                    status.id,
                    status.title.as_deref().unwrap_or("Unknown")
                );
                tasks.insert(status.id, status.clone());
            }
            TaskUpdate::Progress(task_id, progress, downloaded, total, speed, eta) => {
                if let Some(task) = tasks.get_mut(task_id) {
                    task.progress = *progress;
                    task.downloaded_bytes = *downloaded;
                    task.total_bytes = *total;
                    task.speed = *speed;
                    task.eta = *eta;
                }
            }
            TaskUpdate::StateChanged(task_id, state) => {
                tracing::info!("🔄 任务状态变更: {} -> {:?}", task_id, state);
                if let Some(task) = tasks.get_mut(task_id) {
                    task.state = state.clone();
                    if matches!(
                        state,
                        TaskState::Completed | TaskState::Failed(_) | TaskState::Cancelled
                    ) {
                        task.completed_at = Some(std::time::SystemTime::now());
                    }
                }
            }
            TaskUpdate::SpeedUpdate(task_id, speed) => {
                if let Some(task) = tasks.get_mut(task_id) {
                    task.speed = Some(*speed);
                }
            }
            TaskUpdate::Completed(task_id, output_path) => {
                tracing::info!("✅ 任务完成: {} -> {:?}", task_id, output_path);
                if let Some(task) = tasks.get_mut(task_id) {
                    task.state = TaskState::Completed;
                    task.progress = 1.0;
                    task.output_path = Some(output_path.clone());
                    task.completed_at = Some(std::time::SystemTime::now());
                }
            }
            TaskUpdate::Failed(task_id, error) => {
                tracing::error!("❌ 任务失败: {} - {}", task_id, error);
                if let Some(task) = tasks.get_mut(task_id) {
                    task.state = TaskState::Failed(error.clone());
                    task.completed_at = Some(std::time::SystemTime::now());
                }
            }
        }
    }

    /// 创建新的应用状态 (异步版本)
    pub async fn new() -> Result<Self> {
        Self::new_sync()
    }

    /// 获取当前配置
    pub fn config(&self) -> AppConfig {
        self.config.blocking_read().clone()
    }

    /// 更新配置
    pub async fn update_config(&self, new_config: AppConfig) -> Result<()> {
        let mut config = self.config.write().await;
        *config = new_config.clone();

        if let Err(e) = save_app_config(&new_config) {
            tracing::error!("Failed to save config to file: {}", e);
        } else {
            tracing::info!("Config saved successfully");
        }

        let _ = self
            .event_tx
            .send(AppEvent::ConfigChanged(new_config))
            .await;

        Ok(())
    }

    /// 发送应用事件
    pub async fn send_event(&self, event: AppEvent) -> Result<()> {
        self.event_tx
            .send(event)
            .await
            .map_err(|e| anyhow::anyhow!("发送事件失败: {}", e))?;
        Ok(())
    }

    /// 接收应用事件
    pub async fn recv_event(&mut self) -> Option<AppEvent> {
        self.event_rx.recv().await
    }

    // ==================== 下载相关 API（委托给 ToolManager）====================

    /// 开始下载任务
    ///
    /// 委托给 ToolManager 处理，任务状态通过事件自动更新
    pub async fn start_download(&self, url: &str, options: DownloadOptions) -> Result<TaskId> {
        let cookies_vec = {
            let config = self.config.read().await;
            if config.advanced.cookies.is_empty() {
                None
            } else {
                Some(config.advanced.cookies.clone())
            }
        };

        tracing::info!("🚀 发起下载请求: {}", url);

        let task_id = self
            .tool_manager
            .start_download(url, options, cookies_vec.as_deref())
            .await
            .map_err(|e| anyhow::anyhow!("开始下载失败: {}", e))?;

        tracing::info!("✅ 下载任务已创建: {}", task_id);
        Ok(task_id)
    }

    /// 暂停下载任务
    pub async fn pause_download(&self, task_id: TaskId) -> Result<()> {
        self.tool_manager
            .pause_download(task_id)
            .await
            .map_err(|e| anyhow::anyhow!("暂停任务失败: {}", e))
    }

    /// 恢复下载任务
    pub async fn resume_download(&self, task_id: TaskId) -> Result<()> {
        self.tool_manager
            .resume_download(task_id)
            .await
            .map_err(|e| anyhow::anyhow!("恢复任务失败: {}", e))
    }

    /// 取消下载任务
    pub async fn cancel_download(&self, task_id: TaskId) -> Result<()> {
        self.tool_manager
            .cancel_download(task_id)
            .await
            .map_err(|e| anyhow::anyhow!("取消任务失败: {}", e))
    }

    /// 删除任务（从持久化存储中删除）
    pub async fn delete_task(&self, task_id: TaskId) -> Result<()> {
        // 先取消（如果正在运行）
        let _ = self.tool_manager.cancel_download(task_id).await;

        // 从持久化存储删除
        self.tool_manager
            .delete_task_status(task_id)
            .await
            .map_err(|e| anyhow::anyhow!("删除任务失败: {}", e))?;

        // 从本地缓存删除
        {
            let mut tasks = self.tasks.write().await;
            tasks.remove(&task_id);
        }

        Ok(())
    }

    /// 获取视频信息
    pub async fn get_video_info(&self, url: &str) -> Result<magekit_shared::VideoInfo> {
        let cookies_vec = {
            let config = self.config.read().await;
            if config.advanced.cookies.is_empty() {
                None
            } else {
                Some(config.advanced.cookies.clone())
            }
        };

        self.tool_manager
            .get_video_info(url, cookies_vec.as_deref())
            .await
            .map_err(|e| anyhow::anyhow!("获取视频信息失败: {}", e))
    }

    /// 获取频道/播放列表信息
    pub async fn get_channel_videos(&self, url: &str) -> Result<magekit_shared::ChannelInfo> {
        let cookies_vec = {
            let config = self.config.read().await;
            if config.advanced.cookies.is_empty() {
                None
            } else {
                Some(config.advanced.cookies.clone())
            }
        };

        self.tool_manager
            .get_channel_videos(url, cookies_vec.as_deref())
            .await
            .map_err(|e| anyhow::anyhow!("获取频道信息失败: {}", e))
    }

    // ==================== 任务查询 API ====================

    /// 获取所有任务状态（从本地缓存读取）
    pub async fn get_all_tasks(&self) -> Vec<TaskStatus> {
        let tasks = self.tasks.read().await;
        tasks.values().cloned().collect()
    }

    /// 获取指定任务状态（从本地缓存读取）
    pub async fn get_task_status(&self, task_id: TaskId) -> Option<TaskStatus> {
        let tasks = self.tasks.read().await;
        tasks.get(&task_id).cloned()
    }

    /// 清空已完成的任务
    pub async fn clear_completed_tasks(&self) -> Result<()> {
        // 从 ToolManager 清理
        self.tool_manager
            .clear_completed_tasks()
            .await
            .map_err(|e| anyhow::anyhow!("清理已完成任务失败: {}", e))?;

        // 从本地缓存清理
        {
            let mut tasks = self.tasks.write().await;
            tasks.retain(|_, task| {
                !matches!(task.state, TaskState::Completed | TaskState::Cancelled)
            });
        }

        Ok(())
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new_sync().expect("Failed to create default AppState")
    }
}
