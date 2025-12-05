//! 关于区块组件

use gpui::*;
use gpui_component::ActiveTheme;
use crate::ui::widgets::Section;

/// 关于信息卡片
#[derive(IntoElement)]
pub struct AboutSection;

impl RenderOnce for AboutSection {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let card_bg = cx.theme().background;
        let border_color = cx.theme().border;
        let title_color = cx.theme().foreground;
        let muted_color = cx.theme().muted_foreground;
        
        Section::new("关于")
            .icon("ℹ️")
            .description("应用程序信息")
            .child(
                div()
                    .p(px(16.0))
                    .rounded(px(12.0))
                    .bg(card_bg)
                    .border_1()
                    .border_color(border_color)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(AboutItem::new("应用名称", "MageKit 视频下载器", muted_color, title_color))
                            .child(AboutItem::new("版本", env!("CARGO_PKG_VERSION"), muted_color, title_color))
                            .child(AboutItem::new("构建类型", if cfg!(debug_assertions) { "Debug" } else { "Release" }, muted_color, title_color))
                            .child(AboutItem::new("框架", "GPUI + Rust", muted_color, title_color))
                    )
            )
    }
}

/// 关于信息项
#[derive(IntoElement)]
struct AboutItem {
    label: String,
    value: String,
    label_color: Hsla,
    value_color: Hsla,
}

impl AboutItem {
    fn new(label: impl Into<String>, value: impl Into<String>, label_color: Hsla, value_color: Hsla) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            label_color,
            value_color,
        }
    }
}

impl RenderOnce for AboutItem {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .justify_between()
            .child(
                div()
                    .text_sm()
                    .text_color(self.label_color)
                    .child(self.label)
            )
            .child(
                div()
                    .text_sm()
                    .text_color(self.value_color)
                    .child(self.value)
            )
    }
}
