use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// 任务唯一标识符
pub type TaskId = Uuid;

/// 视频格式信息
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoFormat {
    pub format_id: String,
    pub ext: String,
    pub resolution: Option<String>,
    pub fps: Option<f32>,
    pub filesize: Option<u64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub quality: Option<String>,
}

impl VideoFormat {
    /// 格式化文件大小显示
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

/// 视频信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub duration: Option<Duration>,
    pub uploader: Option<String>,
    pub upload_date: Option<String>,
    pub thumbnail: Option<String>,
    pub formats: Vec<VideoFormat>,
    pub url: String,
}

/// 下载选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadOptions {
    pub format_id: String,
    pub output_path: PathBuf,
    pub output_template: Option<String>,
    pub embed_metadata: bool,
    pub embed_thumbnail: bool,
    pub extract_audio: bool,
    pub audio_format: Option<String>,
    pub subtitle_langs: Vec<String>,
    pub embed_subs: bool,
    pub write_subs: bool,
    pub write_auto_subs: bool,
}

impl Default for DownloadOptions {
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
        }
    }
}

/// 任务状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskState {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed(String),
    Cancelled,
}

/// 下载任务的参数（用于暂停后恢复）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadParams {
    pub output_dir: PathBuf,
    pub format_id: String,
    pub embed_metadata: bool,
    pub embed_thumbnail: bool,
    pub download_subtitles: bool,
    pub audio_only: bool,
}

/// 任务状态信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatus {
    pub id: TaskId,
    pub url: String,
    pub title: Option<String>,
    pub state: TaskState,
    pub progress: f32,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed: Option<u64>,
    pub eta: Option<Duration>,
    pub created_at: std::time::SystemTime,
    pub started_at: Option<std::time::SystemTime>,
    pub completed_at: Option<std::time::SystemTime>,
    pub output_path: Option<PathBuf>,
    /// 下载参数（用于暂停后恢复）
    pub download_params: Option<DownloadParams>,
}

impl TaskStatus {
    pub fn new(id: TaskId, url: String, title: Option<String>) -> Self {
        Self {
            id,
            url,
            title,
            state: TaskState::Queued,
            progress: 0.0,
            downloaded_bytes: 0,
            total_bytes: None,
            speed: None,
            eta: None,
            created_at: std::time::SystemTime::now(),
            started_at: None,
            completed_at: None,
            output_path: None,
            download_params: None,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            TaskState::Queued | TaskState::Downloading | TaskState::Paused
        )
    }

    pub fn is_finished(&self) -> bool {
        matches!(
            self.state,
            TaskState::Completed | TaskState::Failed(_) | TaskState::Cancelled
        )
    }
}

/// 任务更新事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskUpdate {
    Created(TaskStatus),
    Progress(TaskId, f32, u64, Option<u64>, Option<u64>, Option<Duration>),
    StateChanged(TaskId, TaskState),
    SpeedUpdate(TaskId, u64),
    Completed(TaskId, PathBuf),
    Failed(TaskId, String),
}

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub download: DownloadConfig,
    pub tools: ToolsConfig,
    pub ui: UiConfig,
    pub advanced: AdvancedConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            download: DownloadConfig::default(),
            tools: ToolsConfig::default(),
            ui: UiConfig::default(),
            advanced: AdvancedConfig::default(),
        }
    }
}

/// 下载配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadConfig {
    pub default_output_path: PathBuf,
    pub max_concurrent_downloads: usize,
    pub default_format: String,
    pub embed_metadata: bool,
    pub embed_thumbnail: bool,
    pub auto_extract_audio: bool,
    pub preferred_audio_format: Option<String>,
    pub subtitle_languages: Vec<String>,
}

impl Default for DownloadConfig {
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

/// 工具配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    pub auto_update: bool,
    pub update_channel: UpdateChannel,
    pub yt_dlp_version: Option<String>,
    pub ffmpeg_version: Option<String>,
    pub custom_yt_dlp_path: Option<PathBuf>,
    pub custom_ffmpeg_path: Option<PathBuf>,
}

impl Default for ToolsConfig {
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

/// 更新通道
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UpdateChannel {
    Stable,
    Nightly,
    Custom(String),
}

/// UI配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: Theme,
    pub language: String,
    pub window_state: Option<WindowState>,
    pub show_notifications: bool,
    pub minimize_to_tray: bool,
}

