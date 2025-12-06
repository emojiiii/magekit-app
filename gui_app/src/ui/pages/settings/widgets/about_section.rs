//! 关于区块组件

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::ActiveTheme;
use gpui_component::{Icon, IconName, Sizable};
use crate::ui::widgets::Section;

/// 关于信息卡片
#[derive(IntoElement)]
pub struct AboutSection;

impl RenderOnce for AboutSection {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let card_bg = theme.secondary;
        let border_color = theme.border;
        let title_color = theme.foreground;
        let muted_color = theme.muted_foreground;
        
        Section::new_with_icon("关于", IconName::Info)
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
                            .gap(px(10.0))
                            .child(AboutItem::new_with_icon(IconName::Star, "应用名称", "MageKit 视频下载器", muted_color, title_color))
                            .child(AboutItem::new_with_icon(IconName::CircleCheck, "版本", env!("CARGO_PKG_VERSION"), muted_color, title_color))
                            .child(AboutItem::new_with_icon(IconName::Settings2, "构建类型", if cfg!(debug_assertions) { "Debug" } else { "Release" }, muted_color, title_color))
                    )
            )
    }
}

/// 关于信息项
#[derive(IntoElement)]
struct AboutItem {
    icon: Option<IconName>,
    label: String,
    value: String,
    label_color: Hsla,
    value_color: Hsla,
}

impl AboutItem {
    fn new_with_icon(icon: IconName, label: impl Into<String>, value: impl Into<String>, label_color: Hsla, value_color: Hsla) -> Self {
        Self {
            icon: Some(icon),
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
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when_some(self.icon, |el, icon| {
                        el.child(Icon::new(icon).small().text_color(self.label_color))
                    })
                    .child(
                        div()
                            .text_sm()
                            .text_color(self.label_color)
                            .child(self.label)
                    )
            )
            .child(
                div()
                    .text_sm()
                    .text_color(self.value_color)
                    .child(self.value)
            )
    }
}
