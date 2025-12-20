//! Twitch 直播平台处理器
//!
//! 参考 `py_demo/src/spider.py#get_twitchtv_stream_data` 的实现。

use async_trait::async_trait;
use rand::{Rng, distributions::Alphanumeric};
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

/// Twitch 直播处理器
pub struct TwitchHandler {
    client: Client,
}

impl TwitchHandler {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:124.0) Gecko/20100101 Firefox/124.0",
            )
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    fn random_device_id() -> String {
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(16)
            .map(|c| (c as char).to_ascii_lowercase())
            .collect()
    }

    async fn fetch_playback_access_token(
        &self,
        login: &str,
        device_id: &str,
    ) -> RecorderResult<(String, String)> {
        let url = "https://gql.twitch.tv/gql";
        let payload = serde_json::json!({
            "operationName": "PlaybackAccessToken_Template",
            "query": "query PlaybackAccessToken_Template($login: String!, $isLive: Boolean!, $vodID: ID!, $isVod: Boolean!, $playerType: String!) {  streamPlaybackAccessToken(channelName: $login, params: {platform: \"web\", playerBackend: \"mediaplayer\", playerType: $playerType}) @include(if: $isLive) {    value    signature   authorization { isForbidden forbiddenReasonCode }   __typename  }  videoPlaybackAccessToken(id: $vodID, params: {platform: \"web\", playerBackend: \"mediaplayer\", playerType: $playerType}) @include(if: $isVod) {    value    signature   __typename  }}",
            "variables": {
                "isLive": true,
                "login": login,
                "isVod": false,
                "vodID": "",
                "playerType": "site"
            }
        });

        let response = self
            .client
            .post(url)
            .header("Accept-Language", "en-US")
            .header("Referer", "https://www.twitch.tv/")
            .header("Client-ID", "kimne78kx3ncx6brgo4mv6wki5h1ko")
            .header("device-id", device_id)
            .json(&payload)
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;
        let token = json["data"]["streamPlaybackAccessToken"]["value"]
            .as_str()
            .ok_or_else(|| {
                RecorderError::InvalidResponseFormat("Missing twitch token".to_string())
            })?
            .to_string();
        let sig = json["data"]["streamPlaybackAccessToken"]["signature"]
            .as_str()
            .ok_or_else(|| {
                RecorderError::InvalidResponseFormat("Missing twitch signature".to_string())
            })?
            .to_string();
        Ok((token, sig))
    }

    async fn fetch_channel_shell(
        &self,
        login: &str,
        token: &str,
    ) -> RecorderResult<(String, bool, String, Option<String>, Option<u64>)> {
        let url = "https://gql.twitch.tv/gql";
        let payload = serde_json::json!([
            {
                "operationName": "ChannelShell",
                "variables": { "login": login },
                "extensions": {
                    "persistedQuery": {
                        "version": 1,
                        "sha256Hash": "580ab410bcd0c1ad194224957ae2241e5d252b2c5173d8e0cce9d32d5bb14efe"
                    }
                }
            }
        ]);

        let response = self
            .client
            .post(url)
            .header("Accept-Language", "en-US")
            .header("Referer", "https://www.twitch.tv/")
            .header("Client-Id", "kimne78kx3ncx6brgo4mv6wki5h1ko")
            // py_demo 使用 Client-Integrity=token 访问 persistedQuery，这里保持一致
            .header("Client-Integrity", token)
            .header("Content-Type", "text/plain;charset=UTF-8")
            .body(payload.to_string())
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;
        Self::parse_channel_shell_response(&json, login)
    }

    fn parse_channel_shell_response(
        json: &serde_json::Value,
        login: &str,
    ) -> RecorderResult<(String, bool, String, Option<String>, Option<u64>)> {
        let user = &json[0]["data"]["userOrError"];

        let login_name = user["login"].as_str().unwrap_or(login);
        let display_name = user["displayName"].as_str().unwrap_or(login_name);
        let anchor_name = format!("{}-{}", display_name, login_name);

        let is_live = !user["stream"].is_null();

        let title = user["stream"]["title"]
            .as_str()
            .or_else(|| user["broadcastSettings"]["title"].as_str())
            .unwrap_or("")
            .to_string();

        let cover_url = user["stream"]["previewImageURL"]
            .as_str()
            .or_else(|| user["profileImageURL"].as_str())
            .map(|s| s.to_string());

        let viewer_count = user["stream"]["viewersCount"].as_u64();

        Ok((anchor_name, is_live, title, cover_url, viewer_count))
    }

    async fn parse_m3u8_variants(&self, master_url: &str) -> RecorderResult<Vec<StreamData>> {
        let response = self.client.get(master_url).send().await?;
        let content = response.text().await?;
        Self::parse_m3u8_variants_text(master_url, &content)
    }

    fn parse_m3u8_variants_text(
        master_url: &str,
        content: &str,
    ) -> RecorderResult<Vec<StreamData>> {
        let base = Url::parse(master_url)
            .map_err(|e| RecorderError::InvalidUrlFormat(format!("Invalid m3u8 url: {}", e)))?;

        let bandwidth_re = Regex::new(r#"BANDWIDTH=(\d+)"#)
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;
        let resolution_re = Regex::new(r#"RESOLUTION=(\d+)x(\d+)"#)
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;

        let mut streams = Vec::new();
        let mut pending: Option<(u64, Option<(u32, u32)>)> = None;

        for line in content.lines() {
            if line.starts_with("#EXT-X-STREAM-INF") {
                let bandwidth = bandwidth_re
                    .captures(line)
                    .and_then(|c| c.get(1))
                    .and_then(|m| m.as_str().parse::<u64>().ok())
                    .unwrap_or(0);
                let resolution = resolution_re.captures(line).and_then(|c| {
                    let w = c.get(1)?.as_str().parse::<u32>().ok()?;
                    let h = c.get(2)?.as_str().parse::<u32>().ok()?;
                    Some((w, h))
                });
                pending = Some((bandwidth, resolution));
                continue;
            }

            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }

            let Some((bandwidth, resolution)) = pending.take() else {
                continue;
            };

            let full = base
                .join(line.trim())
                .map_err(|e| {
                    RecorderError::InvalidResponseFormat(format!("Join m3u8 url failed: {}", e))
                })?
                .to_string();

            let quality = if bandwidth > 6_000_000 {
                VideoQuality::Original
            } else if bandwidth > 3_500_000 {
                VideoQuality::Ultra
            } else if bandwidth > 1_800_000 {
                VideoQuality::High
            } else if bandwidth > 900_000 {
                VideoQuality::Standard
            } else {
                VideoQuality::Low
            };

            streams.push(StreamData {
                quality,
                url: StreamUrl {
                    flv_url: None,
                    hls_url: Some(full),
                    dash_url: None,
                },
                bitrate: Some(bandwidth),
                resolution,
                codec: None,
                cdn: None,
            });
        }

        streams.sort_by(|a, b| b.bitrate.cmp(&a.bitrate));
        Ok(streams)
    }
}

