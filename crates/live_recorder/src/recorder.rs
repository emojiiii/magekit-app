use crate::{
    error::{RecorderError, RecorderResult},
    platforms::PlatformFactory,
    types::{RecordConfig, RecordProgress, RecordStatus, StreamData, StreamInfo, VideoQuality},
};
use magekit_shared::create_tokio_command;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tokio::time::interval;
use tracing::{info, warn};

/// 直播录制器
pub struct LiveRecorder {
    platform_factory: PlatformFactory,
}

impl LiveRecorder {
    /// 创建新的直播录制器
    pub fn new() -> Self {
        Self {
            platform_factory: PlatformFactory::new(),
        }
    }

    /// 使用自定义平台工厂创建录制器
    pub fn with_factory(platform_factory: PlatformFactory) -> Self {
        Self { platform_factory }
    }

    /// 开始录制直播
    pub async fn start_recording(
        &self,
        url: &str,
        config: RecordConfig,
    ) -> RecorderResult<RecordingHandle> {
        info!("开始录制直播: {}", url);

        // 获取平台处理器
        let platform_handler = self.platform_factory.get_handler_for_url(url)?;

        // 获取房间ID
        let room_id = platform_handler.extract_room_id(url).await?;
        info!("获取到房间ID: {}", room_id);

        // 获取流信息
        let stream_info = platform_handler.get_stream_info(&room_id).await?;

        if stream_info.room.status != crate::types::LiveStatus::Live {
            return Err(RecorderError::StreamNotAvailable(
                "直播间未开播".to_string(),
            ));
        }

        // 选择最佳的流
        let selected_stream = self.select_best_stream(&stream_info, &config.quality)?;

        // 优先选择 HLS 流（使用 FFmpeg 录制）
        let stream_url = selected_stream
            .url
            .hls_url
            .clone()
            .or_else(|| selected_stream.url.flv_url.clone())
            .ok_or_else(|| RecorderError::StreamNotAvailable("没有可用的流URL".to_string()))?;

        // 检测是否是 HLS 流
        let is_hls = stream_url.contains(".m3u8");

        info!("选择流URL: {} (HLS: {})", stream_url, is_hls);

        // 创建输出文件路径
        let output_path = self.generate_output_path(&stream_info, &config);

        // 创建录制会话
        let (stop_tx, stop_rx) = oneshot::channel();
        let (progress_tx, progress_rx) = mpsc::unbounded_channel();

        let session = RecordingSession {
            stop_rx,
            progress_tx,
            config: config.clone(),
            stream_url,
            output_path: output_path.clone(),
            start_time: Instant::now(),
        };

        // 启动录制任务
        let session_task = tokio::spawn(session.run());

        Ok(RecordingHandle {
            _task: session_task,
            stop_tx: Some(stop_tx),
            progress_rx,
            output_path,
            status: RecordStatus::Connecting,
        })
    }

    /// 检查直播间状态
    pub async fn check_room_status(&self, url: &str) -> RecorderResult<crate::types::LiveRoomInfo> {
        let platform_handler = self.platform_factory.get_handler_for_url(url)?;
        let room_id = platform_handler.extract_room_id(url).await?;
        let stream_info = platform_handler.get_stream_info(&room_id).await?;
        Ok(stream_info.room)
    }

    /// 获取可用流信息
    pub async fn get_stream_info(&self, url: &str) -> RecorderResult<StreamInfo> {
        let platform_handler = self.platform_factory.get_handler_for_url(url)?;
        let room_id = platform_handler.extract_room_id(url).await?;
        platform_handler.get_stream_info(&room_id).await
    }

    /// 选择最佳的流
    fn select_best_stream<'a>(
        &self,
        stream_info: &'a StreamInfo,
        preferred_quality: &VideoQuality,
    ) -> RecorderResult<&'a StreamData> {
        let target_level = preferred_quality.level();

        // 尝试找到指定质量的流
        if let Some(stream) = stream_info
            .streams
            .iter()
            .find(|s| s.quality.level() == target_level)
        {
            return Ok(stream);
        }

        // 如果没有找到指定质量，选择最接近的
        let closest_stream = stream_info
            .streams
            .iter()
            .min_by_key(|s| (s.quality.level() as i8 - target_level as i8).abs())
            .ok_or_else(|| RecorderError::StreamNotAvailable("没有可用的流".to_string()))?;

        warn!(
            "未找到指定质量 {:?}，选择最接近的质量 {:?}",
            preferred_quality, closest_stream.quality
        );

        Ok(closest_stream)
    }

    /// 生成输出文件路径
    fn generate_output_path(&self, stream_info: &StreamInfo, config: &RecordConfig) -> PathBuf {
        let template = &config.output_path_template;

        let path = template
            .replace("{platform}", "douyin")
            .replace("{anchor_name}", &stream_info.room.anchor_name)
            .replace("{room_id}", &stream_info.room.room_id)
            .replace("{title}", &stream_info.room.title)
            .replace(
                "{timestamp}",
                &chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string(),
            )
            .replace("{quality}", &format!("{:?}", config.quality));

        // 确保目录存在
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!("无法创建输出目录 {}: {}", parent.display(), e);
            }
        }

        path
    }
}

