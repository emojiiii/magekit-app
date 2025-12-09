//! 任务管理功能
//!
//! 所有任务管理逻辑委托给 ToolManager
//! 本模块仅提供便捷的查询和批量操作方法

use anyhow::Result;
use magekit_shared::{TaskId, TaskState, TaskStatus};

use super::state::AppState;

impl AppState {
    /// 获取所有任务状态（从本地缓存读取，同步版本）
    pub fn get_all_tasks_sync(&self) -> Vec<TaskStatus> {
        self.tasks.blocking_read().values().cloned().collect()
    }

    /// 获取指定任务状态（从本地缓存读取，同步版本）
    pub fn get_task_status_sync(&self, task_id: TaskId) -> Option<TaskStatus> {
        self.tasks.blocking_read().get(&task_id).cloned()
    }

    /// 获取活跃任务数量
    pub fn active_task_count(&self) -> usize {
        self.tasks
            .blocking_read()
            .values()
            .filter(|t| t.is_active())
            .count()
    }

    /// 获取已完成任务数量
    pub fn completed_task_count(&self) -> usize {
        self.tasks
            .blocking_read()
            .values()
            .filter(|t| matches!(t.state, TaskState::Completed))
            .count()
    }

    /// 清空已完成的任务（同步包装）
    pub fn clear_completed_tasks_sync(&self) {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();
        let tasks = self.tasks.clone();

        runtime.spawn(async move {
            // 从 ToolManager 清理
            if let Err(e) = tool_manager.clear_completed_tasks().await {
                tracing::error!("❌ 清理已完成任务失败: {}", e);
                return;
            }
            
            // 从本地缓存清理
            {
                let mut tasks = tasks.write().await;
                tasks.retain(|_, task| {
                    !matches!(task.state, TaskState::Completed | TaskState::Cancelled)
                });
            }
            
            tracing::info!("✅ 已清理完成的任务");
        });
    }

    /// 批量暂停所有下载中的任务
    pub fn pause_all_downloads(&self) {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();

        runtime.spawn(async move {
            tool_manager.pause_all().await;
            tracing::info!("⏸️ 已暂停所有下载任务");
        });
    }

    /// 批量恢复所有暂停的任务
    pub fn resume_all_downloads(&self) {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();

        runtime.spawn(async move {
            tool_manager.resume_all().await;
            tracing::info!("▶️ 已恢复所有暂停的任务");
        });
    }

    /// 批量取消所有任务
    pub fn cancel_all_downloads(&self) {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();

        runtime.spawn(async move {
            tool_manager.cancel_all().await;
            tracing::info!("🛑 已取消所有下载任务");
        });
    }

    /// 重试失败的任务
    pub fn retry_task(&self, task_id: TaskId) -> Result<()> {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();

        runtime.spawn(async move {
            if let Err(e) = tool_manager.retry_task(task_id).await {
                tracing::error!("❌ 重试任务失败: {}", e);
            } else {
                tracing::info!("🔄 任务已加入重试队列: {}", task_id);
            }
        });

        Ok(())
    }
}
