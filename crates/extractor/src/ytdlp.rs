use crate::cookies::build_cookie_header;
use crate::error::{ExtractError, ExtractResult};
use crate::platform::{Platform, PlatformSupport};
use magekit_shared::{
    ChannelInfo, ChannelPageResult, ChannelTab, ChannelTabType, ChannelVideoEntry, PlatformCookie,
    VideoFormat, VideoInfo, create_tokio_command,
};
use serde::Deserialize;
use std::path::Path;

fn extract_platform_from_url(url: &str) -> Option<String> {
    match Platform::detect(url) {
        Platform::Douyin => Some("douyin".to_string()),
        Platform::Tiktok => Some("tiktok".to_string()),
        Platform::Bilibili => Some("bilibili".to_string()),
        Platform::Youtube => Some("youtube".to_string()),
        Platform::Twitter => Some("twitter".to_string()),
        Platform::Instagram => Some("instagram".to_string()),
        Platform::Weibo => Some("weibo".to_string()),
        Platform::Xiaohongshu => Some("xiaohongshu".to_string()),
        Platform::Unknown => None,
    }
}

/// 规范化 URL，确保有协议前缀
fn normalize_url(url: &str) -> String {
    let url_trimmed = url.trim();

    // 如果已经有协议，直接返回
    if url_trimmed.starts_with("http://") || url_trimmed.starts_with("https://") {
        return url_trimmed.to_string();
    }

    // 如果没有协议，添加 https://
    // 同时处理常见的输入错误，如 "www.bilibili.com" 或 "bilibili.com"
    if url_trimmed.starts_with("www.") {
        format!("https://{}", url_trimmed)
    } else {
        // 检测平台，添加合适的域名前缀
        let platform = Platform::detect(url_trimmed);
        match platform {
            Platform::Bilibili => {
                if url_trimmed.starts_with("bilibili.com") || url_trimmed.starts_with("b23.tv") {
                    format!("https://{}", url_trimmed)
                } else {
                    format!(
                        "https://www.bilibili.com/{}",
                        url_trimmed.trim_start_matches('/')
                    )
                }
            }
            Platform::Youtube => {
                if url_trimmed.starts_with("youtube.com") || url_trimmed.starts_with("youtu.be") {
                    format!("https://{}", url_trimmed)
                } else {
                    format!(
                        "https://www.youtube.com/{}",
                        url_trimmed.trim_start_matches('/')
                    )
                }
            }
            _ => {
                // 对于其他平台，简单地添加 https://
                format!("https://{}", url_trimmed)
            }
        }
    }
}

fn normalize_youtube_channel_default_tab(url: &str) -> String {
    // ytdlp 对 YouTube 频道根路径（/@handle、/channel/..、/user/..、/c/..）在
    // `--flat-playlist` 模式下可能返回“tab 列表”而非视频列表（例如只拿到 Videos/Shorts/Live 三条）。
    // 这里将根路径规范化到 `/videos` 作为默认作品列表入口。
    let lower = url.to_lowercase();
    if !lower.contains("youtube.com/") {
        return url.to_string();
    }
    if lower.contains("/playlist") || lower.contains("list=") {
        return url.to_string();
    }
    if lower.contains("/videos")
        || lower.contains("/shorts")
        || lower.contains("/streams")
        || lower.contains("/playlists")
    {
        return url.to_string();
    }

    let mut suffix_pos = url.len();
    if let Some(pos) = url.find('?') {
        suffix_pos = suffix_pos.min(pos);
    }
    if let Some(pos) = url.find('#') {
        suffix_pos = suffix_pos.min(pos);
    }
    let (base, suffix) = url.split_at(suffix_pos);
    let mut base = base.trim_end_matches('/').to_string();

    for home in ["/featured", "/home"] {
        if base.to_lowercase().ends_with(home) {
            base = base[..base.len() - home.len()].trim_end_matches('/').to_string();
            break;
        }
    }

    let lower_base = base.to_lowercase();
    let is_root = |prefix: &str| {
        if let Some(idx) = lower_base.find(prefix) {
            let rest = &lower_base[(idx + prefix.len())..];
            !rest.is_empty() && !rest.contains('/')
        } else {
            false
        }
    };

    if is_root("youtube.com/@")
        || is_root("youtube.com/channel/")
        || is_root("youtube.com/user/")
        || is_root("youtube.com/c/")
    {
        format!("{}/videos{}", base, suffix)
    } else {
        url.to_string()
    }
}

fn normalize_ytdlp_channel_url(url: &str) -> String {
    // 某些平台的“频道/作者页”会携带跟踪 query（如 bilibili spm），容易导致解析器走偏。
    // 这里仅对明确安全的场景做清洗，避免影响 playlist 等依赖 query 的 URL。
    let lower = url.to_lowercase();
    if lower.contains("space.bilibili.com/") {
        return url
            .split(|c| c == '?' || c == '#')
            .next()
            .unwrap_or(url)
            .to_string();
    }
    url.to_string()
}

