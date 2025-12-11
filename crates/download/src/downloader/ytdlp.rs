use std::path::PathBuf;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::config::DownloadRequest;
use crate::error::{DownloadError, DownloadResult};
use crate::progress::{DownloadCallback, DownloadOutcome, DownloadProgress, LogLine, LogSource};

/// yt-dlp 下载器（基础实现：构建命令、解析 stdout 进度、stderr 日志）
pub struct YtDlpDownloader;

impl Default for YtDlpDownloader {
    fn default() -> Self {
        Self
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
        callback.on_progress(DownloadProgress::preparing());

        let output_template = build_output_template(&request)?;
        if let Some(dir) = output_template.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }

        // 组装命令
        let mut cmd = Command::new("yt-dlp");
        cmd.arg(request.url.to_string())
            .arg("-o")
            .arg(output_template.to_string_lossy().to_string())
            .arg("--newline")
            .arg("--progress")
            .arg("--no-mtime");

        // headers
        for (k, v) in &request.extra.headers {
            cmd.arg("--add-header").arg(format!("{}: {}", k, v));
        }
        if let Some(cookie) = &request.extra.cookie {
            cmd.arg("--add-header").arg(format!("Cookie: {}", cookie));
        }

        // 透传自定义参数（key = "ytdlp"）
        if let Some(args) = request.extra.tool_args.get("ytdlp") {
            cmd.args(args);
        }

        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| DownloadError::Internal(format!("spawn yt-dlp failed: {}", e)))?;

        let mut stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| DownloadError::Internal("yt-dlp stdout missing".into()))?,
        )
        .lines();
        let mut stderr = BufReader::new(
            child
                .stderr
                .take()
                .ok_or_else(|| DownloadError::Internal("yt-dlp stderr missing".into()))?,
        )
        .lines();

        let mut last_total: Option<u64> = None;
        let mut last_downloaded: u64 = 0;
        let mut stderr_buf = String::new();
        let mut detected_output: Option<PathBuf> = None;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = child.kill().await;
                    return Err(DownloadError::Canceled);
                }
                line = stdout.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            callback.on_log(LogLine { source: LogSource::Stdout, line: line.clone() });
                            if let Some(path) = parse_destination(&line) {
                                detected_output = Some(path);
                            }
                            if let Some((downloaded, total, speed)) = parse_progress_line(&line) {
                                last_total = total.or(last_total);
                                if let Some(d) = downloaded {
                                    last_downloaded = d;
                                }
                                callback.on_progress(DownloadProgress::downloading(
                                    last_downloaded,
                                    last_total,
                                    speed,
                                ));
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            callback.on_log(LogLine { source: LogSource::Stdout, line: format!("read stdout error: {}", e) });
                            break;
                        }
                    }
                }
                line = stderr.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            stderr_buf.push_str(&line);
                            stderr_buf.push('\n');
                            callback.on_log(LogLine { source: LogSource::Stderr, line });
                        }
                        Ok(None) => {}
                        Err(e) => {
                            stderr_buf.push_str(&format!("read stderr error: {}\n", e));
                        }
                    }
                }
            }
        }

        let status = child.wait().await.map_err(|e| {
            DownloadError::Internal(format!("wait yt-dlp failed: {}", e))
        })?;
        if !status.success() {
            return Err(DownloadError::ProcessExit {
                code: status.code(),
                stderr: stderr_buf,
            });
        }

        let final_output = detected_output.unwrap_or(output_template.clone());
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

/// 构建输出模板路径
fn build_output_template(request: &DownloadRequest) -> DownloadResult<PathBuf> {
    let tmpl = request
        .output
        .template
        .clone()
        .unwrap_or_else(|| "%(title)s.%(ext)s".to_string());
    Ok(request.output.directory.join(tmpl))
}

/// 解析 yt-dlp 进度行
/// 例：[download]  45.2% of 12.34MiB at 1.23MiB/s ETA 00:05
pub fn parse_progress_line(line: &str) -> Option<(Option<u64>, Option<u64>, Option<u64>)> {
    if !line.contains("[download]") {
        return None;
    }
    let tokens: Vec<&str> = line.split_whitespace().collect();

    // 百分比
    let mut percent: Option<f64> = None;
    for t in &tokens {
        if let Some(stripped) = t.strip_suffix('%') {
            percent = stripped.parse::<f64>().ok();
            break;
        }
    }

    // total size（在 "of" 之后）
    let mut total_bytes: Option<u64> = None;
    for w in tokens.windows(2) {
        if w[0] == "of" {
            let raw = w[1].trim_start_matches('~');
            total_bytes = parse_size_to_bytes(raw);
            break;
        }
    }

    // speed（在 "at" 之后）
    let mut speed: Option<u64> = None;
    for w in tokens.windows(2) {
        if w[0] == "at" {
            let raw = w[1].trim_end_matches("/s");
            speed = parse_size_to_bytes(raw);
            break;
        }
    }

    let downloaded = if let (Some(p), Some(total)) = (percent, total_bytes) {
        Some(((p / 100.0) * total as f64) as u64)
    } else {
        None
    };

    Some((downloaded, total_bytes, speed))
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

fn parse_size_to_bytes(s: &str) -> Option<u64> {
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
            if let Ok(v) = num.trim().parse::<f64>() {
                return Some((v * m as f64) as u64);
            }
        }
    }
    None
}

