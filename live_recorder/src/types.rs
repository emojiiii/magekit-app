use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// 直播状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LiveStatus {
    /// 直播中
    Live,
    /// 未开播
    Offline,
    /// 轮播中
    Playback,
    /// 未知状态
    Unknown,
}

/// 视频质量
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VideoQuality {
    /// 原画
    Original,
    /// 蓝光
    Blue,
    /// 超清
    Ultra,
    /// 高清
    High,
    /// 标清
    Standard,
    /// 流畅
    Low,
}

impl VideoQuality {
    /// 获取质量等级（数字越大质量越低）
    pub fn level(&self) -> u8 {
        match self {
            VideoQuality::Original => 0,
            VideoQuality::Blue => 0,
            VideoQuality::Ultra => 1,
            VideoQuality::High => 2,
            VideoQuality::Standard => 3,
            VideoQuality::Low => 4,
        }
    }

    /// 从字符串解析视频质量
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "OD" | "BD" | "原画" | "蓝光" => VideoQuality::Original,
            "UHD" | "超清" => VideoQuality::Ultra,
            "HD" | "高清" => VideoQuality::High,
            "SD" | "标清" => VideoQuality::Standard,
            "LD" | "流畅" => VideoQuality::Low,
            _ => VideoQuality::Original,
        }
    }
}

/// 流媒体URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamUrl {
    /// HLS (m3u8) 格式的URL
    pub hls_url: Option<String>,
    /// FLV 格式的URL
    pub flv_url: Option<String>,
    /// DASH 格式的URL
    pub dash_url: Option<String>,
}

/// 直播间信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveRoomInfo {
    /// 房间ID
    pub room_id: String,
    /// 主播名称
    pub anchor_name: String,
    /// 直播标题
    pub title: String,
    /// 直播状态
    pub status: LiveStatus,
    /// 直播开始时间
    pub start_time: Option<DateTime<Utc>>,
    /// 观看人数
    pub viewer_count: Option<u64>,
    /// 直播封面图URL
    pub cover_url: Option<String>,
    /// 平台特定的额外信息
    pub extra: HashMap<String, serde_json::Value>,
}

/// 直播流信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    /// 房间信息
    pub room: LiveRoomInfo,
    /// 可用的流URL列表
    pub streams: Vec<StreamData>,
}

/// 流数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamData {
    /// 视频质量
    pub quality: VideoQuality,
    /// 流URL
    pub url: StreamUrl,
    /// 码率（比特每秒）
    pub bitrate: Option<u64>,
    /// 分辨率
    pub resolution: Option<(u32, u32)>,
    /// 编码格式
    pub codec: Option<String>,
    /// CDN信息
    pub cdn: Option<String>,
}

/// 录制配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordConfig {
    /// 输出文件路径模板
    pub output_path_template: String,
    /// 视频质量
    pub quality: VideoQuality,
    /// 录制格式 (m3u8, flv, mp4)
    pub format: String,
    /// 最大录制时长（秒），None表示无限制
    pub max_duration: Option<u64>,
    /// 自动分段时长（秒），None表示不分段
    pub segment_duration: Option<u64>,
    /// 是否包含弹幕
    pub include_danmaku: bool,
    /// 代理设置
    pub proxy: Option<String>,
    /// 请求头设置
    pub headers: HashMap<String, String>,
    /// 重试次数
    pub retry_count: u32,
    /// 连接超时时间（秒）
    pub timeout: u64,
}

impl Default for RecordConfig {
    fn default() -> Self {
        let mut headers = HashMap::new();
        headers.insert(
            "User-Agent".to_string(),
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36".to_string()
        );

        Self {
            output_path_template: "./downloads/{platform}/{anchor_name}_{room_id}_{timestamp}.mp4".to_string(),
            quality: VideoQuality::Original,
            format: "mp4".to_string(),
            max_duration: None,
            segment_duration: None,
            include_danmaku: false,
            proxy: None,
            headers,
            retry_count: 3,
            timeout: 30,
        }
    }
}

/// 录制状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RecordStatus {
    /// 等待中
    Waiting,
    /// 连接中
    Connecting,
    /// 录制中
    Recording,
    /// 暂停
    Paused,
    /// 停止
    Stopped,
    /// 错误
    Error(String),
    /// 完成
    Completed,
}

/// 录制进度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordProgress {
    /// 状态
    pub status: RecordStatus,
    /// 开始时间
    pub start_time: Option<DateTime<Utc>>,
    /// 已录制时长（秒）
    pub duration: u64,
    /// 已录制大小（字节）
    pub size: u64,
    /// 当前的下载速度（字节/秒）
    pub speed: u64,
    /// 错误信息（如果有）
    pub error: Option<String>,
}