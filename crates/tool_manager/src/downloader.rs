use crate::error::{DownloadError, DownloadResult};
use magekit_extractor::MediaExtractor;
use magekit_shared::{
    create_tokio_command, ChannelInfo, DownloadOptions, PlatformCookie, TaskId, VideoInfo,
};
use std::path::PathBuf;
use std::time::Instant;
use std::process::Stdio;
use tokio::process::Child;
use tokio::sync::mpsc;
use tokio::time::Duration;

/// 视频下载器
/// 
/// 负责视频下载功能，解析逻辑统一交给 extractor 处理
#[derive(Clone)]
pub struct VideoDownloader {
    yt_dlp_path: PathBuf,
    ffmpeg_path: Option<PathBuf>,
}

/// 从 URL 中提取平台名称
fn extract_platform_from_url(url: &str) -> Option<String> {
    let url_lower = url.to_lowercase();
    if url_lower.contains("bilibili.com") || url_lower.contains("b23.tv") {
        Some("bilibili".to_string())
    } else if url_lower.contains("youtube.com") || url_lower.contains("youtu.be") {
        Some("youtube".to_string())
    } else if url_lower.contains("twitter.com") || url_lower.contains("x.com") {
        Some("twitter".to_string())
    } else if url_lower.contains("instagram.com") {
        Some("instagram".to_string())
    } else if url_lower.contains("tiktok.com") {
        Some("tiktok".to_string())
    } else if url_lower.contains("douyin.com") {
        Some("douyin".to_string())
    } else if url_lower.contains("weibo.com") {
        Some("weibo".to_string())
    } else if url_lower.contains("xiaohongshu.com") || url_lower.contains("xhs.link") {
        Some("xiaohongshu".to_string())
    } else {
        None
    }
}

/// 获取指定平台的 Cookie 字符串
fn get_cookie_string(platform: &str, cookies: &[PlatformCookie]) -> Option<String> {
    cookies
        .iter()
        .find(|c| c.enabled && c.platform.to_lowercase() == platform.to_lowercase())
        .map(|c| c.cookie.clone())
}

impl VideoDownloader {
    /// 创建新的视频下载器
    pub fn new(yt_dlp_path: PathBuf, ffmpeg_path: Option<PathBuf>) -> Self {
        Self {
            yt_dlp_path,
            ffmpeg_path,
        }
    }

    /// 获取视频信息
    ///
    /// 统一调用 extractor 进行解析，支持所有平台
    /// 
    /// # 参数
    /// - `url`: 视频 URL
    /// - `cookies`: 可选的平台 Cookie 列表，用于访问需要登录的内容
    pub async fn get_video_info(
        &self,
        url: &str,
        cookies: Option<&[PlatformCookie]>,
    ) -> DownloadResult<VideoInfo> {
        tracing::info!("🔍 获取视频信息，URL: {}", url);
        
        let extractor = MediaExtractor::new(self.yt_dlp_path.clone());
        extractor
            .get_video_info(url, cookies)
            .await
            .map_err(|e| DownloadError::extraction_failed(url, e.to_string()))
    }

    /// 获取频道/播放列表的所有视频信息
    ///
    /// 统一调用 extractor 进行解析，支持所有平台
    /// 支持的 URL 类型：
    /// - YouTube 频道: @username, /user/, /channel/, /c/
    /// - YouTube 播放列表: /playlist?list=
    /// - Bilibili UP主空间: space.bilibili.com/uid
    /// - 抖音用户主页
    /// - TikTok 用户主页
    ///
    /// # 参数
    /// - `url`: 频道或播放列表的 URL
    /// - `cookies`: 可选的平台 Cookie 列表，用于访问需要登录的内容
    pub async fn get_channel_videos(
        &self,
        url: &str,
        cookies: Option<&[PlatformCookie]>,
    ) -> DownloadResult<ChannelInfo> {
        tracing::info!("📺 获取频道/播放列表信息，URL: {}", url);
        
        let extractor = MediaExtractor::new(self.yt_dlp_path.clone());
        extractor
            .get_channel_info(url, cookies)
            .await
            .map_err(|e| DownloadError::extraction_failed(url, e.to_string()))
    }

