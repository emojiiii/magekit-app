//! 静态扫描器：从 HTML 内容中提取资源链接

use crate::filter::ResourceFilter;
use crate::types::{CapturedResource, ResourceType};
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashSet;
use url::Url;

/// 静态扫描器
pub struct StaticScanner {
    client: reqwest::Client,
}

impl StaticScanner {
    /// 创建新的扫描器
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// 扫描页面中的所有资源
    pub async fn scan_all(
        &self,
        target_url: &str,
        page_title: Option<String>,
        filter: &ResourceFilter,
    ) -> Result<Vec<CapturedResource>> {
        let mut results = Vec::new();
        let base_url = Url::parse(target_url).context("URL 不合法")?;

        // 获取页面内容
        let body = self
            .client
            .get(target_url)
            .send()
            .await
            .context("获取页面失败")?
            .text()
            .await
            .context("读取页面内容失败")?;

        let mut seen = HashSet::new();

        // 扫描各种资源模式
        let patterns = vec![
            // 视频
            (
                r#"(?:src|href|data-src|data-url)=["']([^"']*\.(?:mp4|m4v|mov|avi|mkv|webm|flv|f4v|m3u8|m3u|ts|mpd)[^"']*)["']"#,
                ResourceType::Video,
            ),
            // 音频
            (
                r#"(?:src|href|data-src|data-url)=["']([^"']*\.(?:mp3|m4a|aac|ogg|oga|opus|wav|flac|mka)[^"']*)["']"#,
                ResourceType::Audio,
            ),
            // 图片
            (
                r#"(?:src|href|data-src|data-url|data-original)=["']([^"']*\.(?:jpg|jpeg|png|gif|webp|svg|bmp|ico|avif)[^"']*)["']"#,
                ResourceType::Image,
            ),
            // 通用 URL 模式（在引号内）
            (
                r#"(?:src|href|data-src|data-url)=["']([^"']+://[^"']+)["']"#,
                ResourceType::Other,
            ),
        ];

        for (pattern, default_type) in patterns {
            let re = Regex::new(pattern).unwrap();
            for cap in re.captures_iter(&body) {
                if let Some(url_match) = cap.get(1) {
                    let candidate = url_match.as_str().trim_matches(['"', '\'', ',', ';']);
                    if candidate.len() < 5 {
                        continue;
                    }

                    let absolute_url = if let Ok(u) = Url::parse(candidate) {
                        u
                    } else if let Ok(joined) = base_url.join(candidate) {
                        joined
                    } else {
                        continue;
                    };

                    let final_url = absolute_url.to_string();
                    if seen.insert(final_url.clone()) {
                        let resource_type = ResourceType::from_url(&final_url);
                        let resource = CapturedResource {
                            url: final_url,
                            resource_type: if resource_type == ResourceType::Other {
                                default_type
                            } else {
                                resource_type
                            },
                            mime_type: None,
                            referer: Some(target_url.to_string()),
                            title: page_title.clone(),
                            size_bytes: None,
                            duration_seconds: None,
                            headers: None,
                        };

                        if filter.matches(&resource) {
                            results.push(resource);
                        }
                    }
                }
            }
        }

        // 特殊处理：直接检查目标 URL 是否是资源
        if let Ok(resp) = self.client.head(target_url).send().await {
            if let Some(content_type) = resp.headers().get(reqwest::header::CONTENT_TYPE) {
                if let Ok(mime) = content_type.to_str() {
                    // 优先用 URL 判断，判断不出来再用 MIME
                    let resource_type = {
                        let from_url = ResourceType::from_url(target_url);
                        if from_url == ResourceType::Other {
                            ResourceType::from_mime_type(mime)
                        } else {
                            from_url
                        }
                    };

                    if resource_type != ResourceType::Other {
                        let resource = CapturedResource {
                            url: target_url.to_string(),
                            resource_type,
                            mime_type: Some(mime.to_string()),
                            referer: None,
                            title: page_title.clone(),
                            size_bytes: resp
                                .headers()
                                .get(reqwest::header::CONTENT_LENGTH)
                                .and_then(|v| v.to_str().ok())
                                .and_then(|s| s.parse::<u64>().ok()),
                            duration_seconds: None,
                            headers: None,
                        };

                        if filter.matches(&resource) && seen.insert(target_url.to_string()) {
                            results.push(resource);
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// 获取页面标题
    pub async fn fetch_page_title(&self, url: &str) -> Result<Option<String>> {
        let body = self
            .client
            .get(url)
            .send()
            .await
            .context("获取页面失败")?
            .text()
            .await
            .context("读取页面内容失败")?;

        if let Some(start) = body.to_lowercase().find("<title>") {
            if let Some(end) = body.to_lowercase().find("</title>") {
                if end > start + 7 {
                    let title_raw = &body[start + 7..end];
                    let title = title_raw.trim().replace('\n', " ").replace('\r', " ");
                    return Ok(Some(title));
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scan_all() {
        let client = reqwest::Client::new();
        let scanner = StaticScanner::new(client);
        let filter = ResourceFilter::all();
        // 注意：这是一个集成测试，需要网络连接
        // 在实际测试中应该使用 mock server
    }
}
