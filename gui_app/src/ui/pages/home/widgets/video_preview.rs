//! 视频预览组件
//!
//! 显示解析后的视频信息

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::ActiveTheme;
use std::sync::Arc;

/// 视频格式信息 (从解析获取)
#[derive(Debug, Clone, PartialEq)]
pub struct VideoFormatInfo {
    pub format_id: String,
    pub label: String,         // 如 "1080p", "720p", "音频"
    pub ext: String,           // 如 "mp4", "webm"
    pub filesize: Option<u64>, // 文件大小
    pub has_video: bool,
    pub has_audio: bool,
}

impl VideoFormatInfo {
    /// 格式化文件大小
    pub fn format_filesize(&self) -> String {
        match self.filesize {
            Some(bytes) => {
                const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
                let mut size = bytes as f64;
                let mut unit_index = 0;
                while size >= 1024.0 && unit_index < UNITS.len() - 1 {
                    size /= 1024.0;
                    unit_index += 1;
                }
                format!("{:.1} {}", size, UNITS[unit_index])
            }
            None => "未知大小".to_string(),
        }
    }
}

/// 视频预览信息
#[derive(Debug, Clone, PartialEq)]
pub struct VideoInfo {
    pub title: String,
    pub duration: String,
    pub thumbnail: Option<String>,
    pub uploader: Option<String>,
    pub formats: Vec<VideoFormatInfo>,   // 可用格式列表
    pub url: String,                      // 原始 URL
}

/// 视频预览卡片 - 空闲状态
#[derive(IntoElement)]
pub struct VideoPreviewIdle;

impl RenderOnce for VideoPreviewIdle {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_dark = cx.theme().mode.is_dark();
        let bg_color = if is_dark { rgb(0x18181b) } else { rgb(0xffffff) };
        let border_color = if is_dark { rgb(0x3f3f46) } else { rgb(0xe4e4e7) };
        let text_color = if is_dark { rgb(0xa1a1aa) } else { rgb(0x71717a) };
        let sub_text_color = if is_dark { rgb(0x71717a) } else { rgb(0xa1a1aa) };
        
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p(px(40.0))
            .gap(px(12.0))
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
            .rounded(px(12.0))
            .child(div().text_3xl().child("🎬"))
            .child(
                div()
                    .text_base()
                    .text_color(text_color)
                    .child("输入视频链接开始下载")
            )
            .child(
                div()
                    .text_sm()
                    .text_color(sub_text_color)
                    .child("支持 YouTube、Bilibili、Twitter 等主流平台")
            )
    }
}

/// 视频预览卡片 - 加载中状态
#[derive(IntoElement)]
pub struct VideoPreviewLoading;

impl RenderOnce for VideoPreviewLoading {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_dark = cx.theme().mode.is_dark();
        let bg_color = if is_dark { rgb(0x18181b) } else { rgb(0xffffff) };
        let border_color = if is_dark { rgb(0x3f3f46) } else { rgb(0xe4e4e7) };
        let text_color = if is_dark { rgb(0xfafafa) } else { rgb(0x18181b) };
        
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p(px(40.0))
            .gap(px(12.0))
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
            .rounded(px(12.0))
            .child(div().text_3xl().child("⏳"))
            .child(
                div()
                    .text_base()
                    .text_color(text_color)
                    .child("正在获取视频信息...")
            )
    }
}

/// 视频预览卡片 - 就绪状态
#[derive(IntoElement)]
pub struct VideoPreviewReady {
    info: VideoInfo,
    selected_format_id: Option<String>,
    on_cancel: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_download: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_download_thumbnail: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_select_format: Option<Arc<dyn Fn(&str, &mut Window, &mut App) + 'static>>,
}

impl VideoPreviewReady {
    pub fn new(info: VideoInfo) -> Self {
        // 默认选中第一个视频格式
        let default_format = info.formats.iter()
            .find(|f| f.has_video)
            .map(|f| f.format_id.clone());
        
        Self {
            info,
            selected_format_id: default_format,
            on_cancel: None,
            on_download: None,
            on_download_thumbnail: None,
            on_select_format: None,
        }
    }
    
