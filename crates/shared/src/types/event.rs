//! 应用内跨模块事件与通知类型。

use crate::types::config::{AppConfig, Theme};
use crate::types::task::TaskUpdate;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 应用事件。
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// 任务更新。
    TaskUpdate(TaskUpdate),
    /// 配置变更。
    ConfigChanged(AppConfig),
    /// 主题变更。
    ThemeChanged(Theme),
    /// 工具更新事件。
    ToolUpdate(ToolUpdateEvent),
    /// UI 通知。
    ShowNotification(Notification),
}

/// 工具更新事件。
#[derive(Debug, Clone)]
pub struct ToolUpdateEvent {
    /// 工具类型。
    pub tool_type: ToolType,
    /// 旧版本。
    pub old_version: Option<String>,
    /// 新版本。
    pub new_version: String,
    /// 状态。
    pub status: UpdateStatus,
}

/// 工具类型。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ToolType {
    /// yt-dlp。
    YtDlp,
    /// ffmpeg。
    Ffmpeg,
}

/// 更新状态。
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateStatus {
    /// 检查中。
    Checking,
    /// 下载中。
    Downloading,
    /// 安装中。
    Installing,
    /// 已完成。
    Completed,
    /// 失败。
    Failed(String),
}

/// 通知。
#[derive(Debug, Clone)]
pub struct Notification {
    /// 标题。
    pub title: String,
    /// 内容。
    pub message: String,
    /// 通知类型。
    pub notification_type: NotificationType,
    /// 展示时长。
    pub duration: Option<Duration>,
}

/// 通知类型。
#[derive(Debug, Clone)]
pub enum NotificationType {
    /// 信息。
    Info,
    /// 成功。
    Success,
    /// 警告。
    Warning,
    /// 错误。
    Error,
}