fn fallback_channel_id(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_lowercase();
    if host == "space.bilibili.com" {
        let seg = parsed.path_segments()?.next()?;
        if !seg.is_empty() {
            return Some(seg.to_string());
        }
    }
    None
}

pub async fn extract_video_info(
    url: &str,
    cookies: Option<&[PlatformCookie]>,
    yt_dlp_path: &Path,
) -> ExtractResult<VideoInfo> {
    // 🔧 规范化 URL
    let normalized_url = normalize_url(url);
    tracing::info!("🔧 URL 规范化: {} -> {}", url, normalized_url);

    let cookie_header = build_cookie_header(
        extract_platform_from_url(&normalized_url).as_deref(),
        cookies,
    );

    let mut cmd = create_tokio_command(yt_dlp_path);
    cmd.arg("--dump-json")
        .arg("--no-download")
        .arg(&normalized_url);

    if let Some(ref cookie) = cookie_header {
        cmd.arg("--add-header").arg(format!("Cookie: {}", cookie));
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| ExtractError::CommandFailed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ExtractError::CommandFailed(stderr.to_string()));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let video_data: VideoInfoData =
        serde_json::from_str(&json_str).map_err(|e| ExtractError::Parse(e.to_string()))?;

    Ok(video_data.into())
}

pub async fn extract_channel_info(
    url: &str,
    cookies: Option<&[PlatformCookie]>,
    yt_dlp_path: &Path,
) -> ExtractResult<ChannelInfo> {
    // 🔧 规范化 URL
    let normalized_url = normalize_url(url);
    tracing::info!("🔧 URL 规范化: {} -> {}", url, normalized_url);

    let cookie_header = build_cookie_header(
        extract_platform_from_url(&normalized_url).as_deref(),
        cookies,
    );

    let mut cmd = create_tokio_command(yt_dlp_path);
    cmd.arg("--flat-playlist")
        .arg("--dump-single-json")
        .arg("--no-warnings")
        .arg("--extractor-args")
        .arg("BiliBiliSpace:metadata=true")
        .arg("-v")
        .arg(normalize_youtube_channel_default_tab(&normalized_url));

    if let Some(ref cookie) = cookie_header {
        cmd.arg("--add-header").arg(format!("Cookie: {}", cookie));
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| ExtractError::CommandFailed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ExtractError::CommandFailed(stderr.to_string()));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let channel_data: ChannelInfoData =
        serde_json::from_str(&json_str).map_err(|e| ExtractError::Parse(e.to_string()))?;

    let mut entries: Vec<ChannelVideoEntry> = Vec::new();
    let mut tabs: Vec<ChannelTab> = Vec::new();

    if let Some(ref data_entries) = channel_data.entries {
        fn collect_tab_entries(entry: &serde_json::Value, entries: &mut Vec<ChannelVideoEntry>) {
            if let Some(nested_entries) = entry.get("entries").and_then(|e| e.as_array()) {
                for nested_entry in nested_entries {
                    collect_tab_entries(nested_entry, entries);
                }
            } else if let Some(video_entry) = parse_channel_entry(entry) {
                entries.push(video_entry);
            }
        }

        for entry in data_entries {
            if let Some(nested_entries) = entry.get("entries").and_then(|e| e.as_array()) {
                let tab_title = entry
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("视频");
                let tab_type = ChannelTabType::from_title(tab_title);

                let mut tab_entries = Vec::new();
                for nested_entry in nested_entries {
                    collect_tab_entries(nested_entry, &mut tab_entries);
                }

                entries.extend(tab_entries.clone());
                tabs.push(ChannelTab {
                    tab_type,
                    title: tab_title.to_string(),
                    entries: tab_entries,
                });
            } else if let Some(video_entry) = parse_channel_entry(entry) {
                entries.push(video_entry);
            }
        }
    }

    if entries.is_empty() {
        for (index, line) in json_str.lines().skip(1).enumerate() {
            if let Ok(entry_data) = serde_json::from_str::<ChannelEntryData>(line) {
                entries.push(ChannelVideoEntry {
                    id: entry_data.id.clone(),
                    title: entry_data.title.unwrap_or_else(|| entry_data.id.clone()),
                    url: entry_data
                        .url
                        .or(entry_data.webpage_url)
                        .unwrap_or_else(|| {
                            format!("https://www.youtube.com/watch?v={}", entry_data.id)
                        }),
                    duration: entry_data.duration.map(|d| d as u64),
                    thumbnail: entry_data.thumbnail,
                    uploader: entry_data.uploader,
                    playlist_index: Some(index as u32 + 1),
                    selected: false,
                });
            }
        }
    }

    let playlist_count = channel_data.playlist_count.and_then(|n| usize::try_from(n).ok());
    let video_count = playlist_count.unwrap_or_else(|| entries.len());
    Ok(ChannelInfo {
        id: channel_data.id.unwrap_or_else(|| "unknown".to_string()),
        title: channel_data.title.unwrap_or_else(|| "未知频道".to_string()),
        url: url.to_string(),
        uploader: channel_data.uploader.or(channel_data.channel),
        description: channel_data.description,
        video_count,
        thumbnail: channel_data.thumbnail,
        tabs,
        entries,
    })
}

