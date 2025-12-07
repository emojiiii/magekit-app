//! UI组件模块
//!
//! 包含所有用户界面组件的定义和实现。

pub mod batch_panel;
pub mod download_panel;
pub mod format_selector;
pub mod layout;
pub mod main_window;
pub mod notification;
pub mod pages;
pub mod playlist_panel;
pub mod settings;
pub mod task_list;
pub mod tool_panel;
pub mod widgets;

// 重新导出主要组件
pub use main_window::MainWindow;
