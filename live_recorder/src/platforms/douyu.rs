//! 斗鱼直播平台处理器

use async_trait::async_trait;
use reqwest::Client;
use serde_json;
use std::collections::HashMap;
use std::time::Duration;
use regex::Regex;
use md5::{Md5, Digest};

use crate::{
    error::{RecorderError, RecorderResult},
    platforms::PlatformHandler,
    types::{LiveRoomInfo, LiveStatus, StreamData, StreamInfo, StreamUrl, VideoQuality},
};

/// 斗鱼直播处理器
pub struct DouyuHandler {
    client: Client,
}

impl DouyuHandler {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:123.0) Gecko/20100101 Firefox/123.0")
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// 获取房间信息
    async fn get_room_info(&self, room_id: &str) -> RecorderResult<(LiveRoomInfo, bool)> {
        let url = format!("https://www.douyu.com/betard/{}", room_id);

        let response = self.client
            .get(&url)
            .header("Accept", "application/json, text/plain, */*")
            .header("Referer", format!("https://www.douyu.com/{}", room_id))
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        let room = &json["room"];
        let anchor_name = room["nickname"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string();

        let room_name = room["room_name"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let show_status = room["show_status"]
            .as_i64()
            .unwrap_or(0);

        let videoloop = room["videoLoop"]
            .as_i64()
            .unwrap_or(0);

        // show_status == 1 且 videoLoop == 0 表示正在直播
        let is_live = show_status == 1 && videoloop == 0;

        let room_info = LiveRoomInfo {
            room_id: room_id.to_string(),
            anchor_name,
            title: room_name,
            status: if is_live { LiveStatus::Live } else { LiveStatus::Offline },
            start_time: None,
            viewer_count: room["online_num"].as_u64(),
            cover_url: room["room_pic"].as_str().map(|s| s.to_string()),
            extra: HashMap::new(),
        };

        Ok((room_info, is_live))
    }

    /// 获取流 URL（使用 H5 API）
    async fn get_stream_url(&self, room_id: &str) -> RecorderResult<Vec<StreamData>> {
        // 生成签名参数
        let did = format!("{:032x}", rand::random::<u128>());
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // 斗鱼的签名算法比较复杂，这里使用一个简化版本
        // 实际上需要执行 JS 来生成完整的签名
        let sign = self.generate_sign(room_id, &did, timestamp)?;

        let url = format!("https://www.douyu.com/lapi/live/getH5Play/{}", room_id);

        let response = self.client
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Referer", format!("https://www.douyu.com/{}", room_id))
            .body(format!("v=1&did={}&tt={}&{}", did, timestamp, sign))
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        if json["error"].as_i64().unwrap_or(-1) != 0 {
            // 如果签名失败，尝试备用方法
            return self.get_stream_url_fallback(room_id).await;
        }

        let mut streams = Vec::new();

        if let Some(data) = json["data"].as_object() {
            let rtmp_cdn = data.get("rtmp_cdn")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let rtmp_url = data.get("rtmp_url")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let rtmp_live = data.get("rtmp_live")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if !rtmp_url.is_empty() && !rtmp_live.is_empty() {
                let flv_url = format!("{}/{}", rtmp_url, rtmp_live);

                // 确保使用 HTTPS
                let flv_url = if flv_url.starts_with("http://") {
                    flv_url.replace("http://", "https://")
                } else if !flv_url.starts_with("https://") {
                    format!("https://{}", flv_url)
                } else {
                    flv_url
                };

                streams.push(StreamData {
                    quality: VideoQuality::Original,
                    url: StreamUrl {
                        flv_url: Some(flv_url),
                        hls_url: None,
                        dash_url: None,
                    },
                    bitrate: None,
                    resolution: None,
                    codec: None,
                    cdn: Some(rtmp_cdn.to_string()),
                });
            }
        }

        Ok(streams)
    }

    /// 生成签名（简化版本）
    fn generate_sign(&self, room_id: &str, did: &str, timestamp: u64) -> RecorderResult<String> {
        // 这是一个简化的签名生成
        // 实际斗鱼使用了更复杂的 JS 混淆算法
        let secret = "a2053899224e8a92974c729dceed1cc99b3d8282";
        let sign_str = format!("{}{}{}{}", room_id, did, timestamp, secret);
        let mut hasher = Md5::new();
        hasher.update(sign_str.as_bytes());
        let result = hasher.finalize();
        let sign = format!("{:x}", result);
        Ok(format!("sign={}", sign))
    }

    /// 备用获取流方法（从网页提取）
    async fn get_stream_url_fallback(&self, room_id: &str) -> RecorderResult<Vec<StreamData>> {
        tracing::debug!("🔄 使用备用方法获取斗鱼流");

        let url = format!("https://www.douyu.com/{}", room_id);
        let response = self.client
            .get(&url)
            .send()
            .await?;

        let html = response.text().await?;

        // 尝试从页面提取流信息
        // 这个方法可能不太稳定
        let re = Regex::new(r#"\$ROOM\.args\s*=\s*\{([^}]+)\}"#)
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;

        if let Some(_captures) = re.captures(&html) {
            // 从页面参数中提取信息
            // 实际实现可能需要更复杂的解析
        }

        // 尝试使用 m3u8 直播流
        let m3u8_url = format!("https://hlstct.douyucdn2.cn/dyliveflv1a/{}.m3u8", room_id);
        
        Ok(vec![StreamData {
            quality: VideoQuality::Original,
            url: StreamUrl {
                flv_url: None,
                hls_url: Some(m3u8_url),
                dash_url: None,
            },
            bitrate: None,
            resolution: None,
            codec: None,
            cdn: Some("backup".to_string()),
        }])
    }
}

impl Default for DouyuHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformHandler for DouyuHandler {
    fn platform_name(&self) -> &'static str {
        "douyu"
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec![
            "douyu.com",
            "www.douyu.com",
        ]
    }

    async fn extract_room_id(&self, url: &str) -> RecorderResult<String> {
        // 从 URL 提取房间号
        // 支持格式: 
        // - https://www.douyu.com/12345
        // - https://www.douyu.com/topic/xxxxx?rid=12345
        
        // 先检查是否有 rid 参数
        if url.contains("rid=") {
            let re = Regex::new(r"rid=(\d+)")
                .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;
            
            if let Some(captures) = re.captures(url) {
                return Ok(captures[1].to_string());
            }
        }

        let url_without_query = url.split('?').next().unwrap_or(url);
        let room_id = url_without_query
            .rsplit('/')
            .next()
            .ok_or_else(|| RecorderError::InvalidUrlFormat("Cannot extract room ID".to_string()))?;

        if room_id.is_empty() || !room_id.chars().all(|c| c.is_ascii_digit()) {
            return Err(RecorderError::InvalidUrlFormat(
                "Invalid Douyu room ID".to_string()
            ));
        }

        Ok(room_id.to_string())
    }

    async fn get_stream_info(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        tracing::info!("🐟 正在获取斗鱼直播间信息: {}", room_id);

        // 获取房间信息
        let (room_info, is_live) = self.get_room_info(room_id).await?;

        if !is_live {
            tracing::info!("📴 斗鱼直播间未开播");
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        // 获取流 URL
        let streams = match self.get_stream_url(room_id).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("⚠️ 获取斗鱼流失败: {}", e);
                vec![]
            }
        };

        tracing::info!("✅ 斗鱼直播间信息获取成功，找到 {} 个流", streams.len());

        Ok(StreamInfo {
            room: room_info,
            streams,
        })
    }
}
