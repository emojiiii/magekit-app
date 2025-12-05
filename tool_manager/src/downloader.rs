use crate::error::{DownloadError, DownloadResult, ToolManagerError};
use magekit_shared::{VideoFormat, VideoInfo, DownloadOptions, TaskId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::{Command, Child};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

/// 视频下载器
#[derive(Clone)]
pub struct VideoDownloader {
    yt_dlp_path: PathBuf,
    ffmpeg_path: Option<PathBuf>,
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
    pub async fn get_video_info(&self, url: &str) -> DownloadResult<VideoInfo> {
        tracing::info!("Getting video info for URL: {}", url);

        let output = Command::new(&self.yt_dlp_path)
            .arg("--dump-json")
            .arg("--no-download")
            .arg(url)
            .output()
            .await
            .map_err(|e| DownloadError::internal(
                format!("yt-dlp --dump-json failed: {}", e),
            ))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DownloadError::extraction_failed(
                url,
                stderr.to_string(),
            ));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let video_data: VideoInfoData = serde_json::from_str(&json_str)
            .map_err(|e| DownloadError::extraction_failed(
                url,
                format!("Failed to parse JSON: {}", e),
            ))?;

        Ok(video_data.into())
    }

    /// 开始下载视频
    pub async fn start_download(
        &self,
        task_id: TaskId,
        url: &str,
        options: DownloadOptions,
        progress_tx: mpsc::Sender<DownloadProgress>,
    ) -> DownloadResult<PathBuf> {
        tracing::info!("Starting download for task {}: {}", task_id, url);

        let mut cmd = Command::new(&self.yt_dlp_path);

        // 基本参数
        cmd.arg(url)
           .arg("--format")
           .arg(&options.format_id)
           .arg("--output")
           .arg(options.output_path.join(
               options.output_template.as_deref().unwrap_or("%(title)s.%(ext)s")
           ).to_string_lossy().as_ref());

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

        // 使用FFmpeg进行后处理
        if let Some(ffmpeg_path) = &self.ffmpeg_path {
            cmd.env("FFMPEG", ffmpeg_path);
        }

        // 创建子进程
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| DownloadError::internal(
                format!("yt-dlp download failed: {}", e),
            ))?;

        // 发送开始事件
        let _ = progress_tx.send(DownloadProgress::Started {
            task_id,
            url: url.to_string(),
        }).await;

        // 监控下载进度
        let output_path = self.monitor_download_progress(task_id, &mut child, &progress_tx).await?;

        tracing::info!("Download completed for task {}: {:?}", task_id, output_path);
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

        let stderr = child.stderr.take().ok_or_else(|| {
            DownloadError::internal("Failed to capture stderr".to_string())
        })?;
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();

        let mut output_path: Option<PathBuf> = None;

        loop {
            let bytes_read = reader.read_line(&mut line).await
                .map_err(|e| DownloadError::internal(format!("Failed to read stderr: {}", e)))?;

            if bytes_read == 0 {
                break;
            }

            // 解析下载进度信息
            if let Some(progress) = self.parse_progress_line(&line) {
                let _ = progress_tx.send(DownloadProgress::Progress {
                    task_id,
                    percent: progress.percent,
                    speed: progress.speed,
                    eta: progress.eta,
                }).await;
            }

            // 解析输出文件路径
            if line.contains("[download] Destination:") {
                if let Some(path) = self.extract_output_path(&line) {
                    output_path = Some(path);
                }
            }

            line.clear();
        }

        // 等待进程完成
        let status = child.wait().await
            .map_err(|e| DownloadError::internal(
                format!("wait for yt-dlp failed: {}", e),
            ))?;

        if !status.success() {
            return Err(DownloadError::download_failed(
                "unknown",
                format!("Process exited with code: {:?}", status.code()),
            ));
        }

        output_path.ok_or_else(|| {
            DownloadError::internal("Could not determine output file path".to_string())
        })
    }

    /// 解析进度行
    fn parse_progress_line(&self, line: &str) -> Option<ProgressInfo> {
        // yt-dlp 进度格式示例:
        // [download]   5.2% of 125.45MiB at 512.34KiB/s ETA 02:15
        if !line.contains("[download]") {
            return None;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            return None;
        }

        let percent_str = parts[1].trim_end_matches('%');
        let percent = percent_str.parse::<f64>().ok()? as f32;

        let speed = if parts[6] == "at" && parts.len() > 7 {
            self.parse_speed(parts[7])
        } else {
            None
        };

        let eta = if parts.len() > 8 && parts[8] == "ETA" && parts.len() > 9 {
            self.parse_eta(parts[9])
        } else {
            None
        };

        Some(ProgressInfo {
            percent,
            speed,
            eta,
        })
    }

    /// 解析速度 (如 512.34KiB/s)
    fn parse_speed(&self, speed_str: &str) -> Option<u64> {
        let speed_str = speed_str.trim_end_matches("/s");
        let (num_str, unit) = speed_str.split_at(speed_str.len().saturating_sub(2));

        let num: f64 = num_str.parse().ok()?;
        let multiplier = match unit {
            "B" => 1.0,
            "KB" | "KiB" => 1024.0,
            "MB" | "MiB" => 1024.0 * 1024.0,
            "GB" | "GiB" => 1024.0 * 1024.0 * 1024.0,
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
        // 格式: [download] Destination: /path/to/file.mp4
        if let Some(start) = line.find("[download] Destination:") {
            let path_part = &line[start + 23..]; // "[download] Destination:" 的长度
            Some(PathBuf::from(path_part.trim()))
        } else {
            None
        }
    }
}

/// 下载进度信息
#[derive(Debug)]
struct ProgressInfo {
    percent: f32,
    speed: Option<u64>,
    eta: Option<Duration>,
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

/// yt-dlp JSON输出的数据结构
#[derive(Debug, Deserialize)]
struct VideoInfoData {
    id: String,
    title: String,
    description: Option<String>,
    duration: Option<f64>,
    uploader: Option<String>,
    upload_date: Option<String>,
    thumbnail: Option<String>,
    formats: Vec<FormatData>,
    webpage_url: String,
}

#[derive(Debug, Deserialize)]
struct FormatData {
    format_id: String,
    ext: Option<String>,
    resolution: Option<String>,
    fps: Option<f32>,
    filesize: Option<u64>,
    vcodec: Option<String>,
    acodec: Option<String>,
    quality: Option<String>,
}

impl From<VideoInfoData> for VideoInfo {
    fn from(data: VideoInfoData) -> Self {
        Self {
            id: data.id,
            title: data.title,
            description: data.description,
            duration: data.duration.map(Duration::from_secs_f64),
            uploader: data.uploader,
            upload_date: data.upload_date,
            thumbnail: data.thumbnail,
            formats: data.formats.into_iter().map(|f| VideoFormat {
                format_id: f.format_id,
                ext: f.ext.unwrap_or_else(|| "unknown".to_string()),
                resolution: f.resolution,
                fps: f.fps,
                filesize: f.filesize,
                vcodec: f.vcodec,
                acodec: f.acodec,
                quality: f.quality,
            }).collect(),
            url: data.webpage_url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_progress_line() {
        let downloader = VideoDownloader::new(
            PathBuf::from("/usr/bin/yt-dlp"),
            None,
        );

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
        let downloader = VideoDownloader::new(
            PathBuf::from("/usr/bin/yt-dlp"),
            None,
        );

        let line = "[download] Destination: /path/to/video.mp4";
        let path = downloader.extract_output_path(line);

        assert!(path.is_some());
        assert_eq!(path.unwrap(), PathBuf::from("/path/to/video.mp4"));
    }
}