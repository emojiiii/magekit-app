//! 录制页面组件

mod live_room_list;
mod polling_config;
mod room_card;
mod status_badge;

pub use live_room_list::*;
pub use polling_config::*;
pub use room_card::*;
pub use status_badge::*;

use std::time::Duration;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 直播间状态
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LiveStatus {
    /// 直播中
    Live,
    /// 未开播
    Offline,
    /// 轮播中
    Playback,
}

impl Default for LiveStatus {
    fn default() -> Self {
        Self::Offline
    }
}

/// 录制状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingStatus {
    /// 开始时间
    pub started_at: DateTime<Utc>,
    /// 录制时长
    pub duration: Duration,
    /// 文件大小（字节）
    pub file_size: u64,
    /// 录制速度（字节/秒）
    pub speed: u64,
}

/// 直播间状态信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveRoomStatus {
    /// 唯一ID
    pub id: Uuid,
    /// 直播间URL
    pub url: String,
    /// 平台
    pub platform: String,
    /// 主播名称
    pub anchor_name: String,
    /// 直播间标题
    pub title: String,
    /// 直播间状态
    pub status: LiveStatus,
    /// 观看人数
    pub viewer_count: Option<u64>,
    /// 是否正在录制
    pub is_recording: bool,
    /// 录制状态
    pub recording_status: Option<RecordingStatus>,
    /// 最后检查时间
    pub last_checked: DateTime<Utc>,
    /// 自动录制
    pub auto_record: bool,
    /// 录制质量
    pub record_quality: String,
}

/// 轮询配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollingConfig {
    /// 轮询间隔（秒）
    pub interval: u64,
    /// 自动录制
    pub auto_record: bool,
    /// 开播通知
    pub notify_on_live: bool,
    /// 下播通知
    pub notify_on_offline: bool,
    /// 最大并发录制数
    pub max_concurrent_recordings: u32,
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            interval: 60, // 60秒
            auto_record: false,
            notify_on_live: true,
            notify_on_offline: false,
            max_concurrent_recordings: 3,
        }
    }
}

impl LiveRoomStatus {
    pub fn new(
        url: String,
        platform: String,
        anchor_name: String,
        title: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            url,
            platform,
            anchor_name,
            title,
            status: LiveStatus::default(),
            viewer_count: None,
            is_recording: false,
            recording_status: None,
            last_checked: Utc::now(),
            auto_record: false,
            record_quality: "原画".to_string(),
        }
    }
}