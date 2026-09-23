use crate::{
    recorder::LiveRecorder as Recorder,
    types::{RecordConfig, VideoQuality},
};
use magekit_shared::types::PlatformCookie;
use serde_json::Value;

/// 直播录制器核心接口
pub struct LiveRecorderCore {
    recorder: Recorder,
}

impl LiveRecorderCore {
    /// 创建新的录制器核心
    pub fn new() -> Self {
        Self {
            recorder: Recorder::new(),
        }
    }

    /// 配置可选的 Streamlink SOOP 插件登录账号。
    pub fn with_soop_credentials(
        mut self,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        self.recorder = self.recorder.with_soop_credentials(username, password);
        self
    }

    /// 设置应用代理；`system` 表示使用已检测到的系统代理。
    pub fn with_proxy(mut self, proxy: Option<String>) -> Self {
        self.recorder = self.recorder.with_proxy(proxy);
        self
    }

    /// 开始录制直播
    pub async fn start_recording(
        &self,
        url: &str,
        config: RecordConfig,
    ) -> crate::error::RecorderResult<crate::recorder::RecordingHandle> {
        self.recorder.start_recording(url, config).await
    }

    /// 开始录制直播（带 Cookie 支持）
    pub async fn start_recording_with_cookies(
        &self,
        url: &str,
        config: RecordConfig,
        cookies: &[PlatformCookie],
    ) -> crate::error::RecorderResult<crate::recorder::RecordingHandle> {
        self.recorder
            .start_recording_with_cookies(url, config, cookies)
            .await
    }

    /// 录制页传入最近一次 SOOP 房态检查的短期频道元数据。
    pub async fn start_recording_with_cookies_and_hint(
        &self,
        url: &str,
        config: RecordConfig,
        cookies: &[PlatformCookie],
        soop_hint: Option<Value>,
    ) -> crate::error::RecorderResult<crate::recorder::RecordingHandle> {
        self.recorder
            .start_recording_with_cookies_and_hint(url, config, cookies, soop_hint)
            .await
    }

    /// 背景预热 SOOP 手动录制流，返回仅存于内存的短期授权信息。
    pub async fn prepare_soop_stream_with_cookies(
        &self,
        url: &str,
        quality: VideoQuality,
        cookies: &[PlatformCookie],
        soop_hint: Value,
    ) -> crate::error::RecorderResult<Value> {
        self.recorder
            .prepare_soop_stream_with_cookies(url, quality, cookies, soop_hint)
            .await
    }

    /// 检查直播间状态
    pub async fn check_room_status(
        &self,
        url: &str,
    ) -> crate::error::RecorderResult<crate::types::LiveRoomInfo> {
        self.recorder.check_room_status(url).await
    }

    /// 检查直播间状态（带 Cookie 支持）
    pub async fn check_room_status_with_cookies(
        &self,
        url: &str,
        cookies: &[PlatformCookie],
    ) -> crate::error::RecorderResult<crate::types::LiveRoomInfo> {
        self.recorder
            .check_room_status_with_cookies(url, cookies)
            .await
    }

    /// 检查直播状态，可跳过非必要的直播页封面请求。
    pub async fn check_room_status_with_cookies_and_cover(
        &self,
        url: &str,
        cookies: &[PlatformCookie],
        fetch_cover: bool,
    ) -> crate::error::RecorderResult<crate::types::LiveRoomInfo> {
        self.recorder
            .check_room_status_with_cookies_and_cover(url, cookies, fetch_cover)
            .await
    }

    /// 获取可用流信息
    pub async fn get_stream_info(
        &self,
        url: &str,
    ) -> crate::error::RecorderResult<crate::types::StreamInfo> {
        self.recorder.get_stream_info(url).await
    }

    /// 获取可用流信息（带 Cookie 支持）
    pub async fn get_stream_info_with_cookies(
        &self,
        url: &str,
        cookies: &[PlatformCookie],
    ) -> crate::error::RecorderResult<crate::types::StreamInfo> {
        self.recorder
            .get_stream_info_with_cookies(url, cookies)
            .await
    }

    /// 快速录制方法（使用默认配置）
    pub async fn quick_record(
        &self,
        url: &str,
        output_path: &str,
    ) -> crate::error::RecorderResult<crate::recorder::RecordingHandle> {
        let config = RecordConfig {
            output_path_template: output_path.to_string(),
            quality: VideoQuality::Original,
            ..Default::default()
        };
        self.start_recording(url, config).await
    }

    /// 使用自定义配置录制
    pub async fn record(
        &self,
        url: &str,
        config: RecordConfig,
    ) -> crate::error::RecorderResult<crate::recorder::RecordingHandle> {
        self.start_recording(url, config).await
    }

    /// 获取录制器实例
    pub fn recorder(&self) -> &Recorder {
        &self.recorder
    }
}

impl Default for LiveRecorderCore {
    fn default() -> Self {
        Self::new()
    }
}

// 为了向后兼容，提供一个类型别名
pub type LiveRecorder = LiveRecorderCore;
