//! 资源捕捉相关的类型定义

use crate::filter::ResourceFilter;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// 资源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
    /// 视频资源（mp4, m3u8, flv, webm 等）
    Video,
    /// 音频资源（mp3, m4a, aac, ogg 等）
    Audio,
    /// 图片资源（jpg, png, gif, webp 等）
    Image,
    /// 文档资源（pdf, doc, txt 等）
    Document,
    /// 字体资源（woff, ttf, otf 等）
    Font,
    /// 样式表（css）
    Stylesheet,
    /// JavaScript 文件
    Script,
    /// 其他类型
    Other,
}

impl ResourceType {
    /// 从 MIME 类型推断资源类型
    ///
    /// 注意：此方法应该作为降级方案，优先使用 from_url/from_extension
    pub fn from_mime_type(mime: &str) -> Self {
        let mime_lower = mime.to_lowercase();

        // 流媒体 MIME 类型
        if mime_lower.contains("mpegurl") // m3u8 streams
            || mime_lower.contains("vnd.apple.mpegurl") // HLS
            || mime_lower.contains("x-mpegurl") // HLS alternative
            || mime_lower == "application/dash+xml"
        // DASH
        {
            return Self::Video;
        }

        // 标准 MIME 类型
        if mime_lower.starts_with("video/") {
            Self::Video
        } else if mime_lower.starts_with("audio/") {
            Self::Audio
        } else if mime_lower.starts_with("image/") {
            Self::Image
        } else if mime_lower == "application/pdf" {
            Self::Document
        } else if mime_lower.contains("font") || mime_lower.contains("woff") {
            Self::Font
        } else if mime_lower == "text/css" {
            Self::Stylesheet
        } else if mime_lower.contains("javascript") || mime_lower.contains("ecmascript") {
            Self::Script
        } else if mime_lower.starts_with("text/html") {
            Self::Document
        } else {
            // text/plain, text/xml 等都返回 Other
            Self::Other
        }
    }

    /// 从文件扩展名推断资源类型
    pub fn from_extension(ext: &str) -> Self {
        let ext_lower = ext.to_lowercase();
        match ext_lower.as_str() {
            // 视频
            "mp4" | "m4v" | "mov" | "avi" | "mkv" | "webm" | "flv" | "f4v" | "m3u8" | "m3u"
            | "ts" | "mpd" => Self::Video,
            // 音频
            "mp3" | "m4a" | "aac" | "ogg" | "oga" | "opus" | "wav" | "flac" | "mka" => Self::Audio,
            // 图片
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp" | "ico" | "avif" => Self::Image,
            // 文档
            "pdf" | "doc" | "docx" | "txt" | "rtf" | "odt" => Self::Document,
            // 字体
            "woff" | "woff2" | "ttf" | "otf" | "eot" => Self::Font,
            // 样式表
            "css" => Self::Stylesheet,
            // 脚本
            "js" | "mjs" | "cjs" => Self::Script,
            _ => Self::Other,
        }
    }

    /// 从 URL 推断资源类型
    pub fn from_url(url: &str) -> Self {
        // 先尝试从路径扩展名判断
        if let Some(path) = url.split('?').next() {
            if let Some(ext) = path.split('.').last() {
                let resource_type = Self::from_extension(ext);
                if resource_type != Self::Other {
                    return resource_type;
                }
            }
        }

        // 特殊处理：m3u8 流
        if url.contains(".m3u8") || url.contains(".m3u") {
            return Self::Video;
        }

        // 特殊处理：mpd 流
        if url.contains(".mpd") {
            return Self::Video;
        }

        Self::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_m3u8_mime_recognition() {
        // 测试各种 m3u8 MIME 类型
        assert_eq!(
            ResourceType::from_mime_type("application/vnd.apple.mpegurl"),
            ResourceType::Video,
            "应该识别 application/vnd.apple.mpegurl 为视频"
        );
        assert_eq!(
            ResourceType::from_mime_type("application/x-mpegurl"),
            ResourceType::Video,
            "应该识别 application/x-mpegurl 为视频"
        );
        assert_eq!(
            ResourceType::from_mime_type("application/mpegurl"),
            ResourceType::Video,
            "应该识别 application/mpegurl 为视频"
        );
        assert_eq!(
            ResourceType::from_mime_type("application/vnd.apple.mpegURL"),
            ResourceType::Video,
            "应该识别大小写混合的 MIME 为视频"
        );
    }

    #[test]
    fn test_m3u8_url_recognition() {
        // 测试各种 m3u8 URL
        let test_cases = vec![
            "https://example.com/video.m3u8",
            "https://example.com/stream.m3u8?token=abc",
            "https://example.com/path/to/video.m3u",
            "https://cdn.example.com/hls/stream.m3u8",
            "https://example.com/playlist.m3u8#segment",
        ];

        for url in test_cases {
            assert_eq!(
                ResourceType::from_url(url),
                ResourceType::Video,
                "应该识别 {} 为视频",
                url
            );
        }
    }

    #[test]
    fn test_common_mime_types() {
        // 测试其他常见类型不受影响
        assert_eq!(
            ResourceType::from_mime_type("video/mp4"),
            ResourceType::Video
        );
        assert_eq!(
            ResourceType::from_mime_type("image/png"),
            ResourceType::Image
        );
        assert_eq!(
            ResourceType::from_mime_type("audio/mpeg"),
            ResourceType::Audio
        );
        assert_eq!(
            ResourceType::from_mime_type("text/html"),
            ResourceType::Document
        );
        // text/plain 应该是 Other，不是 Document
        assert_eq!(
            ResourceType::from_mime_type("text/plain"),
            ResourceType::Other,
            "text/plain 应该是 Other，避免 m3u8 误判"
        );
    }

    #[test]
    fn test_extension_recognition() {
        assert_eq!(ResourceType::from_extension("m3u8"), ResourceType::Video);
        assert_eq!(ResourceType::from_extension("m3u"), ResourceType::Video);
        assert_eq!(ResourceType::from_extension("ts"), ResourceType::Video);
        assert_eq!(ResourceType::from_extension("mp4"), ResourceType::Video);
        assert_eq!(ResourceType::from_extension("mp3"), ResourceType::Audio);
        assert_eq!(ResourceType::from_extension("png"), ResourceType::Image);
    }
}

/// 捕捉到的资源
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapturedResource {
    /// 资源 URL
    pub url: String,
    /// 资源类型
    pub resource_type: ResourceType,
    /// MIME 类型（如果已知）
    pub mime_type: Option<String>,
    /// Referer（来源页面）
    pub referer: Option<String>,
    /// 标题/描述（如果可用）
    pub title: Option<String>,
    /// 估算大小（字节，如果可用）
    pub size_bytes: Option<u64>,
    /// 估算时长（秒，仅对视频/音频有效）
    pub duration_seconds: Option<f64>,
    /// 请求头（用于下载时透传）
    pub headers: Option<Vec<(String, String)>>,
}

/// 抓取请求配置
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    /// 目标页面 URL
    pub target_url: String,
    /// 自定义浏览器路径（优先级最高）
    pub custom_browser_path: Option<PathBuf>,
    /// 是否启用无头模式
    pub headless: bool,
    /// 抓取超时（仅用于静态扫描，CDP 监听会持续运行直到手动取消）
    pub timeout: Duration,
    /// 资源筛选器
    pub filter: ResourceFilter,
    /// 是否自动加速播放广告（检测到广告时）
    pub auto_speedup_ads: bool,
    /// 加速播放的倍速（1.0-16.0，默认 2.0）
    pub speedup_rate: f64,
}

