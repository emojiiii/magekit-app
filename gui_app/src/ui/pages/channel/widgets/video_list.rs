//! 视频列表组件

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::{Disableable, IconName};
use magekit_shared::ChannelVideoEntry;

use super::video_item::VideoItem;

/// 视频列表组件
pub struct VideoList {
    entries: Vec<ChannelVideoEntry>,
    channel_title: String,
    on_toggle: Option<Box<dyn Fn(usize, bool, &mut Window, &mut App) + 'static>>,
    on_select_all: Option<Box<dyn Fn(bool, &mut Window, &mut App) + 'static>>,
    on_download: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
}

impl VideoList {
    pub fn new(entries: Vec<ChannelVideoEntry>, channel_title: String) -> Self {
        Self {
            entries,
            channel_title,
            on_toggle: None,
            on_select_all: None,
            on_download: None,
        }
    }

    pub fn on_toggle(
        mut self,
        callback: impl Fn(usize, bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_toggle = Some(Box::new(callback));
        self
    }

    pub fn on_select_all(
        mut self,
        callback: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select_all = Some(Box::new(callback));
        self
    }

    pub fn on_download(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_download = Some(Box::new(callback));
        self
    }
}

impl IntoElement for VideoList {
    type Element = Stateful<Div>;

    fn into_element(self) -> Self::Element {
        let entries = self.entries;
        let channel_title = self.channel_title;
        let _on_toggle = self.on_toggle;
        let on_select_all = self.on_select_all;
        let on_download = self.on_download;

        let selected_count = entries.iter().filter(|e| e.selected).count();
        let total_count = entries.len();
        let all_selected = selected_count == total_count && total_count > 0;

        div()
            .id("video-list")
            .w_full()
            .flex()
            .flex_col()
            .gap_4()
            // 标题栏
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(channel_title),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(gpui::hsla(0.0, 0.0, 0.5, 0.6))
                                    .child(format!(
                                        "共 {} 个视频，已选择 {}",
                                        total_count, selected_count
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            // 全选按钮
                            .child(
                                Checkbox::new("select-all")
                                    .checked(all_selected)
                                    .label(if all_selected {
                                        "取消全选"
                                    } else {
                                        "全选"
                                    })
                                    .on_click({
                                        let on_select_all = on_select_all;
                                        move |checked, window, cx| {
                                            if let Some(callback) = &on_select_all {
                                                callback(*checked, window, cx);
                                            }
                                        }
                                    }),
                            )
                            // 下载按钮
                            .child(
                                Button::new("download-selected")
                                    .primary()
                                    .icon(IconName::ArrowDown)
                                    .label(format!("下载选中 ({})", selected_count))
                                    .disabled(selected_count == 0)
                                    .when_some(on_download, |btn, callback| {
                                        btn.on_click(move |_, window, cx| {
                                            callback(window, cx);
                                        })
                                    }),
                            ),
                    ),
            )
            // 视频列表
            .child(
                div()
                    .id("video-list-scroll")
                    .flex()
                    .flex_col()
                    .gap_2()
                    .overflow_y_scroll()
                    .max_h(px(500.0))
                    .children(
                        entries
                            .into_iter()
                            .enumerate()
                            .map(|(index, entry)| VideoItem::new(entry, index)),
                    ),
            )
    }
}
