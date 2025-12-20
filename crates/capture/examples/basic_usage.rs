//! 基本使用示例

use magekit_capture::{CaptureEvent, CaptureRequest, ResourceFilter, start_capture};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 示例 1: 捕捉所有资源（持续运行，不会自动结束）
    println!("示例 1: 捕捉所有资源（持续运行）");
    let request = CaptureRequest {
        target_url: "https://example.com".to_string(),
        custom_browser_path: None,
        headless: true,
        timeout: Duration::from_secs(10),
        filter: ResourceFilter::all(),
        auto_speedup_ads: false, // 不自动加速广告
        speedup_rate: 2.0,
    };

    let mut session = start_capture(request).await?;
    let mut count = 0;

    while let Some(event) = session.rx_mut().recv().await {
        match event {
            CaptureEvent::Found(resource) => {
                count += 1;
                println!(
                    "  [{}] 发现资源: {} ({:?})",
                    count, resource.url, resource.resource_type
                );
                if count >= 5 {
                    // 只显示前 5 个资源
                    session.cancel();
                    break;
                }
            }
            CaptureEvent::Log(msg) => {
                println!("  日志: {}", msg);
            }
            CaptureEvent::Finished => {
                println!("  捕捉完成（通常不会自动触发，需要手动取消）");
                break;
            }
            CaptureEvent::Error(err) => {
                eprintln!("  错误: {}", err);
            }
        }
    }

    println!("\n示例 2: 只捕捉视频资源，并自动加速播放广告");
    let request = CaptureRequest {
        target_url: "https://example.com".to_string(),
        custom_browser_path: None,
        headless: true,
        timeout: Duration::from_secs(10),
        filter: ResourceFilter::video_only(),
        auto_speedup_ads: true, // 启用自动加速广告
        speedup_rate: 2.0,      // 2倍速播放
    };

    let mut session = start_capture(request).await?;
    let mut count = 0;

    while let Some(event) = session.rx_mut().recv().await {
        match event {
            CaptureEvent::Found(resource) => {
                count += 1;
                println!("  [{}] 发现视频: {}", count, resource.url);
                if count >= 3 {
                    session.cancel();
                    break;
                }
            }
            CaptureEvent::Log(msg) => {
                println!("  日志: {}", msg);
            }
            _ => {}
        }
    }

    Ok(())
}
