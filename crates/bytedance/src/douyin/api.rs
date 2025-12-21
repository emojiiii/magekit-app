//! 抖音 API 接口实现
//!
//! 提供抖音 Web API 的完整接口调用

use super::endpoints::DouyinEndpoints;
use super::types::*;
use crate::client::{BdClient, ClientConfig};
use crate::error::{BdError, BdResult};
use crate::sign::ab_sign;
use indexmap::IndexMap;
use regex::Regex;
use serde_json::Value;

/// 默认 Cookie（仅用于测试，生产环境应使用用户自定义 Cookie）
const DEFAULT_COOKIE: &str = concat!(
    "ttwid=1%7C2iDIYVmjzMcpZ20fcaFde0VghXAA3NaNXE_SLR68IyE%7C1761045455%7C",
    "ab35197d5cfb21df6cbb2fa7ef1c9262206b062c315b9d04da746d0b37dfbc7d",
);

/// Douyin Web 端固定 User-Agent（与 A-Bogus 实现的 ua_code 对齐）
///
/// Python 参考实现明确提示不要随意修改 UA，否则请求可能失败。
const DOUYIN_WEB_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.212 Safari/537.36";

/// 抖音 API 客户端
pub struct DouyinApi {
    client: BdClient,
    cookie: String,
    user_agent: String,
}

impl DouyinApi {
    /// 创建新的 API 客户端
    pub fn new() -> BdResult<Self> {
        let client = BdClient::with_config(ClientConfig {
            user_agent: DOUYIN_WEB_UA.to_string(),
            ..Default::default()
        })?;
        Ok(Self {
            user_agent: client.user_agent().to_string(),
            client,
            cookie: DEFAULT_COOKIE.to_string(),
        })
    }

    /// 使用自定义配置创建客户端
    pub fn with_config(config: ClientConfig) -> BdResult<Self> {
        let ua = config.user_agent.clone();
        let client = BdClient::with_config(config)?;
        Ok(Self {
            client,
            cookie: DEFAULT_COOKIE.to_string(),
            user_agent: ua,
        })
    }

    /// 设置 Cookie
    pub fn set_cookie(&mut self, cookie: impl Into<String>) {
        self.cookie = cookie.into();
    }

    /// 获取内部客户端
    pub fn client(&self) -> &BdClient {
        &self.client
    }

    // ========== 基础参数构建 ==========

