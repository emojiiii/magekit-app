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
        match magekit_shared::platform_from_url(url) {
            Some("douyin") => Platform::Douyin,
            Some("tiktok") => Platform::Tiktok,
            Some("bilibili") => Platform::Bilibili,
            Some("youtube") => Platform::Youtube,
            Some("twitter") => Platform::Twitter,
            Some("instagram") => Platform::Instagram,
            Some("weibo") => Platform::Weibo,
            Some("xiaohongshu") => Platform::Xiaohongshu,
            _ => Platform::Unknown,
        }
    }
}
