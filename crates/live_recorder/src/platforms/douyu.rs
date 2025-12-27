//! 斗鱼直播平台处理器
//!
//! 使用 Douyu 加密接口生成签名参数

use async_trait::async_trait;
use md5::{Digest, Md5};
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use serde_json;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:124.0) Gecko/20100101 Firefox/124.0",
            )
            .gzip(true) // 自动解压 gzip
            .deflate(true) // 自动解压 deflate
            .brotli(true) // 自动解压 brotli
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    fn now_unix_secs() -> RecorderResult<u64> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| RecorderError::PlatformError {
                platform: "douyu".to_string(),
                message: format!("SystemTime error: {}", e),
            })
            .map(|d| d.as_secs())
    }

    /// 获取房间信息
    async fn fetch_room_info(&self, room_id: &str) -> RecorderResult<(LiveRoomInfo, bool)> {
        let url = format!("https://www.douyu.com/betard/{}", room_id);

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json, text/plain, */*")
            .header("Referer", format!("https://www.douyu.com/{}", room_id))
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        let room = &json["room"];
        let anchor_name = room["nickname"].as_str().unwrap_or("Unknown").to_string();

        let room_name = room["room_name"].as_str().unwrap_or("").to_string();

        let show_status = room["show_status"].as_i64().unwrap_or(0);

        let videoloop = room["videoLoop"].as_i64().unwrap_or(0);

        // show_status == 1 且 videoLoop == 0 表示正在直播
        let is_live = show_status == 1 && videoloop == 0;

        let room_info = LiveRoomInfo {
            room_id: room_id.to_string(),
            anchor_name,
            title: room_name,
            status: if is_live {
                LiveStatus::Live
            } else {
                LiveStatus::Offline
            },
            start_time: None,
            viewer_count: room["online_num"].as_u64(),
            cover_url: room["room_pic"].as_str().map(|s| s.to_string()),
            extra: HashMap::new(),
        };

        Ok((room_info, is_live))
    }

    /// MD5 哈希
    fn md5_hash(data: &str) -> String {
        let mut hasher = Md5::new();
        hasher.update(data.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// 通过 Douyu 新加密接口生成参数（替代网页 JS 解析/执行）
    async fn get_sign_params(
        &self,
        room_id: &str,
        did: &str,
    ) -> RecorderResult<HashMap<String, String>> {
        #[derive(Debug, Deserialize)]
        struct EncryptionResponse {
            error: i64,
            data: Option<EncryptionData>,
            msg: Option<String>,
        }

        #[derive(Debug, Deserialize)]
        struct EncryptionData {
            enc_data: String,
            rand_str: String,
            key: String,
            enc_time: u32,
            is_special: Option<u32>,
        }

        let key_url = format!(
            "https://www.douyu.com/wgapi/livenc/liveweb/websec/getEncryption?did={}",
            did
        );

        let response = self
            .client
            .get(&key_url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            )
            .header("Referer", format!("https://www.douyu.com/{}", room_id))
            .header("Accept", "application/json, text/plain, */*")
            .send()
            .await?;

        let json: EncryptionResponse = response.json().await?;
        if json.error != 0 {
            let msg = json.msg.unwrap_or_else(|| "Unknown error".to_string());
            return Err(RecorderError::StreamNotAvailable(format!(
                "Douyu encryption API error {}: {}",
                json.error, msg
            )));
        }

        let enc = json.data.ok_or_else(|| {
            RecorderError::InvalidResponseFormat("Douyu encryption API missing data".to_string())
        })?;

        let EncryptionData {
            enc_data,
            rand_str,
            key,
            enc_time,
            is_special,
        } = enc;

        let ts = Self::now_unix_secs()?;
        let sign_str = match is_special {
            Some(1) => String::new(),
            _ => format!("{}{}", room_id, ts),
        };

        let mut auth = rand_str;
        for _ in 0..enc_time {
            auth = Self::md5_hash(&format!("{}{}", auth, key));
        }
        auth = Self::md5_hash(&format!("{}{}{}", auth, key, sign_str));

        let mut result = HashMap::new();
        result.insert("enc_data".to_string(), enc_data);
        result.insert("tt".to_string(), ts.to_string());
        result.insert("did".to_string(), did.to_string());
        result.insert("auth".to_string(), auth);
        Ok(result)
    }

    /// 获取直播流
    async fn get_live_stream(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        // 先获取房间信息
        let (room_info, is_live) = self.fetch_room_info(room_id).await?;

        if !is_live {
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        // 使用固定的 did (设备 ID) - 与 Python 实现一致
        let did = "10000000000000000000000000003306";

        // 获取签名参数
        let sign_params = self.get_sign_params(room_id, &did).await?;

        let enc_data = sign_params.get("enc_data").cloned().unwrap_or_default();
        let tt = sign_params.get("tt").cloned().unwrap_or_default();
        let auth = sign_params.get("auth").cloned().unwrap_or_default();

        if enc_data.is_empty() || tt.is_empty() || auth.is_empty() {
            return Err(RecorderError::StreamNotAvailable(
                "Failed to get Douyu sign params".to_string(),
            ));
        }

        // 构建请求参数（application/x-www-form-urlencoded）
        let api_url = format!("https://www.douyu.com/lapi/live/getH5PlayV1/{}", room_id);
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("enc_data", &enc_data)
            .append_pair("tt", &tt)
            .append_pair("did", did)
            .append_pair("auth", &auth)
            .append_pair("cdn", "")
            .append_pair("rate", "-1")
            .append_pair("hevc", "0")
            .append_pair("fa", "0")
            .append_pair("ive", "0")
            .finish();

        tracing::debug!("🚀 请求斗鱼 API: {}", api_url);
        tracing::debug!("📨 请求参数: {}", body);

        let response = self
            .client
            .post(&api_url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            )
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Referer", format!("https://www.douyu.com/{}", room_id))
            .header("Origin", "https://www.douyu.com")
            .body(body)
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        tracing::debug!("📦 API 响应: {:?}", json);

        // 检查响应
        let error_code = json["error"].as_i64().unwrap_or(-1);
        if error_code != 0 {
            let msg = json["msg"].as_str().unwrap_or("Unknown error");
            return Err(RecorderError::StreamNotAvailable(format!(
                "Douyu API error {}: {}",
                error_code, msg
            )));
        }

        let data = &json["data"];

        // 解析流地址
        let rtmp_url = data["rtmp_url"].as_str().unwrap_or("");
        let rtmp_live = data["rtmp_live"].as_str().unwrap_or("");

        if rtmp_url.is_empty() || rtmp_live.is_empty() {
            return Err(RecorderError::StreamNotAvailable(
                "No stream URL in response".to_string(),
            ));
        }

        // 构建 FLV 流地址
        let flv_url = format!("{}/{}", rtmp_url, rtmp_live);

        // 尝试获取 HLS 流
        let hls_url = data["hls_url"].as_str().map(|s| s.to_string());

        tracing::info!("✅ 获取到斗鱼直播流: {}", flv_url);

        // 构建 StreamData
        let stream = StreamData {
            quality: VideoQuality::Original,
            url: StreamUrl {
                flv_url: Some(flv_url),
                hls_url,
                dash_url: None,
            },
            bitrate: None,
            resolution: None,
            codec: Some("h264".to_string()),
            cdn: None,
        };

        Ok(StreamInfo {
            room: room_info,
            streams: vec![stream],
        })
    }
}

