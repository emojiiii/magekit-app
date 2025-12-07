//! 视频列表项组件

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::checkbox::Checkbox;
use magekit_shared::ChannelVideoEntry;

/// 格式化时长
fn format_duration(seconds: Option<u64>) -> String {
    match seconds {
        Some(secs) => {
            let hours = secs / 3600;
            let minutes = (secs % 3600) / 60;
            let seconds = secs % 60;

            if hours > 0 {
                format!("{}:{:02}:{:02}", hours, minutes, seconds)
            } else {
                format!("{}:{:02}", minutes, seconds)
            }
        }
        None => "未知".to_string(),
    }
}

/// 视频列表项
pub struct VideoItem {
    entry: ChannelVideoEntry,
    index: usize,
    on_toggle: Option<Box<dyn Fn(usize, bool, &mut Window, &mut App) + 'static>>,
}

impl VideoItem {
    pub fn new(entry: ChannelVideoEntry, index: usize) -> Self {
        Self {
            entry,
            index,
            on_toggle: None,
        }
    }

    pub fn on_toggle(
        mut self,
        callback: impl Fn(usize, bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_toggle = Some(Box::new(callback));
        self
    }
}

impl IntoElement for VideoItem {
    type Element = Stateful<Div>;

    fn into_element(self) -> Self::Element {
        let entry = self.entry;
        let index = self.index;
        let is_selected = entry.selected;

        div()
            .id(SharedString::from(format!("video-item-{}", index)))
            .w_full()
            .p_3()
            .rounded_md()
            .border_1()
            .when(is_selected, |div| {
                div.border_color(gpui::hsla(0.6, 0.7, 0.5, 0.5))
                    .bg(gpui::hsla(0.6, 0.7, 0.5, 0.1))
            })
            .when(!is_selected, |div| {
                div.border_color(gpui::hsla(0.0, 0.0, 0.5, 0.15))
                    .bg(gpui::hsla(0.0, 0.0, 0.5, 0.02))
            })
            .hover(|style| style.bg(gpui::hsla(0.0, 0.0, 0.5, 0.08)))
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    // 复选框
                    .child(
                        Checkbox::new(SharedString::from(format!("cb-{}", index)))
                            .checked(is_selected),
                    )
                    // 序号
                    .child(
                        div()
                            .w(px(32.0))
                            .text_sm()
                            .text_color(gpui::hsla(0.0, 0.0, 0.5, 0.5))
                            .child(format!("#{}", index + 1)),
                    )
                    // 缩略图占位
                    .child(
                        div()
                            .w(px(80.0))
                            .h(px(45.0))
                            .rounded_sm()
                            .bg(gpui::hsla(0.0, 0.0, 0.3, 0.2))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .text_color(gpui::hsla(0.0, 0.0, 0.5, 0.4))
                            .child("缩略图"),
                    )
                    // 视频信息
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .overflow_hidden()
                            // 标题
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(entry.title.clone()),
                            )
                            // 时长
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(gpui::hsla(0.0, 0.0, 0.5, 0.6))
                                    .child(format_duration(entry.duration)),
                            ),
                    ),
            )
    }
}
