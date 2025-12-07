//! 直播间卡片组件

use gpui::*;
use gpui_component::ActiveTheme;
use std::time::Duration;

use super::{LiveRoomStatus, LiveStatus, RecordingStatus};

/// 直播间卡片组件
pub struct LiveRoomCard {
    room: LiveRoomStatus,
    compact: bool,
}

impl LiveRoomCard {
    pub fn new(room: LiveRoomStatus) -> Self {
        Self {
            room,
            compact: false,
        }
    }

    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    fn format_duration(duration: &Duration) -> String {
        let total_secs = duration.as_secs();
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;

        if hours > 0 {
            format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
        } else {
            format!("{:02}:{:02}", minutes, seconds)
        }
    }

    fn format_file_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }
}

impl Render for LiveRoomCard {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let card_bg = cx.theme().surface;
        let border_color = cx.theme().border;
        let title_color = cx.theme().foreground;
        let muted_color = cx.theme().muted_foreground;

        let is_live = self.room.status == LiveStatus::Live;
        let is_recording = self.room.is_recording;

        div()
            .flex()
            .flex_col()
            .p_4()
            .bg(card_bg)
            .border_1()
            .border_color(border_color)
            .rounded_lg()
            .gap_3()
            // 顶部信息
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(title_color)
                                            .child(&self.room.anchor_name),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .px_2()
                                            .py_1()
                                            .bg(gpui::blue())
                                            .text_color(gpui::white())
                                            .rounded_md()
                                            .child(&self.room.platform),
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted_color)
                                    .child(&self.room.title),
                            ),
                    )
                    .child(
                        div()
                            .w_3()
                            .h_3()
                            .rounded_full()
                            .bg(if is_live {
                                gpui::green()
                            } else if self.room.status == LiveStatus::Playback {
                                gpui::yellow()
                            } else {
                                gpui::gray()
                            }),
                    ),
            )
            // 状态信息
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted_color)
                                    .child(format!(
                                        "{}",
                                        match self.room.status {
                                            LiveStatus::Live => "🔴 直播中",
                                            LiveStatus::Offline => "⚫ 未开播",
                                            LiveStatus::Playback => "🟡 轮播中",
                                        }
                                    )),
                            )
                            .child(
                                if let Some(viewers) = self.room.viewer_count {
                                    Some(
                                        div()
                                            .text_sm()
                                            .text_color(muted_color)
                                            .child(format!("👥 {}", viewers)),
                                    )
                                } else {
                                    None
                                },
                            )
                            .child(
                                if self.room.auto_record {
                                    Some(
                                        div()
                                            .text_xs()
                                            .px_2()
                                            .py_1()
                                            .bg(gpui::green())
                                            .text_color(gpui::white())
                                            .rounded_md()
                                            .child("自动录制"),
                                    )
                                } else {
                                    None
                                },
                            ),
                    )
                    .child(
                        if is_recording {
                            Some(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        div()
                                            .w_2()
                                            .h_2()
                                            .bg(gpui::red())
                                            .rounded_full(),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(gpui::red())
                                            .child("录制中"),
                                    ),
                            )
                        } else {
                            None
                        },
                    ),
            )
            // 录制进度
            .child_option(if let Some(ref recording) = self.room.recording_status {
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted_color)
                                        .child("录制进度"),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_4()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(muted_color)
                                                .child(Self::format_duration(&recording.duration)),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(muted_color)
                                                .child(Self::format_file_size(recording.file_size)),
                                        )
                                        .child(
                                            if recording.speed > 0 {
                                                Some(
                                                    div()
                                                        .text_xs()
                                                        .text_color(muted_color)
                                                        .child(format!(
                                                            "{}/s",
                                                            Self::format_file_size(recording.speed)
                                                        )),
                                                )
                                            } else {
                                                None
                                            },
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .h_1()
                                .bg(gpui::gray())
                                .rounded_full()
                                .relative()
                                .child(
                                    div()
                                        .h_full()
                                        .w_1() // 这里可以根据进度计算宽度
                                        .bg(gpui::green())
                                        .rounded_full(),
                                ),
                        ),
                )
            } else {
                None
            })
            .into_any_element()
    }
}