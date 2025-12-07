//! 直播间列表组件

use gpui::*;
use gpui_component::ActiveTheme;
use std::sync::Arc;

use super::{LiveRoomStatus, LiveStatus, RecordingStatus};

/// 直播间列表组件
pub struct LiveRoomList {
    rooms: Vec<LiveRoomStatus>,
    auto_record_enabled: bool,
    on_start_recording: Option<Arc<dyn Fn(Uuid, &mut Window, &mut Context<Self>)>>,
    on_stop_recording: Option<Arc<dyn Fn(Uuid, &mut Window, &mut Context<Self>)>>,
    on_remove_room: Option<Arc<dyn Fn(Uuid, &mut Window, &mut Context<Self>)>>,
}

impl LiveRoomList {
    pub fn new(rooms: Vec<LiveRoomStatus>, auto_record_enabled: bool) -> Self {
        Self {
            rooms,
            auto_record_enabled,
            on_start_recording: None,
            on_stop_recording: None,
            on_remove_room: None,
        }
    }

    pub fn on_start_recording<F>(mut self, callback: F) -> Self
    where
        F: Fn(Uuid, &mut Window, &mut Context<Self>) + 'static,
    {
        self.on_start_recording = Some(Arc::new(callback));
        self
    }

    pub fn on_stop_recording<F>(mut self, callback: F) -> Self
    where
        F: Fn(Uuid, &mut Window, &mut Context<Self>) + 'static,
    {
        self.on_stop_recording = Some(Arc::new(callback));
        self
    }

    pub fn on_remove_room<F>(mut self, callback: F) -> Self
    where
        F: Fn(Uuid, &mut Window, &mut Context<Self>) + 'static,
    {
        self.on_remove_room = Some(Arc::new(callback));
        self
    }

    fn start_recording(&self, room_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ref callback) = self.on_start_recording {
            callback(room_id, window, cx);
        }
    }

    fn stop_recording(&self, room_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ref callback) = self.on_stop_recording {
            callback(room_id, window, cx);
        }
    }

    fn remove_room(&self, room_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ref callback) = self.on_remove_room {
            callback(room_id, window, cx);
        }
    }
}

impl Render for LiveRoomList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg_color = cx.theme().background;
        let card_bg = cx.theme().surface;
        let border_color = cx.theme().border;
        let title_color = cx.theme().foreground;
        let muted_color = cx.theme().muted_foreground;

        if self.rooms.is_empty() {
            return div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .p_8()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_4()
                        .child(
                            div()
                                .text_2xl()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(muted_color)
                                .child("📺"),
                        )
                        .child(
                            div()
                                .text_lg()
                                .text_color(muted_color)
                                .child("暂无监控的直播间"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(muted_color)
                                .child("点击上方添加直播间按钮开始监控"),
                        ),
                )
                .into_any_element();
        }

        div()
            .flex()
            .flex_col()
            .gap_4()
            .children(
                self.rooms.iter().map(|room| {
                    let room_id = room.id;
                    let is_live = room.status == LiveStatus::Live;
                    let is_recording = room.is_recording;

                    div()
                        .flex()
                        .items_center()
                        .p_4()
                        .bg(card_bg)
                        .border_1()
                        .border_color(border_color)
                        .rounded_lg()
                        .gap_4()
                        // 状态指示器
                        .child(
                            div()
                                .w_2()
                                .h_12()
                                .rounded_full()
                                .bg(if is_live {
                                    gpui::green()
                                } else if room.status == LiveStatus::Playback {
                                    gpui::yellow()
                                } else {
                                    gpui::gray()
                                }),
                        )
                        // 主要信息
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
                                                .child(&room.anchor_name),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .px_2()
                                                .py_1()
                                                .bg(gpui::blue())
                                                .text_color(gpui::white())
                                                .rounded_md()
                                                .child(&room.platform),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(muted_color)
                                        .child(&room.title),
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
                                                .child(format!(
                                                    "{}",
                                                    room.status_to_string()
                                                )),
                                        )
                                        .child(
                                            if let Some(viewers) = room.viewer_count {
                                                Some(
                                                    div()
                                                        .text_xs()
                                                        .text_color(muted_color)
                                                        .child(format!("👥 {}", viewers)),
                                                )
                                            } else {
                                                None
                                            },
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
                                                                .text_xs()
                                                                .text_color(gpui::red())
                                                                .child("录制中"),
                                                        ),
                                                )
                                            } else {
                                                None
                                            },
                                        ),
                                ),
                        )
                        // 操作按钮
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    if is_live && !is_recording {
                                        Some(
                                            gpui_component::button::Button::new("start-record")
                                                .label("录制")
                                                .style(gpui_component::button::ButtonStyle::Primary)
                                                .size(gpui_component::button::ButtonSize::Sm)
                                                .on_click(cx.listener(move |this, _ev, window, cx| {
                                                    this.start_recording(room_id, window, cx);
                                                })),
                                        )
                                    } else if is_recording {
                                        Some(
                                            gpui_component::button::Button::new("stop-record")
                                                .label("停止")
                                                .style(gpui_component::button::ButtonStyle::Destructive)
                                                .size(gpui_component::button::ButtonSize::Sm)
                                                .on_click(cx.listener(move |this, _ev, window, cx| {
                                                    this.stop_recording(room_id, window, cx);
                                                })),
                                        )
                                    } else {
                                        None
                                    },
                                )
                                .child(
                                    gpui_component::button::Button::new("remove-room")
                                        .label("移除")
                                        .style(gpui_component::button::ButtonStyle::Ghost)
                                        .size(gpui_component::button::ButtonSize::Sm)
                                        .on_click(cx.listener(move |this, _ev, window, cx| {
                                            this.remove_room(room_id, window, cx);
                                        })),
                                ),
                        )
                })
            )
            .into_any_element()
    }
}

impl LiveRoomStatus {
    fn status_to_string(&self) -> &'static str {
        match self.status {
            LiveStatus::Live => "🔴 直播中",
            LiveStatus::Offline => "⚫ 未开播",
            LiveStatus::Playback => "🟡 轮播中",
        }
    }
}