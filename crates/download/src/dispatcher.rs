use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

use crate::config::{DownloadRequest, DownloadStrategy};
use crate::downloader::{
    Downloader, DownloaderRegistry, direct::DirectDownloader, ffmpeg::FfmpegDownloader,
    hls_dash::HlsDashDownloader, ytdlp::YtDlpDownloader,
};
use crate::error::{DownloadError, DownloadResult};
use crate::progress::DownloadCallback;

/// 下载客户端，负责根据策略选择下载器并执行
pub struct DownloadClient {
    registry: DownloaderRegistry,
}

impl DownloadClient {
    pub fn with_defaults() -> Self {
        let mut registry = DownloaderRegistry::new();
        registry.register(DirectDownloader::default());
        registry.register(YtDlpDownloader::default());
        registry.register(FfmpegDownloader::default());
        registry.register(HlsDashDownloader::default());
        Self { registry }
    }

    /// 使用指定的工具路径创建下载客户端
    pub fn with_tools(ytdlp_path: Option<PathBuf>, ffmpeg_path: Option<PathBuf>) -> Self {
        let mut registry = DownloaderRegistry::new();
        registry.register(DirectDownloader::default());

        // 使用提供的路径或降级到默认路径
        if let Some(path) = ytdlp_path {
            registry.register(YtDlpDownloader::new(path));
        } else {
            registry.register(YtDlpDownloader::default());
        }

        if let Some(path) = ffmpeg_path {
            registry.register(FfmpegDownloader::new(path));
        } else {
            registry.register(FfmpegDownloader::default());
        }

        registry.register(HlsDashDownloader::default());
        Self { registry }
    }

    pub fn with_registry(registry: DownloaderRegistry) -> Self {
        Self { registry }
    }

    /// 注册自定义下载器
    pub fn register<D: Downloader + 'static>(&mut self, downloader: D) {
        self.registry.register(downloader);
    }

    /// 选择下载器名称
    fn resolve_downloader(&self, request: &DownloadRequest) -> Option<&'static str> {
        match &request.strategy {
            DownloadStrategy::Direct => Some("direct"),
            DownloadStrategy::YtDlp => Some("ytdlp"),
            DownloadStrategy::Ffmpeg => Some("ffmpeg"),
            DownloadStrategy::HlsDash => Some("hls_dash"),
            DownloadStrategy::Custom(name) => Some(Box::leak(name.clone().into_boxed_str())),
            DownloadStrategy::Auto => {
                // 简单自动策略：先 ytdlp，失败再 direct
                Some("ytdlp")
            }
        }
    }

    /// 执行下载
    pub async fn download(
        &self,
        request: DownloadRequest,
        callback: &dyn DownloadCallback,
        cancel: CancellationToken,
    ) -> DownloadResult<crate::progress::DownloadOutcome> {
        tracing::info!("🎯 DownloadClient::download() 被调用");
        tracing::info!("  ├─ URL: {}", request.url);
        tracing::info!("  └─ 策略: {:?}", request.strategy);

        let name = self
            .resolve_downloader(&request)
            .ok_or_else(|| DownloadError::Unsupported("No downloader matched".into()))?;

        tracing::info!("✅ 选择下载器: {}", name);

        let downloader = self.registry.get(name).ok_or_else(|| {
            DownloadError::Unsupported(format!("Downloader `{}` not found", name))
        })?;

        tracing::info!("🚀 调用下载器 {} 的 download() 方法", name);
        let result = downloader.download(request, callback, cancel).await;
        tracing::info!("✅ 下载器 {} 执行完成，result: {:?}", name, result.is_ok());

        result
    }
}
