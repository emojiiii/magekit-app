use magekit_extractor::MediaExtractor;
use magekit_shared::{resolve_yt_dlp_path, PlatformCookie};
use std::path::PathBuf;
use tokio::time::{timeout, Duration};

fn cookie_json_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("cookie.json")
}

fn cookies_from_test_json(platform: &str) -> Option<Vec<PlatformCookie>> {
    let path = cookie_json_path();
    let raw = std::fs::read_to_string(&path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;

    let val = json.get(platform)?;
    let cookie = if let Some(s) = val.as_str() {
        s.to_string()
    } else {
        val.get("cookie")?.as_str()?.to_string()
    };

    if cookie.trim().is_empty() {
        return None;
    }

    Some(vec![PlatformCookie {
        platform: platform.to_string(),
        cookie,
        enabled: true,
    }])
}

#[tokio::test]
#[ignore = "需要网络连接与本机 yt-dlp"]
async fn youtube_single_video_via_ytdlp() {
    let yt_dlp = match resolve_yt_dlp_path() {
        Some(p) => p,
        None => return,
    };
    let extractor = MediaExtractor::new(yt_dlp);
    let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
    let info = timeout(Duration::from_secs(40), extractor.get_video_info(url, None))
        .await
        .expect("timeout for youtube video")
        .expect("youtube video parse failed");
    assert!(!info.id.is_empty());
    assert!(!info.formats.is_empty());
}

#[tokio::test]
#[ignore = "需要网络连接与本机 yt-dlp（部分视频/频道可能需要 Cookie）"]
async fn bilibili_single_video_via_ytdlp() {
    let yt_dlp = match resolve_yt_dlp_path() {
        Some(p) => p,
        None => return,
    };
    let extractor = MediaExtractor::new(yt_dlp);
    let url = "https://www.bilibili.com/video/BV1fW411W7cR";
    let cookies = cookies_from_test_json("bilibili");
    let info = timeout(
        Duration::from_secs(40),
        extractor.get_video_info(url, cookies.as_deref()),
    )
    .await
    .expect("timeout for bilibili video")
    .expect("bilibili video parse failed");
    assert!(!info.id.is_empty());
    assert!(!info.formats.is_empty());
}

#[tokio::test]
#[ignore = "需要网络连接与有效 Douyin Cookie（crates/extractor/tests/cookie.json）"]
async fn douyin_aweme_detail_long_url() {
    let cookies = match cookies_from_test_json("douyin") {
        Some(c) => c,
        None => {
            eprintln!(
                "skip: missing douyin cookie at {:?} (copy from crates/extractor/tests/cookie.example.json)",
                cookie_json_path()
            );
            return;
        }
    };

    // Douyin 自研解析不依赖 yt-dlp，这里传一个占位路径即可。
    let extractor = MediaExtractor::new(PathBuf::from("yt-dlp"));

    // 带 modal_id/vid 的用户页链接（需能提取出 aweme_id）
    let url = "https://www.douyin.com/user/MS4wLjABAAAAvj9TJ3GAUdUrw5RrFJVovE0O7ch9DAbCkhV5QjhzvE8?from_tab_name=main&modal_id=7585845607949651209&vid=7585845607949651209";

    let info = timeout(
        Duration::from_secs(40),
        extractor.get_video_info(url, Some(cookies.as_slice())),
    )
    .await
    .expect("timeout for douyin video")
    .expect("douyin video parse failed");

    assert_eq!(info.id, "7585845607949651209");
    assert!(!info.title.trim().is_empty());
    assert!(info.duration.is_some());
    assert!(!info.formats.is_empty());
    assert!(info.formats.iter().any(|f| f.download_url.is_some()));

    // 期望能拿到多档清晰度（来自 aweme_detail.video.bit_rate）
    assert!(
        info.formats.len() >= 2,
        "formats too few: {}",
        info.formats.len()
    );
}

#[tokio::test]
#[ignore = "需要网络连接与有效 Douyin Cookie（crates/extractor/tests/cookie.json）"]
async fn douyin_short_url_redirect() {
    let cookies = match cookies_from_test_json("douyin") {
        Some(c) => c,
        None => {
            eprintln!(
                "skip: missing douyin cookie at {:?} (copy from crates/extractor/tests/cookie.example.json)",
                cookie_json_path()
            );
            return;
        }
    };

    let extractor = MediaExtractor::new(PathBuf::from("yt-dlp"));

    // 短链（会先解析重定向，再提取 aweme_id）
    let url = "https://v.douyin.com/YkN6Fl4y2aM/";
    let info = timeout(
        Duration::from_secs(40),
        extractor.get_video_info(url, Some(cookies.as_slice())),
    )
    .await
    .expect("timeout for douyin short url")
    .expect("douyin short url parse failed");

    assert!(!info.id.is_empty());
    assert!(!info.title.trim().is_empty());
    assert!(info.duration.is_some());
    assert!(!info.formats.is_empty());
    assert!(info.formats.iter().any(|f| f.download_url.is_some()));
}

#[tokio::test]
#[ignore = "需要网络连接与本机 yt-dlp"]
async fn youtube_playlist_channel() {
    let yt_dlp = match resolve_yt_dlp_path() {
        Some(p) => p,
        None => return,
    };
    let extractor = MediaExtractor::new(yt_dlp);
    let url = "https://www.youtube.com/playlist?list=PL9tY0BWXOZFtS9DD35Jt_zyXCTITVEWil";
    let info = timeout(
        Duration::from_secs(60),
        extractor.get_channel_info(url, None),
    )
    .await
    .expect("timeout for youtube playlist")
    .expect("youtube playlist parse failed");
    assert!(!info.entries.is_empty());
}

#[tokio::test]
#[ignore = "需要网络连接与本机 yt-dlp（部分频道可能需要 Cookie）"]
async fn bilibili_space_channel() {
    let yt_dlp = match resolve_yt_dlp_path() {
        Some(p) => p,
        None => return,
    };
    let extractor = MediaExtractor::new(yt_dlp);
    let url = "https://space.bilibili.com/282357985";
    let cookies = cookies_from_test_json("bilibili");
    let info = timeout(
        Duration::from_secs(60),
        extractor.get_channel_info(url, cookies.as_deref()),
    )
    .await
    .expect("timeout for bilibili space")
    .expect("bilibili space parse failed");
    assert!(!info.entries.is_empty());
}
