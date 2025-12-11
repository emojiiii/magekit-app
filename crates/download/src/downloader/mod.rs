use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::config::DownloadRequest;
use crate::error::DownloadResult;
use crate::progress::{DownloadCallback, DownloadOutcome};

pub mod direct;
pub mod ffmpeg;
pub mod hls_dash;
pub mod ytdlp;

/// 下载器接口
#[async_trait]
pub trait Downloader: Send + Sync {
    fn name(&self) -> &'static str;

    async fn download(
        &self,
        request: DownloadRequest,
        callback: &dyn DownloadCallback,
        cancel: CancellationToken,
    ) -> DownloadResult<DownloadOutcome>;
}

/// 下载器注册表
#[derive(Default)]
pub struct DownloaderRegistry {
    inner: HashMap<String, Arc<dyn Downloader>>,
}

impl DownloaderRegistry {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    pub fn register<D: Downloader + 'static>(&mut self, downloader: D) {
        self.inner
            .insert(downloader.name().to_string(), Arc::new(downloader));
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Downloader>> {
        self.inner.get(name).cloned()
    }
}
