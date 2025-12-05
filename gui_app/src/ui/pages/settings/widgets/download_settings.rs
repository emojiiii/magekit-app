//! 下载设置组件

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_component::Disableable;
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
    download_path_input: Entity<InputState>,
    max_concurrent: usize,
    on_browse: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_increment: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_decrement: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl DownloadSettingsCard {
    pub fn new(download_path_input: &Entity<InputState>, max_concurrent: usize) -> Self {
        Self {
            download_path_input: download_path_input.clone(),
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
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        Section::new("下载设置")
            .icon("⬇️")
            .description("配置下载相关的默认选项")
            .child(
                div()
                    .p(px(16.0))
                    .rounded(px(12.0))
                    .bg(rgb(0x18181b))
                    .border_1()
                    .border_color(rgb(0x3f3f46))
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
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(0xfafafa))
                                            .child("下载目录")
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(8.0))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .child(Input::new(&self.download_path_input))
                                            )
                                            .child({
                                                let mut btn = Button::new("select-dir").label("浏览");
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
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(0xfafafa))
                                                    .child("最大并发下载数")
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x71717a))
                                                    .child("同时下载的任务数量 (1-10)")
                                            )
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child({
                                                let mut btn = Button::new("dec-concurrent")
                                                    .ghost()
                                                    .label("-")
                                                    .disabled(self.max_concurrent <= 1);
                                                if let Some(handler) = self.on_decrement {
                                                    btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                                                }
                                                btn
                                            })
                                            .child(
                                                div()
                                                    .w(px(40.0))
                                                    .h(px(36.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .bg(rgb(0x09090b))
                                                    .border_1()
                                                    .border_color(rgb(0x3f3f46))
                                                    .rounded(px(6.0))
                                                    .text_sm()
                                                    .text_color(rgb(0xfafafa))
                                                    .child(format!("{}", self.max_concurrent))
                                            )
                                            .child({
                                                let mut btn = Button::new("inc-concurrent")
                                                    .ghost()
                                                    .label("+")
                                                    .disabled(self.max_concurrent >= 10);
                                                if let Some(handler) = self.on_increment {
                                                    btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                                                }
                                                btn
                                            })
                                    )
                            )
                    )
            )
    }
}
