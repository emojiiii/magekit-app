//! BIGO Live 平台处理器
//!
//! 参考 `py_demo/src/spider.py#get_bigo_stream_url`。

use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use serde_json;
use std::collections::HashMap;
use std::time::Duration;
use url::Url;

use crate::{
    error::{RecorderError, RecorderResult},
    platforms::PlatformHandler,
    types::{LiveRoomInfo, LiveStatus, StreamData, StreamInfo, StreamUrl, VideoQuality},
};

/// BIGO 直播处理器
pub struct BigoHandler {
    client: Client,
}

impl BigoHandler {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/119.0",
            )
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    fn extract_room_id_from_url(&self, url: &str) -> RecorderResult<String> {
        // BIGO 常见格式：
        // - https://www.bigo.tv/<country>/<room_id>
        // - https://www.bigo.tv/<room_id>
        // - https://www.bigo.tv/?xxx&h=<room_id>
        let parsed =
            Url::parse(url).map_err(|_| RecorderError::InvalidUrlFormat(url.to_string()))?;
        if let Some(h) = parsed
            .query_pairs()
            .find(|(k, _)| k == "h")
            .map(|(_, v)| v.to_string())
        {
            if !h.is_empty() {
                return Ok(h);
            }
        }

        let path = parsed.path().trim_end_matches('/');
        let seg = path
            .rsplit('/')
            .next()
            .ok_or_else(|| RecorderError::InvalidUrlFormat("Cannot extract room ID".to_string()))?;

        if seg.is_empty() {
            return Err(RecorderError::InvalidUrlFormat(
                "Invalid BIGO room ID".to_string(),
            ));
        }

        Ok(seg.to_string())
    }

    async fn resolve_room_id_from_meta(&self, url: &str) -> RecorderResult<Option<String>> {
        // 兼容：输入不是 bigo.tv 的分享页时，尝试从 meta al:web:url 解析
        let html = self.client.get(url).send().await?.text().await?;
        let re = Regex::new(r#"property="al:web:url"\s+content="([^"]+)""#)
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;
        let meta_url = re
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().replace("&amp;", "&"));
        let Some(meta_url) = meta_url else {
            return Ok(None);
        };
        let room_id = meta_url.split("&h=").last().unwrap_or("").to_string();
        if room_id.is_empty() {
            Ok(None)
        } else {
            Ok(Some(room_id))
        }
    }

    async fn fetch_internal_studio_info(&self, room_id: &str) -> RecorderResult<serde_json::Value> {
        let api = "https://ta.bigo.tv/official_website/studio/getInternalStudioInfo";
        let response = self
            .client
            .post(api)
            .header("Accept-Language", "zh-CN,zh;q=0.8")
            .header("Referer", "https://www.bigo.tv/")
            .header(
                "Content-Type",
                "application/x-www-form-urlencoded; charset=UTF-8",
            )
            .form(&[("siteId", room_id)])
            .send()
            .await?;

        Ok(response.json().await?)
    }

    fn parse_internal_studio_info(
        json: &serde_json::Value,
    ) -> (String, bool, String, Option<String>, Option<String>) {
        let data = &json["data"];
        let anchor_name = data["nick_name"].as_str().unwrap_or("").to_string();
        let live_status = data["alive"].as_i64().unwrap_or(0) == 1;
        let title = data["roomTopic"].as_str().unwrap_or("").to_string();
        let m3u8_url = data["hls_src"].as_str().map(|s| s.to_string());
        let cover_url = data["cover"]
            .as_str()
            .or_else(|| data["coverUrl"].as_str())
            .or_else(|| data["portrait"].as_str())
            .map(|s| s.to_string());
        (anchor_name, live_status, title, m3u8_url, cover_url)
    }
}

impl Default for BigoHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_internal_studio_info() {
        let json: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/bigo_internal.json")).unwrap();
        let (anchor, is_live, title, m3u8, cover) = BigoHandler::parse_internal_studio_info(&json);
        assert_eq!(anchor, "BIGO主播");
        assert!(is_live);
        assert_eq!(title, "BIGO 直播标题");
        assert_eq!(
            m3u8.as_deref(),
            Some("https://bigo.example.com/live/master.m3u8")
        );
        assert_eq!(
            cover.as_deref(),
            Some("https://bigo.example.com/live/cover.jpg")
        );
    }
}

#[async_trait]
impl PlatformHandler for BigoHandler {
    fn platform_name(&self) -> &'static str {
        "bigo"
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["bigo.tv", "www.bigo.tv"]
    }

    async fn extract_room_id(&self, url: &str) -> RecorderResult<String> {
        // 允许传入非 bigo.tv 的分享页（但 PlatformFactory 仅靠 pattern 命中，因此这里只做兜底）
        if url.contains("bigo.tv") {
            return self.extract_room_id_from_url(url);
        }

        if let Some(room_id) = self.resolve_room_id_from_meta(url).await? {
            return Ok(room_id);
        }

        Err(RecorderError::InvalidUrlFormat(
            "Cannot extract BIGO room ID".to_string(),
        ))
    }

    async fn get_stream_info(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        tracing::info!("🎥 正在获取 BIGO 直播间信息: {}", room_id);

        let json = self.fetch_internal_studio_info(room_id).await?;
        let (anchor_name, live_status, title, m3u8_url, cover_url) =
            Self::parse_internal_studio_info(&json);

        let room_info = LiveRoomInfo {
            room_id: room_id.to_string(),
            anchor_name: if anchor_name.is_empty() {
                room_id.to_string()
            } else {
                anchor_name
            },
            title,
            status: if live_status {
                LiveStatus::Live
            } else {
                LiveStatus::Offline
            },
            start_time: None,
            viewer_count: None,
            cover_url,
            extra: HashMap::new(),
        };

        if !live_status {
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        let Some(m3u8_url) = m3u8_url else {
            return Err(RecorderError::StreamNotAvailable(
                "Missing BIGO hls_src".to_string(),
            ));
        };

        Ok(StreamInfo {
            room: room_info,
            streams: vec![StreamData {
                quality: VideoQuality::Original,
                url: StreamUrl {
                    flv_url: None,
                    hls_url: Some(m3u8_url),
                    dash_url: None,
                },
                bitrate: None,
                resolution: None,
                codec: None,
                cdn: None,
            }],
        })
    }
}
