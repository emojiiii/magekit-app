use crate::{error::RecorderResult, types::StreamInfo};
use async_trait::async_trait;

/// 平台 Cookie 配置
#[derive(Debug, Clone, Default)]
pub struct PlatformCookies {
    /// Cookie 字符串
    pub cookie: Option<String>,
    /// 用户名（某些平台需要登录）
    pub username: Option<String>,
    /// 密码
    pub password: Option<String>,
}

impl PlatformCookies {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cookie(mut self, cookie: impl Into<String>) -> Self {
        self.cookie = Some(cookie.into());
        self
    }

    pub fn with_credentials(
        mut self,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }
}

/// 平台特定的直播流获取器
#[async_trait]
pub trait PlatformHandler: Send + Sync {
    /// 平台名称
    fn platform_name(&self) -> &'static str;

    /// 支持的URL模式
    fn supported_url_patterns(&self) -> Vec<&'static str>;

    /// 检查URL是否支持
    fn supports_url(&self, url: &str) -> bool {
        for pattern in self.supported_url_patterns() {
            if url.contains(pattern) {
                return true;
            }
        }
        false
    }

    /// 从URL提取房间ID或其他标识符
    async fn extract_room_id(&self, url: &str) -> RecorderResult<String>;

    /// 获取直播流信息
    async fn get_stream_info(&self, room_id: &str) -> RecorderResult<StreamInfo>;

    /// 获取直播流信息（带 Cookie 支持）
    async fn get_stream_info_with_cookies(
        &self,
        room_id: &str,
        cookies: &PlatformCookies,
    ) -> RecorderResult<StreamInfo> {
        // 默认实现：忽略 cookies，调用基本方法
        let _ = cookies;
        self.get_stream_info(room_id).await
    }

    /// 检查房间是否在线
    async fn check_room_status(&self, room_id: &str) -> RecorderResult<bool> {
        let stream_info = self.get_stream_info(room_id).await?;
        Ok(stream_info.room.status == crate::types::LiveStatus::Live)
    }

    /// 检查房间是否在线（带 Cookie 支持）
    async fn check_room_status_with_cookies(
        &self,
        room_id: &str,
        cookies: &PlatformCookies,
    ) -> RecorderResult<bool> {
        let stream_info = self.get_stream_info_with_cookies(room_id, cookies).await?;
        Ok(stream_info.room.status == crate::types::LiveStatus::Live)
    }
}

pub mod bilibili;
pub mod douyin;
pub mod douyu;
pub mod factory;
pub mod huya;
pub mod kuaishou;
pub mod soop;

pub use bilibili::BilibiliHandler;
pub use douyin::DouyinHandler;
pub use douyu::DouyuHandler;
pub use factory::PlatformFactory;
pub use huya::HuyaHandler;
pub use kuaishou::KuaishouHandler;
pub use soop::SoopHandler;
