//! TikTok 视频解析模块
//!
//! 使用 platform-api 的 X-Bogus 签名实现 TikTok 视频解析

use crate::error::{ExtractError, ExtractResult};
use platform_api::xbogus_sign;
use magekit_shared::{PlatformCookie, VideoFormat, VideoInfo};
use regex::Regex;
use std::collections::HashMap;
use std::time::Duration;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

/// 获取当前时间戳（秒）
fn get_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 生成请求参数
fn build_params(item_id: &str) -> HashMap<&'static str, String> {
    let timestamp = get_timestamp().to_string();
    let mut params = HashMap::new();
    params.insert("WebIdLastTime", timestamp);
    params.insert("aid", "1988".to_string());
    params.insert("app_language", "en".to_string());
    params.insert("app_name", "tiktok_web".to_string());
    params.insert("browser_language", "en-US".to_string());
    params.insert("browser_name", "Mozilla".to_string());
    params.insert("browser_online", "true".to_string());
    params.insert("browser_platform", "Win32".to_string());
    params.insert("browser_version", "5.0%20(Windows)".to_string());
    params.insert("channel", "tiktok_web".to_string());
    params.insert("cookie_enabled", "true".to_string());
    params.insert("device_id", "7380187414842836523".to_string());
    params.insert("device_platform", "web_pc".to_string());
    params.insert("focus_state", "true".to_string());
    params.insert("from_page", "user".to_string());
    params.insert("history_len", "4".to_string());
    params.insert("is_fullscreen", "false".to_string());
    params.insert("is_page_visible", "true".to_string());
    params.insert("language", "en".to_string());
    params.insert("os", "windows".to_string());
    params.insert("priority_region", "US".to_string());
    params.insert("referer", "".to_string());
    params.insert("region", "US".to_string());
    params.insert("screen_height", "1080".to_string());
    params.insert("screen_width", "1920".to_string());
    params.insert("webcast_language", "en".to_string());
    params.insert("tz_name", "America%2FTijuana".to_string());
    params.insert("itemId", item_id.to_string());
    params
}

/// 将参数转换为 URL 编码的查询字符串
fn params_to_query(params: &HashMap<&str, String>) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&")
}

pub async fn extract_video_info(
    url: &str,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<VideoInfo> {
    tracing::info!("🔍 开始解析 TikTok 视频: {}", url);

    let client = reqwest::Client::builder()
        .user_agent(UA)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| ExtractError::Network(e.to_string()))?;

    let resolved = resolve_short_link(&client, url)
        .await
        .unwrap_or_else(|_| url.to_string());
    tracing::debug!("📍 解析后的 URL: {}", resolved);

    let item_id = extract_item_id(&resolved)
        .or_else(|| extract_item_id(url))
        .ok_or_else(|| ExtractError::InvalidUrl(resolved.clone()))?;
    tracing::info!("📝 提取到 itemId: {}", item_id);

    let cookie_header = cookies
        .and_then(|list| {
            list.iter()
                .find(|c| c.enabled && c.platform.to_lowercase().contains("tiktok"))
        })
        .map(|c| c.cookie.clone());

    // 构建参数
    let params = build_params(&item_id);
    let query = params_to_query(&params);

    // 使用 platform-api 的 X-Bogus 签名
    let xbogus_result = xbogus_sign(&query, UA, None);

    let api_url = format!(
        "https://www.tiktok.com/api/item/detail/?{}&X-Bogus={}",
        query, xbogus_result.signature
    );
    tracing::debug!("🌐 API URL: {}", api_url);

    let mut request = client
        .get(&api_url)
        .header("Referer", "https://www.tiktok.com/")
        .header("Accept", "application/json, text/plain, */*")
        .header("Accept-Language", "en-US,en;q=0.9");

    if let Some(cookie) = &cookie_header {
        request = request.header("Cookie", cookie);
    }

    let resp = request
        .send()
        .await
        .map_err(|e| ExtractError::Network(e.to_string()))?;

    let status = resp.status();
    tracing::debug!("📡 响应状态: {}", status);

    if !status.is_success() {
        return Err(ExtractError::Network(format!("status {}", status)));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| ExtractError::Parse(e.to_string()))?;
    tracing::debug!("📦 响应长度: {} bytes", body.len());

    let api_resp: TiktokResponse =
        serde_json::from_str(&body).map_err(|e| ExtractError::Parse(e.to_string()))?;

    let item = api_resp
        .item_info
        .and_then(|i| i.item_struct)
        .ok_or_else(|| ExtractError::Parse("missing itemStruct".to_string()))?;

    let video = item
        .video
        .ok_or_else(|| ExtractError::Parse("missing video".to_string()))?;

    let mut formats = Vec::new();
    if let Some(play_addr) = video.download_addr {
        let url = play_addr.url_list.as_ref().and_then(|l| l.first()).cloned();
        let resolution = match (video.width, video.height) {
            (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
            _ => None,
        };
        formats.push(VideoFormat {
            format_id: "download".to_string(),
            ext: "mp4".to_string(),
            resolution,
            fps: video.ratio.map(|_| 30.0),
            filesize: None,
            vcodec: None,
            acodec: None,
            quality: Some("download".to_string()),
            download_url: url,
        });
    }

    Ok(VideoInfo {
        id: item.id.unwrap_or_else(|| item_id.clone()),
        title: item
            .desc
            .clone()
            .unwrap_or_else(|| "TikTok 视频".to_string()),
        description: item.desc,
        duration: item.duration.map(|d| Duration::from_secs(d as u64)),
        uploader: item.author.map(|a| a.nickname.unwrap_or_default()),
        upload_date: None,
        thumbnail: video
            .cover
            .and_then(|c| c.url_list.and_then(|l| l.first().cloned())),
        formats,
        url: url.to_string(),
    })
}

async fn resolve_short_link(client: &reqwest::Client, url: &str) -> ExtractResult<String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| ExtractError::Network(e.to_string()))?;
    Ok(resp.url().to_string())
}

fn extract_item_id(url: &str) -> Option<String> {
    // 支持多种 URL 格式：
    // - https://www.tiktok.com/@username/video/7339393672959757570
    // - https://www.tiktok.com/@username/photo/7339393672959757570
    // - https://vm.tiktok.com/ZMxxxxxxx/（短链接会被重定向）
    // - itemId=7339393672959757570（直接参数）
    let patterns = [r"/video/(\d+)", r"/photo/(\d+)", r"itemId=(\d+)"];
    for pat in &patterns {
        let re = Regex::new(pat).ok()?;
        if let Some(caps) = re.captures(url) {
            if let Some(id) = caps.get(1) {
                return Some(id.as_str().to_string());
            }
        }
    }
    None
}

#[derive(Debug, serde::Deserialize)]
struct TiktokResponse {
    #[serde(default)]
    item_info: Option<ItemInfo>,
}

#[derive(Debug, serde::Deserialize)]
struct ItemInfo {
    #[serde(rename = "itemStruct")]
    #[serde(default)]
    item_struct: Option<ItemStruct>,
}

#[derive(Debug, serde::Deserialize)]
struct ItemStruct {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    author: Option<Author>,
    #[serde(default)]
    video: Option<VideoData>,
    #[serde(default)]
    duration: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
struct Author {
    #[serde(default)]
    nickname: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct VideoData {
    #[serde(default)]
    download_addr: Option<PlayAddr>,
    #[serde(default)]
    cover: Option<PlayAddr>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    ratio: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct PlayAddr {
    #[serde(default)]
    url_list: Option<Vec<String>>,
}
