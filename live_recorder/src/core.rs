use crate::{
    platforms::PlatformFactory,
    recorder::LiveRecorder as Recorder,
    types::{RecordConfig, VideoQuality},
};

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

    /// 使用自定义平台工厂创建录制器核心
    pub fn with_factory(factory: PlatformFactory) -> Self {
        Self {
            recorder: Recorder::with_factory(factory),
        }
    }

    /// 开始录制直播
    pub async fn start_recording(
        &self,
        url: &str,
        config: RecordConfig,
    ) -> crate::error::RecorderResult<crate::recorder::RecordingHandle> {
        self.recorder.start_recording(url, config).await
    }

    /// 检查直播间状态
    pub async fn check_room_status(&self, url: &str) -> crate::error::RecorderResult<crate::types::LiveRoomInfo> {
        self.recorder.check_room_status(url).await
    }

    /// 获取可用流信息
    pub async fn get_stream_info(&self, url: &str) -> crate::error::RecorderResult<crate::types::StreamInfo> {
        self.recorder.get_stream_info(url).await
    }

    /// 快速录制方法（使用默认配置）
    pub async fn quick_record(&self, url: &str, output_path: &str) -> crate::error::RecorderResult<crate::recorder::RecordingHandle> {
        let config = RecordConfig {
            output_path_template: output_path.to_string(),
            quality: VideoQuality::Original,
            ..Default::default()
        };
        self.start_recording(url, config).await
    }

    /// 使用自定义配置录制
    pub async fn record(&self, url: &str, config: RecordConfig) -> crate::error::RecorderResult<crate::recorder::RecordingHandle> {
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