#[async_trait]
impl PlatformHandler for DouyuHandler {
    fn platform_name(&self) -> &'static str {
        "douyu"
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["douyu.com"]
    }

    async fn extract_room_id(&self, url: &str) -> RecorderResult<String> {
        // 支持的格式:
        // https://www.douyu.com/123456
        // https://www.douyu.com/topic/xxxxx?rid=123456

        let parsed =
            url::Url::parse(url).map_err(|_| RecorderError::InvalidUrlFormat(url.to_string()))?;

        // 优先从查询参数获取
        if let Some(rid) = parsed
            .query_pairs()
            .find(|(k, _)| k == "rid")
            .map(|(_, v)| v.to_string())
        {
            return Ok(rid);
        }

        // 从路径获取（优先取第一个段，避免 /topic/xxx 等形式误判）
        let path = parsed.path().trim_matches('/');
        let first_segment = path.split('/').next().unwrap_or("");

        // 数字房间号
        if !first_segment.is_empty() && first_segment.chars().all(|c| c.is_ascii_digit()) {
            return Ok(first_segment.to_string());
        }

        // 兼容：短链/房间别名（非数字），从移动端页面 pageContext 解析真实 rid
        if !first_segment.is_empty() {
            let mobile_url = format!("https://m.douyu.com/{}", first_segment);
            let html = self
                .client
                .get(&mobile_url)
                .header(
                    "User-Agent",
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
                )
                .header(
                    "Accept",
                    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                )
                .send()
                .await?
                .text()
                .await?;

            let re_ctx = Regex::new(
                r#"(?s)<script id="vike_pageContext" type="application/json">(.*?)</script>"#,
            )
            .map_err(|e| RecorderError::InvalidResponseFormat(format!("Regex error: {}", e)))?;

            if let Some(json_str) = re_ctx
                .captures(&html)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str())
            {
                let ctx_json: serde_json::Value = serde_json::from_str(json_str)?;
                if let Some(rid) = ctx_json
                    .pointer("/pageProps/room/roomInfo/roomInfo/rid")
                    .and_then(|v| {
                        v.as_i64()
                            .map(|n| n.to_string())
                            .or_else(|| v.as_str().map(|s| s.to_string()))
                    })
                {
                    if !rid.is_empty() {
                        return Ok(rid);
                    }
                }
            }
        }

        Err(RecorderError::InvalidUrlFormat(format!(
            "Cannot extract room ID from URL: {}",
            url
        )))
    }

    async fn get_stream_info(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        self.get_live_stream(room_id).await
    }
}

