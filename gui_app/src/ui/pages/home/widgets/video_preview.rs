//! 视频预览组件
//!
//! 显示解析后的视频信息

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::button::{Button, ButtonVariants};

/// 视频预览信息
#[derive(Debug, Clone, PartialEq)]
pub struct VideoInfo {
    pub title: String,
    pub duration: String,
    pub thumbnail: Option<String>,
    pub uploader: Option<String>,
}

/// 视频预览卡片 - 空闲状态
#[derive(IntoElement)]
pub struct VideoPreviewIdle;

impl RenderOnce for VideoPreviewIdle {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p(px(40.0))
            .gap(px(12.0))
            .bg(rgb(0x18181b))
            .border_1()
            .border_color(rgb(0x3f3f46))
            .rounded(px(12.0))
            .child(div().text_3xl().child("🎬"))
            .child(
                div()
                    .text_base()
                    .text_color(rgb(0xa1a1aa))
                    .child("输入视频链接开始下载")
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x71717a))
                    .child("支持 YouTube、Bilibili、Twitter 等主流平台")
            )
    }
}

/// 视频预览卡片 - 加载中状态
#[derive(IntoElement)]
pub struct VideoPreviewLoading;

impl RenderOnce for VideoPreviewLoading {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p(px(40.0))
            .gap(px(12.0))
            .bg(rgb(0x18181b))
            .border_1()
            .border_color(rgb(0x3f3f46))
            .rounded(px(12.0))
            .child(div().text_3xl().child("⏳"))
            .child(
                div()
                    .text_base()
                    .text_color(rgb(0xfafafa))
                    .child("正在获取视频信息...")
            )
    }
}

/// 视频预览卡片 - 就绪状态
#[derive(IntoElement)]
pub struct VideoPreviewReady {
    info: VideoInfo,
    on_cancel: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_download: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl VideoPreviewReady {
    pub fn new(info: VideoInfo) -> Self {
        Self {
            info,
            on_cancel: None,
            on_download: None,
        }
    }

    pub fn on_cancel(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Box::new(handler));
        self
    }

    pub fn on_download(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_download = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for VideoPreviewReady {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .p(px(20.0))
            .bg(rgb(0x18181b))
            .border_1()
            .border_color(rgb(0x3f3f46))
            .rounded(px(12.0))
            .child(
                div()
                    .flex()
                    .gap(px(16.0))
                    // 缩略图
                    .child(
                        div()
                            .w(px(200.0))
                            .h(px(112.0))
                            .bg(rgb(0x27272a))
                            .rounded(px(8.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(div().text_3xl().child("🎬"))
                    )
                    // 信息
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0xfafafa))
                                    .child(self.info.title.clone())
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(16.0))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(0xa1a1aa))
                                            .child(format!("⏱️ {}", self.info.duration))
                                    )
                                    .when_some(self.info.uploader.clone(), |el, uploader| {
                                        el.child(
                                            div()
                                                .text_sm()
                                                .text_color(rgb(0xa1a1aa))
                                                .child(format!("👤 {}", uploader))
                                        )
                                    })
                            )
                    )
            )
            // 操作按钮
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child({
                        let mut btn = Button::new("cancel-btn").ghost().label("取消");
                        if let Some(handler) = self.on_cancel {
                            btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                        }
                        btn
                    })
                    .child({
                        let mut btn = Button::new("start-download-btn").primary().label("开始下载");
                        if let Some(handler) = self.on_download {
                            btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                        }
                        btn
                    })
            )
    }
}

/// 视频预览卡片 - 下载中状态
#[derive(IntoElement)]
pub struct VideoPreviewDownloading {
    progress: f32,
    speed: String,
}

impl VideoPreviewDownloading {
    pub fn new(progress: f32, speed: impl Into<String>) -> Self {
        Self {
            progress,
            speed: speed.into(),
        }
    }
}

impl RenderOnce for VideoPreviewDownloading {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let progress_percent = (self.progress * 100.0).min(100.0);

        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .p(px(20.0))
            .bg(rgb(0x18181b))
            .border_1()
            .border_color(rgb(0x3f3f46))
            .rounded(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0xfafafa))
                            .child("正在下载...")
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xa1a1aa))
                            .child(self.speed)
                    )
            )
            .child(
                div()
                    .h(px(8.0))
                    .w_full()
                    .bg(rgb(0x27272a))
                    .rounded(px(4.0))
                    .overflow_hidden()
                    .child(
                        div()
                            .h_full()
                            .w(relative(self.progress))
                            .bg(rgb(0x3b82f6))
                            .rounded(px(4.0))
                    )
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x71717a))
                    .child(format!("{:.1}% 完成", progress_percent))
            )
    }
}

/// 视频预览卡片 - 完成状态
#[derive(IntoElement)]
pub struct VideoPreviewCompleted {
    path: String,
    on_new_download: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl VideoPreviewCompleted {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            on_new_download: None,
        }
    }

    pub fn on_new_download(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_new_download = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for VideoPreviewCompleted {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p(px(40.0))
            .gap(px(16.0))
            .bg(rgb(0x052e16))
            .border_1()
            .border_color(rgb(0x166534))
            .rounded(px(12.0))
            .child(div().text_3xl().child("✅"))
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0x4ade80))
                    .child("下载完成!")
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x86efac))
                    .child(self.path)
            )
            .child({
                let mut btn = Button::new("new-download-btn").label("新下载");
                if let Some(handler) = self.on_new_download {
                    btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                }
                btn
            })
    }
}

/// 视频预览卡片 - 错误状态
#[derive(IntoElement)]
pub struct VideoPreviewError {
    message: String,
    on_retry: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl VideoPreviewError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            on_retry: None,
        }
    }

    pub fn on_retry(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_retry = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for VideoPreviewError {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p(px(40.0))
            .gap(px(16.0))
            .bg(rgb(0x450a0a))
            .border_1()
            .border_color(rgb(0x991b1b))
            .rounded(px(12.0))
            .child(div().text_3xl().child("❌"))
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xfca5a5))
                    .child("出错了")
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xfecaca))
                    .child(self.message)
            )
            .child({
                let mut btn = Button::new("retry-btn").label("重试");
                if let Some(handler) = self.on_retry {
                    btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                }
                btn
            })
    }
}
