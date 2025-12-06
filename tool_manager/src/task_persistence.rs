//! 任务状态持久化模块
//!
//! 负责将任务状态保存到磁盘，以便应用重启后恢复未完成的任务。

use crate::error::{ToolManagerError, ToolManagerResult};
use magekit_shared::{TaskStatus, TaskState, TaskId, get_app_data_dir};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 持久化任务数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedTask {
    pub status: TaskStatus,
    pub retry_count: u32,
    pub max_retries: u32,
}

/// 持久化的任务列表
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedTasks {
    pub tasks: HashMap<TaskId, PersistedTask>,
    pub version: u32,
}

impl PersistedTasks {
    /// 获取任务文件路径
    fn get_tasks_file_path() -> ToolManagerResult<PathBuf> {
        let data_dir = get_app_data_dir()
            .map_err(|e| ToolManagerError::config(format!("Failed to get app data dir: {}", e)))?;
        Ok(data_dir.join("tasks.json"))
    }

    /// 从磁盘加载任务列表
    pub fn load() -> ToolManagerResult<Self> {
        let path = Self::get_tasks_file_path()?;
        
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .map_err(|e| ToolManagerError::config(
                format!("Failed to read tasks file {}: {}", path.display(), e),
            ))?;

        let tasks: PersistedTasks = serde_json::from_str(&content)
            .map_err(|e| ToolManagerError::config(
                format!("Failed to parse tasks file {}: {}", path.display(), e),
            ))?;

        Ok(tasks)
    }

    /// 保存任务列表到磁盘
    pub fn save(&self) -> ToolManagerResult<()> {
        let path = Self::get_tasks_file_path()?;
        
        // 确保目录存在
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ToolManagerError::config(
                    format!("Failed to create directory {}: {}", parent.display(), e),
                ))?;
        }

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| ToolManagerError::config(
                format!("Failed to serialize tasks: {}", e),
            ))?;

        std::fs::write(&path, content)
            .map_err(|e| ToolManagerError::config(
                format!("Failed to write tasks file {}: {}", path.display(), e),
            ))?;

        tracing::debug!("Tasks saved to: {:?}", path);
        Ok(())
    }

    /// 添加或更新任务
    pub fn upsert_task(&mut self, task: PersistedTask) {
        self.tasks.insert(task.status.id, task);
    }

    /// 移除任务
    pub fn remove_task(&mut self, task_id: TaskId) {
        self.tasks.remove(&task_id);
    }

    /// 获取可恢复的任务（未完成的任务）
    pub fn get_resumable_tasks(&self) -> Vec<&PersistedTask> {
        self.tasks
            .values()
            .filter(|task| {
                matches!(
                    task.status.state,
                    TaskState::Queued | TaskState::Downloading | TaskState::Paused
                )
            })
            .collect()
    }

    /// 清除已完成的任务
    pub fn clear_completed_tasks(&mut self) {
        self.tasks.retain(|_, task| {
            !matches!(
                task.status.state,
                TaskState::Completed | TaskState::Cancelled
            )
        });
    }

    /// 清除所有任务
    pub fn clear_all(&mut self) {
        self.tasks.clear();
    }
}

/// 任务持久化管理器
pub struct TaskPersistence {
    tasks: PersistedTasks,
    auto_save: bool,
}

impl TaskPersistence {
    /// 创建新的持久化管理器
    pub fn new() -> ToolManagerResult<Self> {
        let tasks = PersistedTasks::load().unwrap_or_default();
        Ok(Self {
            tasks,
            auto_save: true,
        })
    }

    /// 设置是否自动保存
    pub fn set_auto_save(&mut self, auto_save: bool) {
        self.auto_save = auto_save;
    }

    /// 添加任务
    pub fn add_task(&mut self, status: TaskStatus, max_retries: u32) -> ToolManagerResult<()> {
        let task = PersistedTask {
            status,
            retry_count: 0,
            max_retries,
        };
        self.tasks.upsert_task(task);
        
        if self.auto_save {
            self.tasks.save()?;
        }
        Ok(())
    }

    /// 更新任务状态（如果不存在则添加）
    pub fn update_task_status(&mut self, task_id: TaskId, status: TaskStatus) -> ToolManagerResult<()> {
        if let Some(task) = self.tasks.tasks.get_mut(&task_id) {
            // 任务已存在，更新状态
            task.status = status;
        } else {
            // 任务不存在，添加新任务
            let persisted_task = PersistedTask {
                status,
                retry_count: 0,
                max_retries: 3,  // 默认最大重试次数
            };
            self.tasks.upsert_task(persisted_task);
        }
        
        if self.auto_save {
            self.tasks.save()?;
        }
        Ok(())
    }

    /// 增加重试次数
    pub fn increment_retry(&mut self, task_id: TaskId) -> ToolManagerResult<Option<u32>> {
        let retry_count = if let Some(task) = self.tasks.tasks.get_mut(&task_id) {
            task.retry_count += 1;
            Some(task.retry_count)
        } else {
            None
        };
        
        if retry_count.is_some() && self.auto_save {
            self.tasks.save()?;
        }
        
        Ok(retry_count)
    }

    /// 检查是否可以重试
    pub fn can_retry(&self, task_id: TaskId) -> bool {
        self.tasks
            .tasks
            .get(&task_id)
            .map(|task| task.retry_count < task.max_retries)
            .unwrap_or(false)
    }

    /// 获取重试次数
    pub fn get_retry_count(&self, task_id: TaskId) -> Option<u32> {
        self.tasks.tasks.get(&task_id).map(|task| task.retry_count)
    }

    /// 移除任务
    pub fn remove_task(&mut self, task_id: TaskId) -> ToolManagerResult<()> {
        self.tasks.remove_task(task_id);
        
        if self.auto_save {
            self.tasks.save()?;
        }
        Ok(())
    }

    /// 获取所有持久化的任务
    pub fn get_all_tasks(&self) -> Vec<&PersistedTask> {
        self.tasks.tasks.values().collect()
    }

    /// 获取可恢复的任务
    pub fn get_resumable_tasks(&self) -> Vec<&PersistedTask> {
        self.tasks.get_resumable_tasks()
    }

    /// 清除已完成的任务
    pub fn clear_completed(&mut self) -> ToolManagerResult<()> {
        self.tasks.clear_completed_tasks();
        
        if self.auto_save {
            self.tasks.save()?;
        }
        Ok(())
    }

    /// 强制保存
    pub fn save(&self) -> ToolManagerResult<()> {
        self.tasks.save()
    }
}

impl Default for TaskPersistence {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            tasks: PersistedTasks::default(),
            auto_save: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persisted_tasks_default() {
        let tasks = PersistedTasks::default();
        assert!(tasks.tasks.is_empty());
    }
}