impl Default for DouyuHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试斗鱼房间信息获取
    #[tokio::test]
    async fn test_fetch_room_info() {
        let handler = DouyuHandler::new();
        // 使用一个热门房间测试
        let result = handler.fetch_room_info("4624967").await;
        println!("房间信息: {:?}", result);
        assert!(result.is_ok());
    }

    /// 测试签名参数生成
    #[tokio::test]
    async fn test_get_sign_params() {
        let handler = DouyuHandler::new();
        let did = "10000000000000000000000000003306";
        let result = handler.get_sign_params("4624967", did).await;
        println!("签名参数: {:?}", result);

        match &result {
            Ok(params) => {
                println!("enc_data = {:?}", params.get("enc_data"));
                println!("did = {:?}", params.get("did"));
                println!("tt = {:?}", params.get("tt"));
                println!("auth = {:?}", params.get("auth"));

                // 验证必要参数存在
                assert!(params.contains_key("enc_data"), "缺少 enc_data 参数");
                assert!(params.contains_key("tt"), "缺少 tt 参数");
                assert!(params.contains_key("auth"), "缺少 auth 参数");
            }
            Err(e) => {
                println!("错误: {:?}", e);
            }
        }

        assert!(result.is_ok());
    }

    /// 测试获取直播流
    #[tokio::test]
    async fn test_get_stream_info() {
        let handler = DouyuHandler::new();
        let result = handler.get_stream_info("4624967").await;
        println!("直播流信息: {:?}", result);

        match &result {
            Ok(info) => {
                println!("房间ID: {}", info.room.room_id);
                println!("主播: {}", info.room.anchor_name);
                println!("标题: {}", info.room.title);
                println!("状态: {:?}", info.room.status);
                println!("流数量: {}", info.streams.len());

                for (i, stream) in info.streams.iter().enumerate() {
                    println!("流 {}: {:?}", i, stream);
                }
            }
            Err(e) => {
                println!("错误: {:?}", e);
            }
        }
    }

    /// 测试房间 ID 提取
    #[tokio::test]
    async fn test_extract_room_id() {
        let handler = DouyuHandler::new();

        // 测试各种 URL 格式
        let test_cases = vec![
            ("https://www.douyu.com/4624967", "4624967"),
            ("https://www.douyu.com/topic/xxxxx?rid=123456", "123456"),
            ("https://douyu.com/9999", "9999"),
        ];

        for (url, expected) in test_cases {
            let result = handler.extract_room_id(url).await;
            println!("URL: {} => {:?}", url, result);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), expected);
        }
    }
}