impl Default for LiveRecorder {
    fn default() -> Self {
        Self::new()
    }
}

/// 录制会话
struct RecordingSession {
    stop_rx: oneshot::Receiver<()>,
    progress_tx: mpsc::UnboundedSender<RecordProgress>,
    config: RecordConfig,
    stream_url: String,
    output_path: PathBuf,
    start_time: Instant,
}

impl RecordingSession {
    /// 运行录制会话
    async fn run(mut self) -> RecorderResult<()> {
        info!("开始录制会话: {:?}", self.output_path);

        // 发送初始状态
        let _ = self.progress_tx.send(RecordProgress {
            status: RecordStatus::Recording,
            start_time: Some(chrono::Utc::now()),
            duration: 0,
            size: 0,
            speed: 0,
            error: None,
        });

        // 始终使用 FFmpeg 录制，因为它更稳定，能处理直播流的各种问题
        // 直接下载 FLV 流容易断开连接
        let result = self.record_with_ffmpeg().await;

        // 发送最终状态
        let final_status = match &result {
            Ok(_) => RecordStatus::Completed,
            Err(e) => RecordStatus::Error(e.to_string()),
        };

        let error_clone = result.as_ref().err().map(|e| e.to_string());

        let _ = self.progress_tx.send(RecordProgress {
            status: final_status,
            start_time: Some(chrono::Utc::now()),
            duration: self.start_time.elapsed().as_secs(),
            size: 0,
            speed: 0,
            error: error_clone,
        });

        result
    }

    /// 使用FFmpeg进行录制
    async fn record_with_ffmpeg(&mut self) -> RecorderResult<()> {
        // 检查FFmpeg是否可用（使用无窗口命令）
        if let Err(_) = create_tokio_command("ffmpeg")
            .arg("-version")
            .output()
            .await
        {
            return Err(RecorderError::FFmpegNotFound);
        }

        info!(
            "🎬 使用 FFmpeg 录制: {} -> {:?}",
            self.stream_url, self.output_path
        );

        let mut cmd = create_tokio_command("ffmpeg");

        // 添加输入选项
        cmd.arg("-hide_banner");
        cmd.arg("-loglevel").arg("warning"); // 降噪，避免 HLS "Opening ..." 刷屏
        cmd.arg("-nostats");
        cmd.arg("-y"); // 覆盖输出文件

        // 重连选项 - 对于直播流很重要
        cmd.arg("-reconnect").arg("1");
        cmd.arg("-reconnect_at_eof").arg("1");
        cmd.arg("-reconnect_streamed").arg("1");
        cmd.arg("-reconnect_delay_max").arg("5");

        // IO 超时（microseconds），避免断流后长时间卡住
        let rw_timeout_us = self.config.timeout.saturating_mul(1_000_000).max(5_000_000);
        cmd.arg("-rw_timeout").arg(rw_timeout_us.to_string());

        // 添加请求头（必须在 -i 之前）
        // 注意：每个 header 后面都需要 \r\n，包括最后一个
        let mut headers = vec![
            "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
        ];

        // 根据流 URL 添加特定的 Referer
        if self.stream_url.contains("huya") {
            headers.push("Referer: https://www.huya.com/".to_string());
            headers.push("Origin: https://www.huya.com".to_string());
        } else if self.stream_url.contains("douyin") {
            headers.push("Referer: https://live.douyin.com/".to_string());
            headers.push("Origin: https://live.douyin.com".to_string());
        } else if self.stream_url.contains("douyu") {
            headers.push("Referer: https://www.douyu.com/".to_string());
            headers.push("Origin: https://www.douyu.com".to_string());
        }

        for (key, value) in &self.config.headers {
            headers.push(format!("{}: {}", key, value));
        }

        if !headers.is_empty() {
            // FFmpeg 要求每个 header 以 \r\n 结尾
            let headers_str = headers
                .iter()
                .map(|h| format!("{}\r\n", h))
                .collect::<String>();
            cmd.arg("-headers");
            cmd.arg(headers_str);
        }

        // 添加代理设置
        if let Some(proxy) = &self.config.proxy {
            cmd.arg("-http_proxy");
            cmd.arg(proxy);
        }

        cmd.arg("-i").arg(&self.stream_url).arg("-c").arg("copy");

        // 根据输出文件扩展名选择格式
        let output_ext = self
            .output_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("ts");

        match output_ext {
            "mp4" => {
                cmd.arg("-f").arg("mp4");
                // mp4 长时间录制：使用 fragmented mp4，避免异常退出导致文件不可用
                cmd.arg("-movflags")
                    .arg("+frag_keyframe+empty_moov+default_base_moof");
            }
            "ts" => {
                cmd.arg("-f").arg("mpegts");
            }
            "flv" => {
                cmd.arg("-f").arg("flv");
            }
            _ => {
                cmd.arg("-f").arg("mpegts");
            }
        }

        cmd.arg(&self.output_path)
            // 不要 pipe stdout/stderr：如果不消费 pipe，ffmpeg 会因为缓冲区写满而卡住（进程还在，但文件不再增长）
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true); // 当任务被 drop 时自动杀死进程

