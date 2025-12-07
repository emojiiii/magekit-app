//! 虎牙直播平台处理器

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use md5::{Digest, Md5};
use rand::Rng;
use regex::Regex;
use reqwest::Client;
use serde_json;
use std::collections::HashMap;
use std::time::Duration;
use url::form_urlencoded;

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
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:123.0) Gecko/20100101 Firefox/123.0",
            )
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// 重新生成 anti-code（参考 py_demo 的实现）
    /// 虎牙的流 URL 需要重新生成 anti-code 才能正常访问
    fn generate_anti_code(&self, old_anti_code: &str, stream_name: &str) -> String {
        let params_t = 100;
        let sdk_version = 2403051612u64;

        // sdk_id 是 13 位数毫秒级时间戳
        let t13 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let sdk_sid = t13;

        // 计算 uuid 和 uid 参数值
        let init_uuid = ((t13 % 10_000_000_000u64 * 1000)
            + (rand::thread_rng().gen_range(0..1000)))
            % 4294967295u64;
        let uid: u64 = rand::thread_rng().gen_range(1400000000000u64..1400009999999u64);
        let seq_id = uid + sdk_sid;

        // 计算 ws_time 参数值（16进制）
        let target_unix_time = (t13 + 110624) / 1000;
        let ws_time = format!("{:x}", target_unix_time).to_lowercase();

        // 解析旧的 anti-code 参数
        let query_params: HashMap<String, String> =
            form_urlencoded::parse(old_anti_code.as_bytes())
                .into_owned()
                .collect();

        let fm = query_params.get("fm").map(|s| s.as_str()).unwrap_or("");
        let ctype = query_params
            .get("ctype")
            .map(|s| s.as_str())
            .unwrap_or("tars_mp");
        let fs = query_params.get("fs").map(|s| s.as_str()).unwrap_or("bgct");

        // fm 参数值是经过 URL 编码然后 base64 编码的
        // 解码后类似 DWq8BcJ3h6DJt6TY_$0_$1_$2_$3
        let fm_decoded = urlencoding::decode(fm).unwrap_or_default();
        let ws_secret_pf = match BASE64.decode(fm_decoded.as_bytes()) {
            Ok(decoded) => String::from_utf8_lossy(&decoded)
                .split('_')
                .next()
                .unwrap_or("")
                .to_string(),
            Err(_) => String::new(),
        };

        // 计算 wsSecret
        let ws_secret_hash_input = format!("{}|{}|{}", seq_id, ctype, params_t);
        let mut hasher1 = Md5::new();
        hasher1.update(ws_secret_hash_input.as_bytes());
        let ws_secret_hash = format!("{:x}", hasher1.finalize());

        let ws_secret_input = format!(
            "{}_{}_{}_{}_{}",
            ws_secret_pf, uid, stream_name, ws_secret_hash, ws_time
        );
        let mut hasher2 = Md5::new();
        hasher2.update(ws_secret_input.as_bytes());
        let ws_secret_md5 = format!("{:x}", hasher2.finalize());

        // 构建新的 anti-code
        let new_anti_code = format!(
            "wsSecret={}&wsTime={}&seqid={}&ctype={}&ver=1&fs={}&uuid={}&u={}&t={}&sv={}&sdk_sid={}&codec=264",
            ws_secret_md5, ws_time, seq_id, ctype, fs, init_uuid, uid, params_t, sdk_version, sdk_sid
        );

        tracing::debug!("🔑 生成新的 anti-code: {}", new_anti_code);
        new_anti_code
    }

    /// 从微信小程序 API 获取流信息（更稳定）
    async fn get_stream_from_wx_api(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        let params = format!("m=Live&do=profileRoom&roomid={}&showSecret=1", room_id);
        let url = format!("https://mp.huya.com/cache.php?{}", params);

        let response = self
            .client
            .get(&url)
            .header(
                "User-Agent",
                "ios/7.830 (ios 17.0; ; iPhone 15 (A2846/A3089/A3090/A3092))",
            )
            .header("xweb_xhr", "1")
            .header(
                "referer",
                "https://servicewechat.com/wx74767bf0b684f7d3/301/page-frame.html",
            )
            .header("accept-language", "zh-CN,zh;q=0.9")
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        tracing::debug!("📋 虎牙 API 响应: {:?}", json);

        let anchor_name = json["data"]["profileInfo"]["nick"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string();

        let live_status = json["data"]["realLiveStatus"].as_str().unwrap_or("OFF");

        let title = json["data"]["liveData"]["introduction"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // 获取封面图 - 优先使用直播截图，其次使用头像
        let cover_url = json["data"]["liveData"]["screenshot"]
            .as_str()
            .or_else(|| json["data"]["liveData"]["screenShot"].as_str())
            .or_else(|| json["data"]["profileInfo"]["avatar"].as_str())
            .or_else(|| json["data"]["profileInfo"]["avatar180"].as_str())
            .map(|s| {
                // 确保使用 https
                if s.starts_with("//") {
                    format!("https:{}", s)
                } else if !s.starts_with("http") {
                    format!("https://{}", s)
                } else {
                    s.to_string()
                }
            });

        tracing::info!(
            "📺 虎牙 主播: {}, 标题: {}, 封面: {:?}",
            anchor_name,
            title,
            cover_url.is_some()
        );

        let room_info = LiveRoomInfo {
            room_id: room_id.to_string(),
            anchor_name,
            title,
            status: if live_status == "ON" {
                LiveStatus::Live
            } else {
                LiveStatus::Offline
            },
            start_time: None,
            viewer_count: None,
            cover_url,
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

                        // 重新生成 anti-code（关键修复）
                        let new_flv_anti_code = self.generate_anti_code(flv_anti_code, stream_name);
                        let new_hls_anti_code = self.generate_anti_code(hls_anti_code, stream_name);

                        let flv_url =
                            format!("{}/{}.flv?{}", flv_url_base, stream_name, new_flv_anti_code);
                        let mut m3u8_url = format!(
                            "{}/{}.m3u8?{}",
                            hls_url_base, stream_name, new_hls_anti_code
                        );

                        // 如果是 TX CDN，进行特殊处理（不再需要，因为新的 anti-code 已经设置了正确的 ctype）
                        // 但保留 HTTPS 转换

                        // 确保使用 HTTPS
                        let flv_url = if flv_url.starts_with("http://") {
                            flv_url.replace("http://", "https://")
                        } else if !flv_url.starts_with("https://") {
                            format!("https://{}", flv_url)
                        } else {
                            flv_url
                        };

                        // m3u8 也要使用 HTTPS
                        m3u8_url = if m3u8_url.starts_with("http://") {
                            m3u8_url.replace("http://", "https://")
                        } else if !m3u8_url.starts_with("https://") {
                            format!("https://{}", m3u8_url)
                        } else {
                            m3u8_url
                        };

                        tracing::info!("🔗 虎牙 FLV URL: {}", flv_url);
                        tracing::info!("🔗 虎牙 M3U8 URL: {}", m3u8_url);

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

        let response = self
            .client
            .get(&url)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header("Accept-Language", "zh-CN,zh;q=0.8")
            .send()
            .await?;

        let html = response.text().await?;

        // 检查是否存在流数据
        let re = Regex::new(r#"stream: (\{"data".*?),"iWebDefaultBitRate""#)
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;

        if let Some(captures) = re.captures(&html) {
            let json_str = format!("{}}}", &captures[1]);
            let json: serde_json::Value =
                serde_json::from_str(&json_str).map_err(|e| RecorderError::JsonError(e))?;

            tracing::debug!("📋 虎牙网页流数据: {:?}", json);

            // 从网页解析主播信息
            let anchor_re = Regex::new(r#""sNick":"([^"]+)""#).ok();
            let anchor_name = anchor_re
                .and_then(|re| re.captures(&html))
                .map(|c| c[1].to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            // 检查直播状态
            let is_live = json["data"]
                .as_array()
                .map(|arr| !arr.is_empty())
                .unwrap_or(false);

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

            // TODO: 解析网页中的流 URL
            // 网页方式比较复杂，优先使用微信 API

            Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            })
        } else {
            Err(RecorderError::StreamNotAvailable(
                "Cannot find stream data in page".to_string(),
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
        vec!["huya.com", "www.huya.com"]
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
                "Invalid Huya room ID".to_string(),
            ));
        }

        // 如果是字母（别名），需要从页面获取真实房间号
        if room_id.chars().any(|c| c.is_alphabetic()) {
            // 从网页获取真实房间号
            let url = format!("https://www.huya.com/{}", room_id);
            let response = self.client.get(&url).send().await?;

            let html = response.text().await?;

            let re = Regex::new(r#"ProfileRoom":(\d+),"sPrivateHost"#)
                .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;

            if let Some(captures) = re.captures(&html) {
                return Ok(captures[1].to_string());
            }

            return Err(RecorderError::InvalidUrlFormat(
                "Cannot find real room ID".to_string(),
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
