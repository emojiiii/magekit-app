use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::config::DownloadRequest;
use crate::error::{DownloadError, DownloadResult};
use crate::progress::{
    DownloadCallback, DownloadOutcome, DownloadProgress, DownloadStage, LogLine, LogSource,
};

/// yt-dlp 下载器（基础实现：构建命令、解析 stdout 进度、stderr 日志）
pub struct YtDlpDownloader {
    /// yt-dlp 可执行文件路径
    ytdlp_path: PathBuf,
    /// ffmpeg 可执行文件路径（用于后处理）
    ffmpeg_path: PathBuf,
}

impl YtDlpDownloader {
    pub fn new(ytdlp_path: PathBuf, ffmpeg_path: Option<PathBuf>) -> Self {
        Self {
            ytdlp_path,
            ffmpeg_path: ffmpeg_path.unwrap_or_else(|| PathBuf::from("ffmpeg")),
        }
    }
}

impl Default for YtDlpDownloader {
    fn default() -> Self {
        Self {
            ytdlp_path: PathBuf::from("yt-dlp"),
            ffmpeg_path: PathBuf::from("ffmpeg"),
        }
    }
}

#[async_trait]
impl crate::downloader::Downloader for YtDlpDownloader {
    fn name(&self) -> &'static str {
        "ytdlp"
    }

    async fn download(
        &self,
        request: DownloadRequest,
        callback: &dyn DownloadCallback,
        cancel: CancellationToken,
    ) -> DownloadResult<DownloadOutcome> {
        tracing::info!("🎬 YtDlpDownloader::download() 开始执行");
        tracing::info!("  ├─ URL: {}", request.url);
        tracing::info!("  ├─ full_path: {:?}", request.output.full_path);
        tracing::info!("  ├─ directory: {:?}", request.output.directory);
        tracing::info!("  └─ template: {:?}", request.output.template);

        callback.on_progress(DownloadProgress::preparing());

        let output_template = build_output_template(&request)?;
        tracing::info!("📁 输出路径: {:?}", output_template);

        if let Some(dir) = output_template.parent() {
            tracing::info!("📁 创建输出目录: {:?}", dir);
            tokio::fs::create_dir_all(dir).await?;
        }

        // 组装命令
        tracing::info!("🔧 构建 yt-dlp 命令");
        tracing::info!("📁 yt-dlp 路径: {:?}", self.ytdlp_path);
        let mut cmd = Command::new(&self.ytdlp_path);
        cmd.arg(request.url.to_string())
            .arg("-o")
            .arg(output_template.to_string_lossy().to_string())
            .arg("--newline")
            .arg("--progress")
            .arg("--no-mtime");

        // headers
        tracing::info!("📝 添加 headers: {} 个", request.extra.headers.len());
        for (k, v) in &request.extra.headers {
            cmd.arg("--add-header").arg(format!("{}: {}", k, v));
            tracing::debug!("  header: {}: {}", k, v);
        }
        if let Some(cookie) = &request.extra.cookie {
            cmd.arg("--add-header").arg(format!("Cookie: {}", cookie));
            tracing::debug!("  cookie: {}", cookie);
        }

        // 透传自定义参数（key = "ytdlp"）
        if let Some(args) = request.extra.tool_args.get("ytdlp") {
            tracing::info!("📝 添加额外参数: {:?}", args);
            cmd.args(args);
        }

        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        tracing::info!("🚀 启动 yt-dlp 进程");
        let mut child = cmd.spawn().map_err(|e| {
            tracing::error!("❌ 启动 yt-dlp 失败: {}", e);
            DownloadError::Internal(format!("spawn yt-dlp failed: {}", e))
        })?;

        tracing::info!("✅ yt-dlp 进程已启动，PID: {:?}", child.id());

        let mut stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| DownloadError::Internal("yt-dlp stdout missing".into()))?,
        );
        let mut stderr = BufReader::new(
            child
                .stderr
                .take()
                .ok_or_else(|| DownloadError::Internal("yt-dlp stderr missing".into()))?,
        );

        let mut last_total: Option<u64> = None;
        let mut last_downloaded: u64 = 0;
        let mut stderr_buf = String::new();
        let mut detected_output: Option<PathBuf> = None;
        let mut line_count = 0;
        let mut stdout_buf = Vec::new();
        let mut stderr_line_buf = Vec::new();

        tracing::info!("📊 开始读取 yt-dlp 输出...");
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::warn!("⚠️ 下载被取消");
                    let _ = child.kill().await;
                    return Err(DownloadError::Canceled);
                }
                result = stdout.read_until(b'\n', &mut stdout_buf) => {
                    match result {
                        Ok(0) => {
                            tracing::info!("📊 yt-dlp stdout 结束，共读取 {} 行", line_count);
                            break;
                        }
                        Ok(_) => {
                            // 使用 lossy conversion 处理非 UTF-8 字节
                            let line = String::from_utf8_lossy(&stdout_buf).to_string();
                            stdout_buf.clear();

                            line_count += 1;
                            if line_count <= 10 {
                                tracing::info!("[yt-dlp stdout #{}] {}", line_count, line.trim());
                            } else if line_count == 11 {
                                tracing::info!("[yt-dlp stdout] ... (后续日志省略，只显示前10行)");
                            }

                            callback.on_log(LogLine { source: LogSource::Stdout, line: line.clone() });
                            if let Some(path) = parse_output_path_from_line(&line) {
                                tracing::info!("📁 检测到输出路径: {:?}", path);
                                detected_output = Some(path);
                            }
                            if let Some((downloaded, total, speed)) = parse_progress_line(&line) {
                                last_total = total.or(last_total);
                                if let Some(d) = downloaded {
                                    last_downloaded = d;
                                }
                                // 兜底：部分站点/协议 yt-dlp 会输出 percent 但 total=Unknown，
                                // 此时用“当前落盘文件大小 + percent”估算 total_bytes，恢复进度条。
                                if last_total.is_none() {
                                    if let Some(percent) = parse_progress_percent(&line) {
                                        if percent > 0.1 {
                                            if let Some(out) = detected_output.as_ref() {
                                                if let Some(size) =
                                                    probe_current_output_size_bytes(out).await
                                                {
                                                    last_downloaded = size;
                                                    let ratio = (percent / 100.0).clamp(0.0, 1.0);
                                                    if ratio > 0.0 {
                                                        let estimate =
                                                            ((size as f64) / ratio).ceil() as u64;
                                                        let estimate = estimate.max(size);
                                                        last_total = Some(estimate);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if line_count <= 5 {
                                    tracing::info!("📊 进度: downloaded={:?}, total={:?}, speed={:?}",
                                        downloaded, total, speed);
                                }
                                callback.on_progress(DownloadProgress::downloading(
                                    last_downloaded,
                                    last_total,
                                    speed,
                                ));
                            }
                        }
                        Err(e) => {
                            tracing::error!("❌ 读取 yt-dlp stdout 失败: {}", e);
                            callback.on_log(LogLine { source: LogSource::Stdout, line: format!("read stdout error: {}", e) });
                            break;
                        }
                    }
                }
                result = stderr.read_until(b'\n', &mut stderr_line_buf) => {
                    match result {
                        Ok(0) => {}
                        Ok(_) => {
                            let line = String::from_utf8_lossy(&stderr_line_buf).to_string();
                            stderr_line_buf.clear();

                            stderr_buf.push_str(&line);
                            if detected_output.is_none() {
                                if let Some(path) = parse_output_path_from_line(&line) {
                                    tracing::info!("📁 (stderr) 检测到输出路径: {:?}", path);
                                    detected_output = Some(path);
                                }
                            }
                            if line.contains("error") || line.contains("Error") || line.contains("WARNING") {
                                tracing::warn!("[yt-dlp stderr] {}", line.trim());
                            } else {
                                tracing::info!("[yt-dlp stderr] {}", line.trim());
                            }
                            callback.on_log(LogLine {
                                source: LogSource::Stderr,
                                line,
                            });
                        }
                        Err(e) => {
                            tracing::error!("❌ 读取 yt-dlp stderr 失败: {}", e);
                            stderr_buf.push_str(&format!("read stderr error: {}\n", e));
                        }
                    }
                }
            }
        }

        tracing::info!("⏳ 等待 yt-dlp 进程退出...");
        let status = child
            .wait()
            .await
            .map_err(|e| DownloadError::Internal(format!("wait yt-dlp failed: {}", e)))?;

        tracing::info!("🏁 yt-dlp 进程已退出，状态码: {:?}", status.code());

        if !status.success() {
            tracing::error!("❌ yt-dlp 失败，stderr:\n{}", stderr_buf);
            return Err(DownloadError::ProcessExit {
                code: status.code(),
                stderr: stderr_buf,
            });
        }

        let has_detected_output = detected_output.is_some();
        let mut final_output = detected_output.unwrap_or(output_template.clone());
        if !has_detected_output && output_template.to_string_lossy().contains("%(") {
            // yt-dlp 成功但没有打出 Destination（例如“已下载”场景），这里尽量从 stdout 内容中推断；
            // 如果仍旧拿不到，则避免进入后处理造成误转码/误失败。
            return Err(DownloadError::Internal(
                "yt-dlp succeeded but output path was not detected".into(),
            ));
        }
        final_output = maybe_transcode_youtube_to_h264_mp4(
            &self.ffmpeg_path,
            &request.url,
            &final_output,
            callback,
            &cancel,
        )
        .await?;
        tracing::info!("✅ 下载完成，输出文件: {:?}", final_output);
        tracing::info!(
            "📊 最终统计: downloaded={}, total={:?}",
            last_downloaded,
            last_total
        );

        callback.on_progress(DownloadProgress::completed(last_downloaded, last_total));
        callback.on_complete(DownloadOutcome {
            output_path: final_output.clone(),
            content_type: None,
            details: Default::default(),
        });

        Ok(DownloadOutcome {
            output_path: final_output,
            content_type: None,
            details: Default::default(),
        })
    }
}

fn is_youtube_url(url: &url::Url) -> bool {
    url.host_str().unwrap_or_default().contains("youtube.com")
        || url.host_str().unwrap_or_default().contains("youtu.be")
}

async fn maybe_transcode_youtube_to_h264_mp4(
    ffmpeg_path: &PathBuf,
    request_url: &url::Url,
    input_path: &PathBuf,
    callback: &dyn DownloadCallback,
    cancel: &CancellationToken,
) -> DownloadResult<PathBuf> {
    if !is_youtube_url(request_url) {
        return Ok(input_path.clone());
    }

    if tokio::fs::metadata(input_path).await.is_err() {
        // 输入文件不存在时不要进入后处理（否则会触发无意义的 “合并中” 并导致 Auto fallback）
        callback.on_log(LogLine {
            source: LogSource::System,
            line: format!("YouTube 后处理跳过：输入文件不存在: {:?}", input_path),
        });
        return Ok(input_path.clone());
    }

    let (video_codec, audio_codec) = probe_codecs_best_effort(ffmpeg_path, input_path).await;
    let ext_is_mp4 = input_path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("mp4"));

    let is_h264 = video_codec
        .as_deref()
        .is_some_and(|c| c.eq_ignore_ascii_case("h264"));
    let is_aac_or_none = audio_codec
        .as_deref()
        .map_or(true, |c| c.eq_ignore_ascii_case("aac"));

    // 已经是 mp4 且编码兼容：无需后处理
    if ext_is_mp4 && is_h264 && is_aac_or_none {
        return Ok(input_path.clone());
    }

    // 探测失败（无法确定编码）时，避免误触发转码
    if ext_is_mp4 && video_codec.is_none() && audio_codec.is_none() {
        callback.on_log(LogLine {
            source: LogSource::System,
            line: "YouTube 后处理跳过：无法探测编码且已是 mp4 容器".to_string(),
        });
        return Ok(input_path.clone());
    }

    // 编码兼容但容器不是 mp4：只做 remux（-c copy）到 mp4，避免重编码
    let can_remux_to_mp4 = is_h264 && is_aac_or_none && !ext_is_mp4;
    callback.on_log(LogLine {
        source: LogSource::System,
        line: format!(
            "YouTube 后处理：{}为 mp4（当前 vcodec={:?}, acodec={:?}, path={:?}）",
            if can_remux_to_mp4 { "remux " } else { "转码" },
            video_codec,
            audio_codec,
            input_path
        ),
    });

    callback.on_progress(DownloadProgress {
        stage: DownloadStage::Merging,
        bytes_downloaded: 0,
        total_bytes: None,
        speed_bps: None,
        eta: None,
    });

    let output_path = choose_transcoded_output_path(input_path).await;
    let tmp_out = with_part_mp4(&output_path);

    if let Some(parent) = tmp_out.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let total_duration_secs = probe_duration_secs_best_effort(ffmpeg_path, input_path).await;

    let mut cmd = Command::new(ffmpeg_path);
    cmd.arg("-hide_banner")
        .arg("-y")
        .arg("-i")
        .arg(input_path.to_string_lossy().to_string())
        .arg("-map")
        .arg("0:v:0");

    if can_remux_to_mp4 {
        cmd.arg("-c:v").arg("copy");
    } else {
        // needs_transcode == true
        cmd.arg("-c:v")
            .arg("libx264")
            .arg("-preset")
            .arg("veryfast")
            .arg("-crf")
            .arg("23")
            .arg("-pix_fmt")
            .arg("yuv420p");
    }

    if audio_codec.is_some() {
        cmd.arg("-map").arg("0:a:0");
        if can_remux_to_mp4 {
            cmd.arg("-c:a").arg("copy");
        } else {
            cmd.arg("-c:a").arg("aac").arg("-b:a").arg("192k");
        }
    } else {
        cmd.arg("-an");
    }

    cmd.arg("-movflags")
        .arg("+faststart")
        .arg("-progress")
        .arg("pipe:1")
        .arg("-nostats")
        .arg(tmp_out.to_string_lossy().to_string());

    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| DownloadError::Internal(format!("spawn ffmpeg failed: {}", e)))?;

    let mut stdout = BufReader::new(
        child
            .stdout
            .take()
            .ok_or_else(|| DownloadError::Internal("ffmpeg stdout missing".into()))?,
    )
    .lines();

    let stderr_handle = {
        let stderr = child.stderr.take();
        tokio::spawn(async move {
            let mut buf = String::new();
            if let Some(stderr) = stderr {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    buf.push_str(&line);
                    buf.push('\n');
                }
            }
            buf
        })
    };

    let mut last_size: u64 = 0;
    let mut last_time_secs: f64 = 0.0;
    let mut estimated_total_bytes: Option<u64> = None;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                stderr_handle.abort();
                return Err(DownloadError::Canceled);
            }
            line = stdout.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        if let Some(size_str) = l.strip_prefix("total_size=") {
                            if let Ok(size) = size_str.parse::<u64>() {
                                last_size = size;
                            }
                        } else if let Some(time_str) = l.strip_prefix("out_time_us=") {
                            if let Ok(time_us) = time_str.parse::<u64>() {
                                last_time_secs = time_us as f64 / 1_000_000.0;
                            }
                        } else if let Some(time_str) = l.strip_prefix("out_time_ms=") {
                            if let Ok(v) = time_str.parse::<u64>() {
                                last_time_secs = if v >= 1_000_000_000 {
                                    v as f64 / 1_000_000.0
                                } else {
                                    v as f64 / 1000.0
                                };
                            }
                        } else if let Some(time_str) = l.strip_prefix("out_time=") {
                            if let Some(secs) = crate::downloader::ffmpeg::parse_timestamp_to_secs(time_str.trim()) {
                                last_time_secs = secs;
                            }
                        }

                        if last_size > 0 {
                            let total_bytes = total_duration_secs.and_then(|total_secs| {
                                if total_secs <= 0.0 || last_time_secs <= 0.0 {
                                    return None;
                                }
                                let ratio = (last_time_secs / total_secs).clamp(0.0, 1.0);
                                if ratio <= 0.0 {
                                    return None;
                                }
                                let estimate = ((last_size as f64) / ratio).ceil() as u64;
                                let estimate = estimate.max(last_size);
                                let prev = estimated_total_bytes.unwrap_or(0);
                                let merged = prev.max(estimate);
                                estimated_total_bytes = Some(merged);
                                Some(merged)
                            });

                            callback.on_progress(DownloadProgress {
                                stage: DownloadStage::Merging,
                                bytes_downloaded: last_size,
                                total_bytes,
                                speed_bps: None,
                                eta: None,
                            });
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        stderr_handle.abort();
                        return Err(DownloadError::Internal(format!("ffmpeg read stdout: {}", e)));
                    }
                }
            }
        }
    }

    tokio::select! {
        _ = cancel.cancelled() => {
            let _ = child.kill().await;
            stderr_handle.abort();
            return Err(DownloadError::Canceled);
        }
        status = child.wait() => {
            let status = status.map_err(|e| DownloadError::Internal(format!("wait ffmpeg failed: {}", e)))?;
            if !status.success() {
                let stderr_buf = stderr_handle.await.unwrap_or_default();
                return Err(DownloadError::ProcessExit { code: status.code(), stderr: stderr_buf });
            }
        }
    }

    tokio::fs::rename(&tmp_out, &output_path).await?;
    Ok(output_path)
}