    /// 构建基础请求参数
    fn base_params() -> IndexMap<&'static str, String> {
        let mut params = IndexMap::new();
        params.insert("device_platform", "webapp".to_string());
        params.insert("aid", "6383".to_string());
        params.insert("channel", "channel_pc_web".to_string());
        params.insert("pc_client_type", "1".to_string());
        params.insert("version_code", "290100".to_string());
        params.insert("version_name", "29.1.0".to_string());
        params.insert("cookie_enabled", "true".to_string());
        params.insert("screen_width", "1920".to_string());
        params.insert("screen_height", "1080".to_string());
        params.insert("browser_language", "zh-CN".to_string());
        params.insert("browser_platform", "Win32".to_string());
        params.insert("browser_name", "Chrome".to_string());
        params.insert("browser_version", "130.0.0.0".to_string());
        params.insert("browser_online", "true".to_string());
        params.insert("engine_name", "Blink".to_string());
        params.insert("engine_version", "130.0.0.0".to_string());
        params.insert("os_name", "Windows".to_string());
        params.insert("os_version", "10".to_string());
        params.insert("cpu_core_num", "12".to_string());
        params.insert("device_memory", "8".to_string());
        params.insert("platform", "PC".to_string());
        params.insert("downlink", "10".to_string());
        params.insert("effective_type", "4g".to_string());
        params.insert("from_user_page", "1".to_string());
        params.insert("locate_query", "false".to_string());
        params.insert("need_time_list", "1".to_string());
        params.insert("pc_libra_divert", "Windows".to_string());
        params.insert("publish_video_strategy_type", "2".to_string());
        params.insert("round_trip_time", "0".to_string());
        params.insert("show_live_replay_strategy", "1".to_string());
        params.insert("time_list_query", "0".to_string());
        params.insert("whale_cut_token", "".to_string());
        params.insert("update_version_code", "170400".to_string());
        params.insert("msToken", "".to_string());
        params
    }

    /// 将参数转换为查询字符串
    fn params_to_query(params: &IndexMap<&str, String>) -> String {
        params
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// 生成带签名的 URL
    fn sign_url(&self, endpoint: &str, params: &IndexMap<&str, String>) -> String {
        let query = Self::params_to_query(params);
        // Python 参考实现（2024-06）已不再使用 X-Bogus，改为仅使用 a_bogus。
        let a_bogus = ab_sign(&query, &self.user_agent, None);
        let a_bogus_encoded = urlencoding::encode(&a_bogus);
        format!("{endpoint}?{query}&a_bogus={a_bogus_encoded}")
    }

    /// 发送 API 请求
    async fn fetch_json(&self, endpoint: &str, params: IndexMap<&str, String>) -> BdResult<Value> {
        let url = self.sign_url(endpoint, &params);
        tracing::debug!("🌐 请求 URL: {}", url);
        tracing::debug!("🍪 Cookie 长度: {} bytes", self.cookie.len());

        let resp = self
            .client
            .inner()
            .get(&url)
            .header("Referer", "https://www.douyin.com/")
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Language", "zh-CN,zh;q=0.9")
            .header("Cookie", &self.cookie)
            .send()
            .await?;

        let status = resp.status();
        tracing::debug!("📡 响应状态: {}", status);

        if !status.is_success() {
            return Err(BdError::Network(format!("HTTP {}", status)));
        }

        let text = resp.text().await?;
        tracing::debug!("📦 响应长度: {} bytes", text.len());

        if text.is_empty() {
            tracing::warn!("⚠️ API 返回空响应，可能需要有效的 Cookie");
            return Err(BdError::Network(
                "服务器返回空响应，请检查 Cookie 是否有效".to_string(),
            ));
        }

        // 尝试解析 JSON，失败时打印前 500 字符
        serde_json::from_str(&text).map_err(|e| {
            let preview = if text.len() > 500 {
                &text[..500]
            } else {
                &text
            };
            tracing::error!("❌ JSON 解析失败: {}", e);
            tracing::error!("❌ 响应内容预览: {}", preview);
            BdError::Other(format!("JSON 解析失败: {}", e))
        })
    }

    // ========== 作品接口 ==========

    /// 获取作品详情
    pub async fn get_post_detail(&self, aweme_id: &str) -> BdResult<Value> {
        let mut params = Self::base_params();
        params.insert("aweme_id", aweme_id.to_string());
        self.fetch_json(DouyinEndpoints::POST_DETAIL, params).await
    }

    /// 获取作品详情（解析后）
    pub async fn get_aweme_info(&self, aweme_id: &str) -> BdResult<AwemeInfo> {
        let json = self.get_post_detail(aweme_id).await?;

        let detail = json
            .get("aweme_detail")
            .ok_or_else(|| BdError::MissingData("aweme_detail".to_string()))?;

        Ok(parse_aweme_detail(detail, aweme_id))
    }

    // ========== 用户接口 ==========

    /// 获取用户详情
    pub async fn get_user_profile(&self, sec_user_id: &str) -> BdResult<Value> {
        let mut params = Self::base_params();
        params.insert("sec_user_id", sec_user_id.to_string());
        self.fetch_json(DouyinEndpoints::USER_DETAIL, params).await
    }

    /// 获取用户详情（解析后）
    pub async fn get_user_info(&self, sec_user_id: &str) -> BdResult<UserInfo> {
        let json = self.get_user_profile(sec_user_id).await?;

        let user = json
            .get("user")
            .ok_or_else(|| BdError::UserNotFound(sec_user_id.to_string()))?;

        Ok(UserInfo {
            uid: user["uid"].as_str().unwrap_or("").to_string(),
            sec_user_id: user["sec_uid"].as_str().unwrap_or(sec_user_id).to_string(),
            nickname: user["nickname"].as_str().unwrap_or("").to_string(),
            signature: user["signature"].as_str().map(|s| s.to_string()),
            avatar_url: user["avatar_larger"]["url_list"][0]
                .as_str()
                .map(|s| s.to_string()),
            following_count: user["following_count"].as_u64(),
            follower_count: user["follower_count"].as_u64(),
            aweme_count: user["aweme_count"].as_u64(),
            total_favorited: user["total_favorited"].as_u64(),
        })
    }

    /// 获取用户发布的作品列表
    pub async fn get_user_posts(
        &self,
        sec_user_id: &str,
        max_cursor: i64,
        count: i64,
    ) -> BdResult<Value> {
        let mut params = Self::base_params();
        params.insert("sec_user_id", sec_user_id.to_string());
        params.insert("max_cursor", max_cursor.to_string());
        params.insert("count", count.to_string());
        params.insert("from_user_page", "1".to_string());
        params.insert("locate_query", "false".to_string());
        params.insert("show_live_replay_strategy", "1".to_string());
        self.fetch_json(DouyinEndpoints::USER_POST, params).await
    }

    /// 获取用户喜欢的作品列表
    pub async fn get_user_likes(
        &self,
        sec_user_id: &str,
        max_cursor: i64,
        count: i64,
    ) -> BdResult<Value> {
        let mut params = Self::base_params();
        params.insert("sec_user_id", sec_user_id.to_string());
        params.insert("max_cursor", max_cursor.to_string());
        params.insert("count", count.to_string());
        self.fetch_json(DouyinEndpoints::USER_FAVORITE_A, params)
            .await
    }

    /// 获取用户关注列表
    pub async fn get_user_following(
        &self,
        sec_user_id: &str,
        offset: i64,
        count: i64,
    ) -> BdResult<Value> {
        let mut params = Self::base_params();
        params.insert("sec_user_id", sec_user_id.to_string());
        params.insert("offset", offset.to_string());
        params.insert("count", count.to_string());
        params.insert("source_type", "1".to_string());
        self.fetch_json(DouyinEndpoints::USER_FOLLOWING, params)
            .await
    }

    /// 获取用户粉丝列表
    pub async fn get_user_followers(
        &self,
        sec_user_id: &str,
        offset: i64,
        count: i64,
    ) -> BdResult<Value> {
        let mut params = Self::base_params();
        params.insert("sec_user_id", sec_user_id.to_string());
        params.insert("offset", offset.to_string());
        params.insert("count", count.to_string());
        params.insert("source_type", "1".to_string());
        self.fetch_json(DouyinEndpoints::USER_FOLLOWER, params)
            .await
    }

    // ========== 评论接口 ==========

    /// 获取作品评论
    pub async fn get_post_comments(
        &self,
        aweme_id: &str,
        cursor: i64,
        count: i64,
    ) -> BdResult<Value> {
        let mut params = Self::base_params();
        params.insert("aweme_id", aweme_id.to_string());
        params.insert("cursor", cursor.to_string());
        params.insert("count", count.to_string());
        params.insert("item_type", "0".to_string());
        self.fetch_json(DouyinEndpoints::POST_COMMENT, params).await
    }

    /// 获取评论回复
    pub async fn get_comment_replies(
        &self,
        item_id: &str,
        comment_id: &str,
        cursor: i64,
        count: i64,
    ) -> BdResult<Value> {
        let mut params = Self::base_params();
        params.insert("item_id", item_id.to_string());
        params.insert("comment_id", comment_id.to_string());
        params.insert("cursor", cursor.to_string());
        params.insert("count", count.to_string());
        params.insert("item_type", "0".to_string());
        self.fetch_json(DouyinEndpoints::POST_COMMENT_REPLY, params)
            .await
    }

    // ========== 搜索接口 ==========

    /// 综合搜索
    pub async fn search_general(&self, keyword: &str, offset: i64, count: i64) -> BdResult<Value> {
        let mut params = Self::base_params();
        params.insert("keyword", keyword.to_string());
        params.insert("offset", offset.to_string());
        params.insert("count", count.to_string());
        params.insert("search_source", "normal_search".to_string());
        params.insert("search_channel", "aweme_general".to_string());
        params.insert("sort_type", "0".to_string());
        params.insert("publish_time", "0".to_string());
        params.insert("filter_duration", "0".to_string());
        self.fetch_json(DouyinEndpoints::GENERAL_SEARCH, params)
            .await
    }

    /// 搜索视频
    pub async fn search_video(&self, keyword: &str, offset: i64, count: i64) -> BdResult<Value> {
        let mut params = Self::base_params();
        params.insert("keyword", keyword.to_string());
        params.insert("offset", offset.to_string());
        params.insert("count", count.to_string());
        params.insert("search_source", "normal_search".to_string());
        params.insert("sort_type", "0".to_string());
        params.insert("publish_time", "0".to_string());
        params.insert("filter_duration", "0".to_string());
        self.fetch_json(DouyinEndpoints::VIDEO_SEARCH, params).await
    }

    /// 搜索用户
    pub async fn search_user(&self, keyword: &str, offset: i64, count: i64) -> BdResult<Value> {
        let mut params = Self::base_params();
        params.insert("keyword", keyword.to_string());
        params.insert("offset", offset.to_string());
        params.insert("count", count.to_string());
        params.insert("search_source", "normal_search".to_string());
        params.insert("is_filter_search", "0".to_string());
        self.fetch_json(DouyinEndpoints::USER_SEARCH, params).await
    }

    /// 获取热搜榜
    pub async fn get_hot_search(&self) -> BdResult<Value> {
        let params = Self::base_params();
        self.fetch_json(DouyinEndpoints::HOT_SEARCH, params).await
    }

    // ========== 合辑接口 ==========

    /// 获取合辑作品
    pub async fn get_mix_aweme(&self, mix_id: &str, cursor: i64, count: i64) -> BdResult<Value> {
        let mut params = Self::base_params();
        params.insert("mix_id", mix_id.to_string());
        params.insert("cursor", cursor.to_string());
        params.insert("count", count.to_string());
        self.fetch_json(DouyinEndpoints::MIX_AWEME, params).await
    }

    // ========== URL 解析工具 ==========

    /// 解析短链接
    pub async fn resolve_short_url(&self, url: &str) -> BdResult<String> {
        let resp = self
            .client
            .inner()
            .get(url)
            .header("Referer", "https://www.douyin.com/")
            .send()
            .await?;
        Ok(resp.url().to_string())
    }

    /// 从 URL 提取 aweme_id
    pub fn extract_aweme_id(url: &str) -> Option<String> {
        let patterns = [
            r"/video/(\d+)",
            r"/note/(\d+)",
            r"aweme_id=(\d+)",
            r"/share/video/(\d+)",
            r"modal_id=(\d+)",
            r"[?&]vid=(\d+)",
        ];

        for pat in &patterns {
            if let Ok(re) = Regex::new(pat) {
                if let Some(caps) = re.captures(url) {
                    if let Some(id) = caps.get(1) {
                        return Some(id.as_str().to_string());
                    }
                }
            }
        }
        None
    }

    /// 从 URL 提取 sec_user_id
    pub fn extract_sec_user_id(url: &str) -> Option<String> {
        let patterns = [
            r"/user/([^/?]+)",
            r"sec_user_id=([^&]+)",
            r"sec_uid=([^&]+)",
        ];

        for pat in &patterns {
            if let Ok(re) = Regex::new(pat) {
                if let Some(caps) = re.captures(url) {
                    if let Some(id) = caps.get(1) {
                        return Some(id.as_str().to_string());
                    }
                }
            }
        }
        None
    }
}

