//! 字节跳动平台 API 接口库
//!
//! 本库提供抖音 (Douyin) 和 TikTok 的 Web API 接口实现
//!
//! # 模块结构
//!
//! - `sign` - 签名算法模块 (X-Bogus, A-Bogus, SM3, RC4)
//! - `client` - HTTP 客户端封装
//! - `douyin` - 抖音 API 接口
//! - `error` - 错误类型定义

pub mod client;
pub mod douyin;
pub mod error;
pub mod sign;

// 重新导出常用类型
pub use client::BdClient;
pub use douyin::DouyinApi;
pub use error::{BdError, BdResult};

// 重新导出签名函数
pub use sign::{ab_sign, xbogus_sign};
