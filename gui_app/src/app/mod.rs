//! 应用程序状态管理
//!
//! 负责管理整个应用的状态，包括工具管理器实例、任务列表、配置等。
//!
//! 模块结构：
//! - `types` - 类型定义
//! - `utils` - 工具函数
//! - `state` - 核心状态结构
//! - `tools` - 工具管理功能
//! - `download` - 下载功能
//! - `tasks` - 任务管理功能

mod download;
mod state;
mod tasks;
mod tools;
mod types;
mod utils;

// 重新导出公共类型
pub use state::{AppState, GlobalAppState};
pub use types::{DownloadVideoOptions, ToolStatus};

// 内部使用的类型（如果外部需要可以添加到上面）
#[allow(unused_imports)]
pub(crate) use types::{AppEvent, NotificationMessage, NotificationType};
