//! 平台、频道以及直播录制相关的数据模型。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// 平台 Cookie 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformCookie {
    /// 平台名称/域名，如 `bilibili`、`youtube`、`bilibili.com`。
    pub platform: String,
    /// Cookie 内容（Netscape 或浏览器导出格式）。
    pub cookie: String,
    /// 是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl PlatformCookie {
    /// 新建启用状态的 Cookie 配置。
    pub fn new(platform: String, cookie: String) -> Self {
        Self {
            platform,
            cookie,
            enabled: true,
        }
    }
}

/// 频道 Tab 类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ChannelTabType {
    /// 视频。
    Videos,
    /// 短视频 (Shorts)。
    Shorts,
    /// 直播。
    Live,
    /// 播放列表。
    Playlists,
    /// 其他。
    Other(String),
}

impl ChannelTabType {
    /// 根据 yt-dlp Tab 标题推断类型。
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

    /// 获取显示名称。
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

/// 频道 Tab 信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelTab {
    /// Tab 类型。
    pub tab_type: ChannelTabType,
    /// Tab 标题。
    pub title: String,
    /// Tab 中的视频条目。
    pub entries: Vec<ChannelVideoEntry>,
}

/// 频道/播放列表信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelInfo {
    /// 频道/播放列表 ID。
    pub id: String,
    /// 频道/播放列表名称。
    pub title: String,
    /// 频道/播放列表 URL。
    pub url: String,
    /// 上传者/频道名称。
    pub uploader: Option<String>,
    /// 频道描述。
    pub description: Option<String>,
    /// 视频数量。
    pub video_count: usize,
    /// 频道缩略图。
    pub thumbnail: Option<String>,
    /// 频道 Tabs（如 Videos、Shorts、Live）。
    pub tabs: Vec<ChannelTab>,
    /// 扁平化的视频条目列表（兼容旧版）。
    pub entries: Vec<ChannelVideoEntry>,
}

/// 频道中的视频条目（精简信息）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelVideoEntry {
    /// 视频 ID。
    pub id: String,
    /// 视频标题。
    pub title: String,
    /// 视频 URL。
    pub url: String,
    /// 视频时长（秒）。
    pub duration: Option<u64>,
    /// 缩略图 URL。
    pub thumbnail: Option<String>,
    /// 上传者。
    pub uploader: Option<String>,
    /// 在播放列表中的索引。
    pub playlist_index: Option<u32>,
    /// 是否被选中下载。
    #[serde(default)]
    pub selected: bool,
}

/// 直播录制配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveRecordConfig {
    /// 录制输出路径（相对于下载路径 + `/record`）。
    #[serde(default = "default_record_path")]
    pub output_base_path: PathBuf,
    /// 录制格式（原始）。
    #[serde(default = "default_record_format")]
    pub record_format: String,
    /// 录制完成后转码格式。
    #[serde(default)]
    pub transcode_format: Option<String>,
    /// 是否自动转码。
    #[serde(default)]
    pub auto_transcode: bool,
    /// 最大分段时长（秒），None 表示不分段。
    #[serde(default)]
    pub segment_duration: Option<u64>,
    /// 视频质量偏好。
    #[serde(default = "default_quality")]
    pub quality: LiveRecordQuality,
    /// 重试次数。
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
    /// 断流后重连延迟（秒）。
    #[serde(default = "default_reconnect_delay")]
    pub reconnect_delay: u64,
    /// 检测间隔（秒）。
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
    /// 构造默认录制配置。
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

/// 直播录制质量。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum LiveRecordQuality {
    /// 原画。
    #[default]
    Original,
    /// 蓝光。
    Blue,
    /// 超清。
    Ultra,
    /// 高清。
    High,
    /// 标清。
    Standard,
}

impl LiveRecordQuality {
    /// 获取质量显示名称。
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

/// 监控的直播间信息（持久化用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoredRoom {
    /// 唯一 ID。
    pub id: Uuid,
    /// 直播间 URL。
    pub url: String,
    /// 平台名称。
    pub platform: String,
    /// 房间 ID。
    pub room_id: String,
    /// 主播名称。
    pub anchor_name: String,
    /// 是否启用监控。
    #[serde(default = "default_true")]
    pub monitoring_enabled: bool,
    /// 是否自动录制。
    #[serde(default = "default_true")]
    pub auto_record: bool,
    /// 添加时间。
    pub added_at: chrono::DateTime<chrono::Utc>,
    /// 最后检查时间。
    pub last_checked: Option<chrono::DateTime<chrono::Utc>>,
    /// 最后直播时间。
    pub last_live_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 缓存的房间标题。
    #[serde(default)]
    pub cached_title: Option<String>,
    /// 缓存的封面图 URL。
    #[serde(default)]
    pub cached_cover_url: Option<String>,
}

impl MonitoredRoom {
    /// 创建新的监控房间条目（默认启用监控与自动录制）。
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

/// 直播间当前状态（运行时）。
#[derive(Debug, Clone, PartialEq)]
pub enum LiveRoomStatus {
    /// 未知。
    Unknown,
    /// 检查中。
    Checking,
    /// 离线。
    Offline,
    /// 直播中。
    Live,
    /// 录制中。
    Recording,
    /// 轮播中。
    Playback,
    /// 错误。
    Error(String),
}

impl LiveRoomStatus {
    /// 获取状态展示名称。
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

    /// 获取状态对应的 emoji。
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

/// 录制任务信息。
#[derive(Debug, Clone)]
pub struct RecordingTask {
    /// 任务 ID。
    pub id: Uuid,
    /// 关联的房间 ID。
    pub room_id: Uuid,
    /// 输出文件路径。
    pub output_path: PathBuf,
    /// 开始时间。
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// 录制时长（秒）。
    pub duration: u64,
    /// 已录制大小（字节）。
    pub recorded_bytes: u64,
    /// 状态。
    pub status: RecordingTaskStatus,
}

/// 录制任务状态。
#[derive(Debug, Clone, PartialEq)]
pub enum RecordingTaskStatus {
    /// 录制中。
    Recording,
    /// 已完成。
    Completed,
    /// 正在转码。
    Transcoding,
    /// 已取消。
    Cancelled,
    /// 失败。
    Failed(String),
}
