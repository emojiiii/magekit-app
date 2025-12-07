use clap::{Arg, Command};
use live_recorder::{LiveRecorder, RecordConfig, VideoQuality};
use tracing::{error, info, level_filters::LevelFilter};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 设置日志
    let subscriber = FmtSubscriber::builder()
        .with_max_level(LevelFilter::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    // 解析命令行参数
    let matches = Command::new("live-recorder")
        .version("0.1.0")
        .about("一个可扩展的直播录制工具")
        .arg(Arg::new("url").help("直播间URL").required(true).index(1))
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .help("输出文件路径")
                .value_name("FILE"),
        )
        .arg(
            Arg::new("quality")
                .short('q')
                .long("quality")
                .help("视频质量 (od, uhd, hd, sd, ld)")
                .value_name("QUALITY")
                .default_value("od"),
        )
        .arg(
            Arg::new("format")
                .short('f')
                .long("format")
                .help("录制格式 (mp4, flv, m3u8)")
                .value_name("FORMAT")
                .default_value("mp4"),
        )
        .arg(
            Arg::new("proxy")
                .short('p')
                .long("proxy")
                .help("代理服务器")
                .value_name("PROXY"),
        )
        .arg(
            Arg::new("check")
                .long("check")
                .help("仅检查直播间状态，不录制")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let url = matches.get_one::<String>("url").unwrap();
    let check_only = matches.get_flag("check");

    // 创建录制器
    let recorder = LiveRecorder::new();

    if check_only {
        // 仅检查状态
        info!("检查直播间状态: {}", url);
        match recorder.check_room_status(url).await {
            Ok(room_info) => {
                println!("直播间信息:");
                println!("  主播: {}", room_info.anchor_name);
                println!("  标题: {}", room_info.title);
                println!("  状态: {:?}", room_info.status);
                println!("  观众数: {:?}", room_info.viewer_count);
            }
            Err(e) => {
                error!("检查失败: {}", e);
                return Err(e.into());
            }
        }
        return Ok(());
    }

    // 配置录制参数
    let quality_str = matches.get_one::<String>("quality").unwrap();
    let quality = match quality_str.to_lowercase().as_str() {
        "od" | "bd" => VideoQuality::Original,
        "uhd" => VideoQuality::Ultra,
        "hd" => VideoQuality::High,
        "sd" => VideoQuality::Standard,
        "ld" => VideoQuality::Low,
        _ => {
            error!("不支持的质量: {}", quality_str);
            return Err("不支持的质量".into());
        }
    };

    let format = matches.get_one::<String>("format").unwrap();
    if !["mp4", "flv", "m3u8"].contains(&format.as_str()) {
        error!("不支持的格式: {}", format);
        return Err("不支持的格式".into());
    }

    let output_path = if let Some(output) = matches.get_one::<String>("output") {
        output.clone()
    } else {
        // 生成默认输出路径
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        format!("./downloads/douyin_{}.{}", timestamp, format)
    };

    let quality_clone = quality.clone();
    let config = RecordConfig {
        output_path_template: output_path,
        quality,
        format: format.clone(),
        proxy: matches.get_one::<String>("proxy").cloned(),
        ..Default::default()
    };

    // 开始录制
    info!(
        "开始录制: {} (质量: {:?}, 格式: {})",
        url, quality_clone, format
    );

    match recorder.start_recording(url, config).await {
        Ok(mut handle) => {
            info!("录制已开始，输出文件: {:?}", handle.output_path());

            // 监控录制进度
            let mut recording_completed = false;
            while !recording_completed {
                if let Some(progress) = handle.get_progress().await {
                    match progress.status {
                        live_recorder::RecordStatus::Recording => {
                            info!(
                                "录制中... 时长: {}秒, 大小: {}MB",
                                progress.duration,
                                progress.size / 1024 / 1024
                            );
                        }
                        live_recorder::RecordStatus::Completed => {
                            info!("录制完成!");
                            recording_completed = true;
                        }
                        live_recorder::RecordStatus::Error(ref e) => {
                            error!("录制错误: {}", e);
                            recording_completed = true;
                        }
                        live_recorder::RecordStatus::Stopped => {
                            info!("录制已停止");
                            recording_completed = true;
                        }
                        _ => {}
                    }
                }

                // 检查是否完成
                if recording_completed {
                    break;
                }

                // 短暂等待
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }

            // 等待录制任务完成
            if let Err(e) = handle.wait().await {
                error!("录制任务失败: {}", e);
                return Err(e.into());
            }
        }
        Err(e) => {
            error!("启动录制失败: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}
