//! 通用下载库
//!
//! 提供可插拔的下载器（直链、yt-dlp、ffmpeg、自定义），并通过回调上报进度和日志。

pub mod config;
pub mod dispatcher;
pub mod downloader;
pub mod error;
pub mod progress;
pub mod utils;

pub use config::*;
pub use dispatcher::DownloadClient;
pub use downloader::{Downloader, DownloaderRegistry};
pub use error::{DownloadError, DownloadResult};
pub use progress::{
    ChannelCallback, DownloadCallback, DownloadEvent, DownloadOutcome, DownloadProgress,
    DownloadStage, LogLine, LogSource,
};