    /// 开始下载视频
    ///
    /// 支持两种下载模式：
    /// 1. 直链下载：当 options.download_url 有值时，直接 HTTP 下载（用于抖音/TikTok）
    /// 2. yt-dlp 下载：其他平台使用 yt-dlp 处理
    ///
    /// # 参数
    /// - `task_id`: 任务 ID
    /// - `url`: 视频 URL
    /// - `options`: 下载选项
    /// - `progress_tx`: 进度通知发送器
    /// - `cookies`: 可选的平台 Cookie 列表
    pub async fn start_download(
        &self,
        task_id: TaskId,
        url: &str,
        options: DownloadOptions,
        progress_tx: mpsc::Sender<DownloadProgress>,
        cookies: Option<&[PlatformCookie]>,
    ) -> DownloadResult<PathBuf> {
        tracing::info!("📥 开始下载任务 {}", task_id);
        tracing::info!("  URL: {}", url);
        tracing::info!("  输出目录: {:?}", options.output_path);
        tracing::info!("  格式: {}", options.format_id);

        // 发送开始事件
        let _ = progress_tx
            .send(DownloadProgress::Started {
                task_id,
                url: url.to_string(),
            })
            .await;

        // ffmpeg 拉流优先
        if let Some(ffmpeg_url) = &options.ffmpeg_url {
            tracing::info!("🚀 使用 ffmpeg m3u8 下载模式");
            return self
                .download_with_ffmpeg(task_id, ffmpeg_url, &options, &progress_tx)
                .await;
        }

        // 如果有直链，使用 HTTP 直接下载
        if let Some(download_url) = &options.download_url {
            tracing::info!("🚀 使用直链下载模式");
            return self
                .download_direct(task_id, download_url, &options, &progress_tx, cookies)
                .await;
        }

        // 否则使用 yt-dlp 下载
        self.download_with_ytdlp(task_id, url, &options, &progress_tx, cookies)
            .await
    }

    /// 使用直链 HTTP 下载
    async fn download_direct(
        &self,
        task_id: TaskId,
        download_url: &str,
        options: &DownloadOptions,
        progress_tx: &mpsc::Sender<DownloadProgress>,
        cookies: Option<&[PlatformCookie]>,
    ) -> DownloadResult<PathBuf> {
        use tokio::io::AsyncWriteExt;

        // 构建输出文件路径
        let filename = options
            .output_template
            .as_deref()
            .unwrap_or("video.mp4")
            .replace("%(title)s", "video")
            .replace("%(ext)s", "mp4");
        let output_path = options.output_path.join(&filename);

        tracing::info!("  直链: {}", download_url);
        tracing::info!("  输出文件: {:?}", output_path);

        // 构建 HTTP 客户端
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"
                .parse()
                .unwrap(),
        );
        headers.insert(
            reqwest::header::REFERER,
            "https://www.douyin.com/".parse().unwrap(),
        );

        // 添加 Cookie
        if let Some(cookies) = cookies {
            let platform = extract_platform_from_url(download_url);
            if let Some(platform) = platform {
                if let Some(cookie_str) = get_cookie_string(&platform, cookies) {
                    if let Ok(cookie_val) = cookie_str.parse() {
                        headers.insert(reqwest::header::COOKIE, cookie_val);
                        tracing::debug!("🍪 已添加 Cookie");
                    }
                }
            }
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| DownloadError::internal(format!("Failed to create HTTP client: {}", e)))?;

        // 发起请求
        let response = client
            .get(download_url)
            .send()
            .await
            .map_err(|e| DownloadError::download_failed(download_url, e.to_string()))?;

        if !response.status().is_success() {
            return Err(DownloadError::download_failed(
                download_url,
                format!("HTTP {}", response.status()),
            ));
        }

        let total_size = response.content_length();
        tracing::info!("  文件大小: {:?}", total_size);

        // 创建输出文件
        let mut file = tokio::fs::File::create(&output_path)
            .await
            .map_err(|e| DownloadError::internal(format!("Failed to create file: {}", e)))?;

        // 下载并写入文件
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk
                .map_err(|e| DownloadError::download_failed(download_url, e.to_string()))?;

            file.write_all(&chunk)
                .await
                .map_err(|e| DownloadError::internal(format!("Failed to write file: {}", e)))?;

            downloaded += chunk.len() as u64;

