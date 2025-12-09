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
const DEFAULT_COOKIE: &str = "ttwid=1%7C2iDIYVmjzMcpZ20fcaFde0VghXAA3NaNXE_SLR68IyE%7C1761045455%7Cab35197d5cfb21df6cbb2fa7ef1c9262206b062c315b9d04da746d0b37dfbc7d";

/// 抖音 API 客户端
pub struct DouyinApi {
    client: BdClient,
    cookie: String,
    user_agent: String,
}

impl DouyinApi {
    /// 创建新的 API 客户端
    pub fn new() -> BdResult<Self> {
        let client = BdClient::new()?;
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
        params.insert("browser_version", "116.0.0.0".to_string());
        params.insert("browser_online", "true".to_string());
        params.insert("engine_name", "Blink".to_string());
        params.insert("engine_version", "116.0.0.0".to_string());
        params.insert("os_name", "Windows".to_string());
        params.insert("os_version", "10".to_string());
        params.insert("cpu_core_num", "12".to_string());
        params.insert("device_memory", "8".to_string());
        params.insert("platform", "PC".to_string());
        params.insert("downlink", "10".to_string());
        params.insert("effective_type", "4g".to_string());
        params.insert("round_trip_time", "0".to_string());
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

        Ok(AwemeInfo {
            aweme_id: detail["aweme_id"].as_str().unwrap_or(aweme_id).to_string(),
            desc: detail["desc"].as_str().unwrap_or("").to_string(),
            author: detail.get("author").map(|a| AuthorInfo {
                uid: a["uid"].as_str().map(|s| s.to_string()),
                sec_uid: a["sec_uid"].as_str().map(|s| s.to_string()),
                nickname: a["nickname"].as_str().map(|s| s.to_string()),
                avatar_url: a["avatar_thumb"]["url_list"][0]
                    .as_str()
                    .map(|s| s.to_string()),
            }),
            video: detail.get("video").map(|v| VideoData {
                duration: v["duration"].as_u64(),
                cover_url: v["cover"]["url_list"][0].as_str().map(|s| s.to_string()),
                play_urls: v["play_addr"]["url_list"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|u| u.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
                width: v["width"].as_u64().map(|n| n as u32),
                height: v["height"].as_u64().map(|n| n as u32),
                formats: vec![],
            }),
            create_time: detail["create_time"].as_u64(),
            digg_count: detail["statistics"]["digg_count"].as_u64(),
            comment_count: detail["statistics"]["comment_count"].as_u64(),
            share_count: detail["statistics"]["share_count"].as_u64(),
        })
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
