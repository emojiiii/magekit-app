//! 区块组件
//!
//! 提供带标题和描述的内容区块

use gpui::*;
use gpui::prelude::FluentBuilder;

/// 区块组件 - 带标题和描述的内容区域
#[derive(IntoElement)]
pub struct Section {
    title: String,
    description: Option<String>,
    icon: Option<String>,
    children: Vec<AnyElement>,
}

impl Section {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: None,
            icon: None,
            children: Vec::new(),
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
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

impl RenderOnce for Section {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                // 标题区域
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xfafafa))
                            .when_some(self.icon.clone(), |el, icon| {
                                el.child(format!("{} {}", icon, self.title))
                            })
                            .when(self.icon.is_none(), |el| {
                                el.child(self.title.clone())
                            })
                    )
                    .when_some(self.description, |el, desc| {
                        el.child(
                            div()
                                .text_sm()
                                .text_color(rgb(0xa1a1aa))
                                .child(desc)
                        )
                    })
            )
            .children(self.children)
    }
}
