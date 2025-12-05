//! 应用程序状态管理
//!
//! 负责管理整个应用的状态，包括工具管理器实例、任务列表、配置等。

use anyhow::Result;
use gpui::Global;
use magekit_shared::{AppConfig, TaskStatus, TaskUpdate, DownloadOptions, TaskId};
use magekit_tool_manager::ToolManager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::runtime::Runtime;

/// Global 包装器，用于在 GPUI 上下文中共享 AppState
pub struct GlobalAppState(pub Arc<AppState>);

impl Global for GlobalAppState {}

/// 应用程序主状态
pub struct AppState {
    /// 工具管理器
    pub tool_manager: Arc<ToolManager>,
    /// 应用配置
    pub config: Arc<RwLock<AppConfig>>,
    /// 任务状态列表
    pub tasks: Arc<RwLock<HashMap<TaskId, TaskStatus>>>,
    /// 事件接收器
    pub event_rx: mpsc::Receiver<AppEvent>,
    /// 事件发送器
    pub event_tx: mpsc::Sender<AppEvent>,
    /// Tokio 运行时 (用于异步任务)
    pub runtime: Arc<Runtime>,
}

/// 应用程序事件
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// 任务更新事件
    TaskUpdate(TaskUpdate),
    /// 配置变更事件
    ConfigChanged(AppConfig),
    /// 工具更新事件
    ToolUpdate(magekit_tool_manager::UpdateInfo),
    /// 显示通知
    ShowNotification(NotificationMessage),
}

/// 通知消息
#[derive(Debug, Clone)]
pub struct NotificationMessage {
    pub title: String,
    pub message: String,
    pub notification_type: NotificationType,
}

/// 通知类型
#[derive(Debug, Clone)]
pub enum NotificationType {
    Info,
    Success,
    Warning,
    Error,
}

/// 工具状态
#[derive(Debug, Clone)]
pub enum ToolStatus {
    /// 未安装
    NotInstalled,
    /// 已安装 (version: 版本号, is_system: 是否是系统安装)
    Installed { version: Option<String>, is_system: bool },
}

impl AppState {
    /// 创建新的应用状态 (同步版本)
    pub fn new_sync() -> Result<Self> {
        // 创建 Tokio 运行时
        let runtime = Runtime::new()
            .map_err(|e| anyhow::anyhow!("创建 Tokio 运行时失败: {}", e))?;
        let runtime = Arc::new(runtime);

        // 创建事件通道
        let (event_tx, event_rx) = mpsc::channel(1000);

        // 使用默认配置
        let config = AppConfig::default();
        let config = Arc::new(RwLock::new(config));

        // 创建工具管理器
        let tool_manager = Arc::new(ToolManager::new_sync()?);

        // 初始化任务列表
        let tasks = Arc::new(RwLock::new(HashMap::new()));

        Ok(Self {
            tool_manager,
            config,
            tasks,
            event_rx,
            event_tx,
            runtime,
        })
    }

    /// 创建新的应用状态 (异步版本，保持兼容)
    pub async fn new() -> Result<Self> {
        Self::new_sync()
    }

    /// 获取当前配置
    pub fn config(&self) -> AppConfig {
        // 使用 blocking_read 在同步上下文中读取
        self.config.blocking_read().clone()
    }

    /// 更新配置
    pub async fn update_config(&self, new_config: AppConfig) -> Result<()> {
        let mut config = self.config.write().await;
        *config = new_config.clone();
        
        // 发送配置变更事件
        let _ = self.event_tx.send(AppEvent::ConfigChanged(new_config)).await;
        
        Ok(())
    }

    /// 开始下载任务
    pub async fn start_download(&self, url: String, options: Option<DownloadOptions>) -> Result<TaskId> {
        let download_options = options.unwrap_or_default();

        // 使用默认输出路径
        let output_path = std::env::current_dir()
            .unwrap_or_default()
            .join("downloads");

        let options_with_path = DownloadOptions {
            output_path,
            ..download_options
        };

        // 开始下载（简化版本）
        let task_id = uuid::Uuid::new_v4();

        tracing::info!("开始下载任务: {} - {}", task_id, url);

        // 创建任务状态
        let task_status = TaskStatus::new(task_id, url.clone(), Some("视频标题".to_string()));

        // 存储任务
        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(task_id, task_status);
        }

        // 发送通知
        let _ = self.event_tx.send(AppEvent::ShowNotification(NotificationMessage {
            title: "下载开始".to_string(),
            message: format!("开始下载: {}", url),
            notification_type: NotificationType::Info,
        })).await;

