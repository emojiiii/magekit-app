#![warn(missing_docs)]
#![warn(clippy::all)]

//! # MageKit Capture Library
//!
//! 通用的资源捕捉库，支持从网页中捕捉各种类型的资源（视频、音频、图片等）。
//!
//! ## 主要功能
//!
//! - 静态扫描：快速扫描 HTML 内容查找资源链接
//! - 浏览器 CDP：通过 Chrome DevTools Protocol 实时监听网络请求
//! - 资源筛选：支持按类型（视频/音频/图片/文档等）筛选资源
//! - 事件驱动：通过事件流实时推送发现的资源
//!
//! ## 使用示例
//!
//! ```rust,no_run
//! use magekit_capture::{CaptureRequest, ResourceFilter, start_capture};
//! use std::time::Duration;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let request = CaptureRequest {
//!     target_url: "https://example.com".to_string(),
//!     custom_browser_path: None,
//!     headless: true,
//!     timeout: Duration::from_secs(30),
//!     filter: ResourceFilter::all(), // 捕捉所有资源
//! };
//!
//! let mut session = start_capture(request).await?;
//!
//! while let Some(event) = session.rx.recv().await {
//!     match event {
//!         CaptureEvent::Found(resource) => {
//!             println!("发现资源: {} ({:?})", resource.url, resource.resource_type);
//!         }
//!         CaptureEvent::Log(msg) => {
//!             println!("日志: {}", msg);
//!         }
//!         _ => {}
//!     }
//! }
//! # Ok(())
//! # }
//! ```

pub mod ad_detector;
pub mod cdp;
pub mod core;
pub mod filter;
pub mod scanner;
pub mod types;

pub use ad_detector::*;
pub use core::*;
pub use filter::*;
pub use types::*;
