//! UI组件模块
//!
//! 包含所有用户界面组件的定义和实现。

pub mod main_window;
pub mod task_list;
pub mod settings;
pub mod download_panel;
pub mod tool_panel;
pub mod format_selector;
pub mod playlist_panel;
pub mod batch_panel;
pub mod notification;
pub mod layout;
pub mod pages;
pub mod widgets;

// 重新导出主要组件
pub use main_window::MainWindow;
pub use task_list::{TaskListView, TaskListData, TaskAction, TaskStats};
pub use settings::{SettingsView, SettingsTab, SettingsChange};
pub use download_panel::{DownloadPanel, DownloadPanelState, DownloadPanelEvent, QualityPreset};
pub use tool_panel::{ToolPanel, ToolInfo, ToolState, ToolPanelEvent};
pub use format_selector::{FormatSelector, FormatType, VideoResolution, AudioQuality, AudioFormat, SubtitleOptions, FormatSelectorEvent};
pub use playlist_panel::{PlaylistPanel, PlaylistItem, PlaylistInfo, PlaylistPanelState, PlaylistPanelEvent};
pub use batch_panel::{BatchPanel, BatchItem, BatchItemStatus, DownloadTemplate, Category, BatchPanelEvent};
pub use notification::{Notification, NotificationType, NotificationContainer, NotificationEvent, ProgressOverlay, ConfirmDialog, ErrorPanel};
pub use layout::{AppLayout, home_page, tasks_page, tools_page, settings_page, not_found_page};
pub use pages::{HomePage, ToolsPage, SettingsPage};