async fn choose_transcoded_output_path(input_path: &Path) -> PathBuf {
    let mut candidate = input_path.with_extension("mp4");
    if candidate == input_path {
        let stem = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        candidate = input_path.with_file_name(format!("{}_h264.mp4", stem));
    }

    if tokio::fs::metadata(&candidate).await.is_err() {
        return candidate;
    }

    let stem = candidate
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output")
        .to_string();
    let parent = candidate
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf();

    for i in 1..=1000u32 {
        let p = parent.join(format!("{}_{}.mp4", stem, i));
        if tokio::fs::metadata(&p).await.is_err() {
            return p;
        }
    }

    candidate
}

fn with_part_mp4(output_path: &Path) -> PathBuf {
    let parent = output_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = output_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    parent.join(format!("{}.part.mp4", stem))
}

async fn probe_codecs_best_effort(
    ffmpeg_path: &PathBuf,
    input_path: &Path,
) -> (Option<String>, Option<String>) {
    if let Some((v, a)) = probe_codecs_with_ffprobe(ffmpeg_path, input_path).await {
        return (v, a);
    }
    probe_codecs_with_ffmpeg(ffmpeg_path, input_path).await
}

async fn probe_duration_secs_best_effort(ffmpeg_path: &PathBuf, input_path: &Path) -> Option<f64> {
    let ffprobe = resolve_ffprobe_path(ffmpeg_path);
    let out = Command::new(&ffprobe)
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("default=nokey=1:noprint_wrappers=1")
        .arg(input_path.to_string_lossy().to_string())
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    s.parse::<f64>().ok()
}

