//! 应用程序类型定义

use magekit_shared::TaskUpdate;

/// 应用程序事件
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// 任务更新事件
    TaskUpdate(TaskUpdate),
    /// 配置变更事件
    ConfigChanged(magekit_shared::AppConfig),
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

/// 视频下载选项
#[derive(Debug, Clone, Default)]
pub struct DownloadVideoOptions {
    pub embed_metadata: bool,
    pub embed_thumbnail: bool,
    pub download_subtitles: bool,
    pub audio_only: bool,
}
