use crate::{
    error::{RecorderError, RecorderResult},
    platforms::PlatformHandler,
};
use std::collections::HashMap;
use std::sync::Arc;

/// 平台工厂
pub struct PlatformFactory {
    handlers: HashMap<String, Arc<dyn PlatformHandler>>,
}

impl PlatformFactory {
    /// 创建新的平台工厂
    pub fn new() -> Self {
        let mut factory = Self {
            handlers: HashMap::new(),
        };

        // 注册默认的平台处理器
        factory.register_platform(Arc::new(crate::platforms::DouyinHandler::new()));
        factory.register_platform(Arc::new(crate::platforms::BilibiliHandler::new()));
        factory.register_platform(Arc::new(crate::platforms::HuyaHandler::new()));
        factory.register_platform(Arc::new(crate::platforms::DouyuHandler::new()));
        factory.register_platform(Arc::new(crate::platforms::KuaishouHandler::new()));
        factory.register_platform(Arc::new(crate::platforms::SoopKrHandler::new()));
        factory.register_platform(Arc::new(crate::platforms::SoopGlobalHandler::new()));

        factory
    }

    /// 注册平台处理器
    pub fn register_platform(&mut self, handler: Arc<dyn PlatformHandler>) {
        self.handlers
            .insert(handler.platform_name().to_string(), handler);
    }

    /// 根据URL获取对应的平台处理器
    pub fn get_handler_for_url(&self, url: &str) -> RecorderResult<Arc<dyn PlatformHandler>> {
        for handler in self.handlers.values() {
            if handler.supports_url(url) {
                return Ok(handler.clone());
            }
        }

        Err(RecorderError::UnsupportedPlatform(url.to_string()))
    }

    /// 根据平台名称获取处理器
    pub fn get_handler_by_name(
        &self,
        platform_name: &str,
    ) -> RecorderResult<Arc<dyn PlatformHandler>> {
        self.handlers
            .get(platform_name)
            .cloned()
            .ok_or_else(|| RecorderError::UnsupportedPlatform(platform_name.to_string()))
    }

    /// 获取所有支持的平台名称
    pub fn supported_platforms(&self) -> Vec<String> {
        self.handlers.keys().cloned().collect()
    }
}

impl Default for PlatformFactory {
    fn default() -> Self {
        Self::new()
    }
}
