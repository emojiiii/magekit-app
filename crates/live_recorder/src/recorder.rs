use crate::{
    error::{RecorderError, RecorderResult},
    platforms::{PlatformFactory, PlatformHandler},
    types::{RecordConfig, RecordProgress, RecordStatus, StreamData, StreamInfo, VideoQuality},
};
use magekit_shared::create_tokio_command;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio::time::interval;
use tracing::{info, warn};
use url::Url;

#[derive(Debug, Default)]
struct FfmpegMetrics {
    total_size: u64,
    out_time_ms: u64,
    speed_bps: u64,
    last_sample_at: Option<Instant>,
    last_sample_size: u64,
    last_progress_at: Option<Instant>,
}

fn is_noise_ffmpeg_stderr_line(line: &str) -> bool {
    // 常见噪声：HLS demuxer 会刷屏打印每个分片的 Opening...，对排障价值很低。
    // 这里过滤仅影响“错误回传的 stderr tail”，不会影响 ffmpeg 自身运行。
    line.contains("[hls @") && line.contains("Opening '")
}

fn is_hevc_codec_url(url: &str) -> bool {
    // 抖音的 URL 常带 codec=h264/h265；FLV 容器通常不支持 H.265，优先回退到 HLS。
    // 这里使用字符串匹配，避免为此引入额外解析复杂度（KISS）。
    let lowered = url.to_ascii_lowercase();
    lowered.contains("codec=h265")
        || lowered.contains("codec=hevc")
        || lowered.contains("codec=hvc1")
        || lowered.contains("codec=hev1")
}

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

        let platform_name = platform_handler.platform_name().to_string();
        let room_url = url.to_string();

        // 构造候选流 URL：抖音优先 FLV（并发下更稳定），其他平台优先 HLS
        let mut stream_urls = Vec::new();
        let hls_url = selected_stream.url.hls_url.clone();
        let flv_url = selected_stream.url.flv_url.clone();

        if platform_name == "douyin" || platform_name == "huya" {
            if let Some(flv) = flv_url.clone() {
                if is_hevc_codec_url(&flv) {
                    warn!("检测到抖音 FLV 为 H.265，跳过 FLV 改用 HLS");
                } else {
                    stream_urls.push(flv);
                }
            }
            if let Some(hls) = hls_url.clone() {
                stream_urls.push(hls);
            }
        } else {
            if let Some(hls) = hls_url.clone() {
                stream_urls.push(hls);
            }
            if let Some(flv) = flv_url.clone() {
                stream_urls.push(flv);
            }
        }

        if stream_urls.is_empty() {
            return Err(RecorderError::StreamNotAvailable(
                "没有可用的流URL".to_string(),
            ));
        }

        let stream_url = stream_urls[0].clone();
        let is_hls = stream_url.contains(".m3u8");
        info!(
            "选择流URL: {} (HLS: {}), 候选数量: {}",
            stream_url,
            is_hls,
            stream_urls.len()
        );

        // 创建输出文件路径
        let output_path = self.generate_output_path(&stream_info, &config);

        // 创建录制会话
        let (stop_tx, stop_rx) = oneshot::channel();
        let (progress_tx, progress_rx) = mpsc::unbounded_channel();

        let session = RecordingSession {
            stop_rx,
            progress_tx,
            config: config.clone(),
            platform_handler: platform_handler.clone(),
            room_id: room_id.clone(),
            platform_name,
            room_url,
            stream_urls,
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

    fn default_referer_and_origin(
        platform_name: &str,
        room_url: &str,
    ) -> (Option<String>, Option<String>) {
        let platform = platform_name;

        let parsed = Url::parse(room_url).ok();
        let origin_from_room = parsed.as_ref().and_then(|u| {
            let host = u.host_str()?;
            Some(format!("{}://{}", u.scheme(), host))
        });

        let referer_from_room = Some(room_url.to_string());

        match platform {
            "douyin" => (
                Some(room_url.to_string()),
                origin_from_room.or_else(|| Some("https://live.douyin.com".to_string())),
            ),
            "bilibili" => (
                Some(room_url.to_string()),
                origin_from_room.or_else(|| Some("https://live.bilibili.com".to_string())),
            ),
            "huya" => (
                Some(room_url.to_string()),
                origin_from_room.or_else(|| Some("https://www.huya.com".to_string())),
            ),
            "douyu" => (
                Some(room_url.to_string()),
                origin_from_room.or_else(|| Some("https://www.douyu.com".to_string())),
            ),
            "kuaishou" => (
                referer_from_room,
                origin_from_room.or_else(|| Some("https://live.kuaishou.com".to_string())),
            ),
            _ => (referer_from_room, origin_from_room),
        }
    }

    async fn hls_preflight(
        config: &RecordConfig,
        stream_url: &str,
        headers: &[String],
    ) -> RecorderResult<()> {
        info!("🔎 HLS 预检开始: {}", stream_url);
        let client = {
            let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(10));

            if let Some(proxy) = &config.proxy {
                let proxy = reqwest::Proxy::all(proxy)
                    .map_err(|e| RecorderError::ProxyError(e.to_string()))?;
                builder = builder.proxy(proxy);
            }

            builder
                .build()
                .map_err(|e| RecorderError::RecordingPreflightFailed {
                    reason: "创建 HTTP Client 失败".to_string(),
                    details: format!("\n{}", e),
                })?
        };

        let m3u8_url =
            Url::parse(stream_url).map_err(|e| RecorderError::RecordingPreflightFailed {
                reason: "m3u8 URL 非法".to_string(),
                details: format!("\n{}", e),
            })?;

        async fn fetch_m3u8_text(
            client: &reqwest::Client,
            headers: &[String],
            url: &str,
        ) -> Result<String, RecorderError> {
            let mut req = client.get(url);
            for h in headers {
                if let Some((k, v)) = h.split_once(':') {
                    req = req.header(k.trim(), v.trim());
                }
            }

            let resp = req
                .send()
                .await
                .map_err(|e| RecorderError::RecordingPreflightFailed {
                    reason: "请求 m3u8 失败".to_string(),
                    details: format!("\n{}", e),
                })?;

            if !resp.status().is_success() {
                return Err(RecorderError::RecordingPreflightFailed {
                    reason: format!("请求 m3u8 返回 {}", resp.status()),
                    details: String::new(),
                });
            }

            let text = resp
                .text()
                .await
                .map_err(|e| RecorderError::RecordingPreflightFailed {
                    reason: "读取 m3u8 内容失败".to_string(),
                    details: format!("\n{}", e),
                })?;

            if !text.trim_start().starts_with("#EXTM3U") {
                let snippet: String = text.chars().take(200).collect();
                return Err(RecorderError::RecordingPreflightFailed {
                    reason: "m3u8 内容异常（缺少 #EXTM3U）".to_string(),
                    details: format!("\n响应前 200 字符:\n{}", snippet),
                });
            }

            Ok::<String, RecorderError>(text)
        }

        let text = fetch_m3u8_text(&client, headers, stream_url).await?;
        let first_media_line = text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with('#'))
            .ok_or_else(|| RecorderError::RecordingPreflightFailed {
                reason: "m3u8 里未找到可用条目".to_string(),
                details: String::new(),
            })?;

        // 兼容 master playlist：首个非 # 行可能是子 m3u8，而不是分片。
        let (playlist_url_for_segment, segment_line) = if first_media_line.contains(".m3u8") {
            let variant_url = if first_media_line.starts_with("http://")
                || first_media_line.starts_with("https://")
            {
                Url::parse(first_media_line).map_err(|e| {
                    RecorderError::RecordingPreflightFailed {
                        reason: "子 m3u8 URL 非法".to_string(),
                        details: format!("\n{}", e),
                    }
                })?
            } else {
                m3u8_url.join(first_media_line).map_err(|e| {
                    RecorderError::RecordingPreflightFailed {
                        reason: "拼接子 m3u8 URL 失败".to_string(),
                        details: format!("\n{}", e),
                    }
                })?
            };

            info!("🔎 HLS master 检测到子 m3u8: {}", variant_url);
            let variant_text = fetch_m3u8_text(&client, headers, variant_url.as_str()).await?;
            let segment_line = variant_text
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty() && !l.starts_with('#'))
                .ok_or_else(|| RecorderError::RecordingPreflightFailed {
                    reason: "子 m3u8 里未找到可用分片".to_string(),
                    details: String::new(),
                })?;

            (variant_url, segment_line.to_string())
        } else {
            (m3u8_url, first_media_line.to_string())
        };

        let segment_url =
            if segment_line.starts_with("http://") || segment_line.starts_with("https://") {
                Url::parse(&segment_line).map_err(|e| RecorderError::RecordingPreflightFailed {
                    reason: "分片 URL 非法".to_string(),
                    details: format!("\n{}", e),
                })?
            } else {
                playlist_url_for_segment.join(&segment_line).map_err(|e| {
                    RecorderError::RecordingPreflightFailed {
                        reason: "拼接分片 URL 失败".to_string(),
                        details: format!("\n{}", e),
                    }
                })?
            };

        let mut seg_req = client.get(segment_url.as_str());
        for h in headers {
            if let Some((k, v)) = h.split_once(':') {
                seg_req = seg_req.header(k.trim(), v.trim());
            }
        }
        seg_req = seg_req.header("Range", "bytes=0-2047");

        let seg_resp =
            seg_req
                .send()
                .await
                .map_err(|e| RecorderError::RecordingPreflightFailed {
                    reason: "请求首个分片失败".to_string(),
                    details: format!("\n{}", e),
                })?;

        if !(seg_resp.status().is_success()
            || seg_resp.status() == reqwest::StatusCode::PARTIAL_CONTENT)
        {
            return Err(RecorderError::RecordingPreflightFailed {
                reason: format!("请求首个分片返回 {}", seg_resp.status()),
                details: String::new(),
            });
        }

        let bytes =
            seg_resp
                .bytes()
                .await
                .map_err(|e| RecorderError::RecordingPreflightFailed {
                    reason: "读取首个分片失败".to_string(),
                    details: format!("\n{}", e),
                })?;

        if bytes.is_empty() {
            return Err(RecorderError::RecordingPreflightFailed {
                reason: "首个分片返回空数据".to_string(),
                details: String::new(),
            });
        }

        // 简单校验：TS 以 0x47 开头；fMP4 通常包含 "ftyp"。
        let is_ts = bytes.first().is_some_and(|b| *b == 0x47);
        let is_fmp4 = bytes.windows(4).any(|w| w == b"ftyp");
        if !is_ts && !is_fmp4 {
            let head = &bytes[..bytes.len().min(256)];
            let head_lossy = String::from_utf8_lossy(head);
            return Err(RecorderError::RecordingPreflightFailed {
                reason: "首个分片内容异常（不是 TS/fMP4）".to_string(),
                details: format!(
                    "\nsegment_url={}\n响应前 256 字节(UTF-8 lossy):\n{}",
                    segment_url, head_lossy
                ),
            });
        }

        info!(
            "✅ HLS 预检通过: segment_url={} (TS={}, fMP4={})",
            segment_url, is_ts, is_fmp4
        );
        Ok(())
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
    platform_handler: Arc<dyn PlatformHandler>,
    room_id: String,
    platform_name: String,
    room_url: String,
    stream_urls: Vec<String>,
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
            status: RecordStatus::Connecting,
            start_time: Some(chrono::Utc::now()),
            duration: 0,
            size: 0,
            speed: 0,
            error: None,
        });

        // 始终使用 FFmpeg 录制，因为它更稳定，能处理直播流的各种问题
        // 直接下载 FLV 流容易断开连接
        let result = if self.platform_name == "huya" {
            self.record_huya_ffmpeg_loop().await
        } else {
            self.record_with_ffmpeg().await
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

    /// 使用 FFmpeg 进行录制（带有限重试）
    #[allow(dead_code)]
    async fn record_huya(&mut self) -> RecorderResult<()> {
        // Huya：优先使用内置 HLS（reqwest）拉流，避免 FFmpeg 在 Huya CDN 上出现 403 导致无法持续录制。
        if let Some(hls_url) = self
            .stream_urls
            .iter()
            .find(|u| u.contains(".m3u8"))
            .cloned()
        {
            self.stream_url = hls_url.clone();
            match self.record_hls_ts_with_reqwest(&hls_url).await {
                Ok(()) => return Ok(()),
                Err(e) => warn!("虎牙 HLS 内置录制失败，回退 FFmpeg：{}", e),
            }
        }

        // 回退：优先尝试 FLV（Huya 的 HLS 在 FFmpeg 下可能 403）
        if let Some(flv_url) = self
            .stream_urls
            .iter()
            .find(|u| !u.contains(".m3u8"))
            .cloned()
        {
            self.stream_urls = vec![flv_url.clone()];
            self.stream_url = flv_url;
        }

        self.record_with_ffmpeg().await
    }

    #[allow(dead_code)]
    async fn record_hls_ts_with_reqwest(&mut self, stream_url: &str) -> RecorderResult<()> {
        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

        info!(
            "🧩 使用内置 HLS 录制（Huya）: {} -> {:?}",
            stream_url, self.output_path
        );

        let output_path = self.output_path.clone();
        let output_ext = output_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("ts")
            .to_ascii_lowercase();
        if output_ext != "ts" {
            return Err(RecorderError::ConfigError(
                "虎牙内置 HLS 录制当前仅支持 ts 输出（请将录制格式设置为 ts）".to_string(),
            ));
        }
        let room_url = self.room_url.clone();
        let platform_name = self.platform_name.clone();
        let start_instant = self.start_time;
        let progress_tx = self.progress_tx.clone();
        let timeout_secs = self.config.timeout.max(15);
        let extra_headers = self.config.headers.clone();

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()?;

        let mut headers_vec = vec![
            "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
        ];

        let (referer, origin) = LiveRecorder::default_referer_and_origin(&platform_name, &room_url);
        if let Some(referer) = referer {
            headers_vec.push(format!("Referer: {}", referer));
        }
        if let Some(origin) = origin {
            headers_vec.push(format!("Origin: {}", origin));
        }
        for (key, value) in extra_headers {
            headers_vec.push(format!("{}: {}", key, value));
        }

        let mut header_map = HeaderMap::new();
        for h in &headers_vec {
            let Some((k, v)) = h.split_once(':') else {
                continue;
            };
            let key = k.trim();
            let val = v.trim();
            let Ok(name) = HeaderName::from_bytes(key.as_bytes()) else {
                continue;
            };
            let Ok(value) = HeaderValue::from_str(val) else {
                continue;
            };
            header_map.insert(name, value);
        }

        let master_url = Url::parse(stream_url).map_err(RecorderError::UrlParse)?;
        let master_resp = tokio::select! {
            _ = &mut self.stop_rx => {
                info!("🛑 收到停止信号，取消 HLS 录制");
                return Ok(());
            }
            resp = client.get(master_url.as_str()).headers(header_map.clone()).send() => resp?,
        };
        if !master_resp.status().is_success() {
            return Err(RecorderError::RecordingPreflightFailed {
                reason: format!("请求 m3u8 返回 {}", master_resp.status()),
                details: String::new(),
            });
        }
        let master_text = master_resp.text().await?;
        if !master_text.contains("#EXTM3U") {
            return Err(RecorderError::RecordingPreflightFailed {
                reason: "m3u8 内容异常（缺少 #EXTM3U）".to_string(),
                details: String::new(),
            });
        }

        // 兼容 master playlist：首个非 # 行可能是子 m3u8（variant），也可能就是分片条目
        let first_media_line = master_text
            .lines()
            .map(|l| l.trim())
            .find(|l| !l.is_empty() && !l.starts_with('#'))
            .ok_or_else(|| RecorderError::RecordingPreflightFailed {
                reason: "m3u8 里未找到可用条目".to_string(),
                details: String::new(),
            })?
            .to_string();

        let variant_url = if first_media_line.contains(".m3u8") {
            master_url.join(&first_media_line).map_err(|e| {
                RecorderError::RecordingPreflightFailed {
                    reason: "拼接子 m3u8 URL 失败".to_string(),
                    details: format!("\n{}", e),
                }
            })?
        } else {
            master_url
        };

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&output_path)
            .await?;

        let poll_interval = Duration::from_secs(1);
        let stall_timeout = Duration::from_secs(self.config.timeout.max(60).saturating_mul(2));
        let mut last_write_at = Instant::now();
        let mut last_seq: Option<u64> = None;

        loop {
            let playlist_resp = tokio::select! {
                _ = &mut self.stop_rx => {
                    info!("🛑 收到停止信号，停止 HLS 录制");
                    return Ok(());
                }
                resp = client.get(variant_url.as_str()).headers(header_map.clone()).send() => resp?,
            };

            if !playlist_resp.status().is_success() {
                return Err(RecorderError::RecordingStalled {
                    reason: format!("请求播放列表失败：{}", playlist_resp.status()),
                    details: String::new(),
                });
            }

            let playlist_text = playlist_resp.text().await?;
            if playlist_text.contains("#EXT-X-KEY") {
                return Err(RecorderError::RecordingError(
                    "暂不支持带加密的 HLS（#EXT-X-KEY）".to_string(),
                ));
            }

            let ended = playlist_text.contains("#EXT-X-ENDLIST");
            let mut media_sequence: Option<u64> = None;
            let mut segments: Vec<String> = Vec::new();

            for line in playlist_text
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
            {
                if let Some(v) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
                    media_sequence = v.trim().parse::<u64>().ok();
                    continue;
                }
                if line.starts_with('#') {
                    continue;
                }
                segments.push(line.to_string());
            }

            if segments.is_empty() {
                if ended {
                    info!("✅ HLS 播放列表结束（#EXT-X-ENDLIST）");
                    return Ok(());
                }

                if last_write_at.elapsed() > stall_timeout {
                    return Err(RecorderError::RecordingStalled {
                        reason: format!("HLS 长时间无新分片（>{}s）", stall_timeout.as_secs()),
                        details: String::new(),
                    });
                }

                tokio::select! {
                    _ = &mut self.stop_rx => {
                        info!("🛑 收到停止信号，停止 HLS 录制");
                        return Ok(());
                    }
                    _ = tokio::time::sleep(poll_interval) => {}
                }
                continue;
            }

            // 首次进入：从尾部开始抓，避免一次性下载太多历史分片
            let start_index = if last_seq.is_none() {
                segments.len().saturating_sub(3)
            } else {
                0
            };
            let base_seq = media_sequence.unwrap_or(0);

            for (idx, seg) in segments.into_iter().enumerate().skip(start_index) {
                let seq = base_seq.saturating_add(idx as u64);
                if let Some(last) = last_seq {
                    if seq <= last {
                        continue;
                    }
                }

                let lowered = seg.to_ascii_lowercase();
                if lowered.ends_with(".m4s") || lowered.ends_with(".mp4") {
                    return Err(RecorderError::RecordingError(
                        "暂不支持 fMP4 分片（.m4s/.mp4）".to_string(),
                    ));
                }

                let seg_url = variant_url.join(&seg).map_err(|e| {
                    RecorderError::RecordingError(format!("拼接分片 URL 失败: {}", e))
                })?;

                let seg_resp = tokio::select! {
                    _ = &mut self.stop_rx => {
                        info!("🛑 收到停止信号，停止 HLS 录制");
                        return Ok(());
                    }
                    resp = client.get(seg_url.as_str()).headers(header_map.clone()).send() => resp?,
                };

                if !seg_resp.status().is_success() {
                    return Err(RecorderError::RecordingStalled {
                        reason: format!("下载分片失败：{}", seg_resp.status()),
                        details: format!("\n{}", seg_url),
                    });
                }

                let bytes = seg_resp.bytes().await?;
                file.write_all(&bytes).await?;
                last_write_at = Instant::now();
                last_seq = Some(seq);

                let size = tokio::fs::metadata(&output_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);

                let _ = progress_tx.send(RecordProgress {
                    status: RecordStatus::Recording,
                    start_time: Some(chrono::Utc::now()),
                    duration: start_instant.elapsed().as_secs().max(1),
                    size,
                    speed: 0,
                    error: None,
                });
            }

            if ended {
                info!("✅ HLS 播放列表结束（#EXT-X-ENDLIST）");
                return Ok(());
            }

            if last_write_at.elapsed() > stall_timeout {
                return Err(RecorderError::RecordingStalled {
                    reason: format!("HLS 长时间无新分片（>{}s）", stall_timeout.as_secs()),
                    details: String::new(),
                });
            }

            tokio::select! {
                _ = &mut self.stop_rx => {
                    info!("🛑 收到停止信号，停止 HLS 录制");
                    return Ok(());
                }
                _ = tokio::time::sleep(poll_interval) => {}
            }
        }
    }

    async fn record_with_ffmpeg(&mut self) -> RecorderResult<()> {
        let max_attempts = self.config.retry_count.saturating_add(1).max(1);
        let mut last_error: Option<RecorderError> = None;

        for attempt in 1..=max_attempts {
            let stream_urls = self.stream_urls.clone();
            for (idx, url) in stream_urls.into_iter().enumerate() {
                if self.stream_url != url {
                    info!(
                        "🎬 切换录制流（{}/{}，候选 {}/{}）: {}",
                        attempt,
                        max_attempts,
                        idx + 1,
                        self.stream_urls.len(),
                        url
                    );
                    self.stream_url = url;
                }

                match self.record_with_ffmpeg_once().await {
                    Ok(()) => return Ok(()),
                    Err(err) => {
                        let retryable = matches!(
                            err,
                            RecorderError::RecordingStartupTimeout { .. }
                                | RecorderError::RecordingStalled { .. }
                                | RecorderError::RecordingPreflightFailed { .. }
                        );

                        if !retryable {
                            return Err(err);
                        }

                        last_error = Some(err);
                    }
                }
            }

            if attempt >= max_attempts {
                break;
            }

            if let Some(ref err) = last_error {
                warn!(
                    "🎬 录制失败，准备重试（{}/{}）: {}",
                    attempt, max_attempts, err
                );
            }

            // 简单退避，避免瞬时并发导致的拉流失败；支持 stop 信号快速退出
            tokio::select! {
                _ = &mut self.stop_rx => {
                    info!("🛑 收到停止信号，取消重试");
                    return Ok(());
                }
                _ = tokio::time::sleep(Duration::from_secs(3)) => {}
            }
        }

        Err(last_error
            .unwrap_or_else(|| RecorderError::RecordingError("录制失败：未知原因".to_string())))
    }

    /// 使用FFmpeg进行录制
    async fn record_with_ffmpeg_once(&mut self) -> RecorderResult<()> {
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
        // 用于实时进度/时长估算：key=value 输出到 stdout（与 stderr 日志分离）
        cmd.arg("-progress").arg("pipe:1");
        cmd.arg("-y"); // 覆盖输出文件

        // 重连选项 - 对于直播流很重要
        cmd.arg("-reconnect").arg("1");
        cmd.arg("-reconnect_at_eof").arg("1");
        cmd.arg("-reconnect_streamed").arg("1");
        // 对齐 py_demo：并发下避免过于激进的重连导致“瞬时风控/EOF”放大
        let reconnect_delay_max_secs = self.config.timeout.max(5).min(60);
        cmd.arg("-reconnect_delay_max")
            .arg(reconnect_delay_max_secs.to_string());

        // IO 超时（microseconds），避免断流后长时间卡住
        let rw_timeout_us = self.config.timeout.saturating_mul(1_000_000).max(5_000_000);
        cmd.arg("-rw_timeout").arg(rw_timeout_us.to_string());

        // 添加请求头（必须在 -i 之前）
        // 注意：每个 header 后面都需要 \r\n，包括最后一个
        let mut headers = vec![
            "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
        ];

        // 根据流 URL 添加特定的 Referer
        // 以“直播间 URL / 平台名”为准设置 Referer/Origin（不要依赖 CDN 的 stream_url）
        let (referer, origin) =
            LiveRecorder::default_referer_and_origin(&self.platform_name, &self.room_url);
        if let Some(referer) = referer {
            headers.push(format!("Referer: {}", referer));
        }
        if let Some(origin) = origin {
            headers.push(format!("Origin: {}", origin));
        }

        for (key, value) in &self.config.headers {
            headers.push(format!("{}: {}", key, value));
        }

        // HLS 预检：先拉 m3u8 + 首段，尽早暴露 403/风控/超时 等原因（并发场景尤为关键）
        if self.stream_url.contains(".m3u8") {
            LiveRecorder::hls_preflight(&self.config, &self.stream_url, &headers).await?;
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
            // stdout 用于 -progress pipe:1
            .stdout(Stdio::piped())
            // stderr 需要消费：用于启动失败诊断，且避免缓冲区写满导致死锁
            .stderr(Stdio::piped())
            .kill_on_drop(true); // 当任务被 drop 时自动杀死进程

        info!("🎬 FFmpeg 命令已构建，开始录制...");

        let mut child = cmd.spawn()?;
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();

        info!("🎬 FFmpeg 进程已启动, PID: {:?}", child.id());

        // 启动进度监控任务
        let progress_tx = self.progress_tx.clone();
        let start_time = self.start_time;
        let output_path = self.output_path.clone();
        let started_writing = Arc::new(AtomicBool::new(false));
        let started_writing_clone = started_writing.clone();
        let metrics = Arc::new(tokio::sync::Mutex::new(FfmpegMetrics::default()));
        let metrics_clone_for_progress = metrics.clone();

        // 启动阶段：首次写入信号（避免 UI “假录制中”）
        let (started_tx, mut started_rx) = oneshot::channel::<()>();
        let mut started_tx = Some(started_tx);

        // 卡住检测：当 ffmpeg 不再输出 progress、且文件/total_size 不再增长，认为断流卡死
        let (stall_tx, mut stall_rx) = oneshot::channel::<String>();
        let mut stall_tx = Some(stall_tx);
        let stall_timeout = Duration::from_secs(self.config.timeout.max(60).saturating_mul(2));

        let progress_task = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(1));
            let mut last_observed_size: u64 = 0;
            let mut last_write_at = Instant::now();
            loop {
                ticker.tick().await;

                // 获取文件大小
                let file_size = tokio::fs::metadata(&output_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);

                let (ff_total_size, ff_out_time_ms, ff_speed_bps, last_progress_at) = {
                    let m = metrics_clone_for_progress.lock().await;
                    (m.total_size, m.out_time_ms, m.speed_bps, m.last_progress_at)
                };

                let size = ff_total_size.max(file_size);

                if size > 0 {
                    started_writing_clone.store(true, Ordering::Relaxed);
                    if let Some(tx) = started_tx.take() {
                        let _ = tx.send(());
                    }
                }

                // 卡住检测：已开始写入但长时间无任何进度/写入增长
                if started_writing_clone.load(Ordering::Relaxed) {
                    if size != last_observed_size {
                        last_write_at = Instant::now();
                    }

                    let stalled_by_progress = match last_progress_at {
                        Some(t) => t.elapsed() > stall_timeout,
                        None => false,
                    };
                    let stalled_by_write = last_write_at.elapsed() > stall_timeout;

                    if (stalled_by_progress || stalled_by_write) && size == last_observed_size {
                        if let Some(tx) = stall_tx.take() {
                            let _ = tx.send(format!(
                                "录制卡住：超过 {} 秒未见进度/写入增长",
                                stall_timeout.as_secs()
                            ));
                        }
                    }
                }

                last_observed_size = size;

                let progress = RecordProgress {
                    status: if started_writing_clone.load(Ordering::Relaxed) {
                        RecordStatus::Recording
                    } else {
                        RecordStatus::Connecting
                    },
                    start_time: Some(chrono::Utc::now()),
                    duration: if ff_out_time_ms > 0 {
                        (ff_out_time_ms / 1_000).max(1)
                    } else {
                        start_time.elapsed().as_secs()
                    },
                    size,
                    speed: ff_speed_bps,
                    error: None,
                };
                if progress_tx.send(progress).is_err() {
                    break;
                }
            }
        });

        // 启动 stdout progress 消费任务（解析 -progress 输出）
        let metrics_clone_for_stdout = metrics.clone();
        let stdout_task = tokio::spawn(async move {
            let Some(stdout) = stdout.take() else {
                return;
            };
            let mut reader = tokio::io::BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let Some((k, v)) = line.split_once('=') else {
                    continue;
                };
                let mut m = metrics_clone_for_stdout.lock().await;
                m.last_progress_at = Some(Instant::now());
                match k {
                    "total_size" => {
                        if let Ok(n) = v.trim().parse::<u64>() {
                            m.total_size = n;
                            let now = Instant::now();
                            if let Some(last) = m.last_sample_at {
                                let dt = now.duration_since(last);
                                if dt.as_secs_f64() >= 0.5 {
                                    let ds = n.saturating_sub(m.last_sample_size);
                                    m.speed_bps = (ds as f64 / dt.as_secs_f64()) as u64;
                                    m.last_sample_at = Some(now);
                                    m.last_sample_size = n;
                                }
                            } else {
                                m.last_sample_at = Some(now);
                                m.last_sample_size = n;
                            }
                        }
                    }
                    // 兼容不同 ffmpeg 版本：有的输出 out_time_ms=ms，有的输出 out_time_us=us
                    "out_time_ms" => {
                        if let Ok(n) = v.trim().parse::<u64>() {
                            m.out_time_ms = n;
                        }
                    }
                    "out_time_us" => {
                        if let Ok(n) = v.trim().parse::<u64>() {
                            m.out_time_ms = n / 1_000;
                        }
                    }
                    _ => {}
                }
            }
        });

        // 启动 stderr 消费任务（只保留少量最近日志，用于失败诊断）
        let stderr_lines = Arc::new(tokio::sync::Mutex::new(
            std::collections::VecDeque::<String>::new(),
        ));
        let stderr_lines_clone = stderr_lines.clone();
        let stderr_task = tokio::spawn(async move {
            let Some(stderr) = stderr.take() else {
                return;
            };
            let mut reader = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if is_noise_ffmpeg_stderr_line(&line) {
                    continue;
                }
                // 保留最近 80 行即可（-loglevel warning 下通常很少）
                let mut buf = stderr_lines_clone.lock().await;
                if buf.len() >= 80 {
                    buf.pop_front();
                }
                buf.push_back(line);
            }
        });

        // 启动超时保护：允许 HLS 首段拉取存在抖动；使用可配置的 timeout（下限 60s）
        // 启动超时保护：并发拉流时可能需要更长启动窗口，避免误判“未写入”
        // - 最小 60s
        // - 随 config.timeout 线性放大
        let startup_timeout_secs = self.config.timeout.max(30).saturating_mul(4).max(60);
        let startup_timeout = Duration::from_secs(startup_timeout_secs);

        // Phase 1：等待“首次写入”或超时/停止/进程退出
        tokio::select! {
            _ = &mut self.stop_rx => {
                info!("🛑 收到停止信号，终止 FFmpeg 进程...");
                child.kill().await.ok();
                let _ = child.wait().await;
                progress_task.abort();
                stdout_task.abort();
                stderr_task.abort();
                info!("✅ FFmpeg 进程已终止");
                return Ok(());
            }
            result = child.wait() => {
                progress_task.abort();
                stdout_task.abort();
                stderr_task.abort();
                return match result {
                    Ok(status) => {
                        if status.success() || status.code() == Some(255) {
                            info!("✅ FFmpeg 录制完成");
                            Ok(())
                        } else {
                            let stderr_tail = {
                                let buf = stderr_lines.lock().await;
                                buf.iter().cloned().collect::<Vec<_>>().join("\n")
                            };
                            let msg = if stderr_tail.trim().is_empty() {
                                format!("FFmpeg录制失败，退出码: {:?}", status.code())
                            } else {
                                format!("FFmpeg录制失败，退出码: {:?}\nffmpeg stderr（最近输出）:\n{}", status.code(), stderr_tail)
                            };
                            Err(RecorderError::RecordingError(msg))
                        }
                    }
                    Err(e) => Err(RecorderError::RecordingError(format!("等待FFmpeg进程失败: {}", e))),
                };
            }
            _ = &mut started_rx => {
                // 已开始写入：进入 Phase 2
            }
            reason = &mut stall_rx => {
                // 启动阶段就卡住（通常是网络/权限/风控）
                let reason = reason.unwrap_or_else(|_| "录制卡住".to_string());
                warn!("{}，终止 FFmpeg 进程", reason);
                child.kill().await.ok();
                let _ = child.wait().await;
                progress_task.abort();
                stdout_task.abort();
                stderr_task.abort();

                let stderr_tail = {
                    let buf = stderr_lines.lock().await;
                    buf.iter().cloned().collect::<Vec<_>>().join("\n")
                };
                let msg = if stderr_tail.trim().is_empty() {
                    reason
                } else {
                    format!("{}\nffmpeg stderr（最近输出）:\n{}", reason, stderr_tail)
                };
                return Err(RecorderError::RecordingStalled {
                    reason: msg,
                    details: String::new(),
                });
            }
            _ = tokio::time::sleep(startup_timeout) => {
                if !started_writing.load(Ordering::Relaxed) {
                    warn!("录制启动超时：{} 秒内未写入任何数据，终止 FFmpeg 进程", startup_timeout.as_secs());
                    child.kill().await.ok();
                    let _ = child.wait().await;
                    progress_task.abort();
                    stdout_task.abort();
                    stderr_task.abort();

                    let stderr_tail = {
                        let buf = stderr_lines.lock().await;
                        buf.iter().cloned().collect::<Vec<_>>().join("\n")
                    };

                    let msg = if stderr_tail.trim().is_empty() {
                        "录制启动超时：未写入任何数据（可能是网络/权限/风控导致无法拉流）".to_string()
                    } else {
                        format!(
                            "录制启动超时：未写入任何数据\nffmpeg stderr（最近输出）:\n{}",
                            stderr_tail
                        )
                    };
                    return Err(RecorderError::RecordingStartupTimeout {
                        timeout_secs: startup_timeout.as_secs(),
                        details: format!("\n{}", msg),
                    });
                }
            }
        }

        // Phase 2：已开始写入后，继续等待停止/退出/卡住
        tokio::select! {
            _ = &mut self.stop_rx => {
                info!("🛑 收到停止信号，终止 FFmpeg 进程...");
                child.kill().await.ok();
                let _ = child.wait().await;
                progress_task.abort();
                stdout_task.abort();
                stderr_task.abort();
                info!("✅ FFmpeg 进程已终止");
                Ok(())
            }
            result = child.wait() => {
                progress_task.abort();
                stdout_task.abort();
                stderr_task.abort();
                match result {
                    Ok(status) => {
                        if status.success() || status.code() == Some(255) {
                            info!("✅ FFmpeg 录制完成");
                            Ok(())
                        } else {
                            let stderr_tail = {
                                let buf = stderr_lines.lock().await;
                                buf.iter().cloned().collect::<Vec<_>>().join("\n")
                            };
                            let msg = if stderr_tail.trim().is_empty() {
                                format!("FFmpeg录制失败，退出码: {:?}", status.code())
                            } else {
                                format!("FFmpeg录制失败，退出码: {:?}\nffmpeg stderr（最近输出）:\n{}", status.code(), stderr_tail)
                            };
                            Err(RecorderError::RecordingError(msg))
                        }
                    }
                    Err(e) => Err(RecorderError::RecordingError(format!("等待FFmpeg进程失败: {}", e))),
                }
            }
            reason = &mut stall_rx => {
                let reason = reason.unwrap_or_else(|_| "录制卡住".to_string());
                warn!("{}，终止 FFmpeg 进程", reason);
                child.kill().await.ok();
                let _ = child.wait().await;
                progress_task.abort();
                stdout_task.abort();
                stderr_task.abort();

                let stderr_tail = {
                    let buf = stderr_lines.lock().await;
                    buf.iter().cloned().collect::<Vec<_>>().join("\n")
                };
                let msg = if stderr_tail.trim().is_empty() {
                    reason
                } else {
                    format!("{}\nffmpeg stderr（最近输出）:\n{}", reason, stderr_tail)
                };
                Err(RecorderError::RecordingStalled {
                    reason: msg,
                    details: String::new(),
                })
            }
        }
    }

    async fn record_huya_ffmpeg_loop(&mut self) -> RecorderResult<()> {
        let output_ext = self
            .output_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("ts")
            .to_ascii_lowercase();
        if output_ext != "ts" {
            return Err(RecorderError::ConfigError(
                "虎牙续录模式当前仅支持 ts 输出（请将录制格式设置为 ts）".to_string(),
            ));
        }

        let mut backoff_secs = 1u64;

        loop {
            match self.refresh_huya_stream_urls_for_ffmpeg().await {
                Ok(false) => {
                    info!("✅ 虎牙直播已结束，停止录制");
                    return Ok(());
                }
                Ok(true) => {}
                Err(e) => warn!("⚠️ 刷新虎牙流失败，将继续使用当前 URL 重试: {}", e),
            }

            // 避免 Huya HLS 在 FFmpeg 下被 CDN 403：优先选择 FLV
            if self.stream_url.contains(".m3u8") {
                if let Some(flv) = self
                    .stream_urls
                    .iter()
                    .find(|u| !u.contains(".m3u8"))
                    .cloned()
                {
                    self.stream_url = flv;
                }
            }

            match self.record_huya_ffmpeg_pipe_once().await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    warn!("⚠️ 虎牙录制中断，准备重连: {}", e);
                    tokio::select! {
                        _ = &mut self.stop_rx => {
                            info!("🛑 收到停止信号，结束虎牙录制");
                            return Ok(());
                        }
                        _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                    }
                    backoff_secs = backoff_secs.saturating_mul(2).min(10);
                }
            }
        }
    }

    async fn refresh_huya_stream_urls_for_ffmpeg(&mut self) -> RecorderResult<bool> {
        let stream_info = self.platform_handler.get_stream_info(&self.room_id).await?;
        if stream_info.room.status != crate::types::LiveStatus::Live {
            return Ok(false);
        }

        let target_level = self.config.quality.level();
        let selected = stream_info
            .streams
            .iter()
            .find(|s| s.quality.level() == target_level)
            .or_else(|| {
                stream_info
                    .streams
                    .iter()
                    .min_by_key(|s| (s.quality.level() as i8 - target_level as i8).abs())
            })
            .ok_or_else(|| RecorderError::StreamNotAvailable("没有可用的流".to_string()))?;

        let mut urls = Vec::new();
        if let Some(flv) = selected.url.flv_url.clone() {
            urls.push(flv);
        }
        if let Some(hls) = selected.url.hls_url.clone() {
            urls.push(hls);
        }

        if urls.is_empty() {
            return Err(RecorderError::StreamNotAvailable(
                "没有可用的流URL".to_string(),
            ));
        }

        // Huya 优先 FLV，其次 HLS
        urls.sort_by_key(|u| u.contains(".m3u8"));
        self.stream_urls = urls;
        self.stream_url = self.stream_urls[0].clone();
        Ok(true)
    }

    async fn record_huya_ffmpeg_pipe_once(&mut self) -> RecorderResult<()> {
        if self.stream_url.contains(".m3u8") {
            return Err(RecorderError::RecordingError(
                "虎牙录制当前仅支持 FLV（HLS 在 FFmpeg 下可能 403）".to_string(),
            ));
        }

        if let Err(_) = create_tokio_command("ffmpeg")
            .arg("-version")
            .output()
            .await
        {
            return Err(RecorderError::FFmpegNotFound);
        }

        info!(
            "🎥 使用 FFmpeg 续录（Huya/pipe）: {} -> {:?}",
            self.stream_url, self.output_path
        );

        let mut cmd = create_tokio_command("ffmpeg");
        cmd.arg("-hide_banner");
        cmd.arg("-loglevel").arg("error");
        cmd.arg("-nostats");
        cmd.arg("-progress").arg("pipe:2");
        cmd.arg("-fflags").arg("+discardcorrupt");

        cmd.arg("-reconnect").arg("1");
        cmd.arg("-reconnect_at_eof").arg("1");
        cmd.arg("-reconnect_streamed").arg("1");
        cmd.arg("-reconnect_on_network_error").arg("1");
        cmd.arg("-reconnect_delay_max")
            .arg(self.config.timeout.max(5).min(60).to_string());

        let rw_timeout_us = self.config.timeout.saturating_mul(1_000_000).max(5_000_000);
        cmd.arg("-rw_timeout").arg(rw_timeout_us.to_string());

        let mut headers = vec![
            "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
        ];
        let (referer, origin) =
            LiveRecorder::default_referer_and_origin(&self.platform_name, &self.room_url);
        if let Some(referer) = referer {
            headers.push(format!("Referer: {}", referer));
        }
        if let Some(origin) = origin {
            headers.push(format!("Origin: {}", origin));
        }
        for (key, value) in &self.config.headers {
            headers.push(format!("{}: {}", key, value));
        }
        if !headers.is_empty() {
            let headers_str = headers
                .iter()
                .map(|h| format!("{}\r\n", h))
                .collect::<String>();
            cmd.arg("-headers").arg(headers_str);
        }

        if let Some(proxy) = &self.config.proxy {
            cmd.arg("-http_proxy").arg(proxy);
        }

        cmd.arg("-i").arg(&self.stream_url);
        cmd.arg("-c").arg("copy");
        cmd.arg("-f").arg("mpegts");
        cmd.arg("pipe:1");

        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn()?;
        let Some(mut stdout) = child.stdout.take() else {
            return Err(RecorderError::RecordingError(
                "无法获取 FFmpeg stdout".to_string(),
            ));
        };
        let Some(stderr) = child.stderr.take() else {
            return Err(RecorderError::RecordingError(
                "无法获取 FFmpeg stderr".to_string(),
            ));
        };

        let output_path = self.output_path.clone();
        let output_path_for_progress = self.output_path.clone();
        let progress_tx = self.progress_tx.clone();
        let start_time = self.start_time;

        let started_writing = Arc::new(AtomicBool::new(false));
        let started_writing_clone = started_writing.clone();

        let last_write_at = Arc::new(tokio::sync::Mutex::new(Instant::now()));
        let last_write_at_clone = last_write_at.clone();
        let stall_timeout = Duration::from_secs(self.config.timeout.max(60).saturating_mul(2));

        let (started_tx, mut started_rx) = oneshot::channel::<()>();
        let mut started_tx = Some(started_tx);

        let (stall_tx, mut stall_rx) = oneshot::channel::<String>();
        let mut stall_tx = Some(stall_tx);

        let mut writer_task = tokio::spawn(async move {
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&output_path)
                .await?;

            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = stdout.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                file.write_all(&buf[..n]).await?;
                *last_write_at_clone.lock().await = Instant::now();

                if !started_writing_clone.swap(true, Ordering::Relaxed) {
                    if let Some(tx) = started_tx.take() {
                        let _ = tx.send(());
                    }
                }
            }

            file.flush().await.ok();
            Ok::<(), std::io::Error>(())
        });

        let stderr_lines = Arc::new(tokio::sync::Mutex::new(VecDeque::<String>::with_capacity(
            200,
        )));
        let stderr_lines_clone = stderr_lines.clone();
        let stderr_task = tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if is_noise_ffmpeg_stderr_line(&line) {
                    continue;
                }
                let mut buf = stderr_lines_clone.lock().await;
                if buf.len() >= 200 {
                    buf.pop_front();
                }
                buf.push_back(line);
            }
        });

        let started_writing_for_progress = started_writing.clone();
        let last_write_at_for_progress = last_write_at.clone();
        let progress_task = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(1));
            loop {
                ticker.tick().await;
                let size = tokio::fs::metadata(&output_path_for_progress)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);

                let started = started_writing_for_progress.load(Ordering::Relaxed) || size > 0;
                let progress = RecordProgress {
                    status: if started {
                        RecordStatus::Recording
                    } else {
                        RecordStatus::Connecting
                    },
                    start_time: Some(chrono::Utc::now()),
                    duration: start_time.elapsed().as_secs().max(1),
                    size,
                    speed: 0,
                    error: None,
                };

                if progress_tx.send(progress).is_err() {
                    break;
                }

                if started {
                    let last = *last_write_at_for_progress.lock().await;
                    if last.elapsed() > stall_timeout {
                        if let Some(tx) = stall_tx.take() {
                            let _ = tx.send(format!(
                                "虎牙录制卡住：超过 {} 秒无写入",
                                stall_timeout.as_secs()
                            ));
                        }
                    }
                }
            }
        });

        let startup_timeout = Duration::from_secs(self.config.timeout.max(20));

        tokio::select! {
            _ = &mut self.stop_rx => {
                info!("🛑 收到停止信号，终止 FFmpeg 进程...");
                child.kill().await.ok();
                let _ = child.wait().await;
                progress_task.abort();
                stderr_task.abort();
                writer_task.abort();
                info!("✅ FFmpeg 进程已终止");
                return Ok(());
            }
            _ = &mut started_rx => {}
            reason = &mut stall_rx => {
                let reason = reason.unwrap_or_else(|_| "虎牙录制卡住".to_string());
                warn!("{}，终止 FFmpeg 进程", reason);
                child.kill().await.ok();
                let _ = child.wait().await;
                progress_task.abort();
                stderr_task.abort();
                writer_task.abort();

                let stderr_tail = {
                    let buf = stderr_lines.lock().await;
                    buf.iter().cloned().collect::<Vec<_>>().join("\n")
                };
                let msg = if stderr_tail.trim().is_empty() {
                    reason
                } else {
                    format!("{}\nffmpeg stderr（最近输出）:\n{}", reason, stderr_tail)
                };
                return Err(RecorderError::RecordingStalled { reason: msg, details: String::new() });
            }
            _ = tokio::time::sleep(startup_timeout) => {
                if !started_writing.load(Ordering::Relaxed) {
                    warn!("录制启动超时（{}s 内未写入任何数据），终止 FFmpeg 进程", startup_timeout.as_secs());
                    child.kill().await.ok();
                    let _ = child.wait().await;
                    progress_task.abort();
                    stderr_task.abort();
                    writer_task.abort();
                    return Err(RecorderError::RecordingStartupTimeout { timeout_secs: startup_timeout.as_secs(), details: String::new() });
                }
            }
            write_res = &mut writer_task => {
                child.kill().await.ok();
                let _ = child.wait().await;
                progress_task.abort();
                stderr_task.abort();
                return Err(RecorderError::RecordingError(format!("写入输出文件失败: {:?}", write_res)));
            }
            exit_res = child.wait() => {
                let status = exit_res?;
                progress_task.abort();
                stderr_task.abort();
                writer_task.abort();
                let stderr_tail = {
                    let buf = stderr_lines.lock().await;
                    buf.iter().cloned().collect::<Vec<_>>().join("\n")
                };
                return Err(RecorderError::RecordingStalled {
                    reason: format!("FFmpeg 进程退出（可能 EOF/断流），退出码: {:?}", status.code()),
                    details: if stderr_tail.trim().is_empty() { String::new() } else { format!("\n{}", stderr_tail) },
                });
            }
        }

        tokio::select! {
            _ = &mut self.stop_rx => {
                info!("🛑 收到停止信号，终止 FFmpeg 进程...");
                child.kill().await.ok();
                let _ = child.wait().await;
                progress_task.abort();
                stderr_task.abort();
                writer_task.abort();
                info!("✅ FFmpeg 进程已终止");
                Ok(())
            }
            reason = &mut stall_rx => {
                let reason = reason.unwrap_or_else(|_| "虎牙录制卡住".to_string());
                warn!("{}，终止 FFmpeg 进程", reason);
                child.kill().await.ok();
                let _ = child.wait().await;
                progress_task.abort();
                stderr_task.abort();
                writer_task.abort();

                let stderr_tail = {
                    let buf = stderr_lines.lock().await;
                    buf.iter().cloned().collect::<Vec<_>>().join("\n")
                };
                let msg = if stderr_tail.trim().is_empty() {
                    reason
                } else {
                    format!("{}\nffmpeg stderr（最近输出）:\n{}", reason, stderr_tail)
                };
                Err(RecorderError::RecordingStalled { reason: msg, details: String::new() })
            }
            write_res = &mut writer_task => {
                child.kill().await.ok();
                let _ = child.wait().await;
                progress_task.abort();
                stderr_task.abort();
                Err(RecorderError::RecordingError(format!("写入输出文件失败: {:?}", write_res)))
            }
            exit_res = child.wait() => {
                let status = exit_res?;
                progress_task.abort();
                stderr_task.abort();
                writer_task.abort();
                let stderr_tail = {
                    let buf = stderr_lines.lock().await;
                    buf.iter().cloned().collect::<Vec<_>>().join("\n")
                };
                Err(RecorderError::RecordingStalled {
                    reason: format!("FFmpeg 进程退出（可能 EOF/断流），退出码: {:?}", status.code()),
                    details: if stderr_tail.trim().is_empty() { String::new() } else { format!("\n{}", stderr_tail) },
                })
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
