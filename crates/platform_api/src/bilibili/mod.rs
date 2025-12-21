//! Bilibili API 模块
//!
//! 提供 Bilibili Web API 的接口封装（视频详情、播放地址、UP 主投稿列表等）。

mod api;
mod endpoints;
mod wbi;
mod wrid;

pub use api::BilibiliApi;
pub use endpoints::BilibiliEndpoints;