fn resolve_ffprobe_path(ffmpeg_path: &PathBuf) -> PathBuf {
    let file_name = ffmpeg_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if file_name == "ffmpeg.exe" || file_name == "ffmpeg" {
        if let Some(parent) = ffmpeg_path.parent() {
            if file_name.ends_with(".exe") {
                return parent.join("ffprobe.exe");
            }
            return parent.join("ffprobe");
        }
    }

    PathBuf::from("ffprobe")
}

async fn probe_codecs_with_ffprobe(
    ffmpeg_path: &PathBuf,
    input_path: &Path,
) -> Option<(Option<String>, Option<String>)> {
    let ffprobe = resolve_ffprobe_path(ffmpeg_path);

    let v = Command::new(&ffprobe)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=codec_name")
        .arg("-of")
        .arg("default=nw=1:nk=1")
        .arg(input_path.to_string_lossy().to_string())
        .output()
        .await
        .ok()
        .and_then(|o| if o.status.success() { Some(o) } else { None })
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let a = Command::new(&ffprobe)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("a:0")
        .arg("-show_entries")
        .arg("stream=codec_name")
        .arg("-of")
        .arg("default=nw=1:nk=1")
        .arg(input_path.to_string_lossy().to_string())
        .output()
        .await
        .ok()
        .and_then(|o| if o.status.success() { Some(o) } else { None })
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    Some((v, a))
}

