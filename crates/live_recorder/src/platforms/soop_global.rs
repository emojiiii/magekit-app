//! SOOP 国际版（sooplive.com）直播平台处理器
//!
//! 说明：
//! - 仅处理 `sooplive.com` 域名（国际版）。
//! - 与 KR（`play.sooplive.co.kr`）逻辑拆分，减少不必要的请求与判断。
//! - 推荐 Cookie key：`sooplive.com`（与 KR 的 `sooplive` 区分）。

use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

use crate::{
    error::{RecorderError, RecorderResult},
    platforms::{PlatformCookies, PlatformHandler},
    types::{LiveRoomInfo, LiveStatus, StreamData, StreamInfo, StreamUrl, VideoQuality},
};

/// SOOP 国际版处理器（sooplive.com）
pub struct SoopGlobalHandler {
    client: Client,
}

impl SoopGlobalHandler {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:122.0) Gecko/20100101 Firefox/122.0",
            )
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    fn extract_bj_id(&self, url: &str) -> RecorderResult<String> {
        let parts: Vec<&str> = url.split('/').collect();
        if parts.len() >= 4 {
            let bj_id = parts[3].split('?').next().unwrap_or(parts[3]);
            if !bj_id.is_empty() {
                return Ok(bj_id.to_string());
            }
        }
        Err(RecorderError::InvalidUrlFormat(
            "Cannot extract BJ ID from SOOP URL".to_string(),
        ))
    }

    /// 获取请求头（Global API / global-media m3u8）
    fn get_headers(&self, cookies: Option<&str>) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("client-id", Uuid::new_v4().to_string().parse().unwrap());
        headers.insert(
            "user-agent",
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1 Edg/141.0.0.0"
                .parse()
                .unwrap(),
        );
        if let Some(cookie) = cookies {
            let sanitized = cookie
                .replace('\r', "")
                .replace('\n', "")
                .trim()
                .to_string();
            if let Ok(val) = sanitized.parse() {
                headers.insert("cookie", val);
            } else {
                tracing::warn!("⚠️ SOOP Global 请求 Cookie HeaderValue 解析失败，已忽略 Cookie");
            }
        }
        headers
    }

    async fn get_global_channel_info_full(
        &self,
        bj_id: &str,
        cookies: Option<&str>,
    ) -> RecorderResult<(String, Option<String>)> {
        let headers = self.get_headers(cookies);
        let api = format!("https://api.sooplive.com/v2/channel/info/{}", bj_id);

        tracing::debug!("📡 SOOP Global channel API: {}", api);

        let response = self.client.get(&api).headers(headers).send().await?;
        let json: serde_json::Value = response.json().await?;

        tracing::debug!("📋 SOOP Global channel 响应: {:?}", json);

        if json["data"].is_null() {
            let status_code = json["statusCode"].as_i64().unwrap_or(0);
            let error_code = json["code"].as_str().unwrap_or("");
            let error_msg = json["message"].as_str().unwrap_or("Unknown error");
            tracing::warn!(
                "⚠️ SOOP Global channel API 错误: statusCode={}, code={}, message={}",
                status_code,
                error_code,
                error_msg
            );
            return Err(RecorderError::StreamNotAvailable(format!(
                "SOOP Global API error: {}",
                error_msg
            )));
        }

        let nickname = json["data"]["streamerChannelInfo"]["nickname"]
            .as_str()
            .unwrap_or("Unknown");
        let channel_id = json["data"]["streamerChannelInfo"]["channelId"]
            .as_str()
            .unwrap_or(bj_id);

        let profile_image = json["data"]["streamerChannelInfo"]["channelProfileImg"]
            .as_str()
            .or_else(|| json["data"]["streamerChannelInfo"]["profileImage"].as_str())
            .map(|s| s.to_string());

        Ok((format!("{}-{}", nickname, channel_id), profile_image))
    }

    async fn get_global_stream_info_full(
        &self,
        bj_id: &str,
        cookies: Option<&str>,
    ) -> RecorderResult<(bool, String, Option<String>, Option<u64>)> {
        let headers = self.get_headers(cookies);
        let api = format!("https://api.sooplive.com/v2/stream/info/{}", bj_id);

        tracing::debug!("📡 SOOP Global stream API: {}", api);

        let response = self.client.get(&api).headers(headers).send().await?;
        let json: serde_json::Value = response.json().await?;

        tracing::debug!("📋 SOOP Global stream 响应: {:?}", json);

        let is_stream = json["data"]["isStream"].as_bool().unwrap_or(false);
        let title = json["data"]["title"].as_str().unwrap_or("").to_string();

        let cover_url = json["data"]["thumbnail"]
            .as_str()
            .or_else(|| json["data"]["broadImg"].as_str())
            .map(|s| s.to_string());

        let viewer_count = json["data"]["viewCount"]
            .as_u64()
            .or_else(|| json["data"]["currentViewCount"].as_u64());

        Ok((is_stream, title, cover_url, viewer_count))
    }

    async fn get_global_stream_data(
        &self,
        bj_id: &str,
        cookies: Option<&str>,
    ) -> RecorderResult<StreamInfo> {
        let (anchor_name, profile_image) =
            self.get_global_channel_info_full(bj_id, cookies).await?;

        let (is_live, title, stream_cover, viewer_count) = self
            .get_global_stream_info_full(bj_id, cookies)
            .await
            .unwrap_or((false, String::new(), None, None));

        let cover_url = stream_cover.or(profile_image);

        tracing::info!(
            "📺 SOOP(Global) 主播: {}, 标题: {}, 直播: {}, 封面: {:?}",
            anchor_name,
            title,
            is_live,
            cover_url.is_some()
        );

        let room_info = LiveRoomInfo {
            room_id: bj_id.to_string(),
            anchor_name,
            title,
            status: if is_live {
                LiveStatus::Live
            } else {
                LiveStatus::Offline
            },
            start_time: None,
            viewer_count,
            cover_url,
            extra: HashMap::new(),
        };

        if !is_live {
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        let m3u8_url = format!(
            "https://global-media.sooplive.com/live/{}/master.m3u8",
            bj_id
        );
        let streams = self
            .parse_m3u8_playlist(&m3u8_url, cookies)
            .await
            .unwrap_or_else(|_| {
                vec![StreamData {
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
                }]
            });

        Ok(StreamInfo {
            room: room_info,
            streams,
        })
    }

    async fn parse_m3u8_playlist(
        &self,
        m3u8_url: &str,
        cookies: Option<&str>,
    ) -> RecorderResult<Vec<StreamData>> {
        let headers = self.get_headers(cookies);
        let response = self.client.get(m3u8_url).headers(headers).send().await?;
        let content = response.text().await?;

        let mut streams = Vec::new();
        let url_prefix = m3u8_url
            .rsplit('/')
            .skip(1)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("/");
        let url_prefix = format!("{}/", url_prefix);

        let bandwidth_re = Regex::new(r"BANDWIDTH=(\d+)")
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;
        let resolution_re = Regex::new(r"RESOLUTION=(\d+)x(\d+)")
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;

        let mut current_bandwidth = 0u64;
        let mut current_resolution: Option<(u32, u32)> = None;
        let mut expecting_variant_uri = false;

        for line in content.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
            if line.starts_with("#EXT-X-STREAM-INF") {
                expecting_variant_uri = true;
                if let Some(caps) = bandwidth_re.captures(line) {
                    current_bandwidth = caps[1].parse().unwrap_or(0);
                }
                current_resolution = resolution_re.captures(line).and_then(|c| {
                    let w = c.get(1)?.as_str().parse::<u32>().ok()?;
                    let h = c.get(2)?.as_str().parse::<u32>().ok()?;
                    Some((w, h))
                });
                continue;
            }
            if !expecting_variant_uri || line.starts_with('#') {
                continue;
            }
            expecting_variant_uri = false;

            let full_url = if line.starts_with("http://") || line.starts_with("https://") {
                line.to_string()
            } else {
                format!("{}{}", url_prefix, line)
            };

            streams.push(StreamData {
                quality: VideoQuality::Standard, // 先占位，后续排序后再映射
                url: StreamUrl {
                    flv_url: None,
                    hls_url: Some(full_url),
                    dash_url: None,
                },
                bitrate: Some(current_bandwidth),
                resolution: current_resolution,
                codec: None,
                cdn: None,
            });
        }

        Self::apply_quality_by_rank(&mut streams);
        Ok(streams)
    }

    fn apply_quality_by_rank(streams: &mut Vec<StreamData>) {
        streams.sort_by(|a, b| b.bitrate.cmp(&a.bitrate));
        for (idx, s) in streams.iter_mut().enumerate() {
            s.quality = match idx {
                0 => VideoQuality::Original,
                1 => VideoQuality::Ultra,
                2 => VideoQuality::High,
                3 => VideoQuality::Standard,
                _ => VideoQuality::Low,
            };
        }
    }
}

impl Default for SoopGlobalHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformHandler for SoopGlobalHandler {
    fn platform_name(&self) -> &'static str {
        "sooplive.com"
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["sooplive.com", "www.sooplive.com", "m.sooplive.com"]
    }

    async fn extract_room_id(&self, url: &str) -> RecorderResult<String> {
        self.extract_bj_id(url)
    }

    async fn get_stream_info(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        self.get_stream_info_with_cookies(room_id, &PlatformCookies::default())
            .await
    }

    async fn get_stream_info_with_cookies(
        &self,
        room_id: &str,
        cookies: &PlatformCookies,
    ) -> RecorderResult<StreamInfo> {
        let cookie_str = cookies.cookie.as_deref();
        self.get_global_stream_data(room_id, cookie_str).await
    }
}
