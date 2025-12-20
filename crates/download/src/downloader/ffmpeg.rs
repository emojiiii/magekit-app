use std::path::PathBuf;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::config::DownloadRequest;
use crate::error::{DownloadError, DownloadResult};
use crate::progress::{DownloadCallback, DownloadOutcome, DownloadProgress};
use crate::utils::resolve_output_path;
use m3u8_rs::{MasterPlaylist, Playlist, VariantStream};

/// ffmpeg 下载器（拉流/封装场景）
pub struct FfmpegDownloader {
    /// ffmpeg 可执行文件路径
    ffmpeg_path: PathBuf,
}

impl FfmpegDownloader {
    pub fn new(ffmpeg_path: PathBuf) -> Self {
        Self { ffmpeg_path }
    }
}

impl Default for FfmpegDownloader {
    fn default() -> Self {
        Self {
            ffmpeg_path: PathBuf::from("ffmpeg"),
        }
    }
}

#[async_trait]
impl crate::downloader::Downloader for FfmpegDownloader {
    fn name(&self) -> &'static str {
        "ffmpeg"
    }

    async fn download(
        &self,
        request: DownloadRequest,
        callback: &dyn DownloadCallback,
        cancel: CancellationToken,
    ) -> DownloadResult<DownloadOutcome> {
        callback.on_progress(DownloadProgress::preparing());

        let total_duration_secs = probe_m3u8_total_duration_secs(&request, &cancel).await;

        let output_path = resolve_output_path(&request, &request.url)?;
        tracing::info!("📁 解析输出路径: {:?}", output_path);
        tracing::info!("📝 输出文件扩展名: {:?}", output_path.extension());

        if let Some(dir) = output_path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }

        // 构建 headers 字符串（FFmpeg 需要 CRLF）
        let mut header_lines = Vec::new();
        for (k, v) in &request.extra.headers {
            header_lines.push(format!("{}: {}", k, v));
        }
        if let Some(cookie) = &request.extra.cookie {
            header_lines.push(format!("Cookie: {}", cookie));
        }
        let header_blob = header_lines.join("\r\n");

        tracing::info!(
            "🔧 构建 ffmpeg 命令，headers数量: {}",
            request.extra.headers.len()
        );
        tracing::info!("📁 ffmpeg 路径: {:?}", self.ffmpeg_path);

        let mut cmd = Command::new(&self.ffmpeg_path);
        cmd.arg("-hide_banner");
        // 降噪：避免刷屏的 hls “Opening ... for reading” 日志
        cmd.arg("-loglevel").arg("warning");
        cmd.arg("-y");
        if !header_blob.is_empty() {
            cmd.arg("-headers").arg(&header_blob);
            tracing::debug!("📝 添加 headers:\n{}", header_blob);
        }
        cmd.arg("-i").arg(request.url.to_string());

        // 透传自定义参数（key = "ffmpeg"）
        if let Some(args) = request.extra.tool_args.get("ffmpeg") {
            cmd.args(args);
            tracing::debug!("📝 添加额外参数: {:?}", args);
        }

        // 直接拷贝封装，避免重编码
        cmd.arg("-c").arg("copy");

        // 🔧 关键：强制输出为单个文件（而不是HLS分片）
        // 当输入是m3u8但输出是mp4时，需要明确告诉ffmpeg输出格式
        if let Some(ext) = output_path.extension() {
            if ext == "mp4" || ext == "mkv" || ext == "avi" {
                cmd.arg("-f").arg(ext.to_string_lossy().as_ref());
                tracing::debug!("🔧 强制输出格式: {}", ext.to_string_lossy());

                // 🔧 对于mp4，添加AAC音频流过滤器（从HLS转换时需要）
                if ext == "mp4" {
                    cmd.arg("-bsf:a").arg("aac_adtstoasc");
                    tracing::debug!("🔧 添加 AAC 音频流过滤器");
                }
            }
        }

        // 🔧 关键：添加进度输出参数
        cmd.arg("-progress").arg("pipe:1"); // 进度输出到stdout
        cmd.arg("-nostats"); // 禁用默认统计信息

        cmd.arg(output_path.to_string_lossy().to_string());

        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped()); // 🔧 改为piped以读取进度
        cmd.stderr(Stdio::piped());

        // 🔧 构建完整的命令字符串用于日志
        let format_arg = if let Some(ext) = output_path.extension() {
            if ext == "mp4" || ext == "mkv" || ext == "avi" {
                format!("-f {} -bsf:a aac_adtstoasc ", ext.to_string_lossy())
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        tracing::info!(
            "🚀 启动 ffmpeg: ffmpeg -y {} -i {} -c copy {}-progress pipe:1 -nostats {}",
            if header_blob.is_empty() {
                ""
            } else {
                "-headers <...> "
            },
            request.url,
            format_arg,
            output_path.display()
        );

        let mut child = cmd
            .spawn()
            .map_err(|e| DownloadError::Internal(format!("spawn ffmpeg failed: {}", e)))?;

        tracing::info!("✅ ffmpeg 进程已启动，PID: {:?}", child.id());

        // 🔧 分别处理stdout（进度）和stderr（错误）
        let mut stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| DownloadError::Internal("ffmpeg stdout missing".into()))?,
        )
        .lines();

        let mut last_size: u64 = 0;
        let mut last_time_secs: f64 = 0.0;
        let mut estimated_total_bytes: Option<u64> = None;

        // 🔧 同时处理stderr（错误日志）
        let stderr_handle = {
            let stderr = child.stderr.take();
            tokio::spawn(async move {
                let mut buf = String::new();
                if let Some(stderr) = stderr {
                    let mut reader = BufReader::new(stderr).lines();
                    while let Ok(Some(line)) = reader.next_line().await {
                        buf.push_str(&line);
                        buf.push('\n');
                        // 默认不打印 stderr，避免刷屏（如 HLS "Opening ... for reading"）。
                        // 仅在明显错误时输出，便于定位问题。
                        if line.contains("error")
                            || line.contains("Error")
                            || line.contains("failed")
                            || line.contains("Invalid data")
                            || line.contains("HTTP")
                        {
                            tracing::error!("[ffmpeg stderr] {}", line);
                        }
                    }
                }
                buf
            })
        };

        // 🔧 读取stdout的进度信息
        tracing::info!("📊 开始读取 ffmpeg 进度输出...");
        let mut progress_count = 0;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::warn!("⚠️ 下载被取消");
                    let _ = child.kill().await;
                    return Err(DownloadError::Canceled);
                }
                line = stdout.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            progress_count += 1;
                            if progress_count <= 5 {
                                tracing::debug!("[ffmpeg stdout] {}", l);
                            }

                            // ffmpeg -progress 输出格式：
                            // out_time_ms=1234567
                            // total_size=123456
                            // progress=continue/end

                            if let Some(size_str) = l.strip_prefix("total_size=") {
                                if let Ok(size) = size_str.parse::<u64>() {
                                    last_size = size;
                                }
                            } else if let Some(time_str) = l.strip_prefix("out_time_us=") {
                                if let Ok(time_us) = time_str.parse::<u64>() {
                                    last_time_secs = time_us as f64 / 1_000_000.0;
                                }
                            } else if let Some(time_str) = l.strip_prefix("out_time_ms=") {
                                // 兼容不同 ffmpeg 版本：有的 out_time_ms 实际是 ms，有的为 us（历史兼容）。
                                if let Ok(v) = time_str.parse::<u64>() {
                                    last_time_secs = if v >= 1_000_000_000 {
                                        v as f64 / 1_000_000.0
                                    } else {
                                        v as f64 / 1000.0
                                    };
                                }
                            } else if let Some(time_str) = l.strip_prefix("out_time=") {
                                if let Some(secs) = parse_timestamp_to_secs(time_str.trim()) {
                                    last_time_secs = secs;
                                }
                            } else if l == "progress=end" {
                                tracing::info!("✅ ffmpeg 完成，total_size={}", last_size);
                            }

                            // 计算速度
                            let speed = if last_time_secs > 0.0 {
                                Some((last_size as f64 / last_time_secs) as u64)
                            } else {
                                None
                            };

                            // 发送进度
                            if last_size > 0 {
                                // ffmpeg 不会提供总大小：如果是 VOD m3u8，可用 out_time / total_duration 估算 total_bytes
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

                                callback.on_progress(DownloadProgress::downloading(
                                    last_size,
                                    total_bytes,
                                    speed,
                                ));
                            }
                        }
                        Ok(None) => {
                            tracing::info!("📊 ffmpeg stdout 结束，共读取 {} 行进度信息", progress_count);
                            break;
                        }
                        Err(e) => {
                            tracing::error!("❌ 读取 ffmpeg stdout 失败: {}", e);
                            return Err(DownloadError::Internal(format!("ffmpeg read stdout: {}", e)));
                        }
                    }
                }
            }
        }

        // 等待进程结束并获取stderr
        tracing::info!("⏳ 等待 ffmpeg 进程退出...");
        let status = child
            .wait()
            .await
            .map_err(|e| DownloadError::Internal(format!("wait ffmpeg failed: {}", e)))?;

        let stderr_buf = stderr_handle.await.unwrap_or_default();

        tracing::info!("🏁 ffmpeg 进程已退出，状态码: {:?}", status.code());

        if !status.success() {
            tracing::error!("❌ ffmpeg 失败，stderr:\n{}", stderr_buf);
            return Err(DownloadError::ProcessExit {
                code: status.code(),
                stderr: stderr_buf,
            });
        }

        callback.on_progress(DownloadProgress::completed(last_size, None));
        callback.on_complete(DownloadOutcome {
            output_path: output_path.clone(),
            content_type: None,
            details: Default::default(),
        });

        Ok(DownloadOutcome {
            output_path,
            content_type: None,
            details: Default::default(),
        })
    }
}