async fn probe_codecs_with_ffmpeg(
    ffmpeg_path: &PathBuf,
    input_path: &Path,
) -> (Option<String>, Option<String>) {
    let out = Command::new(ffmpeg_path)
        .arg("-hide_banner")
        .arg("-i")
        .arg(input_path.to_string_lossy().to_string())
        .output()
        .await;

    let stderr = out
        .ok()
        .and_then(|o| String::from_utf8(o.stderr).ok())
        .unwrap_or_default();

    (
        extract_codec(&stderr, "Video:"),
        extract_codec(&stderr, "Audio:"),
    )
}

fn extract_codec(stderr: &str, marker: &str) -> Option<String> {
    for line in stderr.lines() {
        let idx = line.find(marker)?;
        let after = line[idx + marker.len()..].trim_start();
        let codec = after
            .split(|c: char| c.is_whitespace() || c == ',' || c == '(')
            .next()
            .unwrap_or_default()
            .trim();
        if !codec.is_empty() {
            return Some(codec.to_string());
        }
    }
    None
}

/// 构建输出模板路径
fn build_output_template(request: &DownloadRequest) -> DownloadResult<PathBuf> {
    tracing::info!("🔧 build_output_template() 被调用");
    tracing::info!("  ├─ full_path: {:?}", request.output.full_path);
    tracing::info!("  ├─ directory: {:?}", request.output.directory);
    tracing::info!("  └─ template: {:?}", request.output.template);

    // 🔧 优先使用 full_path
    if let Some(full_path) = &request.output.full_path {
        tracing::info!("✅ 使用 full_path: {:?}", full_path);
        return Ok(full_path.clone());
    }

    // 降级到 directory + template
    let tmpl = request
        .output
        .template
        .clone()
        .unwrap_or_else(|| "%(title)s.%(ext)s".to_string());
    let result = request.output.directory.join(tmpl);
    tracing::info!("✅ 使用 directory + template: {:?}", result);
    Ok(result)
}

