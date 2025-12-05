#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(clippy::type_complexity)]

//! # MageKit Shared Library
//!
//! 这个库包含了 MageKit 应用程序中各个模块共享的数据类型、常量和工具函数。
//!
//! ## 主要模块
//!
//! - [`types`]: 共享的数据类型定义
//! - [`constants`]: 应用程序常量
//! - [`utils`]: 工具函数集合
//!
//! ## 使用示例
//!
//! ```rust
//! use magekit_shared::{types::*, utils::*};
//! use std::path::PathBuf;
//!
//! // 创建下载选项
//! let options = DownloadOptions::default();
//!
//! // 验证 URL
//! let url = validate_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
//!
//! // 格式化文件大小
//! let size_str = format_file_size(1024 * 1024); // "1.0 MB"
//! ```

pub mod constants;
pub mod types;
pub mod utils;

// 重新导出常用的类型和函数
pub use types::*;
pub use utils::*;