    pub fn selected_format(mut self, format_id: Option<String>) -> Self {
        self.selected_format_id = format_id;
        self
    }
    
    /// 获取当前选中的格式 ID
    pub fn get_selected_format_id(&self) -> Option<String> {
        self.selected_format_id.clone()
    }

    pub fn on_cancel(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Box::new(handler));
        self
    }

    pub fn on_download(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_download = Some(Box::new(handler));
        self
    }
    
    pub fn on_download_thumbnail(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_download_thumbnail = Some(Box::new(handler));
        self
    }
    
    /// 设置格式选择回调
    pub fn on_select_format(mut self, handler: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_select_format = Some(Arc::new(handler));
        self
    }
}

impl RenderOnce for VideoPreviewReady {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_dark = cx.theme().mode.is_dark();
        let bg_color = if is_dark { rgb(0x18181b) } else { rgb(0xffffff) };
        let border_color = if is_dark { rgb(0x3f3f46) } else { rgb(0xe4e4e7) };
        let thumb_bg = if is_dark { rgb(0x27272a) } else { rgb(0xf4f4f5) };
        let title_color = if is_dark { rgb(0xfafafa) } else { rgb(0x18181b) };
        let meta_color = if is_dark { rgb(0xa1a1aa) } else { rgb(0x71717a) };
        let format_bg = if is_dark { rgb(0x27272a) } else { rgb(0xf4f4f5) };
        let format_selected_bg = rgb(0x3b82f6);
        let format_text = if is_dark { rgb(0xe4e4e7) } else { rgb(0x3f3f46) };
        let section_title_color = if is_dark { rgb(0xd4d4d8) } else { rgb(0x52525b) };
        
        // 分离视频格式和音频格式
        let video_formats: Vec<_> = self.info.formats.iter()
            .filter(|f| f.has_video)
            .cloned()
            .collect();
        let audio_formats: Vec<_> = self.info.formats.iter()
            .filter(|f| !f.has_video && f.has_audio)
            .cloned()
            .collect();
        
        // 合并所有格式用于选择（暂时保留，将来可能用于其他用途）
        let _all_formats: Vec<_> = video_formats.iter()
            .chain(audio_formats.iter())
            .cloned()
            .collect();
        
