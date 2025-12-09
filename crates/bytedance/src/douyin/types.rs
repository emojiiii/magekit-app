//! 抖音 API 数据类型定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 直播状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiveStatus {
    /// 直播中
    Live,
    /// 未开播
    Offline,
    /// 未知状态
    Unknown,
}

/// 视频画质
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoQuality {
    /// 原画
    Original,
    /// 超清
    Ultra,
    /// 高清
    High,
    /// 标清
    Standard,
    /// 流畅
    Low,
}

/// 直播间信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveRoomInfo {
    /// 房间 ID
    pub room_id: String,
    /// 主播名称
    pub anchor_name: String,
    /// 直播标题
    pub title: String,
    /// 直播状态
    pub status: LiveStatus,
    /// 开播时间
    pub start_time: Option<u64>,
    /// 观看人数
    pub viewer_count: Option<u64>,
    /// 封面图
    pub cover_url: Option<String>,
    /// 额外信息
    pub extra: HashMap<String, String>,
}

/// 直播流 URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamUrl {
    /// HLS 流地址
    pub hls_url: Option<String>,
    /// FLV 流地址
    pub flv_url: Option<String>,
    /// DASH 流地址
    pub dash_url: Option<String>,
}

/// 直播流数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamData {
    /// 画质
    pub quality: VideoQuality,
    /// 流地址
    pub url: StreamUrl,
    /// 码率
    pub bitrate: Option<u64>,
    /// 分辨率
    pub resolution: Option<String>,
    /// 编码
    pub codec: Option<String>,
    /// CDN
    pub cdn: Option<String>,
}

/// 直播流信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    /// 房间信息
    pub room: LiveRoomInfo,
    /// 流列表
    pub streams: Vec<StreamData>,
}

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    /// 用户 ID
    pub uid: String,
    /// sec_user_id
    pub sec_user_id: String,
    /// 昵称
    pub nickname: String,
    /// 签名
    pub signature: Option<String>,
    /// 头像
    pub avatar_url: Option<String>,
    /// 关注数
    pub following_count: Option<u64>,
    /// 粉丝数
    pub follower_count: Option<u64>,
    /// 作品数
    pub aweme_count: Option<u64>,
    /// 总获赞数
    pub total_favorited: Option<u64>,
}

/// 视频信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwemeInfo {
    /// 作品 ID
    pub aweme_id: String,
    /// 描述
    pub desc: String,
    /// 作者信息
    pub author: Option<AuthorInfo>,
    /// 视频信息
    pub video: Option<VideoData>,
    /// 创建时间
    pub create_time: Option<u64>,
    /// 点赞数
    pub digg_count: Option<u64>,
    /// 评论数
    pub comment_count: Option<u64>,
    /// 分享数
    pub share_count: Option<u64>,
}

/// 作者信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorInfo {
    /// 用户 ID
    pub uid: Option<String>,
    /// sec_user_id
    pub sec_uid: Option<String>,
    /// 昵称
    pub nickname: Option<String>,
    /// 头像
    pub avatar_url: Option<String>,
}

/// 视频数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoData {
    /// 时长（毫秒）
    pub duration: Option<u64>,
    /// 封面
    pub cover_url: Option<String>,
    /// 播放地址列表
    pub play_urls: Vec<String>,
    /// 宽度
    pub width: Option<u32>,
    /// 高度
    pub height: Option<u32>,
    /// 格式列表
    pub formats: Vec<VideoFormat>,
}

/// 视频格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoFormat {
    /// 格式 ID
    pub format_id: String,
    /// 扩展名
    pub ext: String,
    /// 分辨率
    pub resolution: Option<String>,
    /// 文件大小
    pub filesize: Option<u64>,
    /// 质量名称
    pub quality: Option<String>,
    /// 下载地址
    pub download_url: Option<String>,
}

/// 评论信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentInfo {
    /// 评论 ID
    pub cid: String,
    /// 评论内容
    pub text: String,
    /// 评论者
    pub user: Option<AuthorInfo>,
    /// 点赞数
    pub digg_count: Option<u64>,
    /// 回复数
    pub reply_count: Option<u64>,
    /// 创建时间
    pub create_time: Option<u64>,
}

/// 热搜项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotSearchItem {
    /// 排名
    pub rank: u32,
    /// 标题
    pub word: String,
    /// 热度值
    pub hot_value: Option<u64>,
    /// 标签
    pub label: Option<String>,
}
