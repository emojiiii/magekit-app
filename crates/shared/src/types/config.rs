//! 应用配置相关的数据模型。

use crate::types::platform::{LiveRecordConfig, MonitoredRoom, PlatformCookie};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// 应用配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 下载配置。
    pub download: DownloadConfig,
    /// 工具配置。
    pub tools: ToolsConfig,
    /// UI 配置。
    pub ui: UiConfig,
    /// 高级配置。
    pub advanced: AdvancedConfig,
    /// 直播录制配置。
    #[serde(default)]
    pub live_record: LiveRecordConfig,
    /// 监控的直播间列表。
    #[serde(default)]
    pub monitored_rooms: Vec<MonitoredRoom>,
}

impl Default for AppConfig {
    /// 构造包含默认值的配置。
    fn default() -> Self {
        Self {
            download: DownloadConfig::default(),
            tools: ToolsConfig::default(),
            ui: UiConfig::default(),
            advanced: AdvancedConfig::default(),
            live_record: LiveRecordConfig::default(),
            monitored_rooms: Vec::new(),
        }
    }
}

/// 下载配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadConfig {
    /// 默认输出目录。
    pub default_output_path: PathBuf,
    /// 最大并发下载数。
    pub max_concurrent_downloads: usize,
    /// 默认格式（yt-dlp `format` 字符串）。
    pub default_format: String,
    /// 是否嵌入元数据。
    pub embed_metadata: bool,
    /// 是否嵌入缩略图。
    pub embed_thumbnail: bool,
    /// 是否自动提取音频。
    pub auto_extract_audio: bool,
    /// 首选音频格式。
    pub preferred_audio_format: Option<String>,
    /// 字幕语言列表。
    pub subtitle_languages: Vec<String>,
}

impl Default for DownloadConfig {
    /// 构造下载默认配置。
    fn default() -> Self {
        Self {
            default_output_path: dirs::download_dir()
                .unwrap_or_else(|| PathBuf::from("./downloads")),
            max_concurrent_downloads: 3,
            default_format: "best".to_string(),
            embed_metadata: true,
            embed_thumbnail: false,
            auto_extract_audio: false,
            preferred_audio_format: Some("mp3".to_string()),
            subtitle_languages: vec!["en".to_string(), "zh".to_string()],
        }
    }
}

/// 工具配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    /// 是否自动更新。
    pub auto_update: bool,
    /// 更新通道。
    pub update_channel: UpdateChannel,
    /// 自定义 yt-dlp 版本。
    pub yt_dlp_version: Option<String>,
    /// 自定义 ffmpeg 版本。
    pub ffmpeg_version: Option<String>,
    /// 自定义 yt-dlp 路径。
    pub custom_yt_dlp_path: Option<PathBuf>,
    /// 自定义 ffmpeg 路径。
    pub custom_ffmpeg_path: Option<PathBuf>,
}

impl Default for ToolsConfig {
    /// 构造工具配置默认值。
    fn default() -> Self {
        Self {
            auto_update: true,
            update_channel: UpdateChannel::Stable,
            yt_dlp_version: None,
            ffmpeg_version: None,
            custom_yt_dlp_path: None,
            custom_ffmpeg_path: None,
        }
    }
}

/// 更新通道。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UpdateChannel {
    /// 稳定版。
    Stable,
    /// 每日构建。
    Nightly,
    /// 自定义通道标识。
    Custom(String),
}

/// UI 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// 主题。
    pub theme: Theme,
    /// 语言（如 `en`/`zh`）。
    pub language: String,
    /// 窗口状态。
    pub window_state: Option<WindowState>,
    /// 是否显示系统通知。
    pub show_notifications: bool,
    /// 关闭时是否最小化到托盘。
    pub minimize_to_tray: bool,
}

impl Default for UiConfig {
    /// 构造默认 UI 配置。
    fn default() -> Self {
        Self {
            theme: Theme::System,
            language: "en".to_string(),
            window_state: None,
            show_notifications: true,
            minimize_to_tray: false,
        }
    }
}

/// 主题设置（新版本）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeConfig {
    /// 主题名称（如 `Default Light`）。
    pub name: String,
    /// 主题模式（明/暗）。
    pub mode: ThemeMode,
}

/// 主题模式。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ThemeMode {
    /// 明色模式。
    #[default]
    Light,
    /// 暗色模式。
    Dark,
}

impl Default for ThemeConfig {
    /// 构造默认主题配置。
    fn default() -> Self {
        Self {
            name: "Default Light".to_string(),
            mode: ThemeMode::Light,
        }
    }
}

/// 旧版主题枚举（兼容）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Theme {
    /// 强制亮色。
    Light,
    /// 强制暗色。
    Dark,
    /// 跟随系统。
    System,
    /// 使用新版主题配置。
    Custom(ThemeConfig),
}

impl Default for Theme {
    /// 构造默认主题（使用自定义配置）。
    fn default() -> Self {
        Theme::Custom(ThemeConfig::default())
    }
}

impl Theme {
    /// 获取主题名称。
    pub fn theme_name(&self) -> String {
        match self {
            Theme::Light => "Default Light".to_string(),
            Theme::Dark => "Default Dark".to_string(),
            Theme::System => "Default Light".to_string(),
            Theme::Custom(config) => config.name.clone(),
        }
    }

    /// 是否为暗色模式。
    pub fn is_dark(&self) -> bool {
        match self {
            Theme::Light => false,
            Theme::Dark => true,
            Theme::System => false,
            Theme::Custom(config) => matches!(config.mode, ThemeMode::Dark),
        }
    }
}

/// 窗口状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    /// 宽度。
    pub width: u32,
    /// 高度。
    pub height: u32,
    /// X 坐标。
    pub x: i32,
    /// Y 坐标。
    pub y: i32,
    /// 是否最大化。
    pub maximized: bool,
}

/// 高级配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedConfig {
    /// 日志级别。
    pub log_level: LogLevel,
    /// 代理配置。
    pub proxy: Option<ProxyConfig>,
    /// 速度限制（B/s）。
    pub speed_limit: Option<u64>,
    /// 重试次数。
    pub retry_times: u32,
    /// 超时。
    pub timeout: Duration,
    /// 平台 Cookie 配置列表。
    #[serde(default)]
    pub cookies: Vec<PlatformCookie>,
}

impl Default for AdvancedConfig {
    /// 构造默认高级配置。
    fn default() -> Self {
        Self {
            log_level: LogLevel::Info,
            proxy: None,
            speed_limit: None,
            retry_times: 3,
            timeout: Duration::from_secs(30),
            cookies: Vec::new(),
        }
    }
}

/// 日志级别。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogLevel {
    /// 错误。
    Error,
    /// 警告。
    Warn,
    /// 信息。
    Info,
    /// 调试。
    Debug,
    /// 追踪。
    Trace,
}

/// 代理配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// 代理地址（支持 http/https/socks）。
    pub url: String,
    /// 用户名。
    pub username: Option<String>,
    /// 密码。
    pub password: Option<String>,
}

