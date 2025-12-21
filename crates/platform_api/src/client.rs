//! HTTP 客户端封装
//!
//! 提供统一的 HTTP 客户端配置和请求封装

use crate::error::{BdError, BdResult};
use reqwest::{Client, Proxy};
use std::time::Duration;

/// 默认 User-Agent
pub const DEFAULT_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.0.0 Safari/537.36";

/// 直播专用 User-Agent
pub const LIVE_UA: &str = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.5845.97 Safari/537.36 Core/1.116.567.400 QQBrowser/19.7.6764.400";

/// 移动端 User-Agent
pub const MOBILE_UA: &str = "Mozilla/5.0 (Linux; Android 11; SAMSUNG SM-G973U) AppleWebKit/537.36 (KHTML, like Gecko) SamsungBrowser/14.2 Chrome/87.0.4280.141 Mobile Safari/537.36";

/// 字节跳动 HTTP 客户端
#[derive(Clone)]
pub struct BdClient {
    client: Client,
    user_agent: String,
}

impl BdClient {
    /// 创建新的客户端
    pub fn new() -> BdResult<Self> {
        Self::with_config(ClientConfig::default())
    }

    /// 使用配置创建客户端
    pub fn with_config(config: ClientConfig) -> BdResult<Self> {
        let mut builder = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .user_agent(&config.user_agent)
            .redirect(reqwest::redirect::Policy::limited(config.max_redirects));

        if let Some(proxy_url) = &config.proxy {
            let proxy = Proxy::all(proxy_url)
                .map_err(|e| BdError::Network(format!("代理配置失败: {}", e)))?;
            builder = builder.proxy(proxy);
        }

        let client = builder
            .build()
            .map_err(|e| BdError::Network(format!("创建客户端失败: {}", e)))?;

        Ok(Self {
            client,
            user_agent: config.user_agent,
        })
    }

    /// 获取内部 reqwest 客户端
    pub fn inner(&self) -> &Client {
        &self.client
    }

    /// 获取 User-Agent
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    /// 发送 GET 请求
    pub async fn get(&self, url: &str) -> BdResult<reqwest::Response> {
        self.client.get(url).send().await.map_err(BdError::from)
    }

    /// 发送带 headers 的 GET 请求
    pub async fn get_with_headers(
        &self,
        url: &str,
        headers: Vec<(&str, &str)>,
    ) -> BdResult<reqwest::Response> {
        let mut req = self.client.get(url);
        for (key, value) in headers {
            req = req.header(key, value);
        }
        req.send().await.map_err(BdError::from)
    }

    /// 发送 POST 请求
    pub async fn post(&self, url: &str) -> BdResult<reqwest::Response> {
        self.client.post(url).send().await.map_err(BdError::from)
    }

    /// 发送带 JSON body 的 POST 请求
    pub async fn post_json<T: serde::Serialize>(
        &self,
        url: &str,
        body: &T,
    ) -> BdResult<reqwest::Response> {
        self.client
            .post(url)
            .json(body)
            .send()
            .await
            .map_err(BdError::from)
    }
}

impl Default for BdClient {
    fn default() -> Self {
        Self::new().expect("创建默认客户端失败")
    }
}

/// 客户端配置
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// 超时时间（秒）
    pub timeout_secs: u64,
    /// User-Agent
    pub user_agent: String,
    /// 代理 URL
    pub proxy: Option<String>,
    /// 最大重定向次数
    pub max_redirects: usize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            user_agent: DEFAULT_UA.to_string(),
            proxy: None,
            max_redirects: 10,
        }
    }
}

impl ClientConfig {
    /// 使用直播专用配置
    pub fn live() -> Self {
        Self {
            user_agent: LIVE_UA.to_string(),
            ..Default::default()
        }
    }

    /// 使用移动端配置
    pub fn mobile() -> Self {
        Self {
            user_agent: MOBILE_UA.to_string(),
            ..Default::default()
        }
    }

    /// 设置代理
    pub fn with_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.proxy = Some(proxy.into());
        self
    }

    /// 设置超时
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
}
