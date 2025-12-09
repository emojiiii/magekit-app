//! 抖音直播 API 接口实现
//!
//! 从 live_recorder 复刻的直播流获取逻辑

use crate::client::{BdClient, ClientConfig, MOBILE_UA};
use crate::error::{BdError, BdResult};
use crate::sign::abogus::ab_sign_live;
use super::endpoints::DouyinEndpoints;
use super::types::*;
use std::collections::HashMap;
use url::Url;

/// 抖音直播 API 客户端
pub struct DouyinLiveApi {
    client: BdClient,
    user_agent: String,
}

impl DouyinLiveApi {
    /// 创建新的直播 API 客户端
    pub fn new() -> BdResult<Self> {
        let config = ClientConfig::live();
        let ua = config.user_agent.clone();
        let client = BdClient::with_config(config)?;
        Ok(Self {
            client,
            user_agent: ua,
        })
    }

    /// 设置代理
    pub fn with_proxy(proxy_url: &str) -> BdResult<Self> {
        let config = ClientConfig::live().with_proxy(proxy_url);
        let ua = config.user_agent.clone();
        let client = BdClient::with_config(config)?;
        Ok(Self {
            client,
            user_agent: ua,
        })
    }

    /// 获取内部客户端
    pub fn client(&self) -> &BdClient {
        &self.client
    }

    // ========== 直播间解析 ==========

    /// 从 URL 提取房间 ID
    pub async fn extract_room_id(&self, url: &str) -> BdResult<String> {
        tracing::info!("🔍 开始解析抖音直播间URL: {}", url);

        // 如果是直播间链接，直接提取 web_rid
        if url.contains("live.douyin.com") {
            let parsed_url = Url::parse(url)?;
            let path_parts: Vec<&str> = parsed_url.path().split('/').collect();
            if let Some(web_rid) = path_parts.last() {
                if !web_rid.is_empty() {
                    let room_id = web_rid.to_string();
                    tracing::info!("✅ 从直播间URL提取房间ID: {}", room_id);
                    return Ok(room_id);
                }
            }
        }

        tracing::info!("🔄 检测到非直播间URL，尝试通过短链接解析...");
        // 如果是短视频链接，需要跳转到直播间
        let (room_id, sec_user_id) = self.get_sec_user_id(url).await?;
        let web_rid = self.get_live_room_id(&room_id, &sec_user_id).await?;
        Ok(web_rid)
    }

    /// 获取房间ID和用户sec_user_id
    async fn get_sec_user_id(&self, url: &str) -> BdResult<(String, String)> {
        let response = self.client.inner()
            .get(url)
            .header("Referer", "https://www.douyin.com/")
            .send()
            .await?;

        let final_url = response.url().to_string();

        if !final_url.contains("reflow/") {
            return Err(BdError::InvalidUrl(
                "The redirect URL does not contain 'reflow/'".to_string(),
            ));
        }

        let parsed_url = Url::parse(&final_url)?;
        let path_parts: Vec<&str> = parsed_url.path().split('/').collect();

        if path_parts.len() < 2 {
            return Err(BdError::InvalidUrl(
                "Cannot extract room ID from URL".to_string(),
            ));
        }

        let room_id = path_parts[path_parts.len() - 1].to_string();

        let query_pairs: HashMap<String, String> = parsed_url.query_pairs().into_owned().collect();

        let sec_user_id = query_pairs
            .get("sec_user_id")
            .ok_or_else(|| BdError::InvalidUrl("Cannot find sec_user_id in URL".to_string()))?
            .clone();

        Ok((room_id, sec_user_id))
    }

    /// 获取直播间web_rid
    async fn get_live_room_id(&self, room_id: &str, sec_user_id: &str) -> BdResult<String> {
        let ms_token = Self::generate_ms_token();

        let mut params = HashMap::new();
        params.insert("verifyFp", "verify_lk07kv74_QZYCUApD_xhiB_405x_Ax51_GYO9bUIyZQVf");
        params.insert("type_id", "0");
        params.insert("live_id", "1");
        params.insert("room_id", room_id);
        params.insert("sec_user_id", sec_user_id);
        params.insert("app_id", "1128");
        params.insert("msToken", &ms_token);

        let query_string = Self::build_query_string(&params);
        let a_bogus = ab_sign_live(&query_string, &self.user_agent);

        let api_url = format!(
            "{}?{}&a_bogus={}",
            DouyinEndpoints::LIVE_INFO_ROOM_ID,
            query_string,
            a_bogus
        );

        let response = self.client.inner()
            .get(&api_url)
            .header("User-Agent", MOBILE_UA)
            .header("Accept-Language", "zh-CN,zh;q=0.8,zh-TW;q=0.7,zh-HK;q=0.5,en-US;q=0.3,en;q=0.2")
            .header("Cookie", "s_v_web_id=verify_lk07kv74_QZYCUApD_xhiB_405x_Ax51_GYO9bUIyZQVf")
            .send()
            .await?;

        let json_text = response.text().await?;
        let json_response: serde_json::Value = serde_json::from_str(&json_text)?;

        if let Some(data) = json_response
            .get("data")
            .and_then(|d| d.get("room"))
            .and_then(|r| r.get("owner"))
            .and_then(|o| o.get("web_rid"))
        {
            Ok(data.as_str().unwrap_or("").to_string())
        } else {
            Err(BdError::LiveNotAvailable(format!(
                "Room {} not found",
                room_id
            )))
        }
    }

