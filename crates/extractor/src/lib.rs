//! 媒体解析器
//!
//! 将平台解析逻辑与 UI 解耦，便于后续扩展更多平台。

pub mod cookies;
mod douyin;
pub mod error;
mod platform;
mod tiktok;
mod ytdlp;

use error::ExtractError;
use magekit_shared::{ChannelInfo, ChannelPageResult, PlatformCookie, VideoInfo};
use platform::{Platform, PlatformSupport};
use std::path::PathBuf;

/// 统一的媒体解析入口
#[derive(Clone)]
pub struct MediaExtractor {
    yt_dlp_path: PathBuf,
}

impl MediaExtractor {
    pub fn new(yt_dlp_path: PathBuf) -> Self {
        Self { yt_dlp_path }
    }

    /// 支持的平台列表
    pub fn supported_platforms(&self) -> Vec<Platform> {
        Platform::all_supported()
    }

    /// 根据 URL 自动选择解析器获取视频信息
    pub async fn get_video_info(
        &self,
        url: &str,
        cookies: Option<&[PlatformCookie]>,
    ) -> Result<VideoInfo, ExtractError> {
        let platform = Platform::detect(url);
        // 优先自研解析（抖音/ TikTok），其余统一走 yt-dlp；yt-dlp 不支持时返回错误
        if matches!(platform, Platform::Douyin) {
            return douyin::extract_video_info(url, cookies).await;
        }
        if matches!(platform, Platform::Tiktok) {
            return tiktok::extract_video_info(url, cookies).await;
        }

        // 其他平台默认交给 yt-dlp 处理，失败则视为不支持
        ytdlp::extract_video_info(url, cookies, &self.yt_dlp_path).await
    }

    /// 获取频道/播放列表信息
    ///
    /// 支持抖音用户主页和其他平台（通过 yt-dlp）
    pub async fn get_channel_info(
        &self,
        url: &str,
        cookies: Option<&[PlatformCookie]>,
    ) -> Result<ChannelInfo, ExtractError> {
        let platform = Platform::detect(url);

        // 抖音用户主页使用自研解析
        if matches!(platform, Platform::Douyin) && douyin::is_douyin_user_url(url) {
            tracing::info!("🚀 使用 extractor 解析抖音用户主页");
            return douyin::extract_channel_info(url, cookies).await;
        }

        // 其他平台使用 yt-dlp
        ytdlp::extract_channel_info(url, cookies, &self.yt_dlp_path).await
    }

    /// 分页获取频道/作者作品列表。
    ///
    /// - 抖音用户主页：走自研分页接口（按 `cursor` 增量拉取）。
    /// - 其他平台：当前仍一次性解析完整列表（`has_more=false`），调用方可在 UI 侧做本地分页。
    pub async fn get_channel_page(
        &self,
        url: &str,
        cursor: Option<i64>,
        count: usize,
        cookies: Option<&[PlatformCookie]>,
    ) -> Result<ChannelPageResult, ExtractError> {
        let platform = Platform::detect(url);

        if matches!(platform, Platform::Douyin) && douyin::is_douyin_user_url(url) {
            return douyin::extract_channel_page(url, cookies, cursor.unwrap_or(0), count as i64)
                .await;
        }

        ytdlp::extract_channel_page(url, cursor, count, cookies, &self.yt_dlp_path).await
    }
}