        Ok(task_id)
    }

    /// 暂停下载任务
    pub async fn pause_download(&self, task_id: TaskId) -> Result<()> {
        self.tool_manager.pause_download(task_id).await?;
        let _ = self.event_tx.send(AppEvent::ShowNotification(NotificationMessage {
            title: "下载已暂停".to_string(),
            message: format!("任务 {} 已暂停", task_id),
            notification_type: NotificationType::Warning,
        })).await;
        Ok(())
    }

    /// 恢复下载任务
    pub async fn resume_download(&self, task_id: TaskId) -> Result<()> {
        self.tool_manager.resume_download(task_id).await?;
        let _ = self.event_tx.send(AppEvent::ShowNotification(NotificationMessage {
            title: "下载已恢复".to_string(),
            message: format!("任务 {} 已恢复", task_id),
            notification_type: NotificationType::Info,
        })).await;
        Ok(())
    }

    /// 取消下载任务
    pub async fn cancel_download(&self, task_id: TaskId) -> Result<()> {
        self.tool_manager.cancel_download(task_id).await?;

        // 从任务列表中移除
        let mut tasks = self.tasks.write().await;
        tasks.remove(&task_id);

        let _ = self.event_tx.send(AppEvent::ShowNotification(NotificationMessage {
            title: "下载已取消".to_string(),
            message: format!("任务 {} 已取消", task_id),
            notification_type: NotificationType::Warning,
        })).await;
        Ok(())
    }

    /// 获取所有任务状态
    pub async fn get_all_tasks(&self) -> Vec<TaskStatus> {
        self.tool_manager.get_all_tasks().await
    }

    /// 获取指定任务状态
    pub async fn get_task_status(&self, task_id: TaskId) -> Option<TaskStatus> {
        self.tool_manager.get_task_status(task_id).await
    }

    /// 发送事件
    pub async fn send_event(&self, event: AppEvent) -> Result<()> {
        self.event_tx.send(event).await
            .map_err(|e| anyhow::anyhow!("发送事件失败: {}", e))?;
        Ok(())
    }

    /// 检查工具是否已安装
    pub async fn is_tool_installed(&self, tool_type: magekit_shared::ToolType) -> bool {
        self.tool_manager.storage.is_tool_installed(tool_type).await
    }

    /// 获取工具版本
    pub async fn get_tool_version(&self, tool_type: magekit_shared::ToolType) -> Option<String> {
        self.tool_manager.storage.get_tool_version(tool_type).await.ok().flatten()
    }

    /// 检测工具状态 - 同步版本 (用于 UI 初始化)
    pub fn check_tool_status_sync(&self, tool_type: magekit_shared::ToolType) -> ToolStatus {
        // 先检查应用内安装 (使用同步版本)
        if self.tool_manager.storage.is_tool_installed_sync(tool_type) {
            let version = self.tool_manager.storage.get_tool_version_sync(tool_type).ok().flatten();
            return ToolStatus::Installed { version, is_system: false };
        }

        // 再检查系统 PATH
        let tool_name = match tool_type {
            magekit_shared::ToolType::YtDlp => "yt-dlp",
            magekit_shared::ToolType::Ffmpeg => "ffmpeg",
        };

        if let Ok(path) = which::which(tool_name) {
            // 获取系统工具版本
            let version = Self::get_system_tool_version_sync(tool_type, &path);
            return ToolStatus::Installed { version, is_system: true };
        }

        ToolStatus::NotInstalled
    }

    /// 检测工具状态 - 异步版本 (用于后台任务)
    /// 注意: 此方法虽然是 async，但内部使用同步调用，因为 GPUI 的 spawn 不在 Tokio 运行时中
    pub async fn check_tool_status(&self, tool_type: magekit_shared::ToolType) -> ToolStatus {
        self.check_tool_status_sync(tool_type)
    }

    /// 获取系统工具版本 (同步版本)
    fn get_system_tool_version_sync(tool_type: magekit_shared::ToolType, path: &std::path::Path) -> Option<String> {
        use std::process::Command;
        
        let output = Command::new(path)
            .arg("--version")
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let version_output = String::from_utf8_lossy(&output.stdout);

        match tool_type {
            magekit_shared::ToolType::YtDlp => {
                // yt-dlp 输出格式: "yt-dlp 2023.07.06" 或 "2023.07.06"
                version_output
                    .lines()
                    .next()
                    .and_then(|line| {
                        // 查找以 20 开头的版本号（如 2023.07.06）
                        line.split_whitespace()
                            .find(|s| s.starts_with("20"))
                    })
                    .map(|v| v.to_string())
            }
            magekit_shared::ToolType::Ffmpeg => {
                // ffmpeg 输出格式: "ffmpeg version 8.0 Copyright..." 或 "ffmpeg version 5.1.2"
                version_output
                    .lines()
                    .find(|line| line.contains("ffmpeg version"))
                    .and_then(|line| {
                        // 获取 "version" 后面的部分
                        line.split("version").nth(1)
                            .and_then(|rest| rest.split_whitespace().next())
                    })
                    .map(|v| v.to_string())
            }
        }
    }

    /// 安装单个工具 (使用内部 Tokio 运行时，在后台线程中运行)
    pub fn install_tool_in_background(&self, tool_type: magekit_shared::ToolType) -> std::thread::JoinHandle<Result<()>> {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();
        
        std::thread::spawn(move || {
            runtime.block_on(async {
                match tool_type {
                    magekit_shared::ToolType::YtDlp => {
                        let updater = magekit_tool_manager::updater::ToolUpdater::new(tool_manager.storage.clone());
                        updater.ensure_yt_dlp(magekit_shared::UpdateChannel::Stable).await
                            .map_err(|e| anyhow::anyhow!("安装 yt-dlp 失败: {}", e))?;
                    }
                    magekit_shared::ToolType::Ffmpeg => {
                        let updater = magekit_tool_manager::updater::ToolUpdater::new(tool_manager.storage.clone());
                        updater.ensure_ffmpeg().await
                            .map_err(|e| anyhow::anyhow!("安装 ffmpeg 失败: {}", e))?;
                    }
                }
                Ok(())
            })
        })
    }

    /// 安装单个工具 (带进度回调，在后台线程中运行)
    /// progress_callback: fn(downloaded, total, speed)
    pub fn install_tool_with_progress(
        &self,
        tool_type: magekit_shared::ToolType,
        progress_callback: std::sync::Arc<dyn Fn(u64, u64, u64) + Send + Sync>,
    ) -> std::thread::JoinHandle<Result<()>> {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();
        
        std::thread::spawn(move || {
            runtime.block_on(async {
                match tool_type {
                    magekit_shared::ToolType::YtDlp => {
                        let updater = magekit_tool_manager::updater::ToolUpdater::new(tool_manager.storage.clone());
                        updater.ensure_yt_dlp_with_progress(
                            magekit_shared::UpdateChannel::Stable,
                            Some(progress_callback),
                        ).await
                            .map_err(|e| anyhow::anyhow!("安装 yt-dlp 失败: {}", e))?;
                    }
                    magekit_shared::ToolType::Ffmpeg => {
                        // ffmpeg 暂不支持进度回调
                        let updater = magekit_tool_manager::updater::ToolUpdater::new(tool_manager.storage.clone());
                        updater.ensure_ffmpeg().await
                            .map_err(|e| anyhow::anyhow!("安装 ffmpeg 失败: {}", e))?;
                    }
                }
                Ok(())
            })
        })
    }

    /// 删除已安装的工具
    pub fn delete_tool_sync(&self, tool_type: magekit_shared::ToolType) -> Result<()> {
        self.tool_manager.storage.delete_tool_sync(tool_type)
            .map_err(|e| anyhow::anyhow!("删除工具失败: {}", e))
    }

    /// 安装工具 (使用内部 Tokio 运行时) - 阻塞版本，不推荐使用
    #[allow(dead_code)]
    pub fn install_tool_blocking(&self, tool_type: magekit_shared::ToolType) -> Result<()> {
        use magekit_shared::UpdateChannel;
        use magekit_tool_manager::updater::ToolUpdater;
        
        self.runtime.block_on(async {
            let updater = ToolUpdater::new(self.tool_manager.storage.clone());
            match tool_type {
                magekit_shared::ToolType::YtDlp => {
                    updater.ensure_yt_dlp(UpdateChannel::Stable).await
                        .map_err(|e| anyhow::anyhow!("安装 yt-dlp 失败: {}", e))?;
                }
                magekit_shared::ToolType::Ffmpeg => {
                    updater.ensure_ffmpeg().await
                        .map_err(|e| anyhow::anyhow!("安装 ffmpeg 失败: {}", e))?;
                }
            }
            Ok(())
        })
    }

    /// 安装工具 (异步版本 - 在 Tokio 运行时中调用)
    pub async fn install_tool(&self, tool_type: magekit_shared::ToolType) -> Result<()> {
        use magekit_shared::UpdateChannel;
        
        match tool_type {
            magekit_shared::ToolType::YtDlp => {
                self.tool_manager.ensure_tools(UpdateChannel::Stable).await
                    .map_err(|e| anyhow::anyhow!("安装 yt-dlp 失败: {}", e))?;
            }
            magekit_shared::ToolType::Ffmpeg => {
                self.tool_manager.ensure_tools(UpdateChannel::Stable).await
                    .map_err(|e| anyhow::anyhow!("安装 ffmpeg 失败: {}", e))?;
            }
        }
        Ok(())
    }

    /// 安装所有工具 (使用内部 Tokio 运行时)
    pub fn install_all_tools_blocking(&self) -> Result<()> {
        use magekit_shared::UpdateChannel;
        self.runtime.block_on(async {
            self.tool_manager.ensure_tools(UpdateChannel::Stable).await
                .map_err(|e| anyhow::anyhow!("安装工具失败: {}", e))
        })
    }

    /// 安装所有工具 (后台线程，非阻塞)
    pub fn install_all_tools_in_background(&self) -> std::thread::JoinHandle<Result<()>> {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();
        
        std::thread::spawn(move || {
            runtime.block_on(async {
                tool_manager.ensure_tools(magekit_shared::UpdateChannel::Stable).await
                    .map_err(|e| anyhow::anyhow!("安装工具失败: {}", e))
            })
        })
    }

    /// 安装所有工具 (异步版本)
    pub async fn install_all_tools(&self) -> Result<()> {
        use magekit_shared::UpdateChannel;
        self.tool_manager.ensure_tools(UpdateChannel::Stable).await
            .map_err(|e| anyhow::anyhow!("安装工具失败: {}", e))?;
        Ok(())
    }

    /// 接收事件
    pub async fn recv_event(&mut self) -> Option<AppEvent> {
        self.event_rx.recv().await
    }

    /// 更新任务状态
    pub async fn update_task(&self, task_update: TaskUpdate) {
        match task_update {
            TaskUpdate::Created(task_status) => {
                let mut tasks = self.tasks.write().await;
                tasks.insert(task_status.id, task_status);
            }
            TaskUpdate::Progress(task_id, progress, downloaded, total, speed, eta) => {
                let mut tasks = self.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.progress = progress;
                    task.downloaded_bytes = downloaded;
                    task.total_bytes = total;
                    task.speed = speed;
                    task.eta = eta;
                }
            }
            TaskUpdate::StateChanged(task_id, state) => {
                let mut tasks = self.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.state = state.clone();

                    // 发送状态变更通知
                    let notification_type = match state {
                        magekit_shared::TaskState::Completed => NotificationType::Success,
                        magekit_shared::TaskState::Failed(_) => NotificationType::Error,
                        _ => NotificationType::Info,
                    };

                    let message = match state {
                        magekit_shared::TaskState::Completed => "下载完成".to_string(),
                        magekit_shared::TaskState::Failed(ref error) => format!("下载失败: {}", error),
                        _ => format!("任务状态变更为: {:?}", state),
                    };

                    // 这里应该发送通知，但由于借用检查器问题，暂时跳过
                    // let _ = self.event_tx.send(AppEvent::ShowNotification(NotificationMessage {
                    //     title: "任务状态更新".to_string(),
                    //     message,
                    //     notification_type,
                    // })).await;
                }
            }
            TaskUpdate::SpeedUpdate(task_id, speed) => {
                let mut tasks = self.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.speed = Some(speed);
                }
            }
            TaskUpdate::Completed(task_id, output_path) => {
                let mut tasks = self.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.state = magekit_shared::TaskState::Completed;
                    task.progress = 100.0;
                    task.output_path = Some(output_path);
                    task.completed_at = Some(std::time::SystemTime::now());
                }
            }
            TaskUpdate::Failed(task_id, error) => {
                let mut tasks = self.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.state = magekit_shared::TaskState::Failed(error.clone());
                    task.completed_at = Some(std::time::SystemTime::now());
                }
            }
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        // 使用 new_sync 来创建默认实例
        // 如果创建失败，程序将 panic (这是合理的，因为没有工具管理器应用无法运行)
        Self::new_sync().expect("Failed to create default AppState")
    }
}