//! Bilibili 直播平台处理器

use async_trait::async_trait;
use reqwest::Client;
use serde_json;
use std::collections::HashMap;
use std::time::Duration;

use crate::{
    error::{RecorderError, RecorderResult},
    platforms::PlatformHandler,
    types::{LiveRoomInfo, LiveStatus, StreamData, StreamInfo, StreamUrl, VideoQuality},
};

/// Bilibili 直播处理器
pub struct BilibiliHandler {
    client: Client,
}

impl BilibiliHandler {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:127.0) Gecko/20100101 Firefox/127.0",
            )
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// 获取房间初始化信息
    async fn get_room_init(&self, room_id: &str) -> RecorderResult<(u64, bool)> {
        let url = format!(
            "https://api.live.bilibili.com/room/v1/Room/room_init?id={}",
            room_id
        );

        let response = self
            .client
            .get(&url)
            .header("Accept-Language", "zh-CN,zh;q=0.8")
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        if json["code"].as_i64() != Some(0) {
            return Err(RecorderError::RoomNotFound(room_id.to_string()));
        }

        let uid = json["data"]["uid"]
            .as_u64()
            .ok_or_else(|| RecorderError::InvalidResponseFormat("Missing uid".to_string()))?;
        let live_status = json["data"]["live_status"].as_i64() == Some(1);

        Ok((uid, live_status))
    }

    /// 获取主播信息
    async fn get_anchor_info(&self, uid: u64) -> RecorderResult<String> {
        let url = format!(
            "https://api.live.bilibili.com/live_user/v1/Master/info?uid={}",
            uid
        );

        let response = self
            .client
            .get(&url)
            .header("Accept-Language", "zh-CN,zh;q=0.8")
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        let anchor_name = json["data"]["info"]["uname"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string();

        Ok(anchor_name)
    }

    /// 获取房间标题（H5 接口）
    async fn get_room_title(&self, room_id: &str) -> RecorderResult<String> {
        let url = format!(
            "https://api.live.bilibili.com/xlive/web-room/v1/index/getH5InfoByRoom?room_id={}",
            room_id
        );

        let response = self
            .client
            .get(&url)
            .header("Accept-Language", "zh-CN,zh;q=0.8")
            .header("Origin", "https://live.bilibili.com")
            .header("Referer", format!("https://live.bilibili.com/{}", room_id))
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        let title = json["data"]["room_info"]["title"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(title)
    }

    /// 获取直播流 URL
    async fn get_stream_urls(&self, room_id: &str, qn: &str) -> RecorderResult<Vec<StreamData>> {
        // 首先尝试简单接口
        let params = format!("cid={}&qn={}&platform=web", room_id, qn);
        let url = format!(
            "https://api.live.bilibili.com/room/v1/Room/playUrl?{}",
            params
        );

        let response = self
            .client
            .get(&url)
            .header("Accept-Language", "zh-CN,zh;q=0.8")
            .header("Origin", "https://live.bilibili.com")
            .header("Referer", format!("https://live.bilibili.com/{}", room_id))
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        if json["code"].as_i64() == Some(0) {
            // 简单接口成功
            if let Some(durl) = json["data"]["durl"].as_array() {
                let mut streams = Vec::new();
                for item in durl {
                    if let Some(url) = item["url"].as_str() {
                        streams.push(StreamData {
                            quality: VideoQuality::Original,
                            url: StreamUrl {
                                flv_url: Some(url.to_string()),
                                hls_url: None,
                                dash_url: None,
                            },
                            bitrate: None,
                            resolution: None,
                            codec: None,
                            cdn: None,
                        });
                    }
                }
                if !streams.is_empty() {
                    return Ok(streams);
                }
            }
        }

        // 使用高级接口
        let params = format!(
            "room_id={}&protocol=0,1&format=0,1,2&codec=0,1,2&qn={}&platform=web&ptype=8&dolby=5&panorama=1&hdr_type=0,1",
            room_id, qn
        );
        let url = format!(
            "https://api.live.bilibili.com/xlive/web-room/v2/index/getRoomPlayInfo?{}",
            params
        );

        let response = self
            .client
            .get(&url)
            .header("Accept-Language", "zh-CN,zh;q=0.8")
            .header("Origin", "https://live.bilibili.com")
            .header("Referer", format!("https://live.bilibili.com/{}", room_id))
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        if json["data"]["live_status"].as_i64() != Some(1) {
            return Ok(vec![]);
        }

        let mut streams = Vec::new();

        if let Some(playurl_info) = json["data"]["playurl_info"].as_object() {
            if let Some(playurl) = playurl_info.get("playurl") {
                if let Some(stream_arr) = playurl["stream"].as_array() {
                    if let Some(first_stream) = stream_arr.first() {
                        if let Some(format_arr) = first_stream["format"].as_array() {
                            if let Some(first_format) = format_arr.first() {
                                if let Some(codec_arr) = first_format["codec"].as_array() {
                                    // qn 映射: 30000=杜比 20000=4K 10000=原画 400=蓝光 250=超清 150=高清 80=流畅
                                    let quality_map = [
                                        (30000, VideoQuality::Original),
                                        (20000, VideoQuality::Ultra),
                                        (10000, VideoQuality::Original),
                                        (400, VideoQuality::High),
                                        (250, VideoQuality::Standard),
                                        (150, VideoQuality::Standard),
                                        (80, VideoQuality::Low),
                                    ];

                                    for codec_item in codec_arr {
                                        let current_qn =
                                            codec_item["current_qn"].as_i64().unwrap_or(0);
                                        let base_url =
                                            codec_item["base_url"].as_str().unwrap_or("");

                                        if let Some(url_info) = codec_item["url_info"].as_array() {
                                            if let Some(first_url) = url_info.first() {
                                                let host = first_url["host"].as_str().unwrap_or("");
                                                let extra =
                                                    first_url["extra"].as_str().unwrap_or("");
                                                let full_url =
                                                    format!("{}{}{}", host, base_url, extra);

                                                let quality = quality_map
                                                    .iter()
                                                    .find(|(qn, _)| *qn == current_qn as i32)
                                                    .map(|(_, q)| q.clone())
                                                    .unwrap_or(VideoQuality::Standard);

                                                streams.push(StreamData {
                                                    quality,
                                                    url: StreamUrl {
                                                        hls_url: if full_url.contains(".m3u8") {
                                                            Some(full_url.clone())
                                                        } else {
                                                            None
                                                        },
                                                        flv_url: if full_url.contains(".flv") {
                                                            Some(full_url.clone())
                                                        } else {
                                                            None
                                                        },
                                                        dash_url: None,
                                                    },
                                                    bitrate: None,
                                                    resolution: None,
                                                    codec: codec_item["codec_name"]
                                                        .as_str()
                                                        .map(|s| s.to_string()),
                                                    cdn: None,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(streams)
    }
}

impl Default for BilibiliHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformHandler for BilibiliHandler {
    fn platform_name(&self) -> &'static str {
        "bilibili"
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["live.bilibili.com", "b23.tv"]
    }

    async fn extract_room_id(&self, url: &str) -> RecorderResult<String> {
        // 从 URL 提取房间号
        // 支持格式: https://live.bilibili.com/12345?xxx
        let url_without_query = url.split('?').next().unwrap_or(url);
        let room_id = url_without_query
            .rsplit('/')
            .next()
            .ok_or_else(|| RecorderError::InvalidUrlFormat("Cannot extract room ID".to_string()))?;

        if room_id.is_empty() || !room_id.chars().all(|c| c.is_ascii_digit()) {
            return Err(RecorderError::InvalidUrlFormat(
                "Invalid Bilibili room ID".to_string(),
            ));
        }

        Ok(room_id.to_string())
    }

    async fn get_stream_info(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        tracing::info!("🎮 正在获取 Bilibili 直播间信息: {}", room_id);

        // 获取房间初始化信息
        let (uid, live_status) = self.get_room_init(room_id).await?;
        tracing::debug!("📋 房间 UID: {}, 直播状态: {}", uid, live_status);

        // 获取主播名称
        let anchor_name = self.get_anchor_info(uid).await?;
        tracing::debug!("👤 主播名称: {}", anchor_name);

        // 获取房间标题
        let title = self.get_room_title(room_id).await.unwrap_or_default();
        tracing::debug!("📝 房间标题: {}", title);

        let status = if live_status {
            LiveStatus::Live
        } else {
            LiveStatus::Offline
        };

        let room_info = LiveRoomInfo {
            room_id: room_id.to_string(),
            anchor_name,
            title,
            status: status.clone(),
            start_time: None,
            viewer_count: None,
            cover_url: None,
            extra: HashMap::new(),
        };

        // 如果不在直播，返回空流列表
        if status != LiveStatus::Live {
            tracing::info!("📴 主播未开播");
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        // 获取直播流
        let streams = self.get_stream_urls(room_id, "10000").await?;
        tracing::info!("✅ 获取到 {} 个直播流", streams.len());

        Ok(StreamInfo {
            room: room_info,
            streams,
        })
    }
}