impl Default for TwitchHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_channel_shell_response() {
        let json: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/twitch_channel_shell.json")).unwrap();
        let (anchor, is_live, title, cover, viewers) =
            TwitchHandler::parse_channel_shell_response(&json, "examplelogin").unwrap();

        assert_eq!(anchor, "ExampleName-examplelogin");
        assert!(is_live);
        assert_eq!(title, "Twitch 直播标题");
        assert!(cover.is_some());
        assert_eq!(viewers, Some(321));
    }

    #[test]
    fn test_parse_m3u8_variants_text() {
        let content = include_str!("fixtures/twitch_master.m3u8");
        let streams = TwitchHandler::parse_m3u8_variants_text(
            "https://usher.ttvnw.net/api/channel/hls/examplelogin.m3u8?foo=bar",
            content,
        )
        .unwrap();

        assert_eq!(streams.len(), 2);
        assert_eq!(streams[0].bitrate, Some(6_500_000));
        assert_eq!(streams[0].resolution, Some((1920, 1080)));
        assert!(
            streams[0]
                .url
                .hls_url
                .as_ref()
                .unwrap()
                .starts_with("https://usher.ttvnw.net/api/channel/hls/chunked/high/index-dvr.m3u8")
        );
    }
}

#[async_trait]
impl PlatformHandler for TwitchHandler {
    fn platform_name(&self) -> &'static str {
        "twitch"
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["twitch.tv", "www.twitch.tv"]
    }

    async fn extract_room_id(&self, url: &str) -> RecorderResult<String> {
        let parsed =
            Url::parse(url).map_err(|_| RecorderError::InvalidUrlFormat(url.to_string()))?;
        let login = parsed
            .path()
            .trim_matches('/')
            .split('/')
            .next()
            .unwrap_or("")
            .trim();

        if login.is_empty() {
            return Err(RecorderError::InvalidUrlFormat(
                "Invalid twitch login".to_string(),
            ));
        }

        Ok(login.to_string())
    }

    async fn get_stream_info(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        tracing::info!("🟣 正在获取 Twitch 直播间信息: {}", room_id);

        let device_id = Self::random_device_id();
        let (token, sig) = self
            .fetch_playback_access_token(room_id, &device_id)
            .await?;
        let (anchor_name, is_live, title, cover_url, viewer_count) =
            self.fetch_channel_shell(room_id, &token).await?;

        let room_info = LiveRoomInfo {
            room_id: room_id.to_string(),
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

        // 组装 master.m3u8（usher 接口）
        let params = [
            ("allow_source", "true"),
            ("allow_audio_only", "true"),
            ("allow_spectre", "true"),
            ("fast_bread", "true"),
            ("platform", "web"),
            ("player_backend", "mediaplayer"),
            ("player_version", "1.28.0-rc.1"),
            ("playlist_include_framerate", "true"),
            ("reassignments_supported", "true"),
            ("sig", sig.as_str()),
            ("token", token.as_str()),
            ("transcode_mode", "cbr_v1"),
        ];

        let query = {
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());
            for (k, v) in params {
                serializer.append_pair(k, v);
            }
            serializer.finish()
        };

        let master_url = format!(
            "https://usher.ttvnw.net/api/channel/hls/{}.m3u8?{}",
            room_id, query
        );

        let streams = self
            .parse_m3u8_variants(&master_url)
            .await
            .unwrap_or_else(|_| {
                vec![StreamData {
                    quality: VideoQuality::Original,
                    url: StreamUrl {
                        flv_url: None,
                        hls_url: Some(master_url),
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
}
