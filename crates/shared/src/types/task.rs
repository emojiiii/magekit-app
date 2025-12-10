//! 任务与任务状态相关的数据模型。

use crate::types::download::DownloadParams;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// 任务唯一标识符。
pub type TaskId = Uuid;

/// 任务状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskState {
    /// 等待中。
    Queued,
    /// 下载中。
    Downloading,
    /// 已暂停。
    Paused,
    /// 已完成。
    Completed,
    /// 失败并附带错误信息。
    Failed(String),
    /// 已取消。
    Cancelled,
}

/// 任务状态信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatus {
    /// 任务 ID。
    pub id: TaskId,
    /// 原始 URL。
    pub url: String,
    /// 标题（可选）。
    pub title: Option<String>,
    /// 当前状态。
    pub state: TaskState,
    /// 进度（0.0~1.0）。
    pub progress: f32,
    /// 已下载字节数。
    pub downloaded_bytes: u64,
    /// 总字节数。
    pub total_bytes: Option<u64>,
    /// 当前速度（B/s）。
    pub speed: Option<u64>,
    /// 预计剩余时间。
    pub eta: Option<Duration>,
    /// 创建时间。
    pub created_at: std::time::SystemTime,
    /// 开始时间。
    pub started_at: Option<std::time::SystemTime>,
    /// 完成时间。
    pub completed_at: Option<std::time::SystemTime>,
    /// 输出路径。
    pub output_path: Option<PathBuf>,
    /// 下载参数（用于暂停后恢复）。
    pub download_params: Option<DownloadParams>,
}

impl TaskStatus {
    /// 创建一个处于队列中的任务。
    pub fn new(id: TaskId, url: String, title: Option<String>) -> Self {
        Self {
            id,
            url,
            title,
            state: TaskState::Queued,
            progress: 0.0,
            downloaded_bytes: 0,
            total_bytes: None,
            speed: None,
            eta: None,
            created_at: std::time::SystemTime::now(),
            started_at: None,
            completed_at: None,
            output_path: None,
            download_params: None,
        }
    }

    /// 是否为活跃状态（队列/下载中/暂停）。
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            TaskState::Queued | TaskState::Downloading | TaskState::Paused
        )
    }

    /// 是否已结束（完成/失败/取消）。
    pub fn is_finished(&self) -> bool {
        matches!(
            self.state,
            TaskState::Completed | TaskState::Failed(_) | TaskState::Cancelled
        )
    }
}

/// 任务更新事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskUpdate {
    /// 新任务创建。
    Created(TaskStatus),
    /// 进度更新。
    Progress(TaskId, f32, u64, Option<u64>, Option<u64>, Option<Duration>),
    /// 状态切换。
    StateChanged(TaskId, TaskState),
    /// 速度更新。
    SpeedUpdate(TaskId, u64),
    /// 成功完成。
    Completed(TaskId, PathBuf),
    /// 失败。
    Failed(TaskId, String),
}

