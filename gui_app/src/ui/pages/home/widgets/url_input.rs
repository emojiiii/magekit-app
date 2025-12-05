//! URL 输入组件
//!
//! 提供视频链接输入和解析功能

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_component::Disableable;

/// URL 输入卡片组件
#[derive(IntoElement)]
pub struct UrlInputCard {
    input_state: Entity<InputState>,
    is_loading: bool,
    is_empty: bool,
    on_paste: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_parse: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl UrlInputCard {
    pub fn new(input_state: &Entity<InputState>) -> Self {
        Self {
            input_state: input_state.clone(),
            is_loading: false,
            is_empty: true,
            on_paste: None,
            on_parse: None,
        }
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.is_loading = loading;
        self
    }

    pub fn empty(mut self, empty: bool) -> Self {
        self.is_empty = empty;
        self
    }

    pub fn on_paste(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_paste = Some(Box::new(handler));
        self
    }

    pub fn on_parse(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_parse = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for UrlInputCard {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let button_label = if self.is_loading { "获取中..." } else { "解析" };

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
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0xfafafa))
                    .child("视频链接")
            )
            .child(
                div()
                    .flex()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .child(
                                Input::new(&self.input_state)
                                    .cleanable(true)
                            )
                    )
                    .child({
                        let mut btn = Button::new("paste-btn").label("粘贴");
                        if let Some(handler) = self.on_paste {
                            btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                        }
                        btn
                    })
                    .child({
                        let mut btn = Button::new("parse-btn")
                            .primary()
                            .label(button_label)
                            .disabled(self.is_loading || self.is_empty);
                        if let Some(handler) = self.on_parse {
                            btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                        }
                        btn
                    })
            )
    }
}
