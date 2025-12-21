//! Bilibili 解析模块
//!
//! - 视频解析：调用 Bilibili Web API 拿到标题/时长/封面/多档清晰度直链
//! - UP 主页：通过 WBI 投稿列表接口分页获取

use crate::error::{ExtractError, ExtractResult};
use magekit_shared::{
    ChannelInfo, ChannelPageResult, ChannelVideoEntry, PlatformCookie, VideoFormat, VideoInfo,
};
use platform_api::BilibiliApi;
use serde_json::Value;
use std::time::Duration;

fn setup_api_cookie(api: &mut BilibiliApi, cookies: Option<&[PlatformCookie]>) {
    let Some(cookie_list) = cookies else {
        tracing::debug!("🍪 Bilibili cookie: 未传入 cookies");
        return;
    };

    tracing::debug!("🍪 Bilibili cookie: cookies 条数={}", cookie_list.len());

    let matched = cookie_list.iter().find(|c| {
        if !c.enabled {
            return false;
        }
        let platform = c.platform.to_lowercase();
        let cookie_text = c.cookie.as_str();
        // 允许用户在设置里用域名/别名/中文平台名填写（避免仅匹配 “bilibili” 导致漏配）
        let is_bilibili_platform = platform.contains("bilibili")
            || platform.contains("space.bilibili.com")
            || platform.contains("b23.tv")
            || platform.contains("bili")
            || c.platform.contains("哔哩")
            || c.platform.contains("B站")
            || c.platform.contains("b站");

        // 兜底：即使平台名没填对，也能通过 Cookie 内容识别（SESSDATA 等）
        let looks_like_bilibili_cookie = cookie_text.contains("SESSDATA=")
            || cookie_text.contains("bili_jct=")
            || cookie_text.contains("DedeUserID=")
            || cookie_text.contains("bili_ticket=");

        (is_bilibili_platform || looks_like_bilibili_cookie) && !c.cookie.trim().is_empty()
    });

    if let Some(c) = matched {
        tracing::info!(
            "🍪 Bilibili cookie: 已启用 platform={} cookie_len={}",
            c.platform,
            c.cookie.len()
        );
        api.set_cookie(&c.cookie);
    } else {
        tracing::warn!(
            "🍪 Bilibili cookie: 未找到可用的 cookie（请在设置里配置 platform=bilibili 或 space.bilibili.com）"
        );
    }
}

pub fn is_bilibili_space_url(url: &str) -> bool {
    url.to_lowercase().contains("space.bilibili.com/")
}

fn normalize_space_url(mid: &str) -> String {
    format!("https://space.bilibili.com/{}", mid)
}

pub async fn extract_video_info(
    url: &str,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<VideoInfo> {
    tracing::info!("📺 开始解析 Bilibili 视频: {}", url);

    let mut api = BilibiliApi::new()
        .map_err(|e| ExtractError::Network(format!("创建 BilibiliApi 失败: {}", e)))?;
    setup_api_cookie(&mut api, cookies);

    let resolved = if url.to_lowercase().contains("b23.tv") {
        api.resolve_short_url(url)
            .await
            .unwrap_or_else(|_| url.to_string())
    } else {
        url.to_string()
    };

    let bvid = BilibiliApi::extract_bvid(&resolved)
        .or_else(|| BilibiliApi::extract_bvid(url))
        .ok_or_else(|| ExtractError::InvalidUrl(resolved.clone()))?;

    let detail = api
        .get_video_detail(&bvid)
        .await
        .map_err(|e| ExtractError::Network(format!("获取视频详情失败: {}", e)))?;
    ensure_ok(&detail, "video_detail")?;

    let data = detail
        .get("data")
        .ok_or_else(|| ExtractError::Parse("video_detail.data 缺失".to_string()))?;

    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(&bvid)
        .to_string();
    let desc = data
        .get("desc")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let duration = data
        .get("duration")
        .and_then(|v| v.as_u64())
        .map(Duration::from_secs);
    let thumbnail = data
        .get("pic")
        .and_then(|v| v.as_str())
        .map(normalize_url_scheme);
    let uploader = data
        .get("owner")
        .and_then(|o| o.get("name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let cid = data
        .get("cid")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            data.get("pages")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|p| p.get("cid"))
                .and_then(|v| v.as_u64())
        })
        .ok_or_else(|| ExtractError::Parse("无法获取 cid".to_string()))?;

    let mut formats = Vec::new();

    // 获取 accept_quality（优先请求 4K 档位以尽可能拿到完整的 DASH 列表）。
    // 注意：Bilibili 高画质通常只在 DASH（fnval=4048）返回里可见。
    let first = api
        .get_video_playurl(&bvid, cid, 120)
        .await
        .map_err(|e| ExtractError::Network(format!("获取播放地址失败: {}", e)))?;
    ensure_ok(&first, "playurl")?;

    // DASH：一次请求通常包含多档视频/音频流（dash.video/audio）
    if let Some(dash_formats) = parse_playurl_dash_formats(&first) {
        formats.extend(dash_formats);
    } else if let Some(list) = first
        .get("data")
        .and_then(|d| d.get("accept_quality"))
        .and_then(|v| v.as_array())
    {
        // progressive(durl) 兜底：逐档请求（一般最高只有 720p）
        for q in list.iter().filter_map(|v| v.as_u64()).take(6) {
            let qn = q as u32;
            let resp = api.get_video_playurl(&bvid, cid, qn).await.map_err(|e| {
                ExtractError::Network(format!("获取播放地址失败(qn={}): {}", qn, e))
            })?;
            if ensure_ok(&resp, "playurl").is_err() {
                continue;
            }
            if let Some(fmt) = parse_playurl_to_format(&resp, qn) {
                formats.push(fmt);
            }
        }
    }

    // 最终兜底：至少给一个可下载条目（避免 formats 为空）
    if formats.is_empty() {
        if let Some(fmt) = parse_playurl_to_format(&first, 80) {
            formats.push(fmt);
        }
    }

    Ok(VideoInfo {
        id: bvid.clone(),
        title,
        description: desc,
        duration,
        uploader,
        upload_date: None,
        thumbnail,
        formats,
        url: resolved,
    })
}

