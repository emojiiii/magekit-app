use std::path::PathBuf;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::config::DownloadRequest;
use crate::error::{DownloadError, DownloadResult};
use crate::progress::{DownloadCallback, DownloadOutcome, DownloadProgress, LogLine, LogSource};

/// yt-dlp 下载器（基础实现：构建命令、解析 stdout 进度、stderr 日志）
pub struct YtDlpDownloader {
    /// yt-dlp 可执行文件路径
    ytdlp_path: PathBuf,
}

impl YtDlpDownloader {
    pub fn new(ytdlp_path: PathBuf) -> Self {
        Self { ytdlp_path }
    }
}

impl Default for YtDlpDownloader {
    fn default() -> Self {
        Self {
            ytdlp_path: PathBuf::from("yt-dlp"),
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
        let mut child = cmd
            .spawn()
            .map_err(|e| {
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
                            if let Some(path) = parse_destination(&line) {
                                tracing::info!("📁 检测到输出路径: {:?}", path);
                                detected_output = Some(path);
                            }
                            if let Some((downloaded, total, speed)) = parse_progress_line(&line) {
                                last_total = total.or(last_total);
                                if let Some(d) = downloaded {
                                    last_downloaded = d;
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
                            if line.contains("error") || line.contains("Error") || line.contains("WARNING") {
                                tracing::warn!("[yt-dlp stderr] {}", line.trim());
                            } else {
                                tracing::info!("[yt-dlp stderr] {}", line.trim());
                            }
                            callback.on_log(LogLine { source: LogSource::Stderr, line });
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

        let final_output = detected_output.unwrap_or(output_template.clone());
        tracing::info!("✅ 下载完成，输出文件: {:?}", final_output);
        tracing::info!("📊 最终统计: downloaded={}, total={:?}", last_downloaded, last_total);

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