fn playlist_items_spec(offset: i64, count: usize) -> Option<String> {
    if count == 0 || offset < 0 {
        return None;
    }
    let start = offset.saturating_add(1);
    let end = start.saturating_add(i64::try_from(count).ok()?.saturating_sub(1));
    Some(format!("{}-{}", start, end))
}

pub async fn extract_channel_page(
    url: &str,
    cursor: Option<i64>,
    count: usize,
    cookies: Option<&[PlatformCookie]>,
    yt_dlp_path: &Path,
) -> ExtractResult<ChannelPageResult> {
    // ✨ ytdlp 侧使用 `--playlist-items` 做“真分页”，避免一次性拉全量。
    let offset = cursor.unwrap_or(0);
    let items = playlist_items_spec(offset, count).ok_or_else(|| {
        ExtractError::Parse(format!("invalid pagination params: cursor={:?} count={}", cursor, count))
    })?;

    let normalized_url = normalize_url(url);
    let normalized_url = normalize_youtube_channel_default_tab(&normalized_url);
    let request_url = normalize_ytdlp_channel_url(&normalized_url);
    tracing::info!(
        "📄 ytdlp 分页解析: url={} offset={} count={} items={}",
        request_url,
        offset,
        count,
        items
    );

    let cookie_header = build_cookie_header(
        extract_platform_from_url(&request_url).as_deref(),
        cookies,
    );

    let mut cmd = create_tokio_command(yt_dlp_path);
    cmd.arg("--flat-playlist")
        .arg("--dump-single-json")
        .arg("--no-warnings")
        .arg("--playlist-items")
        .arg(items)
        .arg("--extractor-args")
        .arg("BiliBiliSpace:metadata=true")
        .arg("-v")
        .arg(&request_url);

    if let Some(ref cookie) = cookie_header {
        cmd.arg("--add-header").arg(format!("Cookie: {}", cookie));
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| ExtractError::CommandFailed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ExtractError::CommandFailed(stderr.to_string()));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let channel_data: ChannelInfoData =
        serde_json::from_str(&json_str).map_err(|e| ExtractError::Parse(e.to_string()))?;

    let mut entries: Vec<ChannelVideoEntry> = Vec::new();
    let mut tabs: Vec<ChannelTab> = Vec::new();

    if let Some(ref data_entries) = channel_data.entries {
        fn collect_tab_entries(entry: &serde_json::Value, entries: &mut Vec<ChannelVideoEntry>) {
            if let Some(nested_entries) = entry.get("entries").and_then(|e| e.as_array()) {
                for nested_entry in nested_entries {
                    collect_tab_entries(nested_entry, entries);
                }
            } else if let Some(video_entry) = parse_channel_entry(entry) {
                entries.push(video_entry);
            }
        }

        for entry in data_entries {
            if let Some(nested_entries) = entry.get("entries").and_then(|e| e.as_array()) {
                let tab_title = entry
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("视频");
                let tab_type = ChannelTabType::from_title(tab_title);

                let mut tab_entries = Vec::new();
                for nested_entry in nested_entries {
                    collect_tab_entries(nested_entry, &mut tab_entries);
                }

                entries.extend(tab_entries.clone());
                tabs.push(ChannelTab {
                    tab_type,
                    title: tab_title.to_string(),
                    entries: tab_entries,
                });
            } else if let Some(video_entry) = parse_channel_entry(entry) {
                entries.push(video_entry);
            }
        }
    }

    // 为本页 entries 赋予连续的 playlist_index（与 cursor 对齐）
    for (idx, entry) in entries.iter_mut().enumerate() {
        entry.playlist_index = Some(offset as u32 + idx as u32 + 1);
        entry.selected = false;
    }

    let playlist_count = channel_data.playlist_count.and_then(|n| usize::try_from(n).ok());
    let loaded = entries.len();
    let has_more = playlist_count
        .map(|total| (offset as usize).saturating_add(loaded) < total)
        .unwrap_or(loaded == count);
    let next_cursor = if has_more {
        Some(offset.saturating_add(i64::try_from(loaded).unwrap_or(0)))
    } else {
        None
    };

    let first_uploader = entries.iter().find_map(|e| e.uploader.clone());
    let uploader = channel_data
        .uploader
        .clone()
        .or(channel_data.channel.clone())
        .or(first_uploader.clone());
    let title = channel_data
        .title
        .clone()
        .or_else(|| uploader.clone())
        .unwrap_or_else(|| "未知频道".to_string());
    let id = channel_data
        .id
        .clone()
        .or_else(|| fallback_channel_id(&request_url))
        .unwrap_or_else(|| "unknown".to_string());
    let video_count = playlist_count.unwrap_or_else(|| {
        // playlist_count 缺失时，用“至少已加载数量”的下界，且若推断还有更多则 +1 以提示 UI
        let base = (offset as usize).saturating_add(loaded);
        if has_more { base.saturating_add(1) } else { base }
    });

    Ok(ChannelPageResult {
        info: ChannelInfo {
            id,
            title,
            url: request_url,
            uploader,
            description: channel_data.description,
            video_count,
            thumbnail: channel_data.thumbnail,
            tabs,
            entries,
        },
        next_cursor,
        has_more,
    })
}

