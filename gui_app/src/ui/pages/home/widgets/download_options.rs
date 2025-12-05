//! 下载选项组件
//!
//! 提供下载配置选项

use gpui::*;
use crate::ui::widgets::Checkbox;

/// 下载选项数据
#[derive(Debug, Clone)]
pub struct DownloadOptionsData {
    pub embed_metadata: bool,
    pub embed_thumbnail: bool,
    pub download_subtitles: bool,
    pub audio_only: bool,
}

impl Default for DownloadOptionsData {
    fn default() -> Self {
        Self {
            embed_metadata: true,
            embed_thumbnail: false,
            download_subtitles: false,
            audio_only: false,
        }
    }
}

/// 下载选项卡片组件
#[derive(IntoElement)]
pub struct DownloadOptionsCard {
    options: DownloadOptionsData,
    on_toggle_metadata: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_toggle_thumbnail: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_toggle_subtitles: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_toggle_audio: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl DownloadOptionsCard {
    pub fn new(options: DownloadOptionsData) -> Self {
        Self {
            options,
            on_toggle_metadata: None,
            on_toggle_thumbnail: None,
            on_toggle_subtitles: None,
            on_toggle_audio: None,
        }
    }

    pub fn on_toggle_metadata(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle_metadata = Some(Box::new(handler));
        self
    }

    pub fn on_toggle_thumbnail(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle_thumbnail = Some(Box::new(handler));
        self
    }

    pub fn on_toggle_subtitles(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle_subtitles = Some(Box::new(handler));
        self
    }

    pub fn on_toggle_audio(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle_audio = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for DownloadOptionsCard {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(16.0))
            .bg(rgb(0x18181b))
            .border_1()
            .border_color(rgb(0x3f3f46))
            .rounded(px(12.0))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0xfafafa))
                    .child("⚙️ 下载选项")
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(24.0))
                    .child({
                        let mut cb = Checkbox::new("opt-metadata", "嵌入元数据")
                            .checked(self.options.embed_metadata);
                        if let Some(handler) = self.on_toggle_metadata {
                            cb = cb.on_toggle(move |ev, window, cx| handler(ev, window, cx));
                        }
                        cb
                    })
                    .child({
                        let mut cb = Checkbox::new("opt-thumbnail", "嵌入缩略图")
                            .checked(self.options.embed_thumbnail);
                        if let Some(handler) = self.on_toggle_thumbnail {
                            cb = cb.on_toggle(move |ev, window, cx| handler(ev, window, cx));
                        }
                        cb
                    })
                    .child({
                        let mut cb = Checkbox::new("opt-subtitles", "下载字幕")
                            .checked(self.options.download_subtitles);
                        if let Some(handler) = self.on_toggle_subtitles {
                            cb = cb.on_toggle(move |ev, window, cx| handler(ev, window, cx));
                        }
                        cb
                    })
                    .child({
                        let mut cb = Checkbox::new("opt-audio", "仅提取音频")
                            .checked(self.options.audio_only);
                        if let Some(handler) = self.on_toggle_audio {
                            cb = cb.on_toggle(move |ev, window, cx| handler(ev, window, cx));
                        }
                        cb
                    })
            )
    }
}

/// 保存位置卡片组件
#[derive(IntoElement)]
pub struct OutputPathCard {
    path: String,
    on_browse: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl OutputPathCard {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            on_browse: None,
        }
    }

    pub fn on_browse(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_browse = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for OutputPathCard {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        use gpui_component::button::{Button, ButtonVariants};

        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(16.0))
            .bg(rgb(0x18181b))
            .border_1()
            .border_color(rgb(0x3f3f46))
            .rounded(px(12.0))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0xfafafa))
                    .child("📁 保存位置")
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .h(px(36.0))
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .bg(rgb(0x09090b))
                            .border_1()
                            .border_color(rgb(0x3f3f46))
                            .rounded(px(6.0))
                            .text_sm()
                            .text_color(rgb(0xa1a1aa))
                            .overflow_hidden()
                            .child(self.path)
                    )
                    .child({
                        let mut btn = Button::new("browse-btn").ghost().label("浏览");
                        if let Some(handler) = self.on_browse {
                            btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                        }
                        btn
                    })
            )
    }
}
