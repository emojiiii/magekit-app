//! 应用程序核心状态
//!
//! 定义 AppState 结构体和基础方法

use anyhow::Result;
use gpui::Global;
use magekit_shared::{AppConfig, TaskStatus, TaskId};
use magekit_shared::{load_app_config_or_default, save_app_config};
use magekit_tool_manager::ToolManager;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::{mpsc, RwLock};
use tokio::runtime::Runtime;
use parking_lot::Mutex;

use super::types::AppEvent;

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
    /// 下载取消标志 (task_id -> cancel_flag)
    pub download_cancel_flags: Arc<Mutex<HashMap<TaskId, Arc<AtomicBool>>>>,
    /// 下载暂停标志 (task_id -> pause_flag)
    pub download_pause_flags: Arc<Mutex<HashMap<TaskId, Arc<AtomicBool>>>>,
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

        // 从文件加载配置
        let config = load_app_config_or_default();
        tracing::info!("Loaded config: download_path={:?}", config.download.default_output_path);
        let config = Arc::new(RwLock::new(config));

        // 创建工具管理器
        let tool_manager = Arc::new(ToolManager::new_sync()?);

        // 初始化任务列表
        let tasks = Arc::new(RwLock::new(HashMap::new()));
        
        // 加载持久化的任务
        let tool_manager_clone = tool_manager.clone();
        let tasks_clone = tasks.clone();
        runtime.block_on(async move {
            let restored_tasks = tool_manager_clone.get_all_tasks().await;
            if !restored_tasks.is_empty() {
                tracing::info!("📦 恢复 {} 个持久化任务", restored_tasks.len());
                let mut tasks_map = tasks_clone.write().await;
                for task_status in restored_tasks {
                    tracing::debug!("  - {} ({:?})", task_status.title.as_deref().unwrap_or("Unknown"), task_status.state);
                    tasks_map.insert(task_status.id, task_status);
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
            download_cancel_flags: Arc::new(Mutex::new(HashMap::new())),
            download_pause_flags: Arc::new(Mutex::new(HashMap::new())),
        })
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
        
        let _ = self.event_tx.send(AppEvent::ConfigChanged(new_config)).await;
        
        Ok(())
    }

    /// 发送事件
    pub async fn send_event(&self, event: AppEvent) -> Result<()> {
        self.event_tx.send(event).await
            .map_err(|e| anyhow::anyhow!("发送事件失败: {}", e))?;
        Ok(())
    }

    /// 接收事件
    pub async fn recv_event(&mut self) -> Option<AppEvent> {
        self.event_rx.recv().await
    }
    
    /// 保存任务状态到持久化存储
    pub fn save_task_to_persistence(&self, task_status: &TaskStatus) {
        let tool_manager = self.tool_manager.clone();
        let task_status = task_status.clone();
        
        self.runtime.spawn(async move {
            if let Err(e) = tool_manager.save_task_status(&task_status).await {
                tracing::error!("🚨 保存任务状态失败: {}", e);
            } else {
                tracing::info!("💾 任务状态已保存: {} ({:?})", 
                    task_status.title.as_deref().unwrap_or("Unknown"), 
                    task_status.state);
            }
        });
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new_sync().expect("Failed to create default AppState")
    }
}
