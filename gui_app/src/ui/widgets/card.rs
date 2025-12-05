//! 卡片组件
//!
//! 提供统一的卡片样式容器

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::ActiveTheme;

/// 卡片组件 - 带有统一样式的容器
#[derive(IntoElement)]
pub struct Card {
    children: Vec<AnyElement>,
    title: Option<String>,
    icon: Option<String>,
    padding: Pixels,
}

impl Card {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            title: None,
            icon: None,
            padding: px(16.0),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn padding(mut self, padding: Pixels) -> Self {
        self.padding = padding;
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    pub fn children<I, E>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = E>,
        E: IntoElement,
    {
        self.children.extend(children.into_iter().map(|c| c.into_any_element()));
        self
    }
}

impl RenderOnce for Card {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let bg_color = cx.theme().background;
        let border_color = cx.theme().border;
        let text_color = cx.theme().foreground;
        
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(self.padding)
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
            .rounded(px(12.0))
            .when_some(self.title, |el, title| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(text_color)
                                .when_some(self.icon.clone(), |el, icon| {
                                    el.child(format!("{} {}", icon, title))
                                })
                                .when(self.icon.is_none(), |el| {
                                    el.child(title)
                                })
                        )
                )
            })
            .children(self.children)
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}
