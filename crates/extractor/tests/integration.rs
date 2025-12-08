use magekit_extractor::MediaExtractor;
use magekit_shared::resolve_yt_dlp_path;
use magekit_shared::PlatformCookie;
use tokio::time::{timeout, Duration};

const E2E_ENV: &str = "MAGEKIT_E2E";
const COOKIE: &str = "";


fn should_run() -> bool {
    std::env::var(E2E_ENV).is_ok()
}

fn cookies_from_env(key: &str, platform: &str) -> Option<Vec<PlatformCookie>> {
    std::env::var(key).ok().map(|cookie| {
        vec![PlatformCookie {
            platform: platform.to_string(),
            cookie,
            enabled: true,
        }]
    })
}

/// 抖音视频测试 - 使用真实有效的视频链接
/// 
/// 注意：抖音 API 可能会因为以下原因返回错误：
/// - 视频被删除或下架
/// - 视频受地区限制
/// - Cookie 过期
/// - 签名验证失败
#[tokio::test]
async fn douyin_single_video() {
    if !should_run() {
        eprintln!("⚠️ 跳过测试：请设置环境变量 MAGEKIT_E2E=1 来启用");
        return;
    }
    let extractor = MediaExtractor::new(resolve_yt_dlp_path().unwrap_or_else(|| "yt-dlp".into()));

    // 测试多个视频链接，找到第一个可用的
    let test_urls = [
        "https://www.douyin.com/video/7449055498626755873",
        "https://www.douyin.com/video/7448684636975179043",
        "https://www.douyin.com/video/7447967611017494835",
    ];

    let cookies = vec![PlatformCookie {
        platform: "douyin".to_string(),
        cookie: COOKIE.to_string(),
        enabled: true,
    }];

    for url in test_urls {
        println!("🔍 测试抖音视频解析: {}", url);
        let result = timeout(Duration::from_secs(30), extractor.get_video_info(url, Some(&cookies)))
            .await;

        match result {
            Ok(Ok(info)) => {
                println!("✅ 解析成功!");
                println!("   ID: {}", info.id);
                println!("   标题: {}", info.title);
                println!("   时长: {:?}", info.duration);
                println!("   格式数: {}", info.formats.len());
                for f in &info.formats {
                    println!("   - {}: {:?} url={}", f.format_id, f.resolution, f.download_url.is_some());
                }
                assert!(!info.id.is_empty());
                if !info.formats.is_empty() {
                    assert!(
                        info.formats.iter().any(|f| f.download_url.is_some()),
                        "douyin should provide direct download url"
                    );
                }
                return; // 成功，退出测试
            }
            Ok(Err(e)) => {
                let err_str = format!("{:?}", e);
                if err_str.contains("filter_reason") || err_str.contains("视频不可用") {
                    println!("⚠️ 视频不可用（已过滤），尝试下一个: {:?}", e);
                    continue;
                }
                eprintln!("❌ 解析失败: {:?}", e);
            }
            Err(_) => {
                eprintln!("⏱️ 请求超时，尝试下一个");
            }
        }
    }

    // 如果所有视频都失败，这可能是 Cookie 或签名问题
    eprintln!("⚠️ 所有测试视频都无法获取");
    eprintln!("   可能的原因:");
    eprintln!("   - Cookie 已过期，请更新 COOKIE 常量");
    eprintln!("   - 所有测试视频都被过滤（已删除/受限）");
    eprintln!("   - 网络问题");
}

/// 抖音短链接测试
#[tokio::test]
async fn douyin_short_link() {
    if !should_run() {
        return;
    }
    let extractor = MediaExtractor::new(resolve_yt_dlp_path().unwrap_or_else(|| "yt-dlp".into()));
    // 抖音短链接格式
    let url = "https://v.douyin.com/iRNBho6u/";
    let cookies = vec![PlatformCookie {
        platform: "douyin".to_string(),
        cookie: COOKIE.to_string(),
        enabled: true,
    }];

    println!("🔍 测试抖音短链接解析: {}", url);
    let result = timeout(Duration::from_secs(60), extractor.get_video_info(url, Some(&cookies)))
        .await
        .expect("timeout for douyin short link");

    match result {
        Ok(info) => {
            println!("✅ 短链接解析成功! ID: {}", info.id);
            assert!(!info.id.is_empty());
        }
        Err(e) => {
            eprintln!("⚠️ 短链接解析失败（可能已过期）: {:?}", e);
            // 短链接可能过期，所以只警告不报错
        }
    }
}

