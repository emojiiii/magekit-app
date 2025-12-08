//! SOOP (原 AfreecaTV) 直播平台处理器
//!
//! 支持以下域名:
//! - sooplive.co.kr (韩国)
//! - sooplive.com (国际)
//! - play.sooplive.co.kr

use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use serde_json;
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

use crate::{
    error::{RecorderError, RecorderResult},
    platforms::{PlatformCookies, PlatformHandler},
    types::{LiveRoomInfo, LiveStatus, StreamData, StreamInfo, StreamUrl, VideoQuality},
};

/// SOOP 直播处理器
pub struct SoopHandler {
    client: Client,
}

impl SoopHandler {
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

    /// 获取请求头
    fn get_headers(&self, cookies: Option<&str>) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("client-id", Uuid::new_v4().to_string().parse().unwrap());
        headers.insert(
            "user-agent",
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1 Edg/141.0.0.0".parse().unwrap()
        );
        if let Some(cookie) = cookies {
            if let Ok(val) = cookie.parse() {
                headers.insert("cookie", val);
            }
        }
        headers
    }

    /// 从 URL 提取 BJ ID
    fn extract_bj_id(&self, url: &str) -> RecorderResult<String> {
        let parts: Vec<&str> = url.split('/').collect();

        // 支持格式:
        // https://play.sooplive.co.kr/bjid
        // https://play.sooplive.co.kr/bjid/broadno
        // https://sooplive.com/bjid

        if parts.len() >= 4 {
            let bj_id = if parts.len() < 6 { parts[3] } else { parts[5] };

            // 去除查询参数
            let bj_id = bj_id.split('?').next().unwrap_or(bj_id);

            if !bj_id.is_empty() {
                return Ok(bj_id.to_string());
            }
        }

        Err(RecorderError::InvalidUrlFormat(
            "Cannot extract BJ ID from SOOP URL".to_string(),
        ))
    }

    /// 获取国际版频道信息（完整版）
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

        // 检查 API 返回的状态码
        let status_code = json["statusCode"].as_i64().unwrap_or(0);
        if status_code != 200 {
            let error_code = json["code"].as_str().unwrap_or("");
            let error_msg = json["message"].as_str().unwrap_or("Unknown error");
            tracing::warn!(
                "⚠️ SOOP Global channel API 错误: {} - {}",
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

        // 获取头像作为封面图 - 修正字段名
        let profile_image = json["data"]["streamerChannelInfo"]["channelProfileImg"]
            .as_str()
            .or_else(|| json["data"]["streamerChannelInfo"]["profileImage"].as_str())
            .map(|s| s.to_string());

        Ok((format!("{}-{}", nickname, channel_id), profile_image))
    }

    /// 获取国际版流信息（增强版）
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

        // 获取直播封面图
        let cover_url = json["data"]["thumbnail"]
            .as_str()
            .or_else(|| json["data"]["broadImg"].as_str())
            .map(|s| s.to_string());

        // 获取观看人数
        let viewer_count = json["data"]["viewCount"]
            .as_u64()
            .or_else(|| json["data"]["currentViewCount"].as_u64());

        Ok((is_stream, title, cover_url, viewer_count))
    }

    /// 获取国际版流数据（增强版）
    async fn get_global_stream_data(
        &self,
        bj_id: &str,
        cookies: Option<&str>,
    ) -> RecorderResult<StreamInfo> {
        // 获取频道信息（包含主播名和头像）- 如果失败则返回错误让调用者尝试韩国版
        let (anchor_name, profile_image) =
            self.get_global_channel_info_full(bj_id, cookies).await?;

        // 获取流信息（包含直播状态、标题、封面图、观看人数）
        let (is_live, title, stream_cover, viewer_count) = self
            .get_global_stream_info_full(bj_id, cookies)
            .await
            .unwrap_or((false, String::new(), None, None));

        // 优先使用直播封面图，其次使用头像
        let cover_url = stream_cover.or(profile_image);

        tracing::info!(
            "📺 SOOP 主播: {}, 标题: {}, 直播: {}, 封面: {:?}",
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

        // 获取 m3u8 URL
        let m3u8_url = format!(
            "https://global-media.sooplive.com/live/{}/master.m3u8",
            bj_id
        );

        // 解析多码率流
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

    /// 获取韩国版流数据
    async fn get_kr_stream_data(
        &self,
        bj_id: &str,
        cookies: Option<&str>,
    ) -> RecorderResult<StreamInfo> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/119.0"
                .parse()
                .unwrap(),
        );
        headers.insert(
            "Accept-Language",
            "zh-CN,zh;q=0.8,zh-TW;q=0.7,zh-HK;q=0.5,en-US;q=0.3,en;q=0.2"
                .parse()
                .unwrap(),
        );
        headers.insert("Referer", "https://m.sooplive.co.kr/".parse().unwrap());
        headers.insert(
            "Content-Type",
            "application/x-www-form-urlencoded".parse().unwrap(),
        );

        if let Some(cookie) = cookies {
            if let Ok(val) = cookie.parse() {
                headers.insert("Cookie", val);
            }
        }

        let data = [
            ("bj_id", bj_id),
            ("broad_no", ""),
            ("agent", "web"),
            ("confirm_adult", "true"),
            ("player_type", "webm"),
            ("mode", "live"),
        ];

        let response = self
            .client
            .post("http://api.m.sooplive.co.kr/broad/a/watch")
            .headers(headers.clone())
            .form(&data)
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        tracing::debug!("📋 SOOP KR API 响应: {:?}", json);

        // 解析主播名称
        let anchor_name = if let Some(nick) = json["data"]["user_nick"].as_str() {
            if let Some(id) = json["data"]["bj_id"].as_str() {
                format!("{}-{}", nick, id)
            } else {
                nick.to_string()
            }
        } else {
            bj_id.to_string()
        };

        // 获取直播标题
        let title = json["data"]["broad_title"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // 获取封面图 - 使用直播预览图或头像
        let cover_url = json["data"]["broad_img"]
            .as_str()
            .or_else(|| json["data"]["profile_image"].as_str())
            .or_else(|| json["data"]["bj_profile_img"].as_str())
            .map(|s| {
                // 如果是相对路径，补全为完整URL
                if s.starts_with("//") {
                    format!("https:{}", s)
                } else if s.starts_with("/") {
                    format!("https://profile.img.sooplive.co.kr{}", s)
                } else {
                    s.to_string()
                }
            });

        // 获取观看人数
        let viewer_count = json["data"]["total_view_cnt"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| json["data"]["total_view_cnt"].as_u64());

        tracing::info!(
            "📺 SOOP KR 主播: {}, 标题: {}, 封面: {:?}",
            anchor_name,
            title,
            cover_url.is_some()
        );

        let room_info = LiveRoomInfo {
            room_id: bj_id.to_string(),
            anchor_name: anchor_name.clone(),
            title,
            status: LiveStatus::Offline,
            start_time: None,
            viewer_count,
            cover_url,
            extra: HashMap::new(),
        };

        // 检查错误码
        if let Some(code) = json["data"]["code"].as_i64() {
            match code {
                -3001 => {
                    tracing::info!("📴 SOOP 直播刚刚结束");
                    return Ok(StreamInfo {
                        room: room_info,
                        streams: vec![],
                    });
                }
                -3002 => {
                    tracing::warn!("🔒 SOOP 直播需要 19+ 认证，请配置 Cookie");
                    return Err(RecorderError::AuthenticationRequired(
                        "SOOP 直播需要登录验证".to_string(),
                    ));
                }
                -3004 => {
                    // 需要 Cookie 的情况，如果有 Cookie 则尝试获取
                    if cookies.is_some() {
                        // 继续处理
                    } else {
                        return Err(RecorderError::AuthenticationRequired(
                            "SOOP 直播需要登录验证".to_string(),
                        ));
                    }
                }
                -6001 => {
                    tracing::warn!("❌ SOOP 直播间地址错误");
                    return Err(RecorderError::InvalidUrlFormat(
                        "请检查 SOOP 直播间地址是否正确".to_string(),
                    ));
                }
                _ => {}
            }
        }

        // 检查是否成功获取直播信息
        if json["result"].as_i64() != Some(1) || anchor_name.is_empty() {
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        // 获取直播流信息
        let broad_no = json["data"]["broad_no"].as_str().unwrap_or("");
        let hls_auth_key = json["data"]["hls_authentication_key"]
            .as_str()
            .unwrap_or("");

        if broad_no.is_empty() {
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        // 获取 CDN URL
        let cdn_data = self.get_cdn_url(broad_no, cookies).await?;
        let view_url = cdn_data["view_url"].as_str().unwrap_or("");

        if view_url.is_empty() {
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        let m3u8_url = format!("{}?aid={}", view_url, hls_auth_key);

        let mut room_info = room_info;
        room_info.status = LiveStatus::Live;

        // 解析多码率流
        let streams = self
            .parse_kr_m3u8_playlist(&m3u8_url, cookies)
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

    /// 获取 CDN URL
    async fn get_cdn_url(
        &self,
        broad_no: &str,
        cookies: Option<&str>,
    ) -> RecorderResult<serde_json::Value> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/119.0"
                .parse()
                .unwrap(),
        );
        headers.insert("Accept-Language", "zh-CN,zh;q=0.8".parse().unwrap());
        headers.insert("Origin", "https://play.sooplive.co.kr".parse().unwrap());
        headers.insert("Referer", "https://play.sooplive.co.kr/".parse().unwrap());
        headers.insert(
            "Content-Type",
            "application/x-www-form-urlencoded".parse().unwrap(),
        );

        if let Some(cookie) = cookies {
            if let Ok(val) = cookie.parse() {
                headers.insert("Cookie", val);
            }
        }

        let params = format!(
            "return_type=gcp_cdn&use_cors=false&cors_origin_url=play.sooplive.co.kr&broad_key={}-common-master-hls&time=8361.086329376785",
            broad_no
        );

        let url = format!(
            "http://livestream-manager.sooplive.co.kr/broad_stream_assign.html?{}",
            params
        );

        let response = self.client.get(&url).headers(headers).send().await?;

        let json: serde_json::Value = response.json().await?;
        Ok(json)
    }

    /// 解析 m3u8 播放列表（国际版）
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
        let url_prefix = format!(
            "{}/",
            url_prefix.split('/').take(3).collect::<Vec<_>>().join("/")
        );

        let bandwidth_re = Regex::new(r"BANDWIDTH=(\d+)")
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;

        let lines: Vec<&str> = content.lines().collect();
        let mut current_bandwidth = 0u64;

        for line in lines.iter() {
            if line.starts_with("#EXT-X-STREAM-INF") {
                if let Some(caps) = bandwidth_re.captures(line) {
                    current_bandwidth = caps[1].parse().unwrap_or(0);
                }
            } else if !line.starts_with('#') && !line.is_empty() {
                let full_url = if line.starts_with("http") {
                    line.to_string()
                } else {
                    format!("{}{}", url_prefix, line.trim())
                };

                let quality = if current_bandwidth > 5000000 {
                    VideoQuality::Original
                } else if current_bandwidth > 3000000 {
                    VideoQuality::Ultra
                } else if current_bandwidth > 1500000 {
                    VideoQuality::High
                } else {
                    VideoQuality::Standard
                };

                streams.push(StreamData {
                    quality,
                    url: StreamUrl {
                        flv_url: None,
                        hls_url: Some(full_url),
                        dash_url: None,
                    },
                    bitrate: Some(current_bandwidth),
                    resolution: None,
                    codec: None,
                    cdn: None,
                });
            }
        }

        // 按码率降序排序
        streams.sort_by(|a, b| b.bitrate.cmp(&a.bitrate));

        Ok(streams)
    }

    /// 解析 m3u8 播放列表（韩国版）
    async fn parse_kr_m3u8_playlist(
        &self,
        m3u8_url: &str,
        cookies: Option<&str>,
    ) -> RecorderResult<Vec<StreamData>> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/119.0"
                .parse()
                .unwrap(),
        );

        if let Some(cookie) = cookies {
            if let Ok(val) = cookie.parse() {
                headers.insert("Cookie", val);
            }
        }

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

        let lines: Vec<&str> = content.lines().collect();
        let mut current_bandwidth = 0u64;

        for line in lines.iter() {
            if line.starts_with("#EXT-X-STREAM-INF") {
                if let Some(caps) = bandwidth_re.captures(line) {
                    current_bandwidth = caps[1].parse().unwrap_or(0);
                }
            } else if line.starts_with("auth_playlist") {
                let full_url = format!("{}{}", url_prefix, line.trim());

                let quality = if current_bandwidth > 5000000 {
                    VideoQuality::Original
                } else if current_bandwidth > 3000000 {
                    VideoQuality::Ultra
                } else if current_bandwidth > 1500000 {
                    VideoQuality::High
                } else {
                    VideoQuality::Standard
                };

                streams.push(StreamData {
                    quality,
                    url: StreamUrl {
                        flv_url: None,
                        hls_url: Some(full_url),
                        dash_url: None,
                    },
                    bitrate: Some(current_bandwidth),
                    resolution: None,
                    codec: None,
                    cdn: None,
                });
            }
        }

        // 按码率降序排序
        streams.sort_by(|a, b| b.bitrate.cmp(&a.bitrate));

        Ok(streams)
    }
}

impl Default for SoopHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformHandler for SoopHandler {
    fn platform_name(&self) -> &'static str {
        "soop"
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec![
            "sooplive.co.kr",
            "sooplive.com",
            "play.sooplive.co.kr",
            "afreecatv.com", // 旧域名兼容
        ]
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
        tracing::info!("🎮 正在获取 SOOP 直播间信息: {}", room_id);

        let cookie_str = cookies.cookie.as_deref();

        // 先尝试国际版
        match self.get_global_stream_data(room_id, cookie_str).await {
            Ok(info)
                if info.room.status == LiveStatus::Live || !info.room.anchor_name.is_empty() =>
            {
                tracing::info!("✅ SOOP 直播间信息获取成功 (Global)");
                return Ok(info);
            }
            Err(e) => {
                tracing::debug!("⚠️ SOOP Global API 失败: {}", e);
            }
            _ => {}
        }

        // 尝试韩国版
        match self.get_kr_stream_data(room_id, cookie_str).await {
            Ok(info) => {
                tracing::info!("✅ SOOP 直播间信息获取成功 (KR)");
                Ok(info)
            }
            Err(e) => {
                tracing::error!("❌ SOOP 直播间信息获取失败: {}", e);
                Err(e)
            }
        }
    }
}