pub async fn extract_channel_info(
    url: &str,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<ChannelInfo> {
    let page = extract_channel_page(url, cookies, None, 20).await?;
    Ok(page.info)
}

pub async fn extract_channel_page(
    url: &str,
    cookies: Option<&[PlatformCookie]>,
    cursor: Option<i64>,
    count: usize,
) -> ExtractResult<ChannelPageResult> {
    tracing::info!(
        "📺 获取 Bilibili UP 分页: url={} cursor={:?} count={}",
        url,
        cursor,
        count
    );

    let mut api = BilibiliApi::new()
        .map_err(|e| ExtractError::Network(format!("创建 BilibiliApi 失败: {}", e)))?;
    setup_api_cookie(&mut api, cookies);

    let resolved = if url.to_lowercase().contains("b23.tv") {
        api.resolve_short_url(url)
            .await
            .unwrap_or_else(|_| url.to_string())
    } else {
        url.to_string()
    };

    let mid = BilibiliApi::extract_mid(&resolved)
        .or_else(|| BilibiliApi::extract_mid(url))
        .ok_or_else(|| ExtractError::InvalidUrl(resolved.clone()))?;

    let pn = cursor.unwrap_or(1).max(1) as u32;
    let ps = (count.max(1).min(50)) as u32;

    let resp = api
        .get_user_post_videos(&mid, pn, ps)
        .await
        .map_err(|e| ExtractError::Network(format!("获取投稿列表失败: {}", e)))?;
    ensure_ok(&resp, "space_arc_search")?;

    // 尝试补齐 UP 主信息（可能需要更完整的 Cookie；失败则兜底）
    let (profile_name, profile_face) = match api.get_user_profile(&mid).await {
        Ok(v) => {
            if ensure_ok(&v, "space_acc_info").is_ok() {
                let data = v.get("data").unwrap_or(&Value::Null);
                let name = data
                    .get("name")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string());
                let face = data
                    .get("face")
                    .and_then(|x| x.as_str())
                    .map(normalize_url_scheme);
                (name, face)
            } else {
                (None, None)
            }
        }
        Err(_) => (None, None),
    };

    let data = resp
        .get("data")
        .ok_or_else(|| ExtractError::Parse("space_arc_search.data 缺失".to_string()))?;

    let page = data.get("page").unwrap_or(&Value::Null);
    let total = page.get("count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    let vlist = data
        .get("list")
        .and_then(|l| l.get("vlist"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut entries: Vec<ChannelVideoEntry> =
        vlist.iter().filter_map(parse_space_vlist_entry).collect();
    for (idx, entry) in entries.iter_mut().enumerate() {
        entry.playlist_index = Some((pn as u32 - 1) * ps + idx as u32 + 1);
        entry.selected = false;
    }

    let has_more = (pn as usize) * (ps as usize) < total && !entries.is_empty();
    let next_cursor = has_more.then_some((pn as i64) + 1);

    let uploader = profile_name.or_else(|| entries.first().and_then(|e| e.uploader.clone()));
    let title = uploader
        .clone()
        .unwrap_or_else(|| format!("Bilibili_{}", mid));

    Ok(ChannelPageResult {
        info: ChannelInfo {
            id: mid.clone(),
            title,
            url: normalize_space_url(&mid),
            uploader,
            description: None,
            video_count: total.max(entries.len()),
            thumbnail: profile_face,
            tabs: Vec::new(),
            entries,
        },
        next_cursor,
        has_more,
    })
}

fn ensure_ok(resp: &Value, name: &str) -> ExtractResult<()> {
    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
    if code == 0 {
        return Ok(());
    }
    let msg = resp
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    Err(ExtractError::Network(format!(
        "{} failed: code={} msg={}",
        name, code, msg
    )))
}

fn parse_playurl_to_format(resp: &Value, qn: u32) -> Option<VideoFormat> {
    let data = resp.get("data")?;
    let durl0 = data.get("durl")?.as_array()?.first()?;
    let url = durl0.get("url")?.as_str()?.to_string();
    let size = durl0.get("size").and_then(|v| v.as_u64());

    let desc = data
        .get("accept_quality")
        .and_then(|v| v.as_array())
        .and_then(|qs| qs.iter().position(|x| x.as_u64() == Some(qn as u64)))
        .and_then(|idx| {
            data.get("accept_description")
                .and_then(|v| v.as_array())
                .and_then(|ds| ds.get(idx))
                .and_then(|v| v.as_str())
        })
        .map(|s| s.to_string());

    let resolution = qn_to_height(qn).map(|h| format!("{}p", h)).or_else(|| {
        desc.as_ref()
            .and_then(|d| extract_height_from_text(d).map(|h| format!("{}p", h)))
    });

    Some(VideoFormat {
        format_id: format!("bili_qn{}", qn),
        ext: "mp4".to_string(),
        resolution,
        fps: None,
        filesize: size,
        vcodec: None,
        acodec: None,
        quality: desc,
        download_url: Some(url),
    })
}

fn parse_playurl_dash_formats(resp: &Value) -> Option<Vec<VideoFormat>> {
    let data = resp.get("data")?;
    let dash = data.get("dash")?;

    let videos = dash.get("video")?.as_array()?;
    let audios = dash.get("audio")?.as_array()?;

    let mut out = Vec::new();

    // 音频：只保留“最优”（按 bandwidth 最大）
    if let Some(best_audio) = audios
        .iter()
        .max_by_key(|a| a.get("bandwidth").and_then(|v| v.as_u64()).unwrap_or(0))
    {
        let id = best_audio.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
        let base_url = best_audio
            .get("base_url")
            .and_then(|v| v.as_str())?
            .to_string();
        let size = best_audio
            .get("bandwidth")
            .and_then(|v| v.as_u64())
            .map(|b| b / 8);
        let codecs = best_audio
            .get("codecs")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        out.push(VideoFormat {
            format_id: format!("bili_a{}", id),
            ext: "m4a".to_string(),
            resolution: None,
            fps: None,
            filesize: size,
            vcodec: None,
            acodec: codecs.or_else(|| Some("aac".to_string())),
            quality: Some("最佳音质".to_string()),
            download_url: Some(base_url),
        });
    }

    // 视频：同一 qn 可能返回多个编码（avc/hev/av1），这里按“兼容优先”只保留一个
    //（否则会出现同一个 format_id 被 UI 选中两次的问题）。
    #[derive(Clone)]
    struct Candidate {
        qn: u32,
        url: String,
        width: u32,
        height: u32,
        fps: Option<f32>,
        codecs: Option<String>,
        bandwidth: u64,
    }

    fn codec_rank(codecs: &Option<String>) -> u8 {
        let s = codecs.as_deref().unwrap_or("").to_lowercase();
        if s.contains("avc") || s.contains("h264") {
            0
        } else if s.contains("hev") || s.contains("h265") || s.contains("hevc") {
            1
        } else if s.contains("av01") || s.contains("av1") {
            2
        } else {
            3
        }
    }

    let mut best_by_qn: std::collections::BTreeMap<u32, Candidate> =
        std::collections::BTreeMap::new();
    for v in videos {
        let qn = v.get("id").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        let url = match v.get("base_url").and_then(|x| x.as_str()) {
            Some(s) if !s.trim().is_empty() => s.to_string(),
            _ => continue,
        };
        let width = v.get("width").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        let height = v.get("height").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        let fps = v
            .get("frame_rate")
            .and_then(|x| x.as_str())
            .and_then(|s| s.parse::<f32>().ok());
        let codecs = v
            .get("codecs")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        let bandwidth = v.get("bandwidth").and_then(|x| x.as_u64()).unwrap_or(0);

        let cand = Candidate {
            qn,
            url,
            width,
            height,
            fps,
            codecs,
            bandwidth,
        };

        match best_by_qn.get(&qn) {
            None => {
                best_by_qn.insert(qn, cand);
            }
            Some(existing) => {
                let r1 = codec_rank(&cand.codecs);
                let r2 = codec_rank(&existing.codecs);
                let better = (r1 < r2) || (r1 == r2 && cand.bandwidth > existing.bandwidth);
                if better {
                    best_by_qn.insert(qn, cand);
                }
            }
        }
    }

    for (qn, v) in best_by_qn {
        let desc = data
            .get("accept_quality")
            .and_then(|qs| qs.as_array())
            .and_then(|qs| qs.iter().position(|x| x.as_u64() == Some(qn as u64)))
            .and_then(|idx| {
                data.get("accept_description")
                    .and_then(|ds| ds.as_array())
                    .and_then(|ds| ds.get(idx))
                    .and_then(|x| x.as_str())
            })
            .map(|s| s.to_string());

        let resolution = if v.width > 0 && v.height > 0 {
            Some(format!("{}x{}", v.width, v.height))
        } else {
            qn_to_height(qn).map(|h| format!("{}p", h))
        };

        out.push(VideoFormat {
            format_id: format!("bili_vqn{}", qn),
            ext: "mp4".to_string(),
            resolution,
            fps: v.fps,
            filesize: None,
            vcodec: v.codecs,
            acodec: None,
            quality: desc.or_else(|| Some(format!("视频 {}", qn))),
            download_url: Some(v.url),
        });
    }

    (!out.is_empty()).then_some(out)
}

fn qn_to_height(qn: u32) -> Option<u32> {
    match qn {
        6 => Some(240),
        16 => Some(360),
        32 => Some(480),
        64 | 74 => Some(720),
        80 | 112 | 116 => Some(1080),
        120 => Some(2160),
        127 => Some(4320), // 8K
        _ => None,
    }
}

fn extract_height_from_text(s: &str) -> Option<u32> {
    // 从类似 "高清 1080P" / "1080P60" 中提取 1080（取第一段 >=3 位的数字）
    let mut buf = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            buf.push(c);
        } else {
            if buf.len() >= 3 {
                return buf.parse::<u32>().ok();
            }
            buf.clear();
        }
    }
    if buf.len() >= 3 {
        return buf.parse::<u32>().ok();
    }
    None
}

fn parse_space_vlist_entry(v: &Value) -> Option<ChannelVideoEntry> {
    let bvid = v.get("bvid")?.as_str()?.to_string();
    let title = v
        .get("title")
        .and_then(|x| x.as_str())
        .unwrap_or(&bvid)
        .to_string();
    let pic = v
        .get("pic")
        .and_then(|x| x.as_str())
        .map(normalize_url_scheme);
    let author = v
        .get("author")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let length = v
        .get("length")
        .and_then(|x| x.as_str())
        .and_then(parse_duration_string);

    Some(ChannelVideoEntry {
        id: bvid.clone(),
        title,
        url: format!("https://www.bilibili.com/video/{}", bvid),
        duration: length,
        thumbnail: pic,
        uploader: author,
        playlist_index: None,
        selected: false,
    })
}

fn parse_duration_string(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 2 {
        let m = parts[0].parse::<u64>().ok()?;
        let sec = parts[1].parse::<u64>().ok()?;
        Some(m * 60 + sec)
    } else if parts.len() == 3 {
        let h = parts[0].parse::<u64>().ok()?;
        let m = parts[1].parse::<u64>().ok()?;
        let sec = parts[2].parse::<u64>().ok()?;
        Some(h * 3600 + m * 60 + sec)
    } else {
        None
    }
}

fn normalize_url_scheme(s: &str) -> String {
    if s.starts_with("//") {
        format!("https:{}", s)
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_playurl_dash_formats_extracts_video_and_audio() {
        let resp = json!({
            "code": 0,
            "data": {
                "accept_quality": [120, 80],
                "accept_description": ["4K 超清", "高清 1080P"],
                "dash": {
                    "video": [
                        {
                            "id": 120,
                            "base_url": "https://example.com/video_4k.m4s",
                            "width": 3840,
                            "height": 2160,
                            "frame_rate": "60",
                            "codecs": "hev1.1.6.L150.90"
                        }
                    ],
                    "audio": [
                        {
                            "id": 30280,
                            "base_url": "https://example.com/audio.m4s",
                            "bandwidth": 320000,
                            "codecs": "mp4a.40.2"
                        }
                    ]
                }
            }
        });

        let formats = parse_playurl_dash_formats(&resp).expect("formats");
        assert!(formats.iter().any(|f| f.format_id == "bili_vqn120"));
        assert!(formats.iter().any(|f| f.format_id == "bili_a30280"));
        let v = formats
            .iter()
            .find(|f| f.format_id == "bili_vqn120")
            .unwrap();
        assert_eq!(v.resolution.as_deref(), Some("3840x2160"));
        assert!(v.download_url.as_deref() == Some("https://example.com/video_4k.m4s"));
    }
}
