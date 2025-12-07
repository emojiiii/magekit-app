use crate::{
    error::{RecorderError, RecorderResult},
    platforms::PlatformFactory,
    types::{RecordConfig, RecordProgress, RecordStatus, StreamData, StreamInfo, VideoQuality},
};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::process::Command as TokioCommand;
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
            platform_handler,
            room_id,
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
    platform_handler: std::sync::Arc<dyn crate::platforms::PlatformHandler>,
    room_id: String,
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

        // HLS 流必须使用 FFmpeg 录制，FLV 流可以直接下载
        let is_hls = self.stream_url.contains(".m3u8");
        let result = if is_hls || self.config.format == "mp4" {
            self.record_with_ffmpeg().await
        } else {
            self.record_direct().await
        };

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
        // 检查FFmpeg是否可用
        if let Err(_) = TokioCommand::new("ffmpeg")
            .arg("-version")
            .output()
            .await
        {
            return Err(RecorderError::FFmpegNotFound);
        }

        info!("🎬 使用 FFmpeg 录制: {} -> {:?}", self.stream_url, self.output_path);

        let mut cmd = TokioCommand::new("ffmpeg");
        
        // 添加输入选项
        cmd.arg("-y"); // 覆盖输出文件
        
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
            let headers_str = headers.iter()
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

        cmd.arg("-i")
            .arg(&self.stream_url)
            .arg("-c")
            .arg("copy");
        
        // 根据输出文件扩展名选择格式
        let output_ext = self.output_path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("ts");
        
        match output_ext {
            "mp4" => {
                cmd.arg("-f").arg("mp4");
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
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        info!("🎬 FFmpeg 命令已构建，开始录制...");

        let child = cmd.spawn()?;
        
        info!("🎬 FFmpeg 进程已启动");

        // 等待进程完成并获取输出
        let output = child.wait_with_output().await?;
        
        if output.status.success() {
            info!("✅ FFmpeg 录制完成");
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            tracing::error!("❌ FFmpeg stderr: {}", stderr);
            if !stdout.is_empty() {
                tracing::error!("❌ FFmpeg stdout: {}", stdout);
            }
            Err(RecorderError::RecordingError(
                format!("FFmpeg录制失败，退出码: {:?}\n错误: {}", output.status.code(), stderr)
            ))
        }
    }

    /// 直接录制流（不使用FFmpeg）
    async fn record_direct(&mut self) -> RecorderResult<()> {
        // 创建带请求头的 HTTP 客户端
        let mut client_builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.config.timeout));

        // 添加代理设置
        if let Some(proxy_url) = &self.config.proxy {
            if let Ok(proxy) = reqwest::Proxy::all(proxy_url) {
                client_builder = client_builder.proxy(proxy);
            }
        }

        let client = client_builder.build()
            .map_err(|e| RecorderError::RecordingError(format!("创建HTTP客户端失败: {}", e)))?;

        // 构建带请求头的请求
        let mut request = client.get(&self.stream_url);
        
        // 添加默认请求头
        request = request.header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");
        request = request.header("Accept", "*/*");
        request = request.header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8");
        request = request.header("Connection", "keep-alive");
        
        // 添加配置中的自定义请求头
        for (key, value) in &self.config.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        // 根据流 URL 添加特定的 Referer
        if self.stream_url.contains("huya") {
            request = request.header("Referer", "https://www.huya.com/");
        } else if self.stream_url.contains("douyin") {
            request = request.header("Referer", "https://live.douyin.com/");
        } else if self.stream_url.contains("douyu") {
            request = request.header("Referer", "https://www.douyu.com/");
        } else if self.stream_url.contains("bilibili") {
            request = request.header("Referer", "https://live.bilibili.com/");
        } else if self.stream_url.contains("kuaishou") {
            request = request.header("Referer", "https://live.kuaishou.com/");
        }

        info!("开始直接录制流: {}", self.stream_url);
        
        let mut response = request.send().await?;

        if !response.status().is_success() {
            return Err(RecorderError::RecordingError(
                format!("HTTP请求失败: {} - 流URL可能已过期或无效", response.status())
            ));
        }

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.output_path)
            .await?;

        let mut writer = BufWriter::new(file);
        let mut downloaded = 0u64;

        // 使用 Arc<AtomicU64> 来在任务间共享下载进度
        let downloaded_shared = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let downloaded_for_task = downloaded_shared.clone();

        // 启动进度监控任务
        let progress_tx = self.progress_tx.clone();
        let start_time = self.start_time;
        let progress_task = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(1));
            let mut prev_size = 0u64;
            loop {
                ticker.tick().await;
                let current_size = downloaded_for_task.load(std::sync::atomic::Ordering::Relaxed);
                let speed = current_size.saturating_sub(prev_size);
                prev_size = current_size;
                
                let progress = RecordProgress {
                    status: RecordStatus::Recording,
                    start_time: Some(chrono::Utc::now()),
                    duration: start_time.elapsed().as_secs(),
                    size: current_size,
                    speed,
                    error: None,
                };
                if progress_tx.send(progress).is_err() {
                    break;
                }
            }
        });

        info!("开始接收流数据...");
        
        let mut chunk_count = 0u64;
        while let Some(chunk) = response.chunk().await? {
            // 检查停止信号
            if self.stop_rx.try_recv().is_ok() {
                info!("收到停止信号，停止录制");
                break;
            }

            let chunk_len = chunk.len() as u64;
            writer.write_all(&chunk).await?;
            downloaded += chunk_len;
            chunk_count += 1;
            
            // 更新共享的下载进度
            downloaded_shared.store(downloaded, std::sync::atomic::Ordering::Relaxed);

            // 每100个chunk记录一次日志
            if chunk_count % 100 == 0 {
                info!("已接收 {} 个数据块，总大小: {} 字节", chunk_count, downloaded);
            }
        }

        // 停止进度监控任务
        progress_task.abort();

        writer.flush().await?;
        info!("✅ 录制完成，总大小: {} 字节 ({:.2} MB)", downloaded, downloaded as f64 / 1024.0 / 1024.0);
        Ok(())
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
                        "录制任务发生panic".to_string()
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