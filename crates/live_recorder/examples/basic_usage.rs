use live_recorder::{LiveRecorder, RecordConfig, VideoQuality};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 设置日志
    tracing_subscriber::fmt::init();

    // 创建录制器
    let recorder = LiveRecorder::new();

    // 抖音直播间URL（请替换为实际的直播间URL）
    let live_url = "https://live.douyin.com/123456";

    // 检查直播间状态
    println!("检查直播间状态...");
    match recorder.check_room_status(live_url).await {
        Ok(room_info) => {
            println!("直播间信息:");
            println!("  主播: {}", room_info.anchor_name);
            println!("  标题: {}", room_info.title);
            println!("  状态: {:?}", room_info.status);
        }
        Err(e) => {
            println!("检查失败: {}", e);
            return Ok(());
        }
    }

    // 获取流信息
    println!("获取流信息...");
    match recorder.get_stream_info(live_url).await {
        Ok(stream_info) => {
            println!("可用流:");
            for (i, stream) in stream_info.streams.iter().enumerate() {
                println!(
                    "  {}. 质量: {:?}, HLS: {}, FLV: {}",
                    i + 1,
                    stream.quality,
                    stream.url.hls_url.is_some(),
                    stream.url.flv_url.is_some()
                );
            }
        }
        Err(e) => {
            println!("获取流信息失败: {}", e);
            return Ok(());
        }
    }

    // 配置录制参数
    let config = RecordConfig {
        output_path_template: "./downloads/test_{timestamp}.mp4".to_string(),
        quality: VideoQuality::High,
        format: "mp4".to_string(),
        ..Default::default()
    };

    // 开始录制
    println!("开始录制...");
    match recorder.start_recording(live_url, config).await {
        Ok(mut handle) => {
            println!("录制已开始: {:?}", handle.output_path());

            // 录制30秒后停止
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

            println!("停止录制...");
            handle.stop().await?;

            // 等待录制完成
            match handle.wait().await {
                Ok(_) => println!("录制完成"),
                Err(e) => println!("录制失败: {}", e),
            }
        }
        Err(e) => {
            println!("启动录制失败: {}", e);
        }
    }

    Ok(())
}