        let selected_id = self.selected_format_id.clone();
        let on_download = self.on_download;
        let on_download_thumbnail = self.on_download_thumbnail;
        let on_select_format = self.on_select_format;
        let has_thumbnail = self.info.thumbnail.is_some();
        
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .p(px(20.0))
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
            .rounded(px(12.0))
            // 顶部：缩略图 + 基本信息
            .child(
                div()
                    .flex()
                    .gap(px(16.0))
                    // 缩略图区域
                    .child(
                        div()
                            .relative()
                            .w(px(200.0))
                            .h(px(112.0))
                            .bg(thumb_bg)
                            .rounded(px(8.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(div().text_3xl().child("🎬"))
                            // 封面下载按钮（悬浮在右下角）
                            .when(has_thumbnail, |el| {
                                el.child(
                                    div()
                                        .absolute()
                                        .bottom(px(4.0))
                                        .right(px(4.0))
                                        .child({
                                            let mut btn = Button::new("download-thumb-btn")
                                                .ghost()
                                                .compact()
                                                .label("📥");
                                            if let Some(handler) = on_download_thumbnail {
                                                btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                                            }
                                            btn
                                        })
                                )
                            })
                    )
                    // 信息区域
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
                                    .text_color(title_color)
                                    .line_clamp(2)
                                    .child(self.info.title.clone())
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(12.0))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(meta_color)
                                            .child(format!("⏱️ {}", self.info.duration))
                                    )
                                    .when_some(self.info.uploader.clone(), |el, uploader| {
                                        el.child(
                                            div()
                                                .text_sm()
                                                .text_color(meta_color)
                                                .child(format!("👤 {}", uploader))
                                        )
                                    })
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(meta_color)
                                            .child(format!("📁 {} 种格式", self.info.formats.len()))
                                    )
                            )
                    )
            )
            // 视频质量选择（使用自定义可点击格式项）
            .when(!video_formats.is_empty(), |el| {
                let selected = selected_id.clone();
                let on_select = on_select_format.clone();
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(section_title_color)
                                .child("🎬 视频质量")
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(8.0))
                                .children(video_formats.into_iter().map({
                                    let selected = selected.clone();
                                    let on_select = on_select.clone();
                                    move |fmt| {
                                        let is_selected = selected.as_ref() == Some(&fmt.format_id);
                                        let bg = if is_selected { format_selected_bg } else { format_bg };
                                        let text_c = if is_selected { rgb(0xffffff) } else { format_text };
                                        let format_id = fmt.format_id.clone();
                                        let on_select = on_select.clone();
                                        
                                        div()
                                            .id(SharedString::from(format!("fmt-{}", fmt.format_id)))
                                            .px(px(12.0))
                                            .py(px(6.0))
                                            .bg(bg)
                                            .rounded(px(6.0))
                                            .cursor_pointer()
                                            .on_click({
                                                let format_id = format_id.clone();
                                                move |_ev, window, cx| {
                                                    if let Some(ref handler) = on_select {
                                                        handler(&format_id, window, cx);
                                                    }
                                                }
                                            })
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(2.0))
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(text_c)
                                                            .child(fmt.label.clone())
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(if is_selected { rgba(0xffffffcc) } else { meta_color })
                                                            .child(format!("{} · {}", fmt.ext.to_uppercase(), fmt.format_filesize()))
                                                    )
                                            )
                                    }
                                }))
                        )
                )
            })
            // 音频格式选择
            .when(!audio_formats.is_empty(), |el| {
                let selected = selected_id.clone();
                let on_select = on_select_format.clone();
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(section_title_color)
                                .child("🎵 仅音频")
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(8.0))
                                .children(audio_formats.into_iter().map({
                                    let selected = selected.clone();
                                    let on_select = on_select.clone();
                                    move |fmt| {
                                        let is_selected = selected.as_ref() == Some(&fmt.format_id);
                                        let bg = if is_selected { format_selected_bg } else { format_bg };
                                        let text_c = if is_selected { rgb(0xffffff) } else { format_text };
                                        let format_id = fmt.format_id.clone();
                                        let on_select = on_select.clone();
                                        
                                        div()
                                            .id(SharedString::from(format!("fmt-{}", fmt.format_id)))
                                            .px(px(12.0))
                                            .py(px(6.0))
                                            .bg(bg)
                                            .rounded(px(6.0))
                                            .cursor_pointer()
                                            .on_click({
                                                let format_id = format_id.clone();
                                                move |_ev, window, cx| {
                                                    if let Some(ref handler) = on_select {
                                                        handler(&format_id, window, cx);
                                                    }
                                                }
                                            })
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(2.0))
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(text_c)
                                                            .child(fmt.label.clone())
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(if is_selected { rgba(0xffffffcc) } else { meta_color })
                                                            .child(format!("{} · {}", fmt.ext.to_uppercase(), fmt.format_filesize()))
                                                    )
                                            )
                                    }
                                }))
                        )
                )
            })
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
                        if let Some(handler) = on_download {
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
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let progress_percent = (self.progress * 100.0).min(100.0);
        let is_dark = cx.theme().mode.is_dark();
        let bg_color = if is_dark { rgb(0x18181b) } else { rgb(0xffffff) };
        let border_color = if is_dark { rgb(0x3f3f46) } else { rgb(0xe4e4e7) };
        let title_color = if is_dark { rgb(0xfafafa) } else { rgb(0x18181b) };
        let meta_color = if is_dark { rgb(0xa1a1aa) } else { rgb(0x71717a) };
        let progress_bg = if is_dark { rgb(0x27272a) } else { rgb(0xe4e4e7) };
        let sub_text_color = if is_dark { rgb(0x71717a) } else { rgb(0xa1a1aa) };

        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .p(px(20.0))
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
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
                            .text_color(title_color)
                            .child("正在下载...")
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(meta_color)
                            .child(self.speed)
                    )
            )
            .child(
                div()
                    .h(px(8.0))
                    .w_full()
                    .bg(progress_bg)
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
                    .text_color(sub_text_color)
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
