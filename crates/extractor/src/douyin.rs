//! 抖音视频解析模块
//!
//! 使用 bytedance 包实现抖音视频和用户主页的解析

use crate::error::{ExtractError, ExtractResult};
use bytedance::douyin::DouyinApi;
use magekit_shared::{ChannelInfo, ChannelVideoEntry, PlatformCookie, VideoFormat, VideoInfo};
use std::time::Duration;

/// 设置 Cookie 的辅助函数
fn setup_api_cookie(api: &mut DouyinApi, cookies: Option<&[PlatformCookie]>) {
    if let Some(cookie_list) = cookies {
        if let Some(c) = cookie_list
            .iter()
            .find(|c| c.enabled && c.platform.to_lowercase().contains("douyin"))
        {
            api.set_cookie(&c.cookie);
        }
    }
}

/// 提取抖音视频信息
pub async fn extract_video_info(
    url: &str,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<VideoInfo> {
    tracing::info!("🔍 开始解析抖音视频: {}", url);

    // 创建 API 客户端
    let mut api = DouyinApi::new()
        .map_err(|e| ExtractError::Network(format!("创建 DouyinApi 失败: {}", e)))?;

    // 设置 Cookie
    setup_api_cookie(&mut api, cookies);

    // 解析短链接
    let resolved = api
        .resolve_short_url(url)
        .await
        .unwrap_or_else(|_| url.to_string());
    tracing::debug!("📍 解析后的 URL: {}", resolved);

    // 提取 aweme_id
    let aweme_id = DouyinApi::extract_aweme_id(&resolved)
        .or_else(|| DouyinApi::extract_aweme_id(url))
        .ok_or_else(|| ExtractError::InvalidUrl(resolved.clone()))?;
    tracing::info!("📝 提取到 aweme_id: {}", aweme_id);

    // 获取视频详情
    let aweme_info = api
        .get_aweme_info(&aweme_id)
        .await
        .map_err(|e| ExtractError::Network(format!("获取视频详情失败: {}", e)))?;

    // 转换为 VideoInfo
    let mut formats = Vec::new();

    if let Some(video) = &aweme_info.video {
        // 使用 bytedance 返回的格式
        for fmt in &video.formats {
            formats.push(VideoFormat {
                format_id: fmt.format_id.clone(),
                ext: fmt.ext.clone(),
                resolution: fmt.resolution.clone(),
                fps: None,
                filesize: fmt.filesize,
                vcodec: None,
                acodec: None,
                quality: fmt.quality.clone(),
                download_url: fmt.download_url.clone(),
            });
        }

        // 如果没有格式，使用播放地址
        if formats.is_empty() && !video.play_urls.is_empty() {
            let resolution = match (video.width, video.height) {
                (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
                _ => None,
            };
            formats.push(VideoFormat {
                format_id: "play".to_string(),
                ext: "mp4".to_string(),
                resolution,
                fps: None,
                filesize: None,
                vcodec: None,
                acodec: None,
                quality: None,
                download_url: video.play_urls.first().cloned(),
            });
        }
    }

    let duration = aweme_info.video.as_ref().and_then(|v| {
        v.duration.map(|d| {
            // 如果时长大于 10000，说明是毫秒
            if d > 10_000 {
                Duration::from_millis(d)
            } else {
                Duration::from_secs(d)
            }
        })
    });

    let thumbnail = aweme_info.video.as_ref().and_then(|v| v.cover_url.clone());
    let uploader = aweme_info.author.as_ref().and_then(|a| a.nickname.clone());

    Ok(VideoInfo {
        id: aweme_info.aweme_id,
        title: aweme_info.desc.clone(),
        description: Some(aweme_info.desc),
        duration,
        uploader,
        upload_date: None,
        thumbnail,
        formats,
        url: url.to_string(),
    })
}

/// 检测 URL 是否为抖音用户主页
pub fn is_douyin_user_url(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    url_lower.contains("douyin.com/user/") || url_lower.contains("sec_user_id=")
}

/// 获取抖音用户主页的作品列表
pub async fn extract_channel_info(
    url: &str,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<ChannelInfo> {
    tracing::info!("🔍 开始解析抖音用户主页: {}", url);

    // 提取 sec_user_id
    let sec_user_id = DouyinApi::extract_sec_user_id(url)
        .ok_or_else(|| ExtractError::Parse("无法从 URL 中提取 sec_user_id".to_string()))?;

    tracing::info!("   sec_user_id: {}", sec_user_id);

    // 创建 API 客户端
    let mut api = DouyinApi::new()
        .map_err(|e| ExtractError::Network(format!("创建 DouyinApi 失败: {}", e)))?;

    // 设置 Cookie
    setup_api_cookie(&mut api, cookies);

    // 获取用户信息
    let user_info = api
        .get_user_info(&sec_user_id)
        .await
        .map_err(|e| ExtractError::Network(format!("获取用户信息失败: {}", e)))?;

    tracing::info!("   用户名: {}", user_info.nickname);
    tracing::info!("   作品数: {:?}", user_info.aweme_count);

    // 获取用户作品列表（分页获取）
    let mut entries: Vec<ChannelVideoEntry> = Vec::new();
    let mut max_cursor: i64 = 0;
    let count: i64 = 20;
    let max_pages = 50; // 最多获取 50 页

    for page in 0..max_pages {
        tracing::info!("   获取第 {} 页作品 (cursor: {})", page + 1, max_cursor);

        let posts_json = api
            .get_user_posts(&sec_user_id, max_cursor, count)
            .await
            .map_err(|e| ExtractError::Network(format!("获取用户作品失败: {}", e)))?;

        // 解析作品列表
        let aweme_list = posts_json
            .get("aweme_list")
            .and_then(|v| v.as_array())
            .unwrap_or(&Vec::new())
            .clone();

        if aweme_list.is_empty() {
            tracing::info!("   没有更多作品，停止获取");
            break;
        }

        for aweme in &aweme_list {
            let aweme_id = aweme["aweme_id"].as_str().unwrap_or("").to_string();
            let desc = aweme["desc"].as_str().unwrap_or(&aweme_id).to_string();
            let duration = aweme["video"]["duration"].as_u64().map(|d| d / 1000);
            let thumbnail = aweme["video"]["cover"]["url_list"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if !aweme_id.is_empty() {
                entries.push(ChannelVideoEntry {
                    id: aweme_id.clone(),
                    title: desc,
                    url: format!("https://www.douyin.com/video/{}", aweme_id),
                    duration,
                    thumbnail,
                    uploader: Some(user_info.nickname.clone()),
                    playlist_index: Some(entries.len() as u32 + 1),
                    selected: false,
                });
            }
        }

        tracing::info!(
            "   本页获取 {} 个作品，已获取总数: {}",
            aweme_list.len(),
            entries.len()
        );

        // 检查是否有更多
        let has_more = posts_json["has_more"].as_i64().unwrap_or(0) == 1;
        if !has_more {
            tracing::info!("   已获取全部作品");
            break;
        }

        // 更新 cursor
        max_cursor = posts_json["max_cursor"].as_i64().unwrap_or(0);

        // 稍微延迟避免请求过快
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    tracing::info!(
        "✅ 抖音用户主页解析完成: {} - 共 {} 个作品",
        user_info.nickname,
        entries.len()
    );

    Ok(ChannelInfo {
        id: sec_user_id,
        title: user_info.nickname.clone(),
        url: url.to_string(),
        uploader: Some(user_info.nickname),
        description: user_info.signature,
        video_count: entries.len(),
        thumbnail: user_info.avatar_url,
        tabs: Vec::new(),
        entries,
    })
}
