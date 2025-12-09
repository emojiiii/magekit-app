use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Douyin,
    Tiktok,
    Bilibili,
    Youtube,
    Twitter,
    Instagram,
    Weibo,
    Xiaohongshu,
    Unknown,
}

pub trait PlatformSupport {
    fn all_supported() -> Vec<Platform>;
    fn detect(url: &str) -> Platform;
}

impl PlatformSupport for Platform {
    fn all_supported() -> Vec<Platform> {
        vec![
            Platform::Douyin,
            Platform::Tiktok,
            Platform::Bilibili,
            Platform::Youtube,
            Platform::Twitter,
            Platform::Instagram,
            Platform::Weibo,
            Platform::Xiaohongshu,
        ]
    }

    fn detect(url: &str) -> Platform {
        let url_lower = url.to_lowercase();
        if url_lower.contains("douyin.com") || url_lower.contains("iesdouyin.com") {
            Platform::Douyin
        } else if url_lower.contains("tiktok.com") {
            Platform::Tiktok
        } else if url_lower.contains("bilibili.com") || url_lower.contains("b23.tv") {
            Platform::Bilibili
        } else if url_lower.contains("youtube.com") || url_lower.contains("youtu.be") {
            Platform::Youtube
        } else if url_lower.contains("twitter.com") || url_lower.contains("x.com") {
            Platform::Twitter
        } else if url_lower.contains("instagram.com") {
            Platform::Instagram
        } else if url_lower.contains("weibo.com") {
            Platform::Weibo
        } else if url_lower.contains("xiaohongshu.com") || url_lower.contains("xhs.link") {
            Platform::Xiaohongshu
        } else {
            // 尝试从短链中提取域名
            let re = Regex::new(r"https?://([^/]+)/").unwrap();
            if let Some(caps) = re.captures(url) {
                let host = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
                match host {
                    h if h.contains("douyin") => Platform::Douyin,
                    h if h.contains("tiktok") => Platform::Tiktok,
                    _ => Platform::Unknown,
                }
            } else {
                Platform::Unknown
            }
        }
    }
}
