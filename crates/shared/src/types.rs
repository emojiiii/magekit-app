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
    /// 直链下载地址（自定义解析使用），yt-dlp 解析时为空
    #[serde(default)]
    pub download_url: Option<String>,
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
    /// 直链下载地址（自研解析时使用，如抖音/TikTok）
    #[serde(default)]
    pub download_url: Option<String>,
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
            download_url: None,
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
    /// 直播录制配置
    #[serde(default)]
    pub live_record: LiveRecordConfig,
    /// 监控的直播间列表
    #[serde(default)]
    pub monitored_rooms: Vec<MonitoredRoom>,
}

impl Default for AppConfig {
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

/// 频道 Tab 类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ChannelTabType {
    /// 视频
    Videos,
    /// 短视频 (Shorts)
    Shorts,
    /// 直播
    Live,
    /// 播放列表
    Playlists,
    /// 其他
    Other(String),
}

impl ChannelTabType {
    /// 从 yt-dlp 的 tab 标题解析 Tab 类型
    pub fn from_title(title: &str) -> Self {
        let lower = title.to_lowercase();
        if lower.contains("video") || lower.contains("视频") {
            Self::Videos
        } else if lower.contains("short") {
            Self::Shorts
        } else if lower.contains("live") || lower.contains("直播") || lower.contains("stream") {
            Self::Live
        } else if lower.contains("playlist") || lower.contains("播放列表") {
            Self::Playlists
        } else {
            Self::Other(title.to_string())
        }
    }

    /// 获取显示名称
    pub fn display_name(&self) -> &str {
        match self {
            Self::Videos => "视频",
            Self::Shorts => "Shorts",
            Self::Live => "直播",
            Self::Playlists => "播放列表",
            Self::Other(name) => name,
        }
    }
}

/// 频道 Tab 信息
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelTab {
    /// Tab 类型
    pub tab_type: ChannelTabType,
    /// Tab 标题
    pub title: String,
    /// Tab 中的视频条目
    pub entries: Vec<ChannelVideoEntry>,
}

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
    /// 频道 Tabs（如 Videos, Shorts, Live）
    pub tabs: Vec<ChannelTab>,
    /// 所有视频条目（扁平化列表，用于向后兼容）
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

// ============= 直播录制相关类型 =============

/// 直播录制配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveRecordConfig {
    /// 录制输出路径（相对于下载路径 + /record）
    #[serde(default = "default_record_path")]
    pub output_base_path: PathBuf,
    /// 录制格式（原始）
    #[serde(default = "default_record_format")]
    pub record_format: String,
    /// 录制完成后转码格式
    #[serde(default)]
    pub transcode_format: Option<String>,
    /// 是否自动转码
    #[serde(default)]
    pub auto_transcode: bool,
    /// 最大分段时长（秒），None 表示不分段
    #[serde(default)]
    pub segment_duration: Option<u64>,
    /// 视频质量偏好
    #[serde(default = "default_quality")]
    pub quality: LiveRecordQuality,
    /// 重试次数
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
    /// 断流后重连延迟（秒）
    #[serde(default = "default_reconnect_delay")]
    pub reconnect_delay: u64,
    /// 检测间隔（秒）
    #[serde(default = "default_check_interval")]
    pub check_interval: u64,
}

fn default_record_path() -> PathBuf {
    PathBuf::from("record")
}

fn default_record_format() -> String {
    "ts".to_string()
}

fn default_quality() -> LiveRecordQuality {
    LiveRecordQuality::Original
}

fn default_retry_count() -> u32 {
    3
}

fn default_reconnect_delay() -> u64 {
    10
}

fn default_check_interval() -> u64 {
    60
}

impl Default for LiveRecordConfig {
    fn default() -> Self {
        Self {
            output_base_path: default_record_path(),
            record_format: default_record_format(),
            transcode_format: None,
            auto_transcode: false,
            segment_duration: None,
            quality: default_quality(),
            retry_count: default_retry_count(),
            reconnect_delay: default_reconnect_delay(),
            check_interval: default_check_interval(),
        }
    }
}

/// 直播录制质量
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum LiveRecordQuality {
    /// 原画
    #[default]
    Original,
    /// 蓝光
    Blue,
    /// 超清
    Ultra,
    /// 高清
    High,
    /// 标清
    Standard,
}

impl LiveRecordQuality {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Original => "原画",
            Self::Blue => "蓝光",
            Self::Ultra => "超清",
            Self::High => "高清",
            Self::Standard => "标清",
        }
    }
}

/// 监控的直播间信息（持久化用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoredRoom {
    /// 唯一 ID
    pub id: Uuid,
    /// 直播间 URL
    pub url: String,
    /// 平台名称
    pub platform: String,
    /// 房间 ID
    pub room_id: String,
    /// 主播名称
    pub anchor_name: String,
    /// 是否启用监控
    #[serde(default = "default_true")]
    pub monitoring_enabled: bool,
    /// 是否自动录制
    #[serde(default = "default_true")]
    pub auto_record: bool,
    /// 添加时间
    pub added_at: chrono::DateTime<chrono::Utc>,
    /// 最后检查时间
    pub last_checked: Option<chrono::DateTime<chrono::Utc>>,
    /// 最后直播时间
    pub last_live_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 缓存的房间标题
    #[serde(default)]
    pub cached_title: Option<String>,
    /// 缓存的封面图 URL
    #[serde(default)]
    pub cached_cover_url: Option<String>,
}

impl MonitoredRoom {
    pub fn new(url: String, platform: String, room_id: String, anchor_name: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            url,
            platform,
            room_id,
            anchor_name,
            monitoring_enabled: true,
            auto_record: true,
            added_at: chrono::Utc::now(),
            last_checked: None,
            last_live_at: None,
            cached_title: None,
            cached_cover_url: None,
        }
    }
}

/// 直播间当前状态（运行时）
#[derive(Debug, Clone, PartialEq)]
pub enum LiveRoomStatus {
    /// 未知
    Unknown,
    /// 检查中
    Checking,
    /// 离线
    Offline,
    /// 直播中
    Live,
    /// 录制中
    Recording,
    /// 轮播中
    Playback,
    /// 错误
    Error(String),
}

impl LiveRoomStatus {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Unknown => "未知",
            Self::Checking => "检查中",
            Self::Offline => "未开播",
            Self::Live => "直播中",
            Self::Recording => "录制中",
            Self::Playback => "轮播中",
            Self::Error(_) => "错误",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Unknown => "❓",
            Self::Checking => "🔄",
            Self::Offline => "⚫",
            Self::Live => "🔴",
            Self::Recording => "⏺️",
            Self::Playback => "🟡",
            Self::Error(_) => "❌",
        }
    }
}

/// 录制任务信息
#[derive(Debug, Clone)]
pub struct RecordingTask {
    /// 任务 ID
    pub id: Uuid,
    /// 关联的房间 ID
    pub room_id: Uuid,
    /// 输出文件路径
    pub output_path: PathBuf,
    /// 开始时间
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// 录制时长（秒）
    pub duration: u64,
    /// 已录制大小（字节）
    pub recorded_bytes: u64,
    /// 状态
    pub status: RecordingTaskStatus,
}

/// 录制任务状态
#[derive(Debug, Clone, PartialEq)]
pub enum RecordingTaskStatus {
    /// 录制中
    Recording,
    /// 已完成
    Completed,
    /// 正在转码
    Transcoding,
    /// 已取消
    Cancelled,
    /// 失败
    Failed(String),
}