impl Default for DouyinApi {
    fn default() -> Self {
        Self::new().expect("创建 DouyinApi 失败")
    }
}

fn parse_aweme_detail(detail: &Value, fallback_aweme_id: &str) -> AwemeInfo {
    let aweme_id = detail
        .get("aweme_id")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback_aweme_id)
        .to_string();

    let mut desc = detail.get("desc").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if desc.trim().is_empty() {
        // 部分作品 desc 为空，尝试使用 share_title 兜底，避免 UI 空标题
        desc = detail
            .pointer("/share_info/share_title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
    }
    if desc.trim().is_empty() {
        desc = aweme_id.clone();
    }

    let author = detail.get("author").and_then(|a| {
        Some(AuthorInfo {
            uid: a.get("uid").and_then(|v| v.as_str()).map(|s| s.to_string()),
            sec_uid: a.get("sec_uid").and_then(|v| v.as_str()).map(|s| s.to_string()),
            nickname: a.get("nickname").and_then(|v| v.as_str()).map(|s| s.to_string()),
            avatar_url: first_url_from_list(a.pointer("/avatar_thumb/url_list")),
        })
    });

    let video = detail.get("video").and_then(|v| {
        let duration = parse_u64(v.get("duration"));
        let width = parse_u64(v.get("width")).map(|n| n as u32);
        let height = parse_u64(v.get("height")).map(|n| n as u32);

        let cover_url = first_url_from_list(v.pointer("/cover/url_list"))
            .or_else(|| first_url_from_list(v.pointer("/origin_cover/url_list")))
            .or_else(|| first_url_from_list(v.pointer("/dynamic_cover/url_list")));

        let mut formats = parse_video_formats(v, width, height);
        if formats.is_empty() {
            // 兜底：直接用 play_addr.url_list 生成一个可下载格式
            if let Some(url) = first_url_from_list(v.pointer("/play_addr/url_list")) {
                formats.push(VideoFormat {
                    format_id: "play".to_string(),
                    ext: "mp4".to_string(),
                    resolution: width
                        .zip(height)
                        .map(|(w, h)| format!("{}x{}", w, h)),
                    filesize: None,
                    quality: Some("play_addr".to_string()),
                    download_url: Some(url),
                });
            }
        }

        let play_urls = v
            .pointer("/play_addr/url_list")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|u| u.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Some(VideoData {
            duration,
            cover_url,
            play_urls,
            width,
            height,
            formats,
        })
    });

    AwemeInfo {
        aweme_id,
        desc,
        author,
        video,
        create_time: parse_u64(detail.get("create_time")),
        digg_count: parse_u64(detail.pointer("/statistics/digg_count")),
        comment_count: parse_u64(detail.pointer("/statistics/comment_count")),
        share_count: parse_u64(detail.pointer("/statistics/share_count")),
    }
}

