//! 虎牙直播平台处理器

use async_trait::async_trait;
use reqwest::Client;
use serde_json;
use std::collections::HashMap;
use std::time::Duration;
use regex::Regex;

use crate::{
    error::{RecorderError, RecorderResult},
    platforms::PlatformHandler,
    types::{LiveRoomInfo, LiveStatus, StreamData, StreamInfo, StreamUrl, VideoQuality},
};

/// 虎牙直播处理器
pub struct HuyaHandler {
    client: Client,
}

impl HuyaHandler {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:123.0) Gecko/20100101 Firefox/123.0")
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// 从微信小程序 API 获取流信息（更稳定）
    async fn get_stream_from_wx_api(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        let params = format!("m=Live&do=profileRoom&roomid={}&showSecret=1", room_id);
        let url = format!("https://mp.huya.com/cache.php?{}", params);

        let response = self.client
            .get(&url)
            .header("User-Agent", "ios/7.830 (ios 17.0; ; iPhone 15 (A2846/A3089/A3090/A3092))")
            .header("xweb_xhr", "1")
            .header("referer", "https://servicewechat.com/wx74767bf0b684f7d3/301/page-frame.html")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        let anchor_name = json["data"]["profileInfo"]["nick"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string();

        let live_status = json["data"]["realLiveStatus"]
            .as_str()
            .unwrap_or("OFF");

        let title = json["data"]["liveData"]["introduction"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let room_info = LiveRoomInfo {
            room_id: room_id.to_string(),
            anchor_name,
            title,
            status: if live_status == "ON" { LiveStatus::Live } else { LiveStatus::Offline },
            start_time: None,
            viewer_count: None,
            cover_url: None,
            extra: HashMap::new(),
        };

        if live_status != "ON" {
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        // 解析流 URL
        let mut streams = Vec::new();
        
        if let Some(stream_info_list) = json["data"]["stream"]["baseSteamInfoList"].as_array() {
            // CDN 优先级：TX > HW > HS > AL
            let priority_order = ["TX", "HW", "HS", "AL"];
            
            for priority_cdn in &priority_order {
                for stream_item in stream_info_list {
                    let cdn_type = stream_item["sCdnType"].as_str().unwrap_or("");
                    
                    if cdn_type == *priority_cdn {
                        let stream_name = stream_item["sStreamName"].as_str().unwrap_or("");
                        let flv_url_base = stream_item["sFlvUrl"].as_str().unwrap_or("");
                        let flv_anti_code = stream_item["sFlvAntiCode"].as_str().unwrap_or("");
                        let hls_url_base = stream_item["sHlsUrl"].as_str().unwrap_or("");
                        let hls_anti_code = stream_item["sHlsAntiCode"].as_str().unwrap_or("");

                        let mut flv_url = format!("{}/{}.flv?{}", flv_url_base, stream_name, flv_anti_code);
                        let m3u8_url = format!("{}/{}.m3u8?{}", hls_url_base, stream_name, hls_anti_code);

                        // 如果是 TX CDN，进行特殊处理
                        if cdn_type == "TX" {
                            flv_url = flv_url
                                .replace("&ctype=tars_mp", "&ctype=huya_webh5")
                                .replace("&fs=bhct", "&fs=bgct");
                        }

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
                                hls_url: Some(m3u8_url),
                                dash_url: None,
                            },
                            bitrate: None,
                            resolution: None,
                            codec: None,
                            cdn: Some(cdn_type.to_string()),
                        });

                        // 只取第一个优先 CDN
                        break;
                    }
                }
                
                if !streams.is_empty() {
                    break;
                }
            }
        }

        Ok(StreamInfo {
            room: room_info,
            streams,
        })
    }

    /// 从网页提取流信息
    async fn get_stream_from_web(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        let url = format!("https://www.huya.com/{}", room_id);
        
        let response = self.client
            .get(&url)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header("Accept-Language", "zh-CN,zh;q=0.8")
            .send()
            .await?;

        let html = response.text().await?;

        // 检查是否存在流数据
        let re = Regex::new(r#"stream: (\{"data".*?),"iWebDefaultBitRate""#)
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;

        if let Some(captures) = re.captures(&html) {
            let json_str = format!("{}}}", &captures[1]);
            let json: serde_json::Value = serde_json::from_str(&json_str)
                .map_err(|e| RecorderError::JsonError(e))?;

            tracing::debug!("📋 虎牙网页流数据: {:?}", json);

            // 从网页解析主播信息
            let anchor_re = Regex::new(r#""sNick":"([^"]+)""#).ok();
            let anchor_name = anchor_re
                .and_then(|re| re.captures(&html))
                .map(|c| c[1].to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            // 检查直播状态
            let is_live = json["data"].as_array()
                .map(|arr| !arr.is_empty())
                .unwrap_or(false);

            let room_info = LiveRoomInfo {
                room_id: room_id.to_string(),
                anchor_name,
                title: String::new(),
                status: if is_live { LiveStatus::Live } else { LiveStatus::Offline },
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

            // TODO: 解析网页中的流 URL
            // 网页方式比较复杂，优先使用微信 API

            Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            })
        } else {
            Err(RecorderError::StreamNotAvailable(
                "Cannot find stream data in page".to_string()
            ))
        }
    }
}

impl Default for HuyaHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformHandler for HuyaHandler {
    fn platform_name(&self) -> &'static str {
        "huya"
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec![
            "huya.com",
            "www.huya.com",
        ]
    }

    async fn extract_room_id(&self, url: &str) -> RecorderResult<String> {
        // 从 URL 提取房间号
        // 支持格式: https://www.huya.com/12345
        let url_without_query = url.split('?').next().unwrap_or(url);
        let room_id = url_without_query
            .rsplit('/')
            .next()
            .ok_or_else(|| RecorderError::InvalidUrlFormat("Cannot extract room ID".to_string()))?;

        if room_id.is_empty() {
            return Err(RecorderError::InvalidUrlFormat(
                "Invalid Huya room ID".to_string()
            ));
        }

        // 如果是字母（别名），需要从页面获取真实房间号
        if room_id.chars().any(|c| c.is_alphabetic()) {
            // 从网页获取真实房间号
            let url = format!("https://www.huya.com/{}", room_id);
            let response = self.client
                .get(&url)
                .send()
                .await?;
            
            let html = response.text().await?;
            
            let re = Regex::new(r#"ProfileRoom":(\d+),"sPrivateHost"#)
                .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;
            
            if let Some(captures) = re.captures(&html) {
                return Ok(captures[1].to_string());
            }
            
            return Err(RecorderError::InvalidUrlFormat(
                "Cannot find real room ID".to_string()
            ));
        }

        Ok(room_id.to_string())
    }

    async fn get_stream_info(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        tracing::info!("🐯 正在获取虎牙直播间信息: {}", room_id);

        // 优先使用微信小程序 API
        match self.get_stream_from_wx_api(room_id).await {
            Ok(info) => {
                tracing::info!("✅ 虎牙直播间信息获取成功");
                Ok(info)
            }
            Err(e) => {
                tracing::warn!("⚠️ 微信API获取失败，尝试网页方式: {}", e);
                self.get_stream_from_web(room_id).await
            }
        }
    }
}