/// 从 ffmpeg stderr 进度行解析 size 字段（bytes）
pub fn parse_ffmpeg_size(line: &str) -> Option<u64> {
    // 常见格式：size=   1234kB time=00:00:12.34 bitrate=...
    if !line.contains("size=") {
        return None;
    }
    let tokens: Vec<&str> = line.split_whitespace().collect();
    for win in tokens.windows(2) {
        if let Some(rest) = win[0].strip_prefix("size=") {
            if !rest.is_empty() {
                if let Some(v) = parse_size_to_bytes(rest.trim()) {
                    return Some(v);
                }
            } else {
                if let Some(v) = parse_size_to_bytes(win[1].trim()) {
                    return Some(v);
                }
            }
        }
    }
    None
}

pub fn parse_ffmpeg_time(line: &str) -> Option<f64> {
    if !line.contains("time=") {
        return None;
    }
    let idx = line.find("time=")?;
    let time_part = line[idx + 5..].split_whitespace().next()?;
    parse_timestamp_to_secs(time_part)
}

fn parse_size_to_bytes(raw: &str) -> Option<u64> {
    let units = [
        ("KiB", 1024u64),
        ("MiB", 1024u64.pow(2)),
        ("GiB", 1024u64.pow(3)),
        ("TiB", 1024u64.pow(4)),
        ("kB", 1000u64),
        ("MB", 1000u64.pow(2)),
        ("GB", 1000u64.pow(3)),
        ("TB", 1000u64.pow(4)),
        ("B", 1u64),
    ];
    for (u, m) in units {
        if let Some(num) = raw.strip_suffix(u) {
            if let Ok(v) = num.trim().parse::<f64>() {
                return Some((v * m as f64) as u64);
            }
        }
    }
    None
}

