/// 流处理相关的工具函数

use crate::{
    error::RecorderResult,
    types::{StreamData, VideoQuality},
};
use url::Url;

/// 流URL解析器
pub struct StreamParser;

impl StreamParser {
    /// 从流数据中获取最佳录制URL
    pub fn get_best_record_url(stream: &StreamData) -> RecorderResult<String> {
        // 优先使用 HLS (m3u8) 格式，因为它更适合录制
        if let Some(hls_url) = &stream.url.hls_url {
            return Ok(hls_url.clone());
        }

        // 如果没有HLS，使用FLV
        if let Some(flv_url) = &stream.url.flv_url {
            return Ok(flv_url.clone());
        }

        // 如果没有DASH，使用DASH
        if let Some(dash_url) = &stream.url.dash_url {
            return Ok(dash_url.clone());
        }

        Err(crate::error::RecorderError::StreamNotAvailable(
            "没有可用的流URL".to_string(),
        ))
    }

    /// 检查流URL是否有效
    pub async fn validate_stream_url(url: &str) -> RecorderResult<bool> {
        let client = reqwest::Client::new();
        let response = client.head(url).send().await?;

        Ok(response.status().is_success())
    }

    /// 从URL中提取CDN信息
    pub fn extract_cdn_info(url: &str) -> Option<String> {
        if let Ok(parsed_url) = Url::parse(url) {
            return Some(parsed_url.host_str()?.to_string());
        }
        None
    }

    /// 估算流的码率（基于URL中的信息）
    pub fn estimate_bitrate(_url: &str, quality: &VideoQuality) -> Option<u64> {
        // 这里可以根据URL中的信息或质量来估算码率
        match quality {
            VideoQuality::Original => Some(8000000),  // 8 Mbps
            VideoQuality::Blue => Some(8000000),      // 8 Mbps
            VideoQuality::Ultra => Some(6000000),     // 6 Mbps
            VideoQuality::High => Some(4000000),      // 4 Mbps
            VideoQuality::Standard => Some(2000000),  // 2 Mbps
            VideoQuality::Low => Some(1000000),       // 1 Mbps
        }
    }
}