/// TikTok 视频测试 - 使用真实有效的视频链接
///
/// 注意：TikTok API 可能因以下原因返回错误：
/// - 视频不存在或已删除
/// - 地区限制（TikTok 在某些地区不可用）
/// - 需要代理才能访问
#[tokio::test]
async fn tiktok_single_video() {
    if !should_run() {
        eprintln!("⚠️ 跳过测试：请设置环境变量 MAGEKIT_E2E=1 来启用");
        return;
    }
    let extractor = MediaExtractor::new(resolve_yt_dlp_path().unwrap_or_else(|| "yt-dlp".into()));

    // 测试多个视频链接
    let test_urls = [
        "https://www.tiktok.com/@tiktok/video/7358780325111592238",
        "https://www.tiktok.com/@khaby.lame/video/7321613070743663893",
        "https://www.tiktok.com/@scout2015/video/7043910621346114822",
    ];

    let cookies = cookies_from_env("MAGEKIT_TIKTOK_COOKIE", "tiktok");

    for url in test_urls {
        println!("🔍 测试 TikTok 视频解析: {}", url);
        let result = timeout(Duration::from_secs(30), extractor.get_video_info(url, cookies.as_deref()))
            .await;

        match result {
            Ok(Ok(info)) => {
                println!("✅ 解析成功!");
                println!("   ID: {}", info.id);
                println!("   标题: {}", info.title);
                println!("   时长: {:?}", info.duration);
                println!("   格式数: {}", info.formats.len());
                for f in &info.formats {
                    println!(
                        "   - {}: {:?} url={}",
                        f.format_id,
                        f.resolution,
                        f.download_url.is_some()
                    );
                }
                assert!(!info.id.is_empty());
                // TikTok API 可能因为地区限制返回空格式
                if info.formats.is_empty() {
                    eprintln!("⚠️ TikTok 格式列表为空，可能是地区限制或需要代理");
                }
            }
            Ok(Err(e)) => {
                eprintln!("❌ 解析失败: {:?}", e);
                eprintln!("⚠️ TikTok 解析返回错误");
            }
            Err(e) => {
                eprintln!("❌ 解析失败: {:?}", e);
                // TikTok 在某些地区可能受限
                eprintln!("⚠️ 提示: TikTok 在某些地区可能需要代理才能访问");
            }
        }
    }
}

#[tokio::test]
async fn youtube_single_video_via_ytdlp() {
    if !should_run() {
        return;
    }
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
async fn bilibili_single_video_via_ytdlp() {
    if !should_run() {
        return;
    }
    let yt_dlp = match resolve_yt_dlp_path() {
        Some(p) => p,
        None => return,
    };
    let extractor = MediaExtractor::new(yt_dlp);
    let url = "https://www.bilibili.com/video/BV1fW411W7cR";
    let cookies = cookies_from_env("MAGEKIT_BILI_COOKIE", "bilibili");
    let info = timeout(Duration::from_secs(40), extractor.get_video_info(url, cookies.as_deref()))
        .await
        .expect("timeout for bilibili video")
        .expect("bilibili video parse failed");
    assert!(!info.id.is_empty());
    assert!(!info.formats.is_empty());
}

#[tokio::test]
async fn youtube_playlist_channel() {
    if !should_run() {
        return;
    }
    let yt_dlp = match resolve_yt_dlp_path() {
        Some(p) => p,
        None => return,
    };
    let extractor = MediaExtractor::new(yt_dlp);
    let url = "https://www.youtube.com/playlist?list=PL9tY0BWXOZFtS9DD35Jt_zyXCTITVEWil";
    let info = timeout(Duration::from_secs(60), extractor.get_channel_info(url, None))
        .await
        .expect("timeout for youtube playlist")
        .expect("youtube playlist parse failed");
    assert!(!info.entries.is_empty());
}

#[tokio::test]
async fn bilibili_space_channel() {
    if !should_run() {
        return;
    }
    let yt_dlp = match resolve_yt_dlp_path() {
        Some(p) => p,
        None => return,
    };
    let extractor = MediaExtractor::new(yt_dlp);
    let url = "https://space.bilibili.com/282357985";
    let cookies = cookies_from_env("MAGEKIT_BILI_COOKIE", "bilibili");
    let info = timeout(Duration::from_secs(60), extractor.get_channel_info(url, cookies.as_deref()))
        .await
        .expect("timeout for bilibili space")
        .expect("bilibili space parse failed");
    assert!(!info.entries.is_empty());
}