/// 解析 yt-dlp 进度行
/// 例：[download]  45.2% of 12.34MiB at 1.23MiB/s ETA 00:05
pub fn parse_progress_line(line: &str) -> Option<(Option<u64>, Option<u64>, Option<u64>)> {
    if !line.contains("[download]") {
        return None;
    }
    let tokens: Vec<&str> = line.split_whitespace().collect();

    // 百分比
    let percent = parse_progress_percent(line);

    // total size（在 "of" 之后）
    let mut total_bytes: Option<u64> = None;
    for (idx, &t) in tokens.iter().enumerate() {
        if t != "of" {
            continue;
        }
        let mut j = idx + 1;
        while j < tokens.len() && (tokens[j].is_empty() || tokens[j] == "~") {
            j += 1;
        }
        if j < tokens.len() {
            let raw = tokens[j].trim_start_matches('~');
            total_bytes = parse_size_to_bytes(raw);
        }
        break;
    }

    // speed（在 "at" 之后）
    let mut speed: Option<u64> = None;
    for (idx, &t) in tokens.iter().enumerate() {
        if t != "at" {
            continue;
        }
        if idx + 1 < tokens.len() {
            let raw = tokens[idx + 1]
                .trim_end_matches(|c: char| matches!(c, ',' | ')' | ']' | ';'))
                .trim_end_matches("/s");
            speed = parse_size_to_bytes(raw);
        }
        break;
    }

    let downloaded = if let (Some(p), Some(total)) = (percent, total_bytes) {
        Some(((p / 100.0) * total as f64) as u64)
    } else {
        None
    };

    if percent.is_none() && total_bytes.is_none() && speed.is_none() {
        return None;
    }

    Some((downloaded, total_bytes, speed))
}

