//! 多平台 Web API 接口库
//!
//! 本库提供多平台的 Web API 接口实现（当前包含 Douyin/TikTok/Bilibili）。
//!
//! # 模块结构
//!
//! - `sign` - 签名算法模块 (X-Bogus, A-Bogus, SM3, RC4)
//! - `client` - HTTP 客户端封装
//! - `douyin` - 抖音 API 接口
//! - `bilibili` - Bilibili API 接口
//! - `error` - 错误类型定义

pub mod client;
pub mod bilibili;
pub mod douyin;
pub mod error;
pub mod sign;

// 重新导出常用类型
pub use client::BdClient;
pub use bilibili::BilibiliApi;
pub use douyin::DouyinApi;
pub use error::{BdError, BdResult};

// 重新导出签名函数
pub use sign::{ab_sign, xbogus_sign};

// 为后续“多平台”语义提供更通用的别名（不破坏现有代码）
pub use client::BdClient as PlatformClient;
pub use error::{BdError as PlatformError, BdResult as PlatformResult};
