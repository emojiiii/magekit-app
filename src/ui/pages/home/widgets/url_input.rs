//! URL 输入组件
//!
//! 提供视频链接输入和解析功能

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_kit::component::Disableable;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme, Icon, IconName};

/// URL 输入卡片组件
#[derive(IntoElement)]
pub struct UrlInputCard {
    input_state: Entity<InputState>,
    is_loading: bool,
    is_empty: bool,
    disabled: bool,
    on_cancel: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_parse: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl UrlInputCard {
    pub fn new(input_state: &Entity<InputState>) -> Self {
        Self {
            input_state: input_state.clone(),
            is_loading: false,
            is_empty: true,
            disabled: false,
            on_cancel: None,
            on_parse: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn on_cancel(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_cancel = Some(Box::new(handler));
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.is_loading = loading;
        self
    }

    pub fn empty(mut self, empty: bool) -> Self {
        self.is_empty = empty;
        self
    }

    pub fn on_parse(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_parse = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for UrlInputCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let button_label = if self.is_loading {
            crate::i18n::tr("获取中...")
        } else {
            crate::i18n::tr("解析")
        };

        GroupBox::new()
            .id("video-link-card")
            .outline()
            .content_style(
                StyleRefinement::default()
                    .p_4()
                    .gap_3()
                    .bg(cx.theme().secondary)
                    .rounded_lg(),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        Icon::new(IconName::Globe)
                            .size_5()
                            .text_color(cx.theme().primary),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(crate::i18n::tr("视频链接")),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(crate::i18n::tr("粘贴视频链接，自动获取视频信息")),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .child(
                        div().flex_1().min_w(px(240.0)).child(
                            Input::new(&self.input_state)
                                .id("home-url-input")
                                .cleanable(true)
                                .disabled(self.disabled)
                                .aria_label(crate::i18n::tr("视频链接")),
                        ),
                    )
                    .child({
                        let mut btn = Button::new("parse-btn")
                            .primary()
                            .icon(IconName::ArrowRight)
                            .label(button_label)
                            .loading(self.is_loading)
                            .disabled(self.disabled || self.is_loading || self.is_empty);
                        if let Some(handler) = self.on_parse {
                            btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                        }
                        btn
                    })
                    .when(self.is_loading, |row| {
                        row.child(
                            Button::new("cancel-parse")
                                .ghost()
                                .label(crate::i18n::tr("取消"))
                                .when_some(self.on_cancel, |button, handler| {
                                    button.on_click(move |event, window, cx| {
                                        handler(event, window, cx)
                                    })
                                }),
                        )
                    }),
            )
    }
}