        info!("🎬 FFmpeg 命令已构建，开始录制...");

        let mut child = cmd.spawn()?;

        info!("🎬 FFmpeg 进程已启动, PID: {:?}", child.id());

        // 启动进度监控任务
        let progress_tx = self.progress_tx.clone();
        let start_time = self.start_time;
        let output_path = self.output_path.clone();

        let progress_task = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(1));
            loop {
                ticker.tick().await;

                // 获取文件大小
                let size = tokio::fs::metadata(&output_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);

                let progress = RecordProgress {
                    status: RecordStatus::Recording,
                    start_time: Some(chrono::Utc::now()),
                    duration: start_time.elapsed().as_secs(),
                    size,
                    speed: 0, // TODO: 计算速度
                    error: None,
                };
                if progress_tx.send(progress).is_err() {
                    break;
                }
            }
        });

        // 等待停止信号或进程结束
        tokio::select! {
            // 收到停止信号
            _ = &mut self.stop_rx => {
                info!("🛑 收到停止信号，终止 FFmpeg 进程...");
                child.kill().await.ok();
                progress_task.abort();
                info!("✅ FFmpeg 进程已终止");
                Ok(())
            }
            // FFmpeg 进程自己退出了（可能是流断开）
            result = child.wait() => {
                progress_task.abort();
                match result {
                    Ok(status) => {
                        if status.success() || status.code() == Some(255) {
                            // 255 通常表示被信号终止，这是正常的
                            info!("✅ FFmpeg 录制完成");
                            Ok(())
                        } else {
                            tracing::error!("❌ FFmpeg 退出码: {:?}", status.code());

                            Err(RecorderError::RecordingError(
                                format!("FFmpeg录制失败，退出码: {:?}", status.code())
                            ))
                        }
                    }
                    Err(e) => {
                        tracing::error!("❌ 等待 FFmpeg 进程失败: {}", e);
                        Err(RecorderError::RecordingError(
                            format!("等待FFmpeg进程失败: {}", e)
                        ))
                    }
                }
            }
        }
    }
}

/// 录制句柄，用于控制录制过程
pub struct RecordingHandle {
    _task: tokio::task::JoinHandle<RecorderResult<()>>,
    stop_tx: Option<oneshot::Sender<()>>,
    progress_rx: mpsc::UnboundedReceiver<RecordProgress>,
    output_path: PathBuf,
    status: RecordStatus,
}

impl RecordingHandle {
    /// 停止录制
    pub async fn stop(&mut self) -> RecorderResult<()> {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        self.status = RecordStatus::Stopped;
        Ok(())
    }

    /// 获取进度信息
    pub async fn get_progress(&mut self) -> Option<RecordProgress> {
        self.progress_rx.recv().await
    }

    /// 等待录制完成
    pub async fn wait(self) -> RecorderResult<()> {
        match self._task.await {
            Ok(result) => result,
            Err(e) => {
                if e.is_panic() {
                    Err(RecorderError::RecordingError(
                        "录制任务发生panic".to_string(),
                    ))
                } else {
                    Err(RecorderError::TaskCancelled)
                }
            }
        }
    }

    /// 获取输出文件路径
    pub fn output_path(&self) -> &Path {
        &self.output_path
    }

    /// 获取当前状态
    pub fn status(&self) -> &RecordStatus {
        &self.status
    }
}
