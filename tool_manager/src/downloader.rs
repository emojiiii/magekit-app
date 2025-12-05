use crate::error::{DownloadError, DownloadResult};
use magekit_shared::{VideoFormat, VideoInfo, DownloadOptions, TaskId};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::{Command, Child};
use tokio::sync::mpsc;
use tokio::time::Duration;

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
        tracing::info!("🔍 获取视频信息，URL: {}", url);

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
            tracing::error!("❌ yt-dlp 获取视频信息失败: {}", stderr);
            return Err(DownloadError::extraction_failed(
                url,
                stderr.to_string(),
            ));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        tracing::debug!("📄 yt-dlp 返回 JSON 长度: {} 字节", json_str.len());
        
        let video_data: VideoInfoData = serde_json::from_str(&json_str)
            .map_err(|e| {
                tracing::error!("❌ JSON 解析失败: {}", e);
                DownloadError::extraction_failed(
                    url,
                    format!("Failed to parse JSON: {}", e),
                )
            })?;

        tracing::info!("✅ 视频信息获取成功: {} - {}", video_data.id, video_data.title);
        tracing::debug!("📊 格式数量: {}, 时长: {:?}秒", video_data.formats.len(), video_data.duration);
        
        // 打印所有格式的详细信息用于调试
        for (i, fmt) in video_data.formats.iter().enumerate() {
            tracing::info!("  格式[{}]: id={}, ext={:?}, res={:?}, vcodec={:?}, acodec={:?}", 
                i, fmt.format_id, fmt.ext, fmt.resolution, fmt.vcodec, fmt.acodec);
        }
        
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
        tracing::info!("📥 开始下载任务 {}", task_id);
        tracing::info!("  URL: {}", url);
        tracing::info!("  输出目录: {:?}", options.output_path);
        tracing::info!("  格式: {}", options.format_id);

        let mut cmd = Command::new(&self.yt_dlp_path);

        // 基本参数
        let output_template = options.output_path.join(
            options.output_template.as_deref().unwrap_or("%(title)s.%(ext)s")
        );
        
        cmd.arg(url)
           .arg("--format")
           .arg(&options.format_id)
           .arg("--output")
           .arg(output_template.to_string_lossy().as_ref())
           // 添加进度相关参数
           .arg("--newline")  // 确保每行进度都输出
           .arg("--progress")  // 显示进度条
           .arg("--print").arg("after_move:filepath");  // 最终文件路径输出到 stdout
        
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

        // 同时读取 stdout 和 stderr
        let stdout = child.stdout.take().ok_or_else(|| {
            DownloadError::internal("Failed to capture stdout".to_string())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            DownloadError::internal("Failed to capture stderr".to_string())
        })?;
        
        let mut stdout_reader = BufReader::new(stdout);
        let mut stderr_reader = BufReader::new(stderr);
        
        let mut stdout_line = String::new();
        let mut stderr_line = String::new();

        let mut output_path: Option<PathBuf> = None;
        let mut final_path_from_print: Option<PathBuf> = None;

        // 使用 select 同时读取两个流
        loop {
            tokio::select! {
                result = stdout_reader.read_line(&mut stdout_line) => {
                    match result {
                        Ok(0) => {}, // stdout EOF
                        Ok(_) => {
                            let trimmed = stdout_line.trim();
                            if !trimmed.is_empty() {
                                tracing::debug!("[yt-dlp stdout] {}", trimmed);
                                
                                // --print after_move:filepath 的输出会在 stdout
                                // 这通常是最终的文件路径
                                if !trimmed.starts_with('[') && !trimmed.contains('%') {
                                    let potential_path = PathBuf::from(trimmed);
                                    // 检查是否像文件路径
                                    if trimmed.contains('/') || trimmed.contains('.') {
                                        tracing::info!("📁 从 stdout 检测到文件路径: {:?}", potential_path);
                                        final_path_from_print = Some(potential_path);
                                    }
                                }
                            }
                            stdout_line.clear();
                        }
                        Err(e) => {
                            tracing::warn!("Error reading stdout: {}", e);
                        }
                    }
                }
                result = stderr_reader.read_line(&mut stderr_line) => {
                    match result {
                        Ok(0) => break, // stderr EOF, 进程可能结束
                        Ok(_) => {
                            let trimmed = stderr_line.trim();
                            if !trimmed.is_empty() {
                                tracing::debug!("[yt-dlp stderr] {}", trimmed);
                            }

                            // 解析下载进度信息
                            if let Some(progress) = self.parse_progress_line(&stderr_line) {
                                let _ = progress_tx.send(DownloadProgress::Progress {
                                    task_id,
                                    percent: progress.percent,
                                    speed: progress.speed,
                                    eta: progress.eta,
                                }).await;
                            }

                            // 解析输出文件路径 - 支持多种格式
                            if let Some(path) = self.extract_output_path(&stderr_line) {
                                tracing::info!("📁 从 stderr 检测到输出文件: {:?}", path);
                                output_path = Some(path);
                            }

                            stderr_line.clear();
                        }
                        Err(e) => {
                            tracing::warn!("Error reading stderr: {}", e);
                        }
                    }
                }
            }
        }

        // 等待进程完成
        let status = child.wait().await
            .map_err(|e| DownloadError::internal(
                format!("wait for yt-dlp failed: {}", e),
            ))?;

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
            Some(path) => tracing::info!("✅ 下载完成，输出文件: {:?}", path),
            None => tracing::warn!("⚠️ 下载完成但未检测到输出文件路径"),
        }

        final_path.ok_or_else(|| {
            DownloadError::internal("无法确定输出文件路径".to_string())
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
    #[serde(default)]
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
    #[serde(default, deserialize_with = "deserialize_quality")]
    quality: Option<String>,
}

/// 自定义反序列化器，处理 quality 字段可能是数字或字符串的情况
fn deserialize_quality<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    
    struct QualityVisitor;
    
    impl<'de> Visitor<'de> for QualityVisitor {
        type Value = Option<String>;
        
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string, number, or null")
        }
        
        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }
        
        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }
        
        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(QualityVisitor)
        }
        
        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(value.to_string()))
        }
        
        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(value))
        }
        
        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(value.to_string()))
        }
        
        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(value.to_string()))
        }
        
        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(value.to_string()))
        }
    }
    
    deserializer.deserialize_option(QualityVisitor)
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