//! 斗鱼直播平台处理器
//!
//! 使用 QuickJS 执行从网页提取的 JS 代码来生成签名

use async_trait::async_trait;
use md5::{Digest, Md5};
use regex::Regex;
use reqwest::Client;
use rquickjs::{CatchResultExt, Context, Function, Runtime};
use serde_json;
use std::collections::HashMap;
use std::time::Duration;

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

    /// 从网页提取 JS 并生成签名参数
    async fn get_sign_params(
        &self,
        room_id: &str,
        did: &str,
    ) -> RecorderResult<HashMap<String, String>> {
        // 1. 请求房间页面
        let url = format!("https://www.douyu.com/{}", room_id);

        tracing::debug!("🌐 请求斗鱼房间页面: {}", url);

        let html = self
            .client
            .get(&url)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .send()
            .await?
            .text()
            .await?;

        tracing::debug!("📄 获取到 HTML, 长度: {}", html.len());

        // 2. 正则提取 JS 代码
        // 匹配 var xxx=[0x... 开头到 function ub98484234 ... 到下一个 function
        // 注意: Douyu 使用随机变量名如 d8b2bedb187f87c
        let re_js = Regex::new(
            r"(var [a-zA-Z0-9_]+=\[0x[a-f0-9]+[\s\S]*?function ub98484234[\s\S]*?)function",
        )
        .map_err(|e| RecorderError::InvalidResponseFormat(format!("Regex error: {}", e)))?;

        let js_code = re_js
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .ok_or_else(|| {
                RecorderError::InvalidResponseFormat("Cannot find JS code in page".to_string())
            })?;

        tracing::debug!("📜 提取到 JS 代码, 长度: {}", js_code.len());

        // 3. 替换 eval 语句: eval(strc)(params);} -> strc;}
        // 实际格式: eval(strc)(d8b2bedb187f87c0,d8b2bedb187f87c1,d8b2bedb187f87c2);}
        let re_eval = Regex::new(r"eval\(strc\)\([^)]*\);\}")
            .map_err(|e| RecorderError::InvalidResponseFormat(format!("Regex error: {}", e)))?;
        let func_ub9 = re_eval.replace_all(js_code, "strc;}").to_string();

        // 4. 使用 QuickJS 执行 ub98484234 函数
        let runtime = Runtime::new().map_err(|e| {
            RecorderError::JavaScriptError(format!("Failed to create JS runtime: {}", e))
        })?;
        let context = Context::full(&runtime).map_err(|e| {
            RecorderError::JavaScriptError(format!("Failed to create JS context: {}", e))
        })?;

        // 执行第一段 JS
        let res: String = context.with(|ctx| {
            // 先执行函数定义
            ctx.eval::<(), _>(func_ub9.as_str())
                .catch(&ctx)
                .map_err(|e| {
                    RecorderError::JavaScriptError(format!("Failed to eval JS: {:?}", e))
                })?;

            // 调用 ub98484234
            let func: Function = ctx.globals().get("ub98484234").catch(&ctx).map_err(|e| {
                RecorderError::JavaScriptError(format!("Failed to get ub98484234: {:?}", e))
            })?;

            let result: String = func.call(()).catch(&ctx).map_err(|e| {
                RecorderError::JavaScriptError(format!("Failed to call ub98484234: {:?}", e))
            })?;

            Ok::<String, RecorderError>(result)
        })?;

        tracing::debug!("📋 ub98484234 返回: {}...", &res[..res.len().min(100)]);

        // 5. 获取时间戳和 v 参数
        let t10 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string();

        let re_v = Regex::new(r"v=(\d+)")
            .map_err(|e| RecorderError::InvalidResponseFormat(format!("Regex error: {}", e)))?;
        let v = re_v
            .captures(&res)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .ok_or_else(|| {
                RecorderError::InvalidResponseFormat("Cannot find v parameter".to_string())
            })?;

        // 6. 计算 rb = md5(rid + did + t10 + v)
        let rb = Self::md5_hash(&format!("{}{}{}{}", room_id, did, t10, v));

        tracing::debug!("🔐 v={}, t10={}, rb={}", v, t10, rb);

        // 7. 构建 sign 函数
        // 替换: return rt;}\);? -> return rt;}
        let re_return = Regex::new(r"return rt;\}\);?")
            .map_err(|e| RecorderError::InvalidResponseFormat(format!("Regex error: {}", e)))?;
        let func_sign = re_return.replace(&res, "return rt;}").to_string();

        // 替换: (function ( -> function sign(
        let func_sign = func_sign.replace("(function (", "function sign(");

        // 替换: CryptoJS.MD5(cb).toString() -> "rb"
        let func_sign = func_sign.replace("CryptoJS.MD5(cb).toString()", &format!("\"{}\"", rb));

        // 8. 使用 QuickJS 执行 sign 函数
        let params: String = context.with(|ctx| {
            // 执行 sign 函数定义
            ctx.eval::<(), _>(func_sign.as_str())
                .catch(&ctx)
                .map_err(|e| {
                    RecorderError::JavaScriptError(format!("Failed to eval sign JS: {:?}", e))
                })?;

            // 调用 sign(rid, did, t10)
            let func: Function = ctx.globals().get("sign").catch(&ctx).map_err(|e| {
                RecorderError::JavaScriptError(format!("Failed to get sign: {:?}", e))
            })?;

            let result: String = func
                .call((room_id, did, t10.as_str()))
                .catch(&ctx)
                .map_err(|e| {
                    RecorderError::JavaScriptError(format!("Failed to call sign: {:?}", e))
                })?;

            Ok::<String, RecorderError>(result)
        })?;

        tracing::debug!("📝 sign 返回: {}", params);

        // 9. 解析参数
        let mut result = HashMap::new();
        for pair in params.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                result.insert(key.to_string(), value.to_string());
            }
        }

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

        // 构建请求参数
        let api_url = format!("https://www.douyu.com/lapi/live/getH5Play/{}", room_id);

        let mut form_data = HashMap::new();
        form_data.insert("v", sign_params.get("v").cloned().unwrap_or_default());
        form_data.insert("did", did.to_string());
        form_data.insert("tt", sign_params.get("tt").cloned().unwrap_or_default());
        form_data.insert("sign", sign_params.get("sign").cloned().unwrap_or_default());
        form_data.insert("ver", "22011191".to_string());
        form_data.insert("rid", room_id.to_string());
        form_data.insert("rate", "-1".to_string()); // -1 表示最高画质

        tracing::debug!("🚀 请求斗鱼 API: {}", api_url);
        tracing::debug!("📨 请求参数: {:?}", form_data);

        let response = self
            .client
            .post(&api_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Referer", format!("https://www.douyu.com/{}", room_id))
            .header("Origin", "https://www.douyu.com")
            .form(&form_data)
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

        // 从路径获取
        let path = parsed.path().trim_start_matches('/');

        // 过滤掉非数字路径
        if !path.is_empty() && path.chars().all(|c| c.is_ascii_digit()) {
            return Ok(path.to_string());
        }

        // 尝试从路径中提取数字
        let re = Regex::new(r"/(\d+)").unwrap();
        if let Some(caps) = re.captures(url) {
            if let Some(m) = caps.get(1) {
                return Ok(m.as_str().to_string());
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
                println!("v = {:?}", params.get("v"));
                println!("did = {:?}", params.get("did"));
                println!("tt = {:?}", params.get("tt"));
                println!("sign = {:?}", params.get("sign"));

                // 验证必要参数存在
                assert!(params.contains_key("v"), "缺少 v 参数");
                assert!(params.contains_key("tt"), "缺少 tt 参数");
                assert!(params.contains_key("sign"), "缺少 sign 参数");
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
