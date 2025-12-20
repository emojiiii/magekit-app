// #![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(dead_code)]

//! # MageKit Tool Manager
//!
//! 工具管理器负责自动下载、更新和管理外部工具（如yt-dlp和ffmpeg）。
//!
//! ## 主要功能
//!
//! - 自动下载和安装yt-dlp和ffmpeg
//! - 版本检查和自动更新
//! - 工具二进制文件管理
//! - 跨平台兼容性支持
//! - 视频下载任务管理
//! - 异步任务处理和进度监控
//!
//! ## 使用示例
//!
//! ```rust,no_run
//! use magekit_tool_manager::ToolManager;
//! use magekit_shared::DownloadOptions;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let tool_manager = ToolManager::new().await?;
//!
//!     // 确保工具已安装
//!     tool_manager.ensure_tools(magekit_shared::UpdateChannel::Stable).await?;
//!
//!     // 获取视频信息
//!     let info = tool_manager.get_video_info("https://www.youtube.com/watch?v=...").await?;
//!     println!("Video: {}", info.title);
//!
//!     // 开始下载
//!     let options = DownloadOptions::default();
//!     let task_id = tool_manager.start_download("https://www.youtube.com/watch?v=...", options).await?;
//!     println!("Download started with task ID: {}", task_id);
//!
//!     Ok(())
//! }
//! ```

/// 配置管理模块
pub mod config;
/// Download 库适配器
mod download_adapter;
/// 视频下载器模块
pub mod downloader;
/// 错误处理模块
pub mod error;
/// 下载历史记录模块
pub mod history;
/// 工具存储模块
pub mod storage;
/// 任务管理器模块
pub mod task_manager;
/// 任务持久化模块
pub mod task_persistence;
/// 任务队列模块
pub mod task_queue;
/// 工具更新模块
pub mod updater;

// 重新导出主要类型
pub use config::{ConfigManager, ToolManagerConfig};
pub use downloader::{DownloadProgress, VideoDownloader};
pub use error::{DownloadError, DownloadResult, ToolManagerError, ToolManagerResult};
pub use history::{HistoryEntry, HistoryManager, HistoryStats, HistoryStore};
pub use storage::ToolStorage;
pub use task_manager::{ToolManager, ToolManagerEvent};
pub use task_persistence::{PersistedTask, TaskPersistence};
pub use task_queue::{QueueStats, QueuedTask, TaskPriority, TaskQueue};
pub use updater::{ToolUpdate, UpdateInfo};
