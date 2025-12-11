use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;

use crate::error::DownloadError;

/// 下载阶段
#[derive(Clone, Debug)]
pub enum DownloadStage {
    Preparing,
    Downloading,
    Merging,
    Completed,
}

/// 进度信息
#[derive(Clone, Debug)]
pub struct DownloadProgress {
    pub stage: DownloadStage,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: Option<u64>,
    pub eta: Option<Duration>,
}

impl DownloadProgress {
    pub fn preparing() -> Self {
        Self {
            stage: DownloadStage::Preparing,
            bytes_downloaded: 0,
            total_bytes: None,
            speed_bps: None,
            eta: None,
        }
    }

    pub fn downloading(
        bytes_downloaded: u64,
        total_bytes: Option<u64>,
        speed_bps: Option<u64>,
    ) -> Self {
        Self {
            stage: DownloadStage::Downloading,
            bytes_downloaded,
            total_bytes,
            speed_bps,
            eta: total_bytes.and_then(|total| {
                speed_bps.and_then(|speed| {
                    if speed == 0 {
                        None
                    } else {
                        let remain = total.saturating_sub(bytes_downloaded);
                        Some(Duration::from_secs(remain / speed))
                    }
                })
            }),
        }
    }

    pub fn completed(bytes_downloaded: u64, total_bytes: Option<u64>) -> Self {
        Self {
            stage: DownloadStage::Completed,
            bytes_downloaded,
            total_bytes,
            speed_bps: None,
            eta: None,
        }
    }
}

/// 下载完成结果
#[derive(Clone, Debug)]
pub struct DownloadOutcome {
    pub output_path: PathBuf,
    pub content_type: Option<String>,
    pub details: HashMap<String, String>,
}

/// 日志来源
#[derive(Clone, Debug)]
pub enum LogSource {
    Stdout,
    Stderr,
    System,
}

/// 日志行
#[derive(Clone, Debug)]
pub struct LogLine {
    pub source: LogSource,
    pub line: String,
}

/// 下载事件（便于通过 channel 分发）
#[derive(Clone, Debug)]
pub enum DownloadEvent {
    Progress(DownloadProgress),
    Complete(DownloadOutcome),
    Error(DownloadError),
    Log(LogLine),
}

/// 进度/结果回调
pub trait DownloadCallback: Send + Sync + 'static {
    fn on_progress(&self, _progress: DownloadProgress) {}
    fn on_complete(&self, _result: DownloadOutcome) {}
    fn on_error(&self, _error: DownloadError) {}
    fn on_log(&self, _line: LogLine) {}
}

/// 基于 channel 的默认回调实现
pub struct ChannelCallback {
    sender: UnboundedSender<DownloadEvent>,
}

impl ChannelCallback {
    pub fn new(sender: UnboundedSender<DownloadEvent>) -> Self {
        Self { sender }
    }
}

impl DownloadCallback for ChannelCallback {
    fn on_progress(&self, progress: DownloadProgress) {
        let _ = self.sender.send(DownloadEvent::Progress(progress));
    }

    fn on_complete(&self, result: DownloadOutcome) {
        let _ = self.sender.send(DownloadEvent::Complete(result));
    }

    fn on_error(&self, error: DownloadError) {
        let _ = self.sender.send(DownloadEvent::Error(error));
    }

    fn on_log(&self, line: LogLine) {
        let _ = self.sender.send(DownloadEvent::Log(line));
    }
}
