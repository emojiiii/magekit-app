//! 区块组件
//!
//! 提供带标题和描述的内容区块

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::{Icon, IconName};

/// 区块组件 - 带标题和描述的内容区域
#[derive(IntoElement)]
pub struct Section {
    title: String,
    icon: Option<IconName>,
    children: Vec<AnyElement>,
}

impl Section {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            icon: None,
            children: Vec::new(),
        }
    }

    pub fn new_with_icon(title: impl Into<String>, icon: IconName) -> Self {
        Self {
            title: title.into(),
            icon: Some(icon),
            children: Vec::new(),
        }
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
        self.children
            .extend(children.into_iter().map(|c| c.into_any_element()));
        self
    }
}

impl RenderOnce for Section {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let title_color = cx.theme().foreground;

        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                // 标题区域
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when_some(self.icon, |el, icon| {
                        el.child(Icon::new(icon).text_color(title_color))
                    })
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(title_color)
                            .child(self.title),
                    ),
            )
            .children(self.children)
    }
}