#[derive(Debug, Deserialize)]
struct VideoInfoData {
    id: String,
    title: String,
    description: Option<String>,
    duration: Option<f64>,
    uploader: Option<String>,
    upload_date: Option<String>,
    thumbnail: Option<String>,
    #[serde(default)]
    formats: Vec<FormatData>,
    webpage_url: String,
}

#[derive(Debug, Deserialize)]
struct ChannelInfoData {
    id: Option<String>,
    title: Option<String>,
    description: Option<String>,
    uploader: Option<String>,
    channel: Option<String>,
    thumbnail: Option<String>,
    #[serde(default)]
    playlist_count: Option<u64>,
    #[serde(default)]
    entries: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct ChannelEntryData {
    id: String,
    title: Option<String>,
    url: Option<String>,
    webpage_url: Option<String>,
    duration: Option<f64>,
    thumbnail: Option<String>,
    #[serde(default)]
    thumbnails: Option<Vec<ThumbnailData>>,
    uploader: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ThumbnailData {
    url: String,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default, rename = "width")]
    _width: Option<u32>,
}

fn parse_channel_entry(value: &serde_json::Value) -> Option<ChannelVideoEntry> {
    let entry: ChannelEntryData = serde_json::from_value(value.clone()).ok()?;
    let title = entry.title.unwrap_or_else(|| entry.id.clone());
    let thumbnail = entry.thumbnail.or_else(|| {
        entry
            .thumbnails
            .as_ref()
            .and_then(|thumbs| thumbs.iter().max_by_key(|t| t.height.unwrap_or(0)))
            .map(|t| t.url.clone())
    });

    Some(ChannelVideoEntry {
        id: entry.id.clone(),
        title,
        url: entry
            .url
            .or(entry.webpage_url)
            .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={}", entry.id)),
        duration: entry.duration.map(|d| d as u64),
        thumbnail,
        uploader: entry.uploader,
        playlist_index: None,
        selected: false,
    })
}

#[derive(Debug, Deserialize)]
struct FormatData {
    format_id: String,
    ext: Option<String>,
    resolution: Option<String>,
    fps: Option<f32>,
    filesize: Option<u64>,
    vcodec: Option<String>,
    acodec: Option<String>,
    #[serde(default, deserialize_with = "deserialize_quality")]
    quality: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

fn deserialize_quality<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct QualityVisitor;

    impl<'de> Visitor<'de> for QualityVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string, number, or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            #[derive(serde::Deserialize)]
            #[serde(untagged)]
            enum QualityValue {
                String(String),
                Number(f64),
            }

            match QualityValue::deserialize(deserializer)? {
                QualityValue::String(s) => Ok(Some(s)),
                QualityValue::Number(n) => Ok(Some(n.to_string())),
            }
        }
    }

    deserializer.deserialize_option(QualityVisitor)
}

impl From<VideoInfoData> for VideoInfo {
    fn from(data: VideoInfoData) -> Self {
        Self {
            id: data.id,
            title: data.title,
            description: data.description,
            duration: data.duration.map(std::time::Duration::from_secs_f64),
            uploader: data.uploader,
            upload_date: data.upload_date,
            thumbnail: data.thumbnail,
            formats: data
                .formats
                .into_iter()
                .map(|f| VideoFormat {
                    format_id: f.format_id,
                    ext: f.ext.unwrap_or_else(|| "unknown".to_string()),
                    resolution: f.resolution,
                    fps: f.fps,
                    filesize: f.filesize,
                    vcodec: f.vcodec,
                    acodec: f.acodec,
                    quality: f.quality,
                    download_url: f.url,
                })
                .collect(),
            url: data.webpage_url,
        }
    }
}
