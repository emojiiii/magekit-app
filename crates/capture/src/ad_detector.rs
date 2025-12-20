//! 广告检测模块

use crate::types::CapturedResource;

/// 广告检测器
pub struct AdDetector {
    /// 广告 URL 关键词
    ad_keywords: Vec<&'static str>,
    /// 广告域名
    ad_domains: Vec<&'static str>,
}

impl AdDetector {
    /// 创建新的广告检测器
    pub fn new() -> Self {
        Self {
            ad_keywords: vec![
                "ad",
                "ads",
                "advertisement",
                "advertising",
                "advert",
                "banner",
                "sponsor",
                "promo",
                "promotion",
                "doubleclick",
                "googleadservices",
                "googlesyndication",
                "adserver",
                "adtech",
                "advertising.com",
                "/ad/",
                "/ads/",
                "/advertisement/",
                "/advertising/",
            ],
            ad_domains: vec![
                "doubleclick.net",
                "googleadservices.com",
                "googlesyndication.com",
                "advertising.com",
                "adtech.com",
                "adsrvr.org",
                "adnxs.com",
                "rubiconproject.com",
                "pubmatic.com",
                "openx.net",
            ],
        }
    }

    /// 检测资源是否是广告
    pub fn is_ad(&self, resource: &CapturedResource) -> bool {
        let url_lower = resource.url.to_lowercase();

        // 检查 URL 关键词
        for keyword in &self.ad_keywords {
            if url_lower.contains(keyword) {
                return true;
            }
        }

        // 检查域名
        if let Ok(parsed) = url::Url::parse(&resource.url) {
            if let Some(host) = parsed.host_str() {
                let host_lower = host.to_lowercase();
                for domain in &self.ad_domains {
                    if host_lower.contains(domain) {
                        return true;
                    }
                }
            }
        }

        // 检查资源大小（广告通常很小，小于 100KB）
        if let Some(size) = resource.size_bytes {
            if size < 100 * 1024 {
                // 小文件 + 视频/音频类型可能是广告
                if matches!(
                    resource.resource_type,
                    crate::types::ResourceType::Video | crate::types::ResourceType::Audio
                ) {
                    // 进一步检查：如果时长很短（小于 30 秒），更可能是广告
                    if let Some(duration) = resource.duration_seconds {
                        if duration < 30.0 {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }
}

impl Default for AdDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ad_detection() {
        let detector = AdDetector::new();

        // 测试广告 URL
        let ad_resource = CapturedResource {
            url: "https://example.com/ads/video.mp4".to_string(),
            resource_type: crate::types::ResourceType::Video,
            mime_type: None,
            referer: None,
            title: None,
            size_bytes: None,
            duration_seconds: None,
            headers: None,
        };
        assert!(detector.is_ad(&ad_resource));

        // 测试正常资源
        let normal_resource = CapturedResource {
            url: "https://example.com/video/main.mp4".to_string(),
            resource_type: crate::types::ResourceType::Video,
            mime_type: None,
            referer: None,
            title: None,
            size_bytes: Some(10 * 1024 * 1024), // 10MB
            duration_seconds: Some(300.0),      // 5 分钟
            headers: None,
        };
        assert!(!detector.is_ad(&normal_resource));
    }
}
