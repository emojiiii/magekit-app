use async_trait::async_trait;
use reqwest::{Client, Proxy};
use serde_json;
use std::collections::HashMap;
use std::time::Duration;
use url::Url;

use crate::{
    error::{RecorderError, RecorderResult},
    platforms::PlatformHandler,
    types::{LiveRoomInfo, LiveStatus, StreamData, StreamInfo, StreamUrl, VideoQuality},
};

/// 抖音直播处理器
pub struct DouyinHandler {
    client: Client,
}

impl DouyinHandler {
    pub fn new() -> Self {
        // reqwest 默认启用 gzip/deflate 解压缩
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.0.0 Safari/537.36")
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// 设置代理
    pub fn with_proxy(mut self, proxy_url: &str) -> RecorderResult<Self> {
        let proxy = Proxy::all(proxy_url)
            .map_err(|e| RecorderError::ProxyError(e.to_string()))?;

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36")
            .proxy(proxy)
            .build()
            .map_err(|e| RecorderError::ProxyError(e.to_string()))?;

        self.client = client;
        Ok(self)
    }

    /// 获取房间ID和用户sec_user_id
    async fn get_sec_user_id(&self, url: &str) -> RecorderResult<(String, String)> {
        let response = self.client
            .get(url)
            .header("Referer", "https://www.douyin.com/")
            .send()
            .await?;

        let final_url = response.url().to_string();

        if !final_url.contains("reflow/") {
            return Err(RecorderError::InvalidUrlFormat(
                "The redirect URL does not contain 'reflow/'".to_string()
            ));
        }

        let parsed_url = Url::parse(&final_url)?;
        let path_parts: Vec<&str> = parsed_url.path().split('/').collect();

        if path_parts.len() < 2 {
            return Err(RecorderError::InvalidUrlFormat(
                "Cannot extract room ID from URL".to_string()
            ));
        }

        let room_id = path_parts[path_parts.len() - 1].to_string();

        let query_pairs: HashMap<String, String> = parsed_url
            .query_pairs()
            .into_owned()
            .collect();

        let sec_user_id = query_pairs
            .get("sec_user_id")
            .ok_or_else(|| RecorderError::InvalidUrlFormat(
                "Cannot find sec_user_id in URL".to_string()
            ))?
            .clone();

        Ok((room_id, sec_user_id))
    }

    /// 获取直播间web_rid
    async fn get_live_room_id(&self, room_id: &str, sec_user_id: &str) -> RecorderResult<String> {
        let mut params = HashMap::new();
        params.insert("verifyFp", "verify_lk07kv74_QZYCUApD_xhiB_405x_Ax51_GYO9bUIyZQVf");
        params.insert("type_id", "0");
        params.insert("live_id", "1");
        params.insert("room_id", room_id);
        params.insert("sec_user_id", sec_user_id);
        params.insert("app_id", "1128");

        // 生成msToken
        let ms_token = self.generate_ms_token();
        params.insert("msToken", &ms_token);

        let query_string = Self::build_query_string(&params);

        // 生成 a_bogus 签名（使用 ab_sign）
        let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";
        let a_bogus = xbogus::ab_sign(&query_string, user_agent);

        let api_url = format!(
            "https://webcast.amemv.com/webcast/room/reflow/info/?{}&a_bogus={}",
            query_string,
            a_bogus
        );

        let response = self.client
            .get(&api_url)
            .header("User-Agent", "Mozilla/5.0 (Linux; Android 11; SAMSUNG SM-G973U) AppleWebKit/537.36 (KHTML, like Gecko) SamsungBrowser/14.2 Chrome/87.0.4280.141 Mobile Safari/537.36")
            .header("Accept-Language", "zh-CN,zh;q=0.8,zh-TW;q=0.7,zh-HK;q=0.5,en-US;q=0.3,en;q=0.2")
            .header("Cookie", "s_v_web_id=verify_lk07kv74_QZYCUApD_xhiB_405x_Ax51_GYO9bUIyZQVf")
            .send()
            .await?;

        let json_text = response.text().await?;
        let json_response: serde_json::Value = serde_json::from_str(&json_text)
            .map_err(|e| RecorderError::JsonError(e))?;

        if let Some(data) = json_response.get("data")
            .and_then(|d| d.get("room"))
            .and_then(|r| r.get("owner"))
            .and_then(|o| o.get("web_rid")) {

            Ok(data.as_str().unwrap_or("").to_string())
        } else {
            Err(RecorderError::RoomNotFound(format!("Room {} not found", room_id)))
        }
    }

    /// 获取直播流数据
    async fn get_live_stream_data(&self, web_rid: &str) -> RecorderResult<serde_json::Value> {
        // 按照 Python 版本的参数顺序构建
        let params_ordered = vec![
            ("aid", "6383"),
            ("app_name", "douyin_web"),
            ("live_id", "1"),
            ("device_platform", "web"),
            ("language", "zh-CN"),
            ("browser_language", "zh-CN"),
            ("browser_platform", "Win32"),
            ("browser_name", "Chrome"),
            ("browser_version", "116.0.0.0"),
            ("web_rid", web_rid),
            ("msToken", ""),
        ];

        let query_string: String = params_ordered
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect::<Vec<String>>()
            .join("&");

        // User-Agent 必须与签名使用的一致
        let user_agent = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.5845.97 Safari/537.36 Core/1.116.567.400 QQBrowser/19.7.6764.400";
        
        // 生成 a_bogus 签名（使用 ab_sign）
        tracing::debug!("🔑 Query string: {}", query_string);
        let a_bogus = xbogus::ab_sign(&query_string, user_agent);
        tracing::debug!("🔑 Generated a_bogus: {}", a_bogus);

        let api_url = format!(
            "https://live.douyin.com/webcast/room/web/enter/?{}&a_bogus={}",
            query_string,
            a_bogus
        );

        tracing::info!("🌐 正在请求抖音直播API: {}", api_url);
        
        // 使用 Python spider.py 中的简化 Cookie
        let cookie = "ttwid=1%7C2iDIYVmjzMcpZ20fcaFde0VghXAA3NaNXE_SLR68IyE%7C1761045455%7Cab35197d5cfb21df6cbb2fa7ef1c9262206b062c315b9d04da746d0b37dfbc7d";
        
        let response = self.client
            .get(&api_url)
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Cookie", cookie)
            .header("Referer", format!("https://live.douyin.com/{}", web_rid))
            .header("User-Agent", user_agent)
            .send()
            .await?;

        let status = response.status();
        let headers = response.headers().clone();
        tracing::info!("📡 API响应状态: {}", status);
        tracing::debug!("📋 响应Headers: {:?}", headers);
        
        // 检查 Content-Encoding
        if let Some(encoding) = headers.get("content-encoding") {
            tracing::debug!("📦 Content-Encoding: {:?}", encoding);
        }
        if let Some(content_type) = headers.get("content-type") {
            tracing::debug!("📝 Content-Type: {:?}", content_type);
        }
        if let Some(content_length) = headers.get("content-length") {
            tracing::debug!("📏 Content-Length header: {:?}", content_length);
        }

        let bytes = response.bytes().await?;
        tracing::info!("📄 API响应字节数: {} bytes", bytes.len());
        
        if bytes.is_empty() {
            tracing::error!("❌ API返回空内容");
            tracing::error!("💡 可能原因:");
            tracing::error!("   1. 直播间不存在或已关闭");
            tracing::error!("   2. Cookie 或签名无效，触发风控");
            tracing::error!("   3. IP被限制或需要验证");
            tracing::error!("   4. 请尝试更换直播间URL或更新Cookie");
            return Err(RecorderError::StreamNotAvailable(format!("API返回空内容，房间号: {}", web_rid)));
        }

        let json_text = String::from_utf8_lossy(&bytes).to_string();
        tracing::debug!("📄 API响应内容前200字符: {}", &json_text[..json_text.len().min(200)]);
        
        if json_text.len() < 500 {
            tracing::debug!("📄 完整响应: {}", json_text);
        } else {
            tracing::debug!("📄 响应前200字符: {}", &json_text[..200.min(json_text.len())]);
        }

        if json_text.is_empty() {
            tracing::error!("❌ API返回空内容");
            return Err(RecorderError::StreamNotAvailable(format!("API返回空内容，房间号: {}", web_rid)));
        }

        let json_response: serde_json::Value = serde_json::from_str(&json_text)
            .map_err(|e| {
                tracing::error!("❌ JSON解析失败: {}，内容: {}", e, &json_text[..json_text.len().min(200)]);
                RecorderError::JsonError(e)
            })?;

        if let Some(_data) = json_response.get("data") {
            tracing::info!("✅ 成功获取直播间数据");
            Ok(json_response)
        } else {
            tracing::error!("❌ API响应缺少data字段");
            Err(RecorderError::StreamNotAvailable(format!("Stream data not available for room {}", web_rid)))
        }
    }

    /// 生成msToken
    fn generate_ms_token(&self) -> String {
        use base64::{Engine as _, engine::general_purpose};
        use rand::{thread_rng, Rng};

        let mut rng = thread_rng();
        let bytes: Vec<u8> = (0..128).map(|_| rng.gen()).collect();
        format!("{}==", general_purpose::STANDARD.encode(&bytes))
    }

    /// 构建查询字符串
    fn build_query_string(params: &HashMap<&str, &str>) -> String {
        params
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// 解析流URL
    fn parse_stream_urls(json_data: &serde_json::Value) -> RecorderResult<(Vec<StreamData>, LiveRoomInfo)> {
        let data = json_data.get("data")
            .ok_or_else(|| RecorderError::InvalidResponseFormat("Missing data field".to_string()))?;

        // 抖音 web API 返回的结构是 data.data[0]，而不是 data.room
        let room_info = data.get("data")
            .and_then(|d| d.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| RecorderError::InvalidResponseFormat("Missing room data in data.data[0]".to_string()))?;

        // 主播名字在 data.user.nickname
        let anchor_name = data.get("user")
            .and_then(|u| u.get("nickname"))
            .and_then(|n| n.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let title = room_info.get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        let status_value = room_info.get("status")
            .and_then(|s| s.as_u64())
            .unwrap_or(4);

        let status = match status_value {
            2 => LiveStatus::Live,
            4 => LiveStatus::Offline,
            _ => LiveStatus::Unknown,
        };

        let live_room_info = LiveRoomInfo {
            room_id: room_info.get("id_str")
                .and_then(|id| id.as_str())
                .unwrap_or("")
                .to_string(),
            anchor_name,
            title,
            status: status.clone(),
            start_time: None,
            viewer_count: room_info.get("user_count")
                .and_then(|uc| uc.as_u64()),
            cover_url: room_info.get("cover")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|url_list| url_list.as_str())
                .map(|s| s.to_string()),
            extra: HashMap::new(),
        };

        if status != LiveStatus::Live {
            return Ok((vec![], live_room_info));
        }

        // 解析流URL - 在 room_info.stream_url 中
        let stream_url = room_info.get("stream_url")
            .ok_or_else(|| RecorderError::StreamNotAvailable("No stream URL available".to_string()))?;

        let flv_pull_url = stream_url.get("flv_pull_url")
            .and_then(|flv| flv.as_object());

        let hls_pull_url_map = stream_url.get("hls_pull_url_map")
            .and_then(|hls| hls.as_object());

        let mut streams = Vec::new();

        // 质量映射
        let quality_names = ["OD", "UHD", "HD", "SD", "LD"];
        let quality_enums = [
            VideoQuality::Original,
            VideoQuality::Ultra,
            VideoQuality::High,
            VideoQuality::Standard,
            VideoQuality::Low,
        ];

        for (quality_name, quality_enum) in quality_names.iter().zip(quality_enums.iter()) {
            let flv_url = flv_pull_url
                .and_then(|flv| flv.get(*quality_name))
                .and_then(|url| url.as_str())
                .map(|s| s.to_string());

            let hls_url = hls_pull_url_map
                .and_then(|hls| hls.get(*quality_name))
                .and_then(|url| url.as_str())
                .map(|s| s.to_string());

            if flv_url.is_some() || hls_url.is_some() {
                streams.push(StreamData {
                    quality: quality_enum.clone(),
                    url: StreamUrl {
                        hls_url,
                        flv_url,
                        dash_url: None,
                    },
                    bitrate: None,
                    resolution: None,
                    codec: None,
                    cdn: None,
                });
            }
        }

        if streams.is_empty() {
            return Err(RecorderError::StreamNotAvailable("No valid stream URLs found".to_string()));
        }

        Ok((streams, live_room_info))
    }
}

#[async_trait]
impl PlatformHandler for DouyinHandler {
    fn platform_name(&self) -> &'static str {
        "douyin"
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec![
            "v.douyin.com",
            "live.douyin.com",
            "douyin.com",
            "iesdouyin.com",
        ]
    }

    async fn extract_room_id(&self, url: &str) -> RecorderResult<String> {
        tracing::info!("🔍 开始解析抖音直播间URL: {}", url);
        
        // 如果是直播间链接，直接提取web_rid
        if url.contains("live.douyin.com") {
            let parsed_url = Url::parse(url)?;
            let path_parts: Vec<&str> = parsed_url.path().split('/').collect();
            if let Some(web_rid) = path_parts.last() {
                let room_id = web_rid.to_string();
                tracing::info!("✅ 从直播间URL提取房间ID: {}", room_id);
                return Ok(room_id);
            }
        }

        tracing::info!("🔄 检测到非直播间URL，尝试通过短链接解析...");
        // 如果是短视频链接，需要跳转到直播间
        let (room_id, sec_user_id) = self.get_sec_user_id(url).await?;
        let web_rid = self.get_live_room_id(&room_id, &sec_user_id).await?;
        Ok(web_rid)
    }

    async fn get_stream_info(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        let stream_data = self.get_live_stream_data(room_id).await?;
        let (streams, room) = Self::parse_stream_urls(&stream_data)?;

        Ok(StreamInfo {
            room,
            streams,
        })
    }
}