impl Default for UiConfig {
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

/// 主题设置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeConfig {
    /// 主题名称 (如 "Default Light", "Default Dark", "Ayu Light", "Nord" 等)
    pub name: String,
    /// 主题模式 (light/dark)
    pub mode: ThemeMode,
}

/// 主题模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "Default Light".to_string(),
            mode: ThemeMode::Light,
        }
    }
}

/// 旧版主题枚举，保留用于兼容性
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Theme {
    Light,
    Dark,
    System,
    /// 使用新版主题配置
    Custom(ThemeConfig),
}

impl Default for Theme {
    fn default() -> Self {
        Theme::Custom(ThemeConfig::default())
    }
}

impl Theme {
    /// 获取主题名称
    pub fn theme_name(&self) -> String {
        match self {
            Theme::Light => "Default Light".to_string(),
            Theme::Dark => "Default Dark".to_string(),
            Theme::System => "Default Light".to_string(), // 默认使用 Light
            Theme::Custom(config) => config.name.clone(),
        }
    }

    /// 是否为暗色模式
    pub fn is_dark(&self) -> bool {
        match self {
            Theme::Light => false,
            Theme::Dark => true,
            Theme::System => false, // 需要检测系统
            Theme::Custom(config) => matches!(config.mode, ThemeMode::Dark),
        }
    }
}

/// 窗口状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub maximized: bool,
}

/// 高级配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedConfig {
    pub log_level: LogLevel,
    pub proxy: Option<ProxyConfig>,
    pub speed_limit: Option<u64>,
    pub retry_times: u32,
    pub timeout: Duration,
    /// 平台 Cookie 配置列表
    #[serde(default)]
    pub cookies: Vec<PlatformCookie>,
}

impl Default for AdvancedConfig {
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

/// 平台 Cookie 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformCookie {
    /// 平台名称/域名，如 "bilibili", "youtube", "bilibili.com"
    pub platform: String,
    /// Cookie 内容（Netscape 格式或浏览器格式的 cookie 字符串）
    pub cookie: String,
    /// 是否启用
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl PlatformCookie {
    pub fn new(platform: String, cookie: String) -> Self {
        Self {
            platform,
            cookie,
            enabled: true,
        }
    }
}

/// 日志级别
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// 代理配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// 应用事件
#[derive(Debug, Clone)]
pub enum AppEvent {
    TaskUpdate(TaskUpdate),
    ConfigChanged(AppConfig),
    ThemeChanged(Theme),
    ToolUpdate(ToolUpdateEvent),
    ShowNotification(Notification),
}

/// 工具更新事件
#[derive(Debug, Clone)]
pub struct ToolUpdateEvent {
    pub tool_type: ToolType,
    pub old_version: Option<String>,
    pub new_version: String,
    pub status: UpdateStatus,
}

/// 工具类型
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ToolType {
    YtDlp,
    Ffmpeg,
}

/// 更新状态
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateStatus {
    Checking,
    Downloading,
    Installing,
    Completed,
    Failed(String),
}

/// 通知类型
#[derive(Debug, Clone)]
pub struct Notification {
    pub title: String,
    pub message: String,
    pub notification_type: NotificationType,
    pub duration: Option<Duration>,
}

/// 通知类型
#[derive(Debug, Clone)]
pub enum NotificationType {
    Info,
    Success,
    Warning,
    Error,
}

// ============================================================================
// 频道/作者相关类型
// ============================================================================

/// 频道/播放列表信息
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelInfo {
    /// 频道/播放列表 ID
    pub id: String,
    /// 频道/播放列表名称
    pub title: String,
    /// 频道/播放列表 URL
    pub url: String,
    /// 上传者/频道名称
    pub uploader: Option<String>,
    /// 频道描述
    pub description: Option<String>,
    /// 视频数量
    pub video_count: usize,
    /// 频道缩略图
    pub thumbnail: Option<String>,
    /// 视频条目列表
    pub entries: Vec<ChannelVideoEntry>,
}

/// 频道中的视频条目（精简信息）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelVideoEntry {
    /// 视频 ID
    pub id: String,
    /// 视频标题
    pub title: String,
    /// 视频 URL
    pub url: String,
    /// 视频时长（秒）
    pub duration: Option<u64>,
    /// 缩略图 URL
    pub thumbnail: Option<String>,
    /// 上传者
    pub uploader: Option<String>,
    /// 在播放列表中的索引
    pub playlist_index: Option<u32>,
    /// 是否被选中下载
    #[serde(default)]
    pub selected: bool,
}
