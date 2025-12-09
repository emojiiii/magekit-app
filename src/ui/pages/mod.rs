//! 页面组件模块
//!
//! 包含所有有状态的页面组件

pub mod channel;
pub mod capture;
pub mod home;
pub mod record;
pub mod settings;
pub mod tasks;
pub mod tools;

pub use channel::ChannelPage;
pub use capture::CapturePage;
pub use home::HomePage;
pub use record::RecordingPage;
pub use settings::SettingsPage;
pub use tasks::TasksPage;
pub use tools::ToolsPage;