    // ========== 直播流获取 ==========

    /// 获取直播流信息
    pub async fn get_stream_info(&self, web_rid: &str) -> BdResult<StreamInfo> {
        let stream_data = self.get_live_stream_data(web_rid).await?;
        Self::parse_stream_urls(&stream_data)
    }

    /// 获取直播流数据
    async fn get_live_stream_data(&self, web_rid: &str) -> BdResult<serde_json::Value> {
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

        // 生成 a_bogus 签名
        tracing::debug!("🔑 Query string: {}", query_string);
        let a_bogus = ab_sign_live(&query_string, &self.user_agent);
        tracing::debug!("🔑 Generated a_bogus: {}", a_bogus);

        let api_url = format!(
            "{}?{}&a_bogus={}",
            DouyinEndpoints::LIVE_INFO,
            query_string,
            a_bogus
        );

        tracing::info!("🌐 正在请求抖音直播API: {}", api_url);

        // 使用简化 Cookie
        let cookie = "ttwid=1%7C2iDIYVmjzMcpZ20fcaFde0VghXAA3NaNXE_SLR68IyE%7C1761045455%7Cab35197d5cfb21df6cbb2fa7ef1c9262206b062c315b9d04da746d0b37dfbc7d";

        let response = self.client.inner()
            .get(&api_url)
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Cookie", cookie)
            .header("Referer", format!("https://live.douyin.com/{}", web_rid))
            .header("User-Agent", &self.user_agent)
            .send()
            .await?;

        let status = response.status();
        tracing::info!("📡 API响应状态: {}", status);

        let bytes = response.bytes().await?;
        tracing::info!("📄 API响应字节数: {} bytes", bytes.len());

        if bytes.is_empty() {
            tracing::error!("❌ API返回空内容");
            return Err(BdError::LiveNotAvailable(format!(
                "API返回空内容，房间号: {}",
                web_rid
            )));
        }

        let json_text = String::from_utf8_lossy(&bytes).to_string();

        if json_text.is_empty() {
            return Err(BdError::LiveNotAvailable(format!(
                "API返回空内容，房间号: {}",
                web_rid
            )));
        }

        let json_response: serde_json::Value = serde_json::from_str(&json_text)?;

        if json_response.get("data").is_some() {
            tracing::info!("✅ 成功获取直播间数据");
            Ok(json_response)
        } else {
            tracing::error!("❌ API响应缺少data字段");
            Err(BdError::LiveNotAvailable(format!(
                "Stream data not available for room {}",
                web_rid
            )))
        }
    }

