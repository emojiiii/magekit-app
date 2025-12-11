//! 共享的数据模型聚合模块，按领域拆分 download/task/config/platform/event。

pub mod config;
pub mod download;
pub mod event;
pub mod platform;
pub mod task;

pub use config::*;
pub use download::*;
pub use event::*;
pub use platform::*;
pub use task::*;
