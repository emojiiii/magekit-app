//! YY 直播平台处理器
//!
//! 参考 `py_demo/src/spider.py` 与 `py_demo/src/stream.py` 的实现。

use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use serde_json;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{
    error::{RecorderError, RecorderResult},
    platforms::PlatformHandler,
    types::{LiveRoomInfo, LiveStatus, StreamData, StreamInfo, StreamUrl, VideoQuality},
};

/// YY 直播处理器
pub struct YyHandler {
    client: Client,
}

impl YyHandler {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36",
            )
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    fn now_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_millis() as u64
    }

    async fn fetch_room_html(&self, room_id: &str) -> RecorderResult<String> {
        let url = format!("https://www.yy.com/{}", room_id);
        let response = self
            .client
            .get(&url)
            .header("Accept-Language", "zh-CN,zh;q=0.8")
            // py_demo 中的默认 Cookie（用于避免部分页面拦截），这里不强依赖，但带上更稳
            .header("Referer", "https://www.yy.com/")
            .header(
                "Cookie",
                "hd_newui=0.2103068903976506; hdjs_session_id=0.4929014850884579; hdjs_session_time=1694004002636; hiido_ui=0.923076230899782",
            )
            .send()
            .await?;

        Ok(response.text().await?)
    }

    fn parse_anchor_and_cid_from_html(
        &self,
        html: &str,
    ) -> RecorderResult<(String, String, Option<String>)> {
        let anchor_re = Regex::new(r#"nick:\s*"([^"]+)",\s*\n\s+logo"#)
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;
        let cid_re = Regex::new(r#"sid\s*:\s*"([^"]+)",\s*\n\s+ssid"#)
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;

        let anchor_name = anchor_re
            .captures(html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let cid = cid_re
            .captures(html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| {
                RecorderError::InvalidResponseFormat("Missing sid/cid in page".to_string())
            })?;

        let logo_re = Regex::new(r#"logo:\s*"([^"]+)""#)
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;
        let cover_url = logo_re
            .captures(html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string());

        Ok((anchor_name, cid, cover_url))
    }

    fn parse_room_title_detail(json: &serde_json::Value) -> String {
        json["data"]["roomName"].as_str().unwrap_or("").to_string()
    }

    fn parse_stream_flv_url(json: &serde_json::Value) -> Option<String> {
        let stream_line_addr = json["avp_info_res"]["stream_line_addr"].as_object()?;
        let cdn_info = stream_line_addr.values().next()?;
        cdn_info
            .get("cdn_info")?
            .get("url")?
            .as_str()
            .map(|s| s.to_string())
    }

    async fn fetch_room_title(&self, cid: &str) -> RecorderResult<String> {
        let ts = Self::now_millis();
        let params = format!("uid=&sid={}&ssid={}&_={}", cid, cid, ts);
        let url = format!("https://www.yy.com/live/detail?{}", params);
        let response = self
            .client
            .get(&url)
            .header("Accept-Language", "zh-CN,zh;q=0.8")
            .header("Referer", "https://www.yy.com/")
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;
        Ok(Self::parse_room_title_detail(&json))
    }

    async fn fetch_stream_flv_url(&self, cid: &str) -> RecorderResult<Option<String>> {
        // 基本沿用 py_demo 的字段结构；部分字段是“看起来很固定”的版本号/类型，保持常量即可。
        let seq_ms = Self::now_millis();
        let send_time = seq_ms / 1000;

        let body = serde_json::json!({
            "head": {
                "seq": seq_ms,
                "appidstr": "0",
                "bidstr": "121",
                "cidstr": cid,
                "sidstr": cid,
                "uid64": 0,
                "client_type": 108,
                "client_ver": "5.17.0",
                "stream_sys_ver": 1,
                "app": "yylive_web",
                "playersdk_ver": "5.17.0",
                "thundersdk_ver": "0",
                "streamsdk_ver": "5.17.0"
            },
            "client_attribute": {
                "client": "web",
                "model": "web0",
                "cpu": "",
                "graphics_card": "",
                "os": "chrome",
                "osversion": "0",
                "vsdk_version": "",
                "app_identify": "",
                "app_version": "",
                "business": "",
                "width": "1920",
                "height": "1080",
                "scale": "",
                "client_type": 8,
                "h265": 0
            },
            "avp_parameter": {
                "version": 1,
                "client_type": 8,
                "service_type": 0,
                "imsi": 0,
                "send_time": send_time,
                "line_seq": -1,
                "gear": 4,
                "ssl": 1,
                "stream_format": 0
            }
        });

        let params = format!(
            "uid=0&cid={}&sid={}&appid=0&sequence={}&encode=json",
            cid, cid, seq_ms
        );
        let url = format!(
            "https://stream-manager.yy.com/v3/channel/streams?{}",
            params
        );

        let response = self
            .client
            .post(&url)
            .header("Accept-Language", "zh-CN,zh;q=0.8")
            .header("Referer", "https://www.yy.com/")
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;
        Ok(Self::parse_stream_flv_url(&json))
    }
}

impl Default for YyHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_anchor_and_cid_from_html() {
        let handler = YyHandler::new();
        let html = include_str!("fixtures/yy_room_snippet.html");
        let (anchor, cid, cover) = handler.parse_anchor_and_cid_from_html(html).unwrap();
        assert_eq!(anchor, "YY主播");
        assert_eq!(cid, "123456");
        assert_eq!(
            cover.as_deref(),
            Some("https://yyimg.example.com/avatar.jpg")
        );
    }

    #[test]
    fn test_parse_room_title_detail() {
        let json: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/yy_detail.json")).unwrap();
        assert_eq!(YyHandler::parse_room_title_detail(&json), "YY 房间标题");
    }

    #[test]
    fn test_parse_stream_flv_url() {
        let json: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/yy_stream.json")).unwrap();
        assert_eq!(
            YyHandler::parse_stream_flv_url(&json).as_deref(),
            Some("https://yycdn.example.com/live/test.flv")
        );
    }
}

#[async_trait]
impl PlatformHandler for YyHandler {
    fn platform_name(&self) -> &'static str {
        "yy"
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["yy.com", "www.yy.com"]
    }

    async fn extract_room_id(&self, url: &str) -> RecorderResult<String> {
        let url_without_query = url.split('?').next().unwrap_or(url);
        let room_id = url_without_query
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .ok_or_else(|| RecorderError::InvalidUrlFormat("Cannot extract room ID".to_string()))?;

        if room_id.is_empty() {
            return Err(RecorderError::InvalidUrlFormat(
                "Invalid YY room ID".to_string(),
            ));
        }

        Ok(room_id.to_string())
    }

    async fn get_stream_info(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        tracing::info!("🎙️ 正在获取 YY 直播间信息: {}", room_id);

        let html = self.fetch_room_html(room_id).await?;
        let (anchor_name_raw, cid, cover_url) = self.parse_anchor_and_cid_from_html(&html)?;
        let anchor_name = if anchor_name_raw.is_empty() {
            cid.clone()
        } else {
            format!("{}-{}", anchor_name_raw, cid)
        };

        let title = self.fetch_room_title(&cid).await.unwrap_or_default();
        let flv_url = self.fetch_stream_flv_url(&cid).await?;

        let is_live = flv_url.is_some();

        let room_info = LiveRoomInfo {
            room_id: cid.clone(),
            anchor_name,
            title,
            status: if is_live {
                LiveStatus::Live
            } else {
                LiveStatus::Offline
            },
            start_time: None,
            viewer_count: None,
            cover_url,
            extra: HashMap::new(),
        };

        if !is_live {
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        let streams = vec![StreamData {
            quality: VideoQuality::Original,
            url: StreamUrl {
                flv_url,
                hls_url: None,
                dash_url: None,
            },
            bitrate: None,
            resolution: None,
            codec: None,
            cdn: None,
        }];

        Ok(StreamInfo {
            room: room_info,
            streams,
        })
    }
}