pub fn parse_progress_percent(line: &str) -> Option<f64> {
    if !line.contains("[download]") {
        return None;
    }
    for t in line.split_whitespace() {
        let cleaned = t.trim().trim_matches(|c: char| c == '\r' || c == '\n');
        let cleaned = cleaned.trim_matches(|c: char| c == '[' || c == ']' || c == '(' || c == ')');
        if let Some(stripped) = cleaned.strip_suffix('%') {
            let stripped = stripped.trim_matches(|c: char| !c.is_ascii_digit() && c != '.');
            if let Ok(v) = stripped.parse::<f64>() {
                return Some(v);
            }
        }
    }
    None
}

pub fn parse_destination(line: &str) -> Option<PathBuf> {
    if !line.contains("Destination:") {
        return None;
    }
    let idx = line.find("Destination:")?;
    let path = line[(idx + "Destination:".len())..].trim();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn parse_already_downloaded_path(line: &str) -> Option<PathBuf> {
    if !line.contains("[download]") {
        return None;
    }
    let marker = "has already been downloaded";
    let idx = line.find(marker)?;
    let mut prefix = line[..idx].to_string();
    if let Some(pos) = prefix.find("[download]") {
        prefix = prefix[(pos + "[download]".len())..].to_string();
    }
    let path = prefix.trim();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn parse_merging_formats_into(line: &str) -> Option<PathBuf> {
    let marker = "Merging formats into";
    let idx = line.find(marker)?;
    let mut rest = line[(idx + marker.len())..].trim();
    if rest.starts_with(':') {
        rest = rest[1..].trim();
    }
    // 常见格式：... into "C:\path\file.ext"
    if let Some(start_q) = rest.find('"') {
        let after = &rest[start_q + 1..];
        if let Some(end_q) = after.find('"') {
            let path = &after[..end_q];
            if !path.trim().is_empty() {
                return Some(PathBuf::from(path.trim()));
            }
        }
    }
    // 无引号兜底：取剩余整段
    if !rest.is_empty() {
        return Some(PathBuf::from(rest));
    }
    None
}

fn parse_output_path_from_line(line: &str) -> Option<PathBuf> {
    parse_destination(line)
        .or_else(|| parse_already_downloaded_path(line))
        .or_else(|| parse_merging_formats_into(line))
}

fn parse_size_to_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("unknown") {
        return None;
    }
    // 去掉常见尾部标点（例如 "12.3MiB,"）
    let s = s.trim_end_matches(|c: char| matches!(c, ',' | ')' | ']' | ';'));
    let units = [
        ("KiB", 1024u64),
        ("MiB", 1024u64.pow(2)),
        ("GiB", 1024u64.pow(3)),
        ("TiB", 1024u64.pow(4)),
        ("KB", 1000u64),
        ("MB", 1000u64.pow(2)),
        ("GB", 1000u64.pow(3)),
        ("TB", 1000u64.pow(4)),
        ("B", 1u64),
    ];

    for (u, m) in units {
        if let Some(num) = s.strip_suffix(u) {
            let num = num.trim().replace(',', "");
            if let Ok(v) = num.parse::<f64>() {
                return Some((v * m as f64) as u64);
            }
        }
    }
    None
}

async fn probe_current_output_size_bytes(output: &Path) -> Option<u64> {
    if let Ok(m) = tokio::fs::metadata(output).await {
        return Some(m.len());
    }
    // yt-dlp 常见临时文件：<file>.<ext>.part
    let part = PathBuf::from(format!("{}.part", output.to_string_lossy()));
    if let Ok(m) = tokio::fs::metadata(part).await {
        return Some(m.len());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_already_downloaded_path_windows() {
        let line = r#"[download] D:\out\video.mp4 has already been downloaded"#;
        let p = parse_already_downloaded_path(line).unwrap();
        assert_eq!(p, PathBuf::from(r#"D:\out\video.mp4"#));
    }

    #[test]
    fn test_parse_merging_formats_into_quoted() {
        let line = r#"[Merger] Merging formats into "D:\out\video.mp4""#;
        let p = parse_merging_formats_into(line).unwrap();
        assert_eq!(p, PathBuf::from(r#"D:\out\video.mp4"#));
    }

    #[test]
    fn test_parse_output_path_best_effort_prefers_destination() {
        let line = r#"[download] Destination: D:\out\video.webm"#;
        let p = parse_output_path_from_line(line).unwrap();
        assert_eq!(p, PathBuf::from(r#"D:\out\video.webm"#));
    }
}
