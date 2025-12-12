//! 资源筛选器

use crate::types::{CapturedResource, ResourceType};
use std::collections::HashSet;

/// 资源筛选器
#[derive(Debug, Clone)]
pub struct ResourceFilter {
    /// 允许的资源类型集合（如果为空，则允许所有类型）
    allowed_types: HashSet<ResourceType>,
    /// 排除的 URL 模式（包含这些字符串的 URL 将被过滤）
    exclude_patterns: Vec<String>,
    /// 最小文件大小（字节，None 表示不限制）
    min_size: Option<u64>,
    /// 最大文件大小（字节，None 表示不限制）
    max_size: Option<u64>,
}

impl ResourceFilter {
    /// 创建允许所有资源的筛选器
    pub fn all() -> Self {
        Self {
            allowed_types: HashSet::new(),
            exclude_patterns: Vec::new(),
            min_size: None,
            max_size: None,
        }
    }

    /// 创建只允许指定类型的筛选器
    pub fn only_types(types: &[ResourceType]) -> Self {
        Self {
            allowed_types: types.iter().copied().collect(),
            exclude_patterns: Vec::new(),
            min_size: None,
            max_size: None,
        }
    }

    /// 创建只允许视频的筛选器
    pub fn video_only() -> Self {
        Self::only_types(&[ResourceType::Video])
    }

    /// 创建只允许音频的筛选器
    pub fn audio_only() -> Self {
        Self::only_types(&[ResourceType::Audio])
    }

    /// 创建只允许图片的筛选器
    pub fn image_only() -> Self {
        Self::only_types(&[ResourceType::Image])
    }

    /// 创建允许视频和音频的筛选器
    pub fn media_only() -> Self {
        Self::only_types(&[ResourceType::Video, ResourceType::Audio])
    }

    /// 添加排除模式
    pub fn exclude_pattern(mut self, pattern: String) -> Self {
        self.exclude_patterns.push(pattern);
        self
    }

    /// 设置最小文件大小
    pub fn min_size(mut self, bytes: u64) -> Self {
        self.min_size = Some(bytes);
        self
    }

    /// 设置最大文件大小
    pub fn max_size(mut self, bytes: u64) -> Self {
        self.max_size = Some(bytes);
        self
    }

    /// 检查资源是否通过筛选
    pub fn matches(&self, resource: &CapturedResource) -> bool {
        // 检查资源类型
        if !self.allowed_types.is_empty() {
            if !self.allowed_types.contains(&resource.resource_type) {
                return false;
            }
        }

        // 检查排除模式
        for pattern in &self.exclude_patterns {
            if resource.url.contains(pattern) {
                return false;
            }
        }

        // 检查文件大小
        if let Some(size) = resource.size_bytes {
            if let Some(min) = self.min_size {
                if size < min {
                    return false;
                }
            }
            if let Some(max) = self.max_size {
                if size > max {
                    return false;
                }
            }
        }

        true
    }
}

impl Default for ResourceFilter {
    fn default() -> Self {
        Self::all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_all() {
        let filter = ResourceFilter::all();
        let resource = CapturedResource {
            url: "https://example.com/video.mp4".to_string(),
            resource_type: ResourceType::Video,
            mime_type: None,
            referer: None,
            title: None,
            size_bytes: None,
            duration_seconds: None,
            headers: None,
        };
        assert!(filter.matches(&resource));
    }

    #[test]
    fn test_filter_video_only() {
        let filter = ResourceFilter::video_only();
        let video = CapturedResource {
            url: "https://example.com/video.mp4".to_string(),
            resource_type: ResourceType::Video,
            mime_type: None,
            referer: None,
            title: None,
            size_bytes: None,
            duration_seconds: None,
            headers: None,
        };
        let audio = CapturedResource {
            url: "https://example.com/audio.mp3".to_string(),
            resource_type: ResourceType::Audio,
            mime_type: None,
            referer: None,
            title: None,
            size_bytes: None,
            duration_seconds: None,
            headers: None,
        };
        assert!(filter.matches(&video));
        assert!(!filter.matches(&audio));
    }

    #[test]
    fn test_filter_exclude_pattern() {
        let filter = ResourceFilter::all().exclude_pattern("ad".to_string());
        let resource1 = CapturedResource {
            url: "https://example.com/video.mp4".to_string(),
            resource_type: ResourceType::Video,
            mime_type: None,
            referer: None,
            title: None,
            size_bytes: None,
            duration_seconds: None,
            headers: None,
        };
        let resource2 = CapturedResource {
            url: "https://example.com/ad.mp4".to_string(),
            resource_type: ResourceType::Video,
            mime_type: None,
            referer: None,
            title: None,
            size_bytes: None,
            duration_seconds: None,
            headers: None,
        };
        assert!(filter.matches(&resource1));
        assert!(!filter.matches(&resource2));
    }
}
