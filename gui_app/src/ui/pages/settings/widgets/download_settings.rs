//! 下载设置组件

use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::IconName;
use crate::ui::widgets::Section;

/// 下载设置数据
#[derive(Debug, Clone)]
pub struct DownloadSettingsData {
    pub download_path: String,
    pub max_concurrent: usize,
}

/// 下载设置卡片
#[derive(IntoElement)]
pub struct DownloadSettingsCard {
    download_path: String,
    max_concurrent: usize,
    on_browse: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_increment: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_decrement: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl DownloadSettingsCard {
    pub fn new(download_path: impl Into<String>, max_concurrent: usize) -> Self {
        Self {
            download_path: download_path.into(),
            max_concurrent,
            on_browse: None,
            on_increment: None,
            on_decrement: None,
        }
    }

    pub fn on_browse(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_browse = Some(Box::new(handler));
        self
    }

    pub fn on_increment(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_increment = Some(Box::new(handler));
        self
    }

    pub fn on_decrement(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_decrement = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for DownloadSettingsCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let card_bg = theme.secondary;
        let border_color = theme.border;
        let title_color = theme.foreground;
        let muted_color = theme.muted_foreground;
        let input_bg = theme.background;

        Section::new("📥 下载")
            .child(
                div()
                    .p(px(20.0))
                    .rounded(px(12.0))
                    .bg(card_bg)
                    .border_1()
                    .border_color(border_color)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(20.0))
                            // 下载目录
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(
                                                div()
                                                    .text_lg()
                                                    .child("📁")
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(title_color)
                                                            .child("下载目录")
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(muted_color)
                                                            .child("视频下载的默认保存位置")
                                                    )
                                            )
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(8.0))
                                            .child(
                                                // 路径显示区域
                                                div()
                                                    .flex_1()
                                                    .px(px(12.0))
                                                    .py(px(8.0))
                                                    .bg(input_bg)
                                                    .border_1()
                                                    .border_color(border_color)
                                                    .rounded(px(6.0))
                                                    .text_sm()
                                                    .text_color(title_color)
                                                    .overflow_hidden()
                                                    .child(self.download_path.clone())
                                            )
                                            .child({
                                                let mut btn = Button::new("select-dir")
                                                    .label("浏览...");
                                                if let Some(handler) = self.on_browse {
                                                    btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                                                }
                                                btn
                                            })
                                    )
                            )
                            // 最大并发数
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(
                                                div()
                                                    .text_lg()
                                                    .child("⚡")
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(title_color)
                                                            .child("最大并发下载数")
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(muted_color)
                                                            .child("同时下载的任务数量 (1-10)")
                                                    )
                                            )
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .child({
                                                let mut btn = Button::new("dec-concurrent")
                                                    .ghost()
                                                    .compact()
                                                    .icon(IconName::Minus);
                                                if self.max_concurrent > 1 {
                                                    if let Some(handler) = self.on_decrement {
                                                        btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                                                    }
                                                }
                                                btn
                                            })
                                            .child(
                                                div()
                                                    .w(px(40.0))
                                                    .h(px(32.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .bg(input_bg)
                                                    .border_1()
                                                    .border_color(border_color)
                                                    .rounded(px(6.0))
                                                    .text_sm()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(title_color)
                                                    .child(format!("{}", self.max_concurrent))
                                            )
                                            .child({
                                                let mut btn = Button::new("inc-concurrent")
                                                    .ghost()
                                                    .compact()
                                                    .icon(IconName::Plus);
                                                if self.max_concurrent < 10 {
                                                    if let Some(handler) = self.on_increment {
                                                        btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                                                    }
                                                }
                                                btn
                                            })
                                    )
                            )
                    )
            )
    }
}