impl Default for CaptureRequest {
    fn default() -> Self {
        Self {
            target_url: String::new(),
            custom_browser_path: None,
            headless: true,
            timeout: Duration::from_secs(30),
            filter: ResourceFilter::all(),
            auto_speedup_ads: false,
            speedup_rate: 2.0,
        }
    }
}

/// 抓取事件
#[derive(Debug, Clone)]
pub enum CaptureEvent {
    /// 日志消息
    Log(String),
    /// 发现资源
    Found(CapturedResource),
    /// 完成
    Finished,
    /// 错误
    Error(String),
}

/// 抓取会话句柄
pub struct CaptureSession {
    /// 事件接收器（使用 ManuallyDrop 以支持 split）
    rx: std::mem::ManuallyDrop<tokio::sync::mpsc::Receiver<CaptureEvent>>,
    /// 取消发送器（内部使用，使用 Arc 以支持 Drop）
    cancel_tx: std::sync::Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl CaptureSession {
    /// 创建新的会话（内部使用）
    pub(crate) fn new(
        rx: tokio::sync::mpsc::Receiver<CaptureEvent>,
        cancel_tx: tokio::sync::oneshot::Sender<()>,
    ) -> Self {
        Self {
            rx: std::mem::ManuallyDrop::new(rx),
            cancel_tx: std::sync::Arc::new(std::sync::Mutex::new(Some(cancel_tx))),
        }
    }

    /// 获取事件接收器的可变引用
    pub fn rx_mut(&mut self) -> &mut tokio::sync::mpsc::Receiver<CaptureEvent> {
        &mut self.rx
    }
    /// 取消抓取任务
    pub fn cancel(&mut self) {
        if let Ok(mut guard) = self.cancel_tx.lock() {
            if let Some(tx) = guard.take() {
                let _ = tx.send(());
            }
        }
    }

    /// 分离接收器和取消器（用于内部实现）
    pub fn split(
        mut self,
    ) -> (
        tokio::sync::mpsc::Receiver<CaptureEvent>,
        Option<tokio::sync::oneshot::Sender<()>>,
    ) {
        let cancel_tx = if let Ok(mut guard) = self.cancel_tx.lock() {
            guard.take()
        } else {
            None
        };
        // 安全地 move 出 ManuallyDrop 的值
        let rx = unsafe { std::mem::ManuallyDrop::take(&mut self.rx) };
        // 阻止 Drop 被调用（因为我们已经手动处理了）
        std::mem::forget(self);
        (rx, cancel_tx)
    }
}

/// 自动清理：当 CaptureSession 被销毁时，自动关闭浏览器进程
impl Drop for CaptureSession {
    fn drop(&mut self) {
        // 发送取消信号，触发浏览器进程清理
        if let Ok(mut guard) = self.cancel_tx.lock() {
            if let Some(tx) = guard.take() {
                let _ = tx.send(());
                tracing::info!("🧹 CaptureSession 销毁，已发送浏览器清理信号");
            }
        }
    }
}
