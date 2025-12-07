//! 测试多平台直播流获取
//! 
//! 运行示例: cargo run --example test_platforms

use live_recorder::platforms::{PlatformFactory, PlatformCookies};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 设置日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let factory = PlatformFactory::new();

    // 显示支持的平台
    println!("🎬 支持的平台:");
    for platform in factory.supported_platforms() {
        println!("  - {}", platform);
    }
    println!();

    // 测试URL列表
    let test_urls: Vec<(&str, &str, Option<&str>)> = vec![
        ("抖音", "https://live.douyin.com/313899056971", None),
        ("哔哩哔哩", "https://live.bilibili.com/21470918", None),
        ("虎牙", "https://www.huya.com/116", None),
        ("斗鱼", "https://www.douyu.com/288016", None),
        ("快手", "https://live.kuaishou.com/u/yxjrhy", None),
        ("SOOP (国际)", "https://sooplive.com/oul282", None),
        ("SOOP (韩国)", "https://play.sooplive.co.kr/secretx", None),
    ];

    for (platform_name, url, cookies) in test_urls {
        println!("========================================");
        println!("📺 测试 {} 平台", platform_name);
        println!("🔗 URL: {}", url);
        
        match factory.get_handler_for_url(url) {
            Ok(handler) => {
                println!("✅ 找到处理器: {}", handler.platform_name());
                
                // 提取房间ID
                match handler.extract_room_id(url).await {
                    Ok(room_id) => {
                        println!("🏠 房间ID: {}", room_id);
                        
                        // 构建 cookies
                        let platform_cookies = if let Some(c) = cookies {
                            PlatformCookies::new().with_cookie(c)
                        } else {
                            PlatformCookies::new()
                        };
                        
                        // 获取流信息（带 cookies）
                        match handler.get_stream_info_with_cookies(&room_id, &platform_cookies).await {
                            Ok(stream_info) => {
                                println!("👤 主播: {}", stream_info.room.anchor_name);
                                println!("📝 标题: {}", stream_info.room.title);
                                println!("🔴 状态: {:?}", stream_info.room.status);
                                println!("📡 可用流: {} 个", stream_info.streams.len());
                                
                                for (i, stream) in stream_info.streams.iter().enumerate() {
                                    println!("   流 #{}: 质量={:?}, CDN={:?}, 码率={:?}", 
                                        i + 1, 
                                        stream.quality,
                                        stream.cdn,
                                        stream.bitrate
                                    );
                                    if let Some(flv) = &stream.url.flv_url {
                                        println!("      FLV: {}...", &flv[..flv.len().min(80)]);
                                    }
                                    if let Some(hls) = &stream.url.hls_url {
                                        println!("      HLS: {}...", &hls[..hls.len().min(80)]);
                                    }
                                }
                            }
                            Err(e) => {
                                println!("❌ 获取流信息失败: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        println!("❌ 提取房间ID失败: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("❌ 找不到处理器: {}", e);
            }
        }
        println!();
    }

    Ok(())
}
