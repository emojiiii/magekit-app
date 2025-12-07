//! 质量选择器组件
//!
//! 提供视频质量选择功能

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme;
use std::sync::Arc;

/// 质量选项
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityOption {
    Best,
    P1080,
    P720,
    P480,
    AudioOnly,
}

impl QualityOption {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Best => "最佳",
            Self::P1080 => "1080p",
            Self::P720 => "720p",
            Self::P480 => "480p",
            Self::AudioOnly => "仅音频",
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::Best,
            Self::P1080,
            Self::P720,
            Self::P480,
            Self::AudioOnly,
        ]
    }
}

/// 质量选择器组件
#[derive(IntoElement)]
pub struct QualitySelector {
    selected: QualityOption,
    on_select: Option<Arc<dyn Fn(&QualityOption, &mut Window, &mut App) + Send + Sync + 'static>>,
}

impl QualitySelector {
    pub fn new(selected: QualityOption) -> Self {
        Self {
            selected,
            on_select: None,
        }
    }

    pub fn on_select(
        mut self,
        handler: impl Fn(&QualityOption, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.on_select = Some(Arc::new(handler));
        self
    }
}

impl RenderOnce for QualitySelector {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let selected = self.selected;
        let on_select = self.on_select;

        let card_bg = cx.theme().secondary;
        let border_color = cx.theme().border;
        let title_color = cx.theme().foreground;
        let primary_color = cx.theme().primary;
        let primary_fg = cx.theme().primary_foreground;
        let muted_bg = cx.theme().muted;
        let muted_color = cx.theme().muted_foreground;

        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(16.0))
            .bg(card_bg)
            .border_1()
            .border_color(border_color)
            .rounded(px(12.0))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(title_color)
                    .child("🎬 视频质量"),
            )
            .child(div().flex().flex_wrap().gap(px(8.0)).children(
                QualityOption::all().into_iter().map({
                    let on_select = on_select.clone();
                    move |quality| {
                        let is_selected = selected == quality;
                        let bg_color = if is_selected { primary_color } else { muted_bg };
                        let text_color = if is_selected { primary_fg } else { muted_color };
                        let btn_id: SharedString = format!("quality-{:?}", quality).into();
                        let on_select = on_select.clone();

                        div()
                            .id(btn_id)
                            .px(px(12.0))
                            .py(px(6.0))
                            .bg(bg_color)
                            .rounded(px(6.0))
                            .cursor_pointer()
                            .text_sm()
                            .text_color(text_color)
                            .hover(|this| this.opacity(0.8))
                            .child(quality.label())
                            .when_some(on_select, |el, handler| {
                                el.on_click(move |_ev, window, cx| {
                                    handler(&quality, window, cx);
                                })
                            })
                    }
                }),
            ))
    }
}
