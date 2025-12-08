//! 复选框组件
//!
//! 提供可点击的复选框组件

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::ActiveTheme;

/// 复选框组件
#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    label: String,
    checked: bool,
    on_toggle: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl Checkbox {
    pub fn new(id: impl Into<ElementId>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            checked: false,
            on_toggle: None,
        }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn on_toggle(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Checkbox {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted;
        let check_bg = if self.checked { primary } else { muted };
        let check_border = if self.checked { primary } else { cx.theme().border };
        let checked = self.checked;
        let text_color = cx.theme().muted_foreground;
        let checkmark_color = cx.theme().primary_foreground;

        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .when_some(self.on_toggle, |el, handler| {
                el.on_click(move |ev, window, cx| handler(ev, window, cx))
            })
            .child(
                div()
                    .w(px(18.0))
                    .h(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(check_bg)
                    .border_1()
                    .border_color(check_border)
                    .rounded(px(4.0))
                    .when(checked, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(checkmark_color)
                                .child("✓")
                        )
                    })
            )
            .child(
                div()
                    .text_sm()
                    .text_color(text_color)
                    .child(self.label)
            )
    }
}
