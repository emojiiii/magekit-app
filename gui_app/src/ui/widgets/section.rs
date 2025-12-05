//! 区块组件
//!
//! 提供带标题和描述的内容区块

use gpui::*;
use gpui_component::ActiveTheme;

/// 区块组件 - 带标题和描述的内容区域
#[derive(IntoElement)]
pub struct Section {
    title: String,
    children: Vec<AnyElement>,
}

impl Section {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
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
        self.children.extend(children.into_iter().map(|c| c.into_any_element()));
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
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(title_color)
                    .child(self.title)
            )
            .children(self.children)
    }
}