    /// 解析流URL
    fn parse_stream_urls(json_data: &serde_json::Value) -> BdResult<StreamInfo> {
        let data = json_data.get("data")
            .ok_or_else(|| BdError::MissingData("data".to_string()))?;

        // 抖音 web API 返回的结构是 data.data[0]
        let data_array = data.get("data").and_then(|d| d.as_array());

        // 检查数组是否存在且非空
        if data_array.is_none() || data_array.map(|arr| arr.is_empty()).unwrap_or(true) {
            // data.data 为空，说明直播间未开播或不存在
            let anchor_name = data
                .get("user")
                .and_then(|u| u.get("nickname"))
                .and_then(|n| n.as_str())
                .unwrap_or("Unknown")
                .to_string();

            tracing::info!("ℹ️ 直播间未开播或不存在，主播: {}", anchor_name);

            let live_room_info = LiveRoomInfo {
                room_id: String::new(),
                anchor_name,
                title: String::new(),
                status: LiveStatus::Offline,
                start_time: None,
                viewer_count: None,
                cover_url: None,
                extra: HashMap::new(),
            };

            return Ok(StreamInfo {
                room: live_room_info,
                streams: vec![],
            });
        }

        let room_info = data_array.unwrap().first()
            .ok_or_else(|| BdError::MissingData("room data".to_string()))?;

        // 主播名字在 data.user.nickname
        let anchor_name = data
            .get("user")
            .and_then(|u| u.get("nickname"))
            .and_then(|n| n.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let title = room_info
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        let status_value = room_info
            .get("status")
            .and_then(|s| s.as_u64())
            .unwrap_or(4);

        let status = match status_value {
            2 => LiveStatus::Live,
            4 => LiveStatus::Offline,
            _ => LiveStatus::Unknown,
        };

        let live_room_info = LiveRoomInfo {
            room_id: room_info
                .get("id_str")
                .and_then(|id| id.as_str())
                .unwrap_or("")
                .to_string(),
            anchor_name: anchor_name.clone(),
            title: title.clone(),
            status: status.clone(),
            start_time: None,
            viewer_count: room_info.get("user_count").and_then(|uc| uc.as_u64()),
            cover_url: Self::extract_cover_url(room_info),
            extra: HashMap::new(),
        };

        tracing::info!(
            "📺 抖音 主播: {}, 标题: {}, 封面: {:?}",
            live_room_info.anchor_name,
            live_room_info.title,
            live_room_info.cover_url.is_some()
        );

        if status != LiveStatus::Live {
            tracing::info!("ℹ️ 直播间状态: {:?}，非直播中", status);
            return Ok(StreamInfo {
                room: live_room_info,
                streams: vec![],
            });
        }

        // 解析流URL
        let stream_url = room_info.get("stream_url")
            .ok_or_else(|| BdError::LiveNotAvailable("No stream URL available".to_string()))?;

        // 获取原画流
        let (origin_flv, origin_hls) = Self::extract_origin_stream(stream_url);

        // 从 flv_pull_url 和 hls_pull_url_map 获取其他质量的流
        let flv_pull_url = stream_url
            .get("flv_pull_url")
            .and_then(|flv| flv.as_object());

        let hls_pull_url_map = stream_url
            .get("hls_pull_url_map")
            .and_then(|hls| hls.as_object());

        let mut streams = Vec::new();

        // 首先添加 ORIGIN 质量的流（如果有）
        if origin_flv.is_some() || origin_hls.is_some() {
            streams.push(StreamData {
                quality: VideoQuality::Original,
                url: StreamUrl {
                    hls_url: origin_hls,
                    flv_url: origin_flv,
                    dash_url: None,
                },
                bitrate: None,
                resolution: None,
                codec: None,
                cdn: None,
            });
        }

        // 质量映射
        let quality_names = ["FULL_HD1", "HD1", "SD1", "SD2"];
        let quality_enums = [
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
            return Err(BdError::LiveNotAvailable(
                "No valid stream URLs found".to_string(),
            ));
        }

        Ok(StreamInfo {
            room: live_room_info,
            streams,
        })
    }

    /// 提取原画流
    fn extract_origin_stream(stream_url: &serde_json::Value) -> (Option<String>, Option<String>) {
        let mut origin_flv: Option<String> = None;
        let mut origin_hls: Option<String> = None;

        if let Some(live_core_sdk_data) = stream_url.get("live_core_sdk_data") {
            tracing::debug!("📦 找到 live_core_sdk_data");

            let stream_data_str = stream_url
                .get("pull_datas")
                .and_then(|pd| pd.as_object())
                .and_then(|obj| obj.values().next())
                .and_then(|v| v.get("stream_data"))
                .and_then(|sd| sd.as_str())
                .or_else(|| {
                    live_core_sdk_data
                        .get("pull_data")
                        .and_then(|pd| pd.get("stream_data"))
                        .and_then(|sd| sd.as_str())
                });

            if let Some(stream_data_str) = stream_data_str {
                if let Ok(stream_data) = serde_json::from_str::<serde_json::Value>(stream_data_str) {
                    if let Some(origin_main) = stream_data
                        .get("data")
                        .and_then(|d| d.get("origin"))
                        .and_then(|o| o.get("main"))
                    {
                        let codec = origin_main
                            .get("sdk_params")
                            .and_then(|sp| sp.as_str())
                            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                            .and_then(|v| {
                                v.get("VCodec")
                                    .and_then(|c| c.as_str())
                                    .map(|s| s.to_string())
                            })
                            .unwrap_or_default();

                        if let Some(flv) = origin_main.get("flv").and_then(|f| f.as_str()) {
                            origin_flv = Some(format!("{}&codec={}", flv, codec));
                        }
                        if let Some(hls) = origin_main.get("hls").and_then(|h| h.as_str()) {
                            origin_hls = Some(format!("{}&codec={}", hls, codec));
                        }
                    }
                }
            }
        }

        (origin_flv, origin_hls)
    }

    /// 提取封面 URL
    fn extract_cover_url(room_info: &serde_json::Value) -> Option<String> {
        room_info.get("cover").and_then(|c| {
            if let Some(obj) = c.as_object() {
                obj.get("url_list")
                    .and_then(|ul| ul.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|url| url.as_str())
                    .map(|s| s.to_string())
            } else if let Some(arr) = c.as_array() {
                arr.first()
                    .and_then(|url| url.as_str())
                    .map(|s| s.to_string())
            } else if let Some(s) = c.as_str() {
                Some(s.to_string())
            } else {
                None
            }
        })
    }

    // ========== 工具函数 ==========

    /// 生成 msToken
    fn generate_ms_token() -> String {
        use base64::{engine::general_purpose, Engine as _};
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let bytes: Vec<u8> = (0..128).map(|_| rng.r#gen()).collect();
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
}

impl Default for DouyinLiveApi {
    fn default() -> Self {
        Self::new().expect("创建 DouyinLiveApi 失败")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ms_token() {
        let token = DouyinLiveApi::generate_ms_token();
        assert!(!token.is_empty());
        assert!(token.ends_with("=="));
    }

    #[test]
    fn test_build_query_string() {
        let mut params = HashMap::new();
        params.insert("a", "1");
        params.insert("b", "2");
        let query = DouyinLiveApi::build_query_string(&params);
        assert!(query.contains("a=1"));
        assert!(query.contains("b=2"));
    }
}