fn parse_video_formats(video: &Value, fallback_w: Option<u32>, fallback_h: Option<u32>) -> Vec<VideoFormat> {
    let mut out = Vec::new();

    // 优先使用 download_addr（通常更适合直接下载）
    if let Some(url) = first_url_from_list(video.pointer("/download_addr/url_list")) {
        out.push(VideoFormat {
            format_id: "download".to_string(),
            ext: "mp4".to_string(),
            resolution: fallback_w
                .zip(fallback_h)
                .map(|(w, h)| format!("{}x{}", w, h)),
            filesize: parse_u64(video.pointer("/download_addr/data_size")),
            quality: Some("download_addr".to_string()),
            download_url: Some(url),
        });
    }

    let bit_rate = video.get("bit_rate").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    for item in bit_rate {
        let gear_name = item
            .get("gear_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let codec_from_item = item
            .get("codec_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let bitrate = parse_u64(item.get("bit_rate"));

        // 同一档位可能同时包含 h264/h265 的播放地址：尽量都暴露出来，便于用户选择兼容编码
        let mut addrs: Vec<(&str, &Value)> = Vec::new();
        if let Some(pa) = item.get("play_addr_h264") {
            addrs.push(("h264", pa));
        }
        if let Some(pa) = item.get("play_addr") {
            addrs.push((codec_from_item, pa));
        }
        if let Some(pa) = item.get("play_addr_265") {
            addrs.push(("h265", pa));
        }

        for (codec_label, play_addr) in addrs {
            let Some(url) = first_url_from_list(play_addr.get("url_list")) else {
                continue;
            };

            let width = parse_u64(play_addr.get("width"))
                .map(|n| n as u32)
                .or(fallback_w);
            let height = parse_u64(play_addr.get("height"))
                .map(|n| n as u32)
                .or(fallback_h);

            let resolution = width.zip(height).map(|(w, h)| format!("{}x{}", w, h));
            let filesize = parse_u64(item.get("size")).or_else(|| parse_u64(play_addr.get("data_size")));

            let format_id = make_format_id(gear_name, codec_label, height, bitrate);
            let quality = if !gear_name.trim().is_empty() {
                Some(gear_name.to_string())
            } else if !codec_label.trim().is_empty() {
                Some(codec_label.to_string())
            } else {
                None
            };

            out.push(VideoFormat {
                format_id,
                ext: "mp4".to_string(),
                resolution,
                filesize,
                quality,
                download_url: Some(url),
            });
        }
    }

    // 去重：同 URL 只保留第一个
    let mut seen = std::collections::HashSet::new();
    out.retain(|f| f.download_url.as_deref().map(|u| seen.insert(u.to_string())).unwrap_or(true));
    out
}

fn make_format_id(gear_name: &str, codec: &str, height: Option<u32>, bitrate: Option<u64>) -> String {
    // format_id 用于 UI 展示/选择，尽量友好且可区分
    let mut parts: Vec<String> = Vec::new();
    if let Some(h) = height {
        parts.push(format!("{}p", h));
    }
    if !codec.trim().is_empty() {
        parts.push(codec.to_string());
    }
    if let Some(br) = bitrate {
        // kbps
        parts.push(format!("{}k", br / 1000));
    }
    if parts.is_empty() && !gear_name.trim().is_empty() {
        return gear_name.to_string();
    }
    if parts.is_empty() {
        return "unknown".to_string();
    }
    parts.join("_")
}

fn parse_u64(v: Option<&Value>) -> Option<u64> {
    let v = v?;
    if let Some(n) = v.as_u64() {
        return Some(n);
    }
    if let Some(n) = v.as_i64() {
        if n >= 0 {
            return Some(n as u64);
        }
    }
    if let Some(s) = v.as_str() {
        return s.parse::<u64>().ok();
    }
    None
}

fn first_url_from_list(v: Option<&Value>) -> Option<String> {
    v.and_then(|x| x.as_array())
        .and_then(|arr| arr.iter().filter_map(|u| u.as_str()).next())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_aweme_id() {
        assert_eq!(
            DouyinApi::extract_aweme_id("https://www.douyin.com/video/7321613070743663893"),
            Some("7321613070743663893".to_string())
        );
        assert_eq!(
            DouyinApi::extract_aweme_id(
                "https://www.douyin.com/discover?modal_id=7321613070743663893"
            ),
            Some("7321613070743663893".to_string())
        );
    }

    #[test]
    fn test_extract_sec_user_id() {
        assert_eq!(
            DouyinApi::extract_sec_user_id("https://www.douyin.com/user/MS4wLjABAAAA123abc"),
            Some("MS4wLjABAAAA123abc".to_string())
        );
    }

    #[test]
    fn test_parse_aweme_detail_builds_multiple_formats_and_fallback_title() {
        let detail = json!({
            "aweme_id": "123",
            "desc": "",
            "share_info": { "share_title": "fallback-title" },
            "author": {
                "uid": "u1",
                "sec_uid": "s1",
                "nickname": "nick",
                "avatar_thumb": { "url_list": ["https://example.com/a.jpg"] }
            },
            "video": {
                "duration": 6500,
                "width": 1920,
                "height": 1080,
                "cover": { "url_list": ["https://example.com/c.jpg"] },
                "download_addr": { "url_list": ["https://example.com/dl.mp4"], "data_size": 123 },
                "play_addr": { "url_list": ["https://example.com/play.mp4"] },
                "bit_rate": [
                    {
                        "gear_name": "normal",
                        "codec_type": "h264",
                        "bit_rate": 1000000,
                        "size": 111,
                        "play_addr": { "url_list": ["https://example.com/720.mp4"], "width": 1280, "height": 720 }
                    },
                    {
                        "gear_name": "hd",
                        "codec_type": "h264",
                        "bit_rate": 2000000,
                        "size": 222,
                        "play_addr": { "url_list": ["https://example.com/1080.mp4"], "width": 1920, "height": 1080 }
                    },
                    {
                        "gear_name": "hd",
                        "codec_type": "h265",
                        "bit_rate": 2500000,
                        "size": 333,
                        "play_addr_h264": { "url_list": ["https://example.com/1080_h264.mp4"], "width": 1920, "height": 1080 },
                        "play_addr_265": { "url_list": ["https://example.com/1080_h265.mp4"], "width": 1920, "height": 1080 }
                    }
                ]
            }
        });

        let parsed = parse_aweme_detail(&detail, "fallback");
        assert_eq!(parsed.aweme_id, "123");
        assert_eq!(parsed.desc, "fallback-title");

        let video = parsed.video.expect("video");
        assert_eq!(video.duration, Some(6500));
        assert_eq!(video.width, Some(1920));
        assert_eq!(video.height, Some(1080));
        assert_eq!(
            video.cover_url.as_deref(),
            Some("https://example.com/c.jpg")
        );

        // download_addr + bit_rate(2) = 3 条直链格式（去重后仍应 >= 3）
        assert!(
            video.formats.len() >= 3,
            "formats too few: {}",
            video.formats.len()
        );
        assert!(video
            .formats
            .iter()
            .any(|f| f.format_id == "download" && f.download_url.as_deref() == Some("https://example.com/dl.mp4")));
        assert!(video
            .formats
            .iter()
            .any(|f| f.download_url.as_deref() == Some("https://example.com/1080.mp4")));
        assert!(video
            .formats
            .iter()
            .any(|f| f.download_url.as_deref() == Some("https://example.com/1080_h264.mp4")));
        assert!(video
            .formats
            .iter()
            .any(|f| f.download_url.as_deref() == Some("https://example.com/1080_h265.mp4")));
    }
}
