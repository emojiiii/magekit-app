//! 任务管理功能
//!
//! 包含下载任务的创建、暂停、恢复、取消等功能

use anyhow::Result;
use magekit_shared::{DownloadOptions, TaskId, TaskStatus, TaskUpdate};

use super::state::AppState;
use super::types::{AppEvent, NotificationMessage, NotificationType};

impl AppState {
    /// 开始下载任务
    pub async fn start_download(
        &self,
        url: String,
        options: Option<DownloadOptions>,
    ) -> Result<TaskId> {
        let download_options = options.unwrap_or_default();

        let output_path = std::env::current_dir()
            .unwrap_or_default()
            .join("downloads");

        let _options_with_path = DownloadOptions {
            output_path,
            ..download_options
        };

        let task_id = uuid::Uuid::new_v4();

        tracing::info!("开始下载任务: {} - {}", task_id, url);

        let task_status = TaskStatus::new(task_id, url.clone(), Some("视频标题".to_string()));

        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(task_id, task_status);
        }

        let _ = self
            .event_tx
            .send(AppEvent::ShowNotification(NotificationMessage {
                title: "下载开始".to_string(),
                message: format!("开始下载: {}", url),
                notification_type: NotificationType::Info,
            }))
            .await;

        Ok(task_id)
    }

    /// 暂停下载任务
    pub async fn pause_download(&self, task_id: TaskId) -> Result<()> {
        self.tool_manager.pause_download(task_id).await?;
        let _ = self
            .event_tx
            .send(AppEvent::ShowNotification(NotificationMessage {
                title: "下载已暂停".to_string(),
                message: format!("任务 {} 已暂停", task_id),
                notification_type: NotificationType::Warning,
            }))
            .await;
        Ok(())
    }

    /// 恢复下载任务
    pub async fn resume_download(&self, task_id: TaskId) -> Result<()> {
        self.tool_manager.resume_download(task_id).await?;
        let _ = self
            .event_tx
            .send(AppEvent::ShowNotification(NotificationMessage {
                title: "下载已恢复".to_string(),
                message: format!("任务 {} 已恢复", task_id),
                notification_type: NotificationType::Info,
            }))
            .await;
        Ok(())
    }

    /// 取消下载任务
    pub async fn cancel_download(&self, task_id: TaskId) -> Result<()> {
        self.tool_manager.cancel_download(task_id).await?;

        let mut tasks = self.tasks.write().await;
        tasks.remove(&task_id);

        let _ = self
            .event_tx
            .send(AppEvent::ShowNotification(NotificationMessage {
                title: "下载已取消".to_string(),
                message: format!("任务 {} 已取消", task_id),
                notification_type: NotificationType::Warning,
            }))
            .await;
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
                    task.state = state;
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
                    task.state = magekit_shared::TaskState::Failed(error);
                    task.completed_at = Some(std::time::SystemTime::now());
                }
            }
        }
    }
}
