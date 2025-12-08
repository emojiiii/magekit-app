//! 快手直播平台处理器

use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use serde_json;
use std::collections::HashMap;
use std::time::Duration;

use crate::{
    error::{RecorderError, RecorderResult},
    platforms::PlatformHandler,
    types::{LiveRoomInfo, LiveStatus, StreamData, StreamInfo, StreamUrl, VideoQuality},
};

/// 快手直播处理器
pub struct KuaishouHandler {
    client: Client,
}

impl KuaishouHandler {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/115.0",
            )
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// 从网页获取流信息
    async fn get_stream_from_web(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        let url = format!("https://live.kuaishou.com/u/{}", room_id);

        let response = self
            .client
            .get(&url)
            .header(
                "Accept-Language",
                "zh-CN,zh;q=0.8,zh-TW;q=0.7,zh-HK;q=0.5,en-US;q=0.3,en;q=0.2",
            )
            .send()
            .await?;

        let html = response.text().await?;

        // 解析 __INITIAL_STATE__
        let re = Regex::new(r"window\.__INITIAL_STATE__=(.*?);\(function\(\)\{var s;")
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;

        let json_str = re
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .ok_or_else(|| {
                RecorderError::InvalidResponseFormat(
                    "Cannot find __INITIAL_STATE__ in page".to_string(),
                )
            })?;

        let json: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| RecorderError::JsonError(e))?;

        // 解析直播信息
        let live_stream = &json["liveroom"]["playList"][0];

        let anchor_name = live_stream["author"]["name"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string();

        let is_live = live_stream["liveStream"].as_object().is_some()
            && live_stream["liveStream"]["playUrls"].as_object().is_some();

        let room_info = LiveRoomInfo {
            room_id: room_id.to_string(),
            anchor_name,
            title: String::new(),
            status: if is_live {
                LiveStatus::Live
            } else {
                LiveStatus::Offline
            },
            start_time: None,
            viewer_count: None,
            cover_url: None,
            extra: HashMap::new(),
        };

        if !is_live {
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        // 解析流 URL
        let mut streams = Vec::new();

        // 尝试 H264 格式
        if let Some(play_urls) =
            live_stream["liveStream"]["playUrls"]["h264"]["adaptationSet"]["representation"]
                .as_array()
        {
            for play_url in play_urls {
                let url = play_url["url"].as_str().unwrap_or("");
                let quality_type = play_url["qualityType"].as_str().unwrap_or("");
                let bitrate = play_url["bitrate"].as_u64();

                let quality = match quality_type {
                    "STANDARD" => VideoQuality::Standard,
                    "HIGH" => VideoQuality::High,
                    "SUPER" => VideoQuality::Ultra,
                    "BLUE_RAY" => VideoQuality::Original,
                    _ => VideoQuality::Original,
                };

                if !url.is_empty() {
                    // 确保使用 HTTPS
                    let url = if url.starts_with("http://") {
                        url.replace("http://", "https://")
                    } else {
                        url.to_string()
                    };

                    streams.push(StreamData {
                        quality,
                        url: StreamUrl {
                            flv_url: Some(url),
                            hls_url: None,
                            dash_url: None,
                        },
                        bitrate,
                        resolution: None,
                        codec: Some("h264".to_string()),
                        cdn: None,
                    });
                }
            }
        }

        Ok(StreamInfo {
            room: room_info,
            streams,
        })
    }

    /// 使用 App API 获取流信息
    async fn get_stream_from_app_api(&self, user_id: &str) -> RecorderResult<StreamInfo> {
        let app_api =
            "https://livev.m.chenzhongtech.com/rest/k/live/byUser?kpn=GAME_ZONE&captchaToken=";

        let data = serde_json::json!({
            "source": 5,
            "eid": user_id,
            "shareMethod": "card",
            "clientType": "WEB_OUTSIDE_SHARE_H5"
        });

        let response = self
            .client
            .post(app_api)
            .header(
                "User-Agent",
                "ios/7.830 (ios 17.0; ; iPhone 15 (A2846/A3089/A3090/A3092))",
            )
            .header("Accept-Language", "zh-CN,zh;q=0.8")
            .header("Referer", "https://www.kuaishou.com/")
            .header("Content-Type", "application/json")
            .header(
                "Cookie",
                "did=web_e988652e11b545469633396abe85a89f; didv=1796004001000",
            )
            .json(&data)
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        let live_stream = &json["liveStream"];

        let anchor_name = live_stream["user"]["user_name"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string();

        let is_live = live_stream["living"].as_bool().unwrap_or(false);

        let room_info = LiveRoomInfo {
            room_id: user_id.to_string(),
            anchor_name,
            title: String::new(),
            status: if is_live {
                LiveStatus::Live
            } else {
                LiveStatus::Offline
            },
            start_time: None,
            viewer_count: None,
            cover_url: None,
            extra: HashMap::new(),
        };

        if !is_live {
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        let mut streams = Vec::new();

        // 获取 HLS URL
        if let Some(hls_url) = live_stream["hlsPlayUrl"].as_str() {
            if !hls_url.is_empty() {
                streams.push(StreamData {
                    quality: VideoQuality::Original,
                    url: StreamUrl {
                        flv_url: None,
                        hls_url: Some(hls_url.to_string()),
                        dash_url: None,
                    },
                    bitrate: None,
                    resolution: None,
                    codec: None,
                    cdn: None,
                });
            }
        }

        // 获取 FLV URL
        if let Some(play_urls) = live_stream["playUrls"].as_array() {
            if let Some(first_url) = play_urls.first() {
                if let Some(url) = first_url["url"].as_str() {
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
        }

        // 获取多分辨率 URL
        if let Some(multi_res) = live_stream["multiResolutionHlsPlayUrls"].as_array() {
            if let Some(first) = multi_res.first() {
                if let Some(urls) = first["urls"].as_array() {
                    for url_item in urls {
                        let url = url_item["url"].as_str().unwrap_or("");
                        let quality_type = url_item["qualityType"].as_str().unwrap_or("");

                        let quality = match quality_type {
                            "STANDARD" => VideoQuality::Standard,
                            "HIGH" => VideoQuality::High,
                            "SUPER" => VideoQuality::Ultra,
                            _ => VideoQuality::Original,
                        };

                        if !url.is_empty() {
                            streams.push(StreamData {
                                quality,
                                url: StreamUrl {
                                    flv_url: None,
                                    hls_url: Some(url.to_string()),
                                    dash_url: None,
                                },
                                bitrate: None,
                                resolution: None,
                                codec: None,
                                cdn: None,
                            });
                        }
                    }
                }
            }
        }

        Ok(StreamInfo {
            room: room_info,
            streams,
        })
    }
}

impl Default for KuaishouHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformHandler for KuaishouHandler {
    fn platform_name(&self) -> &'static str {
        "kuaishou"
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["kuaishou.com", "live.kuaishou.com", "www.kuaishou.com"]
    }

    async fn extract_room_id(&self, url: &str) -> RecorderResult<String> {
        // 从 URL 提取用户 ID
        // 支持格式:
        // - https://live.kuaishou.com/u/xxx
        // - https://www.kuaishou.com/profile/xxx

        let re_live = Regex::new(r"live\.kuaishou\.com/u/([^/?]+)")
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;

        if let Some(captures) = re_live.captures(url) {
            return Ok(captures[1].to_string());
        }

        let re_profile = Regex::new(r"kuaishou\.com/profile/([^/?]+)")
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;

        if let Some(captures) = re_profile.captures(url) {
            return Ok(captures[1].to_string());
        }

        // 尝试直接从路径提取
        let url_without_query = url.split('?').next().unwrap_or(url);
        let user_id = url_without_query
            .rsplit('/')
            .next()
            .ok_or_else(|| RecorderError::InvalidUrlFormat("Cannot extract user ID".to_string()))?;

        if user_id.is_empty() {
            return Err(RecorderError::InvalidUrlFormat(
                "Invalid Kuaishou user ID".to_string(),
            ));
        }

        Ok(user_id.to_string())
    }

    async fn get_stream_info(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        tracing::info!("🎬 正在获取快手直播间信息: {}", room_id);

        // 优先使用 App API
        match self.get_stream_from_app_api(room_id).await {
            Ok(info) if !info.room.anchor_name.is_empty() => {
                tracing::info!("✅ 快手直播间信息获取成功 (App API)");
                return Ok(info);
            }
            Err(e) => {
                tracing::warn!("⚠️ App API 获取失败: {}", e);
            }
            _ => {}
        }

        // 备用：使用网页方式
        match self.get_stream_from_web(room_id).await {
            Ok(info) => {
                tracing::info!("✅ 快手直播间信息获取成功 (Web)");
                Ok(info)
            }
            Err(e) => {
                tracing::error!("❌ 快手直播间信息获取失败: {}", e);
                Err(e)
            }
        }
    }
}