            // 发送进度
            if let Some(total) = total_size {
                let percent = (downloaded as f32 / total as f32) * 100.0;
                let _ = progress_tx
                    .send(DownloadProgress::Progress {
                        task_id,
                        percent,
                        speed: None,
                        eta: None,
                        downloaded_bytes: Some(downloaded as u64),
                        total_bytes: Some(total as u64),
                    })
                    .await;
            }
        }

        file.flush()
            .await
            .map_err(|e| DownloadError::internal(format!("Failed to flush file: {}", e)))?;

        tracing::info!("✅ 直链下载完成: {:?}", output_path);
        
        // 发送完成事件
        let _ = progress_tx
            .send(DownloadProgress::Completed {
                task_id,
                output_path: output_path.clone(),
            })
            .await;
        
        Ok(output_path)
    }

    /// 使用 yt-dlp 下载
    async fn download_with_ytdlp(
        &self,
        task_id: TaskId,
        url: &str,
        options: &DownloadOptions,
        progress_tx: &mpsc::Sender<DownloadProgress>,
        cookies: Option<&[PlatformCookie]>,
    ) -> DownloadResult<PathBuf> {
        let mut cmd = create_tokio_command(&self.yt_dlp_path);

        // 基本参数
        let output_template = options.output_path.join(
            options
                .output_template
                .as_deref()
                .unwrap_or("%(title)s.%(ext)s"),
        );

        // Cookie 通过 --add-header 传递，不再创建临时文件
        if let Some(cookies) = cookies {
            if let Some(platform) = extract_platform_from_url(url) {
                if let Some(cookie_str) = get_cookie_string(&platform, cookies) {
                    cmd.arg("--add-header")
                        .arg(format!("Cookie:{}", cookie_str));
                    tracing::debug!("🍪 使用 Cookie Header");
                }
            }
        }

        cmd.arg(url)
            .arg("--format")
            .arg(&options.format_id)
            .arg("--output")
            .arg(output_template.to_string_lossy().as_ref())
            .arg("--newline")
            .arg("--progress")
            .arg("--console-title")
            .arg("--print")
            .arg("after_move:filepath");

        tracing::debug!("  输出模板: {:?}", output_template);

        // 元数据选项
        if options.embed_metadata {
            cmd.arg("--embed-metadata");
        }
        if options.embed_thumbnail {
            cmd.arg("--embed-thumbnail");
        }

        // 音频提取选项
        if options.extract_audio {
            cmd.arg("--extract-audio");
            if let Some(audio_format) = &options.audio_format {
                cmd.arg("--audio-format").arg(audio_format);
            }
        }

        // 字幕选项
        for lang in &options.subtitle_langs {
            cmd.arg("--sub-langs").arg(lang);
        }
        if options.embed_subs {
            cmd.arg("--embed-subs");
        }
        if options.write_subs {
            cmd.arg("--write-subs");
        }
        if options.write_auto_subs {
            cmd.arg("--write-auto-subs");
        }

        // 使用 FFmpeg 进行后处理
        if let Some(ffmpeg_path) = &self.ffmpeg_path {
            cmd.env("FFMPEG", ffmpeg_path);
        }

        // 创建子进程
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| DownloadError::internal(format!("yt-dlp download failed: {}", e)))?;

        // 监控下载进度
        let output_path = self
            .monitor_download_progress(task_id, &mut child, progress_tx)
            .await?;

        tracing::info!("✅ yt-dlp 下载完成: {:?}", output_path);
        Ok(output_path)
    }

    /// 使用 ffmpeg 下载 m3u8 直链
    async fn download_with_ffmpeg(
        &self,
        task_id: TaskId,
        ffmpeg_url: &str,
        options: &DownloadOptions,
        progress_tx: &mpsc::Sender<DownloadProgress>,
    ) -> DownloadResult<PathBuf> {
        let ffmpeg_path = self
            .ffmpeg_path
            .clone()
            .ok_or_else(|| DownloadError::internal("ffmpeg 未配置"))?;

        // 输出文件名：用任务 id 保证唯一，后缀 mp4
        let filename = format!("{}.mp4", task_id);
        let output_path = options.output_path.join(&filename);

        tracing::info!("  ffmpeg url: {}", ffmpeg_url);
        tracing::info!("  输出文件: {:?}", output_path);

        // 启动 ffmpeg 子进程，打开 stdout 以便解析进度
        let mut child = {
            let mut cmd = create_tokio_command(ffmpeg_path);
            cmd.arg("-y")
                .arg("-i")
                .arg(ffmpeg_url)
                .arg("-c")
                .arg("copy")
                .arg("-progress")
                .arg("pipe:1")
                .arg("-nostats")
                .arg(&output_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            cmd.spawn()
                .map_err(|e| DownloadError::internal(format!("ffmpeg 启动失败: {}", e)))?
        };

        // 解析 ffmpeg progress 输出（pipe:1）
        if let Some(stdout) = child.stdout.take() {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut reader = BufReader::new(stdout);
            let mut buf = String::new();
            let mut last_percent = 0.0;
            let mut last_size: u64 = 0;
            let mut last_ts = Instant::now();
            let mut downloaded: Option<u64> = None;
            let mut speed_bps: Option<u64> = None;
            while reader.read_line(&mut buf).await.unwrap_or(0) > 0 {
                let line = buf.trim();
                // ffmpeg -progress 输出键值对，如：
                // frame=..., out_time_ms=1230000, speed=2.0x, progress=continue/end, total_size=12345
                let mut send_progress = false;
                let mut percent = None;

                if let Some(ms_str) = line.strip_prefix("out_time_ms=") {
                    if let Ok(ms) = ms_str.parse::<f64>() {
                        // 粗略用输出时长近似百分比，不超 99%
                        let p = ((ms / 1_000_000.0) / 600.0).min(0.99) * 100.0;
                        if (p - last_percent).abs() >= 1.0 {
                            last_percent = p;
                            percent = Some(p as f32);
                            send_progress = true;
                        }
                    }
                } else if let Some(size_str) = line.strip_prefix("total_size=") {
                    if let Ok(size) = size_str.parse::<u64>() {
                        downloaded = Some(size);
                        let now = Instant::now();
                        let elapsed = now.duration_since(last_ts).as_secs_f64().max(0.001);
                        let delta = size.saturating_sub(last_size) as f64;
                        speed_bps = Some((delta / elapsed) as u64);
                        last_size = size;
                        last_ts = now;
                        send_progress = true;
                    }
                } else if let Some(br_str) = line.strip_prefix("bitrate=") {
                    // 示例: bitrate=2037.4kbits/s
                    let clean = br_str.trim_end_matches("bits/s").trim();
                    let (num, unit) = if clean.ends_with('k') {
                        (clean.trim_end_matches('k'), 1_000f64)
                    } else if clean.ends_with('M') {
                        (clean.trim_end_matches('M'), 1_000_000f64)
                    } else {
                        (clean, 1f64)
                    };
                    if let Ok(val) = num.parse::<f64>() {
                        speed_bps = Some((val * unit / 8.0) as u64); // 字节每秒
                        send_progress = true;
                    }
                } else if line == "progress=end" {
                    percent = Some(99.0);
                    send_progress = true;
                }

                if send_progress {
                    let total_est = if let (Some(d), Some(p)) = (downloaded, percent) {
                        if p > 0.1 {
                            Some((d as f64 / (p as f64 / 100.0)) as u64)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let _ = progress_tx
                        .send(DownloadProgress::Progress {
                            task_id,
                            percent: percent.unwrap_or(last_percent as f32),
                            speed: speed_bps,
                            eta: None,
                            downloaded_bytes: downloaded,
                            total_bytes: total_est,
                        })
                        .await;
                }

                buf.clear();
            }
        }

        let status = child
            .wait()
            .await
            .map_err(|e| DownloadError::internal(format!("ffmpeg 等待退出失败: {}", e)))?;

        if !status.success() {
            return Err(DownloadError::internal(format!(
                "ffmpeg 退出码非 0: {:?}",
                status.code()
            )));
        }

        let _ = progress_tx
            .send(DownloadProgress::Completed {
                task_id,
                output_path: output_path.clone(),
            })
            .await;

        Ok(output_path)
    }

    /// 监控下载进度
    async fn monitor_download_progress(
        &self,
        task_id: TaskId,
        child: &mut Child,
        progress_tx: &mpsc::Sender<DownloadProgress>,
    ) -> DownloadResult<PathBuf> {
        use tokio::io::{AsyncBufReadExt, BufReader};

        // 同时读取 stdout 和 stderr
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| DownloadError::internal("Failed to capture stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| DownloadError::internal("Failed to capture stderr".to_string()))?;

        let mut stdout_reader = BufReader::new(stdout);
        let mut stderr_reader = BufReader::new(stderr);

        // 使用字节缓冲区读取，避免 UTF-8 解码错误
        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();

        let mut output_path: Option<PathBuf> = None;
        let mut final_path_from_print: Option<PathBuf> = None;
        let mut stdout_eof = false;
        let mut stderr_eof = false;

        // 使用 select 同时读取两个流
        loop {
            if stdout_eof && stderr_eof {
                break;
            }

            tokio::select! {
                result = stdout_reader.read_until(b'\n', &mut stdout_buf), if !stdout_eof => {
                    match result {
                        Ok(0) => {
                            stdout_eof = true;
                        }
                        Ok(_) => {
                            // 尝试 UTF-8 解码，失败则尝试 lossy 转换
                            let line = Self::decode_line(&stdout_buf);
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                tracing::debug!("[yt-dlp stdout] {}", trimmed);

                                // 解析下载进度信息（yt-dlp 进度输出在 stdout）
                                if let Some(progress) = self.parse_progress_line(&line) {
                                    // tracing::info!("📊 下载进度: {:.1}%", progress.percent);
                                    let _ = progress_tx.send(DownloadProgress::Progress {
                                        task_id,
                                        percent: progress.percent,
                                        speed: progress.speed,
                                        eta: progress.eta,
                                        downloaded_bytes: progress.downloaded_bytes,
                                        total_bytes: progress.total_bytes,
                                    }).await;
                                }

                                // 解析输出文件路径 - 支持多种格式
                                if let Some(path) = self.extract_output_path(&line) {
                                    tracing::info!("📁 从 stdout 检测到输出文件: {:?}", path);
                                    output_path = Some(path);
                                }

                                // --print after_move:filepath 的输出会在 stdout
                                // 这通常是最终的文件路径
                                if !trimmed.starts_with('[') && !trimmed.contains('%') {
                                    let potential_path = PathBuf::from(trimmed);
                                    // 检查是否像文件路径 (Windows 路径可能包含 \ 或 :)
                                    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains('.') {
                                        tracing::info!("📁 从 stdout 检测到文件路径: {:?}", potential_path);
                                        final_path_from_print = Some(potential_path);
                                    }
                                }
                            }
                            stdout_buf.clear();
                        }
                        Err(e) => {
                            tracing::warn!("Error reading stdout: {}", e);
                            stdout_eof = true;
                        }
                    }
                }
                result = stderr_reader.read_until(b'\n', &mut stderr_buf), if !stderr_eof => {
                    match result {
                        Ok(0) => {
                            stderr_eof = true;
                        }
                        Ok(_) => {
                            // 尝试 UTF-8 解码，失败则尝试 lossy 转换
                            let line = Self::decode_line(&stderr_buf);
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                // 输出所有 yt-dlp 的 stderr 以便调试
                                tracing::info!("[yt-dlp stderr] {}", trimmed);
                            }

                            // 解析下载进度信息
                            if let Some(progress) = self.parse_progress_line(&line) {
                                // tracing::info!("📊 下载进度: {:.1}%", progress.percent);
                                let _ = progress_tx.send(DownloadProgress::Progress {
                                    task_id,
                                    percent: progress.percent,
                                    speed: progress.speed,
                                    eta: progress.eta,
                                    downloaded_bytes: progress.downloaded_bytes,
                                    total_bytes: progress.total_bytes,
                                }).await;
                            }

                            // 解析输出文件路径 - 支持多种格式
                            if let Some(path) = self.extract_output_path(&line) {
                                tracing::info!("📁 从 stderr 检测到输出文件: {:?}", path);
                                output_path = Some(path);
                            }

                            stderr_buf.clear();
                        }
                        Err(e) => {
                            tracing::warn!("Error reading stderr: {}", e);
                            stderr_eof = true;
                        }
                    }
                }
            }
        }

        // 等待进程完成
        let status = child
            .wait()
            .await
            .map_err(|e| DownloadError::internal(format!("wait for yt-dlp failed: {}", e)))?;

        if !status.success() {
            tracing::error!("❌ yt-dlp 进程退出码: {:?}", status.code());
            return Err(DownloadError::download_failed(
                "unknown",
                format!("Process exited with code: {:?}", status.code()),
            ));
        }

        // 优先使用 --print 输出的路径，否则使用从 stderr 检测到的路径
        let final_path = final_path_from_print.or(output_path);

        match &final_path {
            Some(path) => {
                tracing::info!("✅ 下载完成，输出文件: {:?}", path);
                
                // 发送完成事件
                let _ = progress_tx
                    .send(DownloadProgress::Completed {
                        task_id,
                        output_path: path.clone(),
                    })
                    .await;
            }
            None => tracing::warn!("⚠️ 下载完成但未检测到输出文件路径"),
        }

        final_path.ok_or_else(|| DownloadError::internal("无法确定输出文件路径".to_string()))
    }

    /// 解码字节行为字符串
    /// 优先尝试 UTF-8，失败则使用 lossy 转换
    #[cfg(target_os = "windows")]
    fn decode_line(bytes: &[u8]) -> String {
        // Windows 上 yt-dlp 可能输出 GBK 编码
        // 先尝试 UTF-8
        if let Ok(s) = std::str::from_utf8(bytes) {
            return s.to_string();
        }
        // 尝试 GBK 解码 (使用 encoding_rs)
        let (decoded, _, _) = encoding_rs::GBK.decode(bytes);
        decoded.into_owned()
    }

    #[cfg(not(target_os = "windows"))]
    fn decode_line(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    /// 解析进度行
    fn parse_progress_line(&self, line: &str) -> Option<ProgressInfo> {
        // yt-dlp 进度格式示例:
        // [download]   5.2% of 125.45MiB at 512.34KiB/s ETA 02:15
        // [download]  45.2% of ~12.34MiB at 1.23MiB/s ETA 00:05
        // [download] 100% of 125.45MiB in 00:02:15
        // [download]  39.7% of    3.18GiB at  Unknown B/s ETA Unknown
        if !line.contains("[download]") {
            return None;
        }

        // 查找百分比
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            return None;
        }

        // 找到包含 % 的部分
        let percent_part = parts.iter().find(|p| p.ends_with('%'))?;
        let percent_str = percent_part.trim_end_matches('%');
        let percent = percent_str.parse::<f64>().ok()? as f32;

        // 尝试解析总大小 (格式: "of 125.45MiB" 或 "of ~125.45MiB" 或 "of    3.18GiB")
        let total_bytes = parts.iter()
            .position(|p| *p == "of")
            .and_then(|i| parts.get(i + 1))
            .and_then(|s| self.parse_size(s));

        // 计算已下载大小
        let downloaded_bytes = total_bytes.map(|total| {
            ((percent as f64 / 100.0) * total as f64) as u64
        });

        // 尝试解析速度
        let speed = parts.iter()
            .find(|p| p.ends_with("/s"))
            .and_then(|s| self.parse_speed(s));

        // 尝试解析 ETA
        let eta = parts.iter()
            .position(|p| *p == "ETA")
            .and_then(|i| parts.get(i + 1))
            .and_then(|s| self.parse_eta(s));

        Some(ProgressInfo {
            percent,
            speed,
            eta,
            downloaded_bytes,
            total_bytes,
        })
    }

    /// 解析大小 (如 125.45MiB, ~12.34MiB, 3.18GiB)
    fn parse_size(&self, size_str: &str) -> Option<u64> {
        // 移除前导 ~ 符号
        let size_str = size_str.trim_start_matches('~');
        
        // 找到单位的起始位置（第一个字母）
        let unit_start = size_str.find(|c: char| c.is_alphabetic())?;
        let (num_str, unit) = size_str.split_at(unit_start);
        
        let num: f64 = num_str.parse().ok()?;
        let multiplier = match unit {
            "B" => 1.0,
            "KB" | "KiB" => 1024.0,
            "MB" | "MiB" => 1024.0 * 1024.0,
            "GB" | "GiB" => 1024.0 * 1024.0 * 1024.0,
            "TB" | "TiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
            _ => return None,
        };

        Some((num * multiplier) as u64)
    }

    /// 解析速度 (如 512.34KiB/s，4.57MiB/s)
    fn parse_speed(&self, speed_str: &str) -> Option<u64> {
        // 去掉 /s
        let speed_str = speed_str.trim_end_matches("/s");

        // 找到单位的起始位置（第一个字母）
        let unit_start = speed_str.find(|c: char| c.is_alphabetic())?;
        let (num_str, unit) = speed_str.split_at(unit_start);

        let num: f64 = num_str.parse().ok()?;
        let multiplier = match unit {
            "B" => 1.0,
            "KB" | "KiB" => 1024.0,
            "MB" | "MiB" => 1024.0 * 1024.0,
            "GB" | "GiB" => 1024.0 * 1024.0 * 1024.0,
            "TB" | "TiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
            _ => return None,
        };

        Some((num * multiplier) as u64)
    }

    /// 解析ETA (如 02:15)
    fn parse_eta(&self, eta_str: &str) -> Option<Duration> {
        let parts: Vec<&str> = eta_str.split(':').collect();
        if parts.len() != 2 {
            return None;
        }

        let minutes: u64 = parts[0].parse().ok()?;
        let seconds: u64 = parts[1].parse().ok()?;

        Some(Duration::from_secs(minutes * 60 + seconds))
    }

    /// 从yt-dlp输出中提取文件路径
    fn extract_output_path(&self, line: &str) -> Option<PathBuf> {
        // 支持多种 yt-dlp 输出格式:
        // [download] Destination: /path/to/file.mp4
        // [Merger] Merging formats into "/path/to/file.mp4"
        // [ExtractAudio] Destination: /path/to/file.mp3
        // [download] /path/to/file.mp4 has already been downloaded
        // [ffmpeg] Merging formats into "/path/to/file.webm"

        if let Some(start) = line.find("Destination:") {
            let path_part = &line[start + 12..]; // "Destination:" 的长度
            return Some(PathBuf::from(path_part.trim()));
        }

        // 处理 [Merger] Merging formats into "path" 格式
        if line.contains("Merging formats into") {
            if let Some(start) = line.find('"') {
                if let Some(end) = line.rfind('"') {
                    if end > start {
                        let path_str = &line[start + 1..end];
                        return Some(PathBuf::from(path_str));
                    }
                }
            }
        }

        // 处理 "has already been downloaded" 格式
        if line.contains("has already been downloaded") {
            // [download] /path/to/file.mp4 has already been downloaded
            if let Some(start) = line.find("[download]") {
                let rest = &line[start + 10..].trim();
                if let Some(end) = rest.find(" has already been downloaded") {
                    return Some(PathBuf::from(&rest[..end]));
                }
            }
        }

        None
    }
}

/// 下载进度信息
#[derive(Debug)]
struct ProgressInfo {
    percent: f32,
    speed: Option<u64>,
    eta: Option<Duration>,
    downloaded_bytes: Option<u64>,
    total_bytes: Option<u64>,
}

/// 下载进度事件
#[derive(Debug, Clone)]
pub enum DownloadProgress {
    Started {
        task_id: TaskId,
        url: String,
    },
    Progress {
        task_id: TaskId,
        percent: f32,
        speed: Option<u64>,
        eta: Option<Duration>,
        downloaded_bytes: Option<u64>,
        total_bytes: Option<u64>,
    },
    Completed {
        task_id: TaskId,
        output_path: PathBuf,
    },
    Error {
        task_id: TaskId,
        error: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_progress_line() {
        let downloader = VideoDownloader::new(PathBuf::from("/usr/bin/yt-dlp"), None);

        let line = "[download]   45.2% of 125.45MiB at 1.2MiB/s ETA 01:23";
        let progress = downloader.parse_progress_line(line);

        assert!(progress.is_some());
        let progress = progress.unwrap();
        assert!((progress.percent - 45.2).abs() < 0.01);
        assert!(progress.speed.is_some());
        assert!(progress.eta.is_some());
    }

    #[test]
    fn test_extract_output_path() {
        let downloader = VideoDownloader::new(PathBuf::from("/usr/bin/yt-dlp"), None);

        let line = "[download] Destination: /path/to/video.mp4";
        let path = downloader.extract_output_path(line);

        assert!(path.is_some());
        assert_eq!(path.unwrap(), PathBuf::from("/path/to/video.mp4"));
    }
}
