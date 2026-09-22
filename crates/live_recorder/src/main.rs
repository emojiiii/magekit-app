use clap::Parser;
use live_recorder::recorder::LiveRecorder;
use live_recorder::{
    RecordConfig, RecordStatus, RecordingBackend, VideoQuality, streamlink_runtime,
};
use tracing::{info, level_filters::LevelFilter};

#[derive(Parser)]
#[command(version, about = "MageKit 直播录制 / Streamlink 环境管理")]
struct Arguments {
    #[arg(required_unless_present = "runtime")]
    url: Option<String>,
    #[arg(long, value_parser = ["status", "install", "update"], conflicts_with = "url")]
    runtime: Option<String>,
    #[arg(short, long)]
    output: Option<String>,
    #[arg(short, long, default_value = "od", value_parser = ["od", "bd", "uhd", "hd", "sd", "ld"])]
    quality: String,
    #[arg(short, long, default_value = "ts", value_parser = ["ts", "mp4", "mkv", "flv"])]
    format: String,
    #[arg(short, long)]
    proxy: Option<String>,
    #[arg(long)]
    check: bool,
    #[arg(long, value_parser = ["auto", "streamlink", "native"])]
    backend: Option<String>,
    #[arg(long)]
    duration: Option<u64>,
    #[arg(long, default_value_t = 3)]
    retries: u32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .with_writer(std::io::stderr)
        .init();
    let arguments = Arguments::parse();
    if let Some(action) = arguments.runtime {
        let result = match action.as_str() {
            "install" => Some(streamlink_runtime::ensure().await?),
            "update" => Some(streamlink_runtime::update().await?),
            _ => streamlink_runtime::status().await?,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    let recorder = match arguments.backend.as_deref() {
        Some("native") => LiveRecorder::with_backend(RecordingBackend::Native),
        Some("streamlink") => LiveRecorder::with_backend(RecordingBackend::Streamlink),
        Some("auto") => LiveRecorder::with_backend(RecordingBackend::Auto),
        _ => LiveRecorder::new(),
    };
    let url = arguments.url.ok_or("缺少直播间 URL")?;
    if arguments.check {
        println!(
            "{}",
            serde_json::to_string_pretty(&recorder.check_room_status(&url).await?)?
        );
        return Ok(());
    }
    if arguments.duration == Some(0) {
        return Err("录制时长必须大于 0".into());
    }
    let config = RecordConfig {
        output_path_template: arguments.output.unwrap_or_else(|| {
            format!(
                "./downloads/{{platform}}/{{anchor_name}}_{{timestamp}}.{}",
                arguments.format
            )
        }),
        quality: VideoQuality::from_str(&arguments.quality),
        format: arguments.format,
        proxy: arguments.proxy,
        max_duration: arguments.duration,
        retry_count: arguments.retries,
        ..Default::default()
    };
    let mut handle = recorder.start_recording(&url, config).await?;
    info!("录制文件: {}", handle.output_path().display());
    loop {
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                signal?;
                handle.stop().await?; // Await output finalization, not just a stop signal.
                break;
            }
            progress = handle.get_progress() => {
                let Some(progress) = progress else { break }; // Closed channels must not spin forever.
                info!("{:?}: {}s, {} bytes, {} bytes/s", progress.status, progress.duration, progress.size, progress.speed);
                if matches!(progress.status, RecordStatus::Stopped | RecordStatus::Completed | RecordStatus::Error(_)) {
                    break;
                }
            }
        }
    }
    handle.wait().await?;
    Ok(())
}
