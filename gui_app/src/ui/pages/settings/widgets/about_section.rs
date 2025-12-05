//! 关于区块组件

use gpui::*;
use crate::ui::widgets::Section;

/// 关于信息卡片
#[derive(IntoElement)]
pub struct AboutSection;

impl RenderOnce for AboutSection {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        Section::new("关于")
            .icon("ℹ️")
            .description("应用程序信息")
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
                            .gap(px(12.0))
                            .child(AboutItem::new("应用名称", "MageKit 视频下载器"))
                            .child(AboutItem::new("版本", env!("CARGO_PKG_VERSION")))
                            .child(AboutItem::new("构建类型", if cfg!(debug_assertions) { "Debug" } else { "Release" }))
                            .child(AboutItem::new("框架", "GPUI + Rust"))
                    )
            )
    }
}

/// 关于信息项
#[derive(IntoElement)]
struct AboutItem {
    label: String,
    value: String,
}

impl AboutItem {
    fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
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
                    .text_color(rgb(0x71717a))
                    .child(self.label)
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xfafafa))
                    .child(self.value)
            )
    }
}
