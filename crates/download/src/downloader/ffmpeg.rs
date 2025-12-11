use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::config::DownloadRequest;
use crate::error::{DownloadError, DownloadResult};
use crate::progress::{DownloadCallback, DownloadOutcome, DownloadProgress, LogLine, LogSource};
use crate::utils::resolve_output_path;

/// ffmpeg 下载器（拉流/封装场景）
pub struct FfmpegDownloader;

impl Default for FfmpegDownloader {
    fn default() -> Self {
        Self
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

        let output_path = resolve_output_path(&request, &request.url)?;
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

        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y");
        if !header_blob.is_empty() {
            cmd.arg("-headers").arg(header_blob.clone());
        }
        cmd.arg("-i").arg(request.url.to_string());

        // 透传自定义参数（key = "ffmpeg"）
        if let Some(args) = request.extra.tool_args.get("ffmpeg") {
            cmd.args(args);
        }

        // 直接拷贝封装，避免重编码
        cmd.arg("-c").arg("copy");
        cmd.arg(output_path.to_string_lossy().to_string());

        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| DownloadError::Internal(format!("spawn ffmpeg failed: {}", e)))?;

        let mut stderr = BufReader::new(
            child
                .stderr
                .take()
                .ok_or_else(|| DownloadError::Internal("ffmpeg stderr missing".into()))?,
        )
        .lines();

        let mut last_size: u64 = 0;
        let mut last_time_secs: f64 = 0.0;
        let mut stderr_buf = String::new();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = child.kill().await;
                    let _ = tokio::fs::remove_file(&output_path).await;
                    return Err(DownloadError::Canceled);
                }
                line = stderr.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            stderr_buf.push_str(&l);
                            stderr_buf.push('\n');
                            callback.on_log(LogLine { source: LogSource::Stderr, line: l.clone() });
                            let size_bytes = parse_ffmpeg_size(&l).unwrap_or(last_size);
                            let time_secs = parse_ffmpeg_time(&l).unwrap_or(last_time_secs);
                            last_size = size_bytes;
                            last_time_secs = time_secs;

                            let speed = if time_secs > 0.0 {
                                Some((size_bytes as f64 / time_secs) as u64)
                            } else {
                                None
                            };

                            callback.on_progress(DownloadProgress::downloading(
                                last_size,
                                None,
                                speed,
                            ));
                        }
                        Ok(None) => break,
                        Err(e) => {
                            return Err(DownloadError::Internal(format!("ffmpeg read stderr: {}", e)));
                        }
                    }
                }
            }
        }

        let status = child
            .wait()
            .await
            .map_err(|e| DownloadError::Internal(format!("wait ffmpeg failed: {}", e)))?;

        if !status.success() {
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
