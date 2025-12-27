use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

use crate::config::{DownloadRequest, DownloadStrategy};
use crate::downloader::{
    Downloader, DownloaderRegistry, direct::DirectDownloader, ffmpeg::FfmpegDownloader,
    hls_dash::HlsDashDownloader, ytdlp::YtDlpDownloader,
};
use crate::error::{DownloadError, DownloadResult};
use crate::progress::{DownloadCallback, LogLine, LogSource};

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
            registry.register(YtDlpDownloader::new(path, ffmpeg_path.clone()));
        } else {
            registry.register(YtDlpDownloader::default());
        }

        if let Some(path) = ffmpeg_path.clone() {
            registry.register(FfmpegDownloader::new(path));
        } else {
            registry.register(FfmpegDownloader::default());
        }

        if let Some(path) = ffmpeg_path {
            registry.register(HlsDashDownloader::new(path));
        } else {
            registry.register(HlsDashDownloader::default());
        }
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
    fn resolve_downloader_name<'a>(&self, request: &'a DownloadRequest) -> Option<&'a str> {
        match &request.strategy {
            DownloadStrategy::Direct => Some("direct"),
            DownloadStrategy::YtDlp => Some("ytdlp"),
            DownloadStrategy::Ffmpeg => Some("ffmpeg"),
            DownloadStrategy::HlsDash => Some("hls_dash"),
            DownloadStrategy::Custom(name) => Some(name.as_str()),
            // Auto 的 fallback 逻辑在 `download()` 内处理
            DownloadStrategy::Auto => Some("ytdlp"),
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

        if matches!(request.strategy, DownloadStrategy::Auto) {
            // 简单自动策略：
            // - m3u8/mpd 优先走 hls_dash（支持分段与 AES-128）
            // - 其余先 ytdlp，失败再尝试 direct（仅对直链场景有效）。
            if looks_like_manifest(&request.url) {
                let mut primary = request.clone();
                primary.strategy = DownloadStrategy::HlsDash;

                let hls = self.registry.get("hls_dash").ok_or_else(|| {
                    DownloadError::Unsupported("Downloader `hls_dash` not found".into())
                })?;

                return hls.download(primary, callback, cancel).await;
            }

            // 无后缀/被隐藏的 manifest：做一次轻量探测，避免误走 ytdlp
            if probe_manifest_kind(&request, &cancel).await.is_some() {
                let mut primary = request.clone();
                primary.strategy = DownloadStrategy::HlsDash;

                let hls = self.registry.get("hls_dash").ok_or_else(|| {
                    DownloadError::Unsupported("Downloader `hls_dash` not found".into())
                })?;

                return hls.download(primary, callback, cancel).await;
            }

            let mut primary = request.clone();
            primary.strategy = DownloadStrategy::YtDlp;

            let ytdlp = self
                .registry
                .get("ytdlp")
                .ok_or_else(|| DownloadError::Unsupported("Downloader `ytdlp` not found".into()))?;

            match ytdlp.download(primary, callback, cancel.clone()).await {
                Ok(outcome) => return Ok(outcome),
                Err(primary_err) => {
                    callback.on_log(LogLine {
                        source: LogSource::System,
                        line: format!("Auto fallback: ytdlp failed, try direct: {}", primary_err),
                    });

                    let mut fallback = request;
                    fallback.strategy = DownloadStrategy::Direct;

                    let direct = self.registry.get("direct").ok_or_else(|| {
                        DownloadError::Unsupported("Downloader `direct` not found".into())
                    })?;

                    return match direct.download(fallback, callback, cancel).await {
                        Ok(outcome) => Ok(outcome),
                        Err(fallback_err) => {
                            callback.on_log(LogLine {
                                source: LogSource::System,
                                line: format!(
                                    "Auto fallback failed: direct also failed (ignored): {}",
                                    fallback_err
                                ),
                            });
                            // 保留 primary_err 作为最终错误，避免被 direct 的 “HTML content-type” 等误导信息覆盖
                            Err(primary_err)
                        }
                    };
                }
            }
        }

        let name = self
            .resolve_downloader_name(&request)
            .ok_or_else(|| DownloadError::Unsupported("No downloader matched".into()))?
            .to_string();

        tracing::info!("✅ 选择下载器: {}", name);

        let downloader = self.registry.get(&name).ok_or_else(|| {
            DownloadError::Unsupported(format!("Downloader `{}` not found", name))
        })?;

        tracing::info!("🚀 调用下载器 {} 的 download() 方法", name);
        let result = downloader.download(request, callback, cancel).await;
        tracing::info!("✅ 下载器 {} 执行完成，result: {:?}", name, result.is_ok());

        result
    }
}

fn looks_like_manifest(url: &url::Url) -> bool {
    let p = url.path().to_ascii_lowercase();
    p.ends_with(".m3u8")
        || p.ends_with(".mpd")
        || url.as_str().to_ascii_lowercase().contains(".m3u8")
}

#[derive(Debug, Clone, Copy)]
enum ManifestKind {
    Hls,
    Dash,
}

async fn probe_manifest_kind(
    request: &DownloadRequest,
    cancel: &CancellationToken,
) -> Option<ManifestKind> {
    if cancel.is_cancelled() {
        return None;
    }

    let mut url = request.url.clone();
    if !request.extra.query.is_empty() {
        let mut pairs = url.query_pairs().into_owned().collect::<Vec<_>>();
        pairs.extend(request.extra.query.clone());
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }

    let timeout = request
        .timeout
        .unwrap_or_else(|| std::time::Duration::from_secs(5));
    let client = reqwest::Client::builder().timeout(timeout).build().ok()?;

    let mut req = client.get(url);
    for (k, v) in &request.extra.headers {
        req = req.header(k, v);
    }
    if let Some(cookie) = &request.extra.cookie {
        req = req.header(reqwest::header::COOKIE, cookie);
    }
    req = req.header(reqwest::header::RANGE, "bytes=0-2047");

    let resp = tokio::select! {
        _ = cancel.cancelled() => return None,
        r = req.send() => r.ok()?,
    };
    if !resp.status().is_success() {
        return None;
    }

    let bytes = tokio::select! {
        _ = cancel.cancelled() => return None,
        b = resp.bytes() => b.ok()?,
    };

    let s = String::from_utf8_lossy(&bytes);
    let trimmed = s
        .trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
        .to_ascii_uppercase();

    if trimmed.starts_with("#EXTM3U") {
        return Some(ManifestKind::Hls);
    }
    if trimmed.contains("<MPD") {
        return Some(ManifestKind::Dash);
    }
    None
}
