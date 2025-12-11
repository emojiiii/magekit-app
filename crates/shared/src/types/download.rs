//! 下载相关的数据模型。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// 视频/音频格式信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoFormat {
    /// yt-dlp 提供的 `format_id`。
    pub format_id: String,
    /// 文件扩展名（如 `mp4`、`webm`、`m4a`）。
    pub ext: String,
    /// 分辨率描述（如 `1080p`）。
    pub resolution: Option<String>,
    /// 帧率，单位 FPS。
    pub fps: Option<f32>,
    /// 预计文件大小，单位字节。
    pub filesize: Option<u64>,
    /// 视频编码（如 `h264`、`hevc`）。
    pub vcodec: Option<String>,
    /// 音频编码（如 `aac`、`opus`）。
    pub acodec: Option<String>,
    /// 站点定义的质量标签。
    pub quality: Option<String>,
    /// 直链下载地址（自定义解析使用），yt-dlp 解析时通常为空。
    #[serde(default)]
    pub download_url: Option<String>,
}

impl VideoFormat {
    /// 以人类可读方式格式化文件大小。
    pub fn format_filesize(&self) -> String {
        match self.filesize {
            Some(bytes) => {
                const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
                let mut size = bytes as f64;
                let mut unit_index = 0;

                while size >= 1024.0 && unit_index < UNITS.len() - 1 {
                    size /= 1024.0;
                    unit_index += 1;
                }

                format!("{:.1} {}", size, UNITS[unit_index])
            }
            None => "未知".to_string(),
        }
    }
}

/// 视频元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    /// 视频 ID。
    pub id: String,
    /// 标题。
    pub title: String,
    /// 描述。
    pub description: Option<String>,
    /// 时长。
    pub duration: Option<Duration>,
    /// 上传者。
    pub uploader: Option<String>,
    /// 上传日期（yyyyMMdd）。
    pub upload_date: Option<String>,
    /// 缩略图地址。
    pub thumbnail: Option<String>,
    /// 可用格式列表。
    pub formats: Vec<VideoFormat>,
    /// 原始 URL。
    pub url: String,
}

/// 下载选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadOptions {
    /// 选择的 format_id，默认 `best`。
    pub format_id: String,
    /// 输出目录。
    pub output_path: PathBuf,
    /// 输出模板（如 `%(title)s.%(ext)s`）。
    pub output_template: Option<String>,
    /// 是否写入元数据。
    pub embed_metadata: bool,
    /// 是否嵌入缩略图。
    pub embed_thumbnail: bool,
    /// 是否仅提取音频。
    pub extract_audio: bool,
    /// 音频格式（如 `mp3`、`m4a`）。
    pub audio_format: Option<String>,
    /// 字幕语言列表。
    pub subtitle_langs: Vec<String>,
    /// 是否嵌入字幕。
    pub embed_subs: bool,
    /// 是否写入字幕文件。
    pub write_subs: bool,
    /// 是否写入自动字幕。
    pub write_auto_subs: bool,
    /// 自研解析时的直链下载地址。
    #[serde(default)]
    pub download_url: Option<String>,
    /// 使用 ffmpeg 拉流的 m3u8 直链。
    #[serde(default)]
    pub ffmpeg_url: Option<String>,
    /// ffmpeg 直链时附加的自定义参数。
    #[serde(default)]
    pub ffmpeg_args: Vec<String>,
    /// 任务标题（用于任务列表显示/命名）。
    #[serde(default)]
    pub task_title: Option<String>,
}

impl Default for DownloadOptions {
    /// 构造带有合理默认值的下载选项。
    fn default() -> Self {
        Self {
            format_id: "best".to_string(),
            output_path: dirs::download_dir().unwrap_or_else(|| PathBuf::from("./downloads")),
            output_template: Some("%(title)s.%(ext)s".to_string()),
            embed_metadata: true,
            embed_thumbnail: false,
            extract_audio: false,
            audio_format: None,
            subtitle_langs: Vec::new(),
            embed_subs: false,
            write_subs: false,
            write_auto_subs: false,
            download_url: None,
            ffmpeg_url: None,
            ffmpeg_args: Vec::new(),
            task_title: None,
        }
    }
}

/// 下载任务参数（用于暂停后恢复）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadParams {
    /// 输出目录。
    pub output_dir: PathBuf,
    /// 选择的格式 ID。
    pub format_id: String,
    /// 是否嵌入元数据。
    pub embed_metadata: bool,
    /// 是否嵌入缩略图。
    pub embed_thumbnail: bool,
    /// 是否下载字幕。
    pub download_subtitles: bool,
    /// 是否仅音频。
    pub audio_only: bool,
}
