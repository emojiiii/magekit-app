//! M3U8 抓取与下载能力（基于 magekit-capture 库）
//!
//! - 提供向后兼容的 M3u8Stream 类型
//! - 作为 magekit_capture 的薄包装层

use crate::app::AppState;
use anyhow::Result;
use magekit_capture::{CapturedResource, ResourceFilter, ResourceType};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

// 重新导出 magekit_capture 的类型
pub use magekit_capture::CaptureRequest as CaptureCoreRequest;
pub use magekit_capture::ResourceType as CaptureResourceType;

/// 资源筛选选项（UI 友好版本）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureFilterType {
    /// 全部资源
    All,
    /// 仅视频
    Video,
    /// 仅音频
    Audio,
    /// 仅图片
    Image,
    /// 视频和音频
    Media,
}

impl CaptureFilterType {
    pub fn to_resource_filter(self) -> ResourceFilter {
        match self {
            Self::All => ResourceFilter::all(),
            Self::Video => ResourceFilter::video_only(),
            Self::Audio => ResourceFilter::audio_only(),
            Self::Image => ResourceFilter::image_only(),
            Self::Media => ResourceFilter::media_only(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::All => "全部资源",
            Self::Video => "仅视频",
            Self::Audio => "仅音频",
            Self::Image => "仅图片",
            Self::Media => "视频+音频",
        }
    }
}

/// M3U8 流（向后兼容类型，用于 UI）
#[derive(Debug, Clone, PartialEq)]
pub struct M3u8Stream {
    pub url: String,
    pub mime_type: Option<String>,
    pub referer: Option<String>,
    pub title: Option<String>,
    /// 估算总时长（秒）
    pub duration_seconds: Option<f64>,
    /// 抓到时的请求头（用于透传 UA/Cookie/Referer 等）
    pub headers: Option<Vec<(String, String)>>,
    /// 资源类型（新增）
    pub resource_type: ResourceType,
}

impl From<CapturedResource> for M3u8Stream {
    fn from(resource: CapturedResource) -> Self {
        Self {
            url: resource.url,
            mime_type: resource.mime_type,
            referer: resource.referer,
            title: resource.title,
            duration_seconds: resource.duration_seconds,
            headers: resource.headers,
            resource_type: resource.resource_type,
        }
    }
}

/// 抓取请求（简化版，用于 UI）
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    /// 目标页面 URL
    pub target_url: String,
    /// 自定义浏览器路径（优先级最高）
    pub custom_browser_path: Option<PathBuf>,
    /// 是否启用无头模式
    pub headless: bool,
    /// 抓取超时
    pub timeout: Duration,
    /// 资源筛选类型
    pub filter_type: CaptureFilterType,
}

impl From<CaptureRequest> for CaptureCoreRequest {
    fn from(req: CaptureRequest) -> Self {
        Self {
            target_url: req.target_url,
            custom_browser_path: req.custom_browser_path,
            headless: req.headless,
            timeout: req.timeout,
            filter: ResourceFilter::all(), // 始终监听所有资源类型，筛选在前端进行
            auto_speedup_ads: false,
            speedup_rate: 2.0,
        }
    }
}

/// 抓取事件
#[derive(Debug, Clone)]
pub enum CaptureEvent {
    Log(String),
    Found(M3u8Stream),
    Finished,
    Error(String),
}

impl From<magekit_capture::CaptureEvent> for CaptureEvent {
    fn from(event: magekit_capture::CaptureEvent) -> Self {
        match event {
            magekit_capture::CaptureEvent::Log(msg) => Self::Log(msg),
            magekit_capture::CaptureEvent::Found(resource) => {
                // 转发所有资源（根据用户选择的筛选器已经过滤过了）
                Self::Found(M3u8Stream::from(resource))
            }
            magekit_capture::CaptureEvent::Finished => Self::Finished,
            magekit_capture::CaptureEvent::Error(err) => Self::Error(err),
        }
    }
}

/// 包装后的捕捉会话
pub struct CaptureLegacySession {
    pub rx: mpsc::Receiver<CaptureEvent>,
    cancel_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl CaptureLegacySession {
    pub fn cancel(&mut self) {
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
    }

    pub fn split(
        self,
    ) -> (
        mpsc::Receiver<CaptureEvent>,
        Option<tokio::sync::oneshot::Sender<()>>,
    ) {
        (self.rx, self.cancel_tx)
    }
}

impl AppState {
    /// 启动 m3u8 嗅探任务（基于 magekit_capture）
    pub async fn start_m3u8_capture(
        &self,
        request: CaptureRequest,
    ) -> Result<CaptureLegacySession> {
        // 转换为核心 CaptureRequest
        let core_request: CaptureCoreRequest = request.into();

        // 启动捕捉会话
        let session = magekit_capture::start_capture(core_request).await?;

        // 创建事件转换通道
        let (tx, rx) = mpsc::channel(200);

        // 获取取消句柄
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel();

        // 从 session 中分离接收器和取消器（使用 split 方法）
        let (mut core_rx, core_cancel_tx) = session.split();

        // 启动事件转换任务
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // 接收核心事件并转换
                    event = core_rx.recv() => {
                        match event {
                            Some(evt) => {
                                let legacy_event = CaptureEvent::from(evt);
                                if tx.send(legacy_event).await.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                    // 处理取消请求
                    _ = &mut cancel_rx => {
                        if let Some(cancel) = core_cancel_tx {
                            let _ = cancel.send(());
                        }
                        break;
                    }
                }
            }
        });

        Ok(CaptureLegacySession {
            rx,
            cancel_tx: Some(cancel_tx),
        })
    }
}