pub fn parse_timestamp_to_secs(ts: &str) -> Option<f64> {
    let parts: Vec<&str> = ts.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let h = parts[0].parse::<f64>().ok()?;
    let m = parts[1].parse::<f64>().ok()?;
    let s = parts[2].parse::<f64>().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

async fn probe_m3u8_total_duration_secs(
    request: &DownloadRequest,
    cancel: &CancellationToken,
) -> Option<f64> {
    let url_str = request.url.as_str();
    if !request.url.path().ends_with(".m3u8") && !url_str.contains(".m3u8") {
        return None;
    }

    let manifest_text = fetch_text(&request.url, request, cancel).await.ok()?;
    let parsed = m3u8_rs::parse_playlist_res(manifest_text.as_bytes()).ok()?;

    let media = match parsed {
        Playlist::MasterPlaylist(master) => {
            let variant = pick_variant(&master)?;
            let uri = request.url.join(variant.uri.as_str()).ok()?;
            let text = fetch_text(&uri, request, cancel).await.ok()?;
            let parsed_media = m3u8_rs::parse_playlist_res(text.as_bytes()).ok()?;
            match parsed_media {
                Playlist::MediaPlaylist(m) => m,
                _ => return None,
            }
        }
        Playlist::MediaPlaylist(m) => m,
    };

    let total_secs: f64 = media.segments.iter().map(|s| s.duration as f64).sum();
    if total_secs > 0.0 {
        Some(total_secs)
    } else {
        None
    }
}

fn pick_variant(master: &MasterPlaylist) -> Option<&VariantStream> {
    master.variants.iter().max_by_key(|v| v.bandwidth)
}

async fn fetch_text(
    url: &url::Url,
    request: &DownloadRequest,
    cancel: &CancellationToken,
) -> DownloadResult<String> {
    let bytes = fetch_bytes(url, request, cancel).await?;
    String::from_utf8(bytes)
        .map_err(|e| DownloadError::Internal(format!("utf8 decode playlist: {}", e)))
}

async fn fetch_bytes(
    url: &url::Url,
    request: &DownloadRequest,
    cancel: &CancellationToken,
) -> DownloadResult<Vec<u8>> {
    if cancel.is_cancelled() {
        return Err(DownloadError::Canceled);
    }

    let mut client_builder = reqwest::Client::builder();
    if let Some(timeout) = request.timeout {
        client_builder = client_builder.timeout(timeout);
    }
    let client = client_builder
        .build()
        .map_err(|e| DownloadError::Internal(format!("HTTP client build failed: {}", e)))?;

    let mut req = client.get(url.clone());
    for (k, v) in &request.extra.headers {
        req = req.header(k, v);
    }
    if let Some(cookie) = &request.extra.cookie {
        req = req.header(reqwest::header::COOKIE, cookie);
    }

    let resp = req.send().await.map_err(DownloadError::from)?;
    if !resp.status().is_success() {
        return Err(DownloadError::Network(format!("HTTP {}", resp.status())));
    }

    if cancel.is_cancelled() {
        return Err(DownloadError::Canceled);
    }

    let bytes = resp.bytes().await.map_err(DownloadError::from)?;
    Ok(bytes.to_vec())
}
