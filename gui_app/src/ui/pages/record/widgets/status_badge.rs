//! 状态徽章组件

use gpui::*;
use gpui_component::ActiveTheme;

use super::{LiveStatus, RecordingStatus, LiveRoomStatus};

/// 状态徽章组件
pub struct StatusBadge {
    status: StatusType,
}

#[derive(Debug, Clone)]
pub enum StatusType {
    /// 直播状态
    Live(LiveStatus),
    /// 录制状态
    Recording(RecordingStatus),
    /// 自动录制
    AutoRecord,
    /// 平台标识
    Platform(String),
}

impl StatusBadge {
    pub fn new(status: StatusType) -> Self {
        Self { status }
    }
}

impl Render for StatusBadge {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.status {
            StatusType::Live(live_status) => self.render_live_status(*live_status, cx),
            StatusType::Recording(_recording) => self.render_recording_status(cx),
            StatusType::AutoRecord => self.render_auto_record(cx),
            StatusType::Platform(platform) => self.render_platform(platform, cx),
        }
    }
}

impl StatusBadge {
    fn render_live_status(&self, status: LiveStatus, cx: &mut Context<Self>) -> impl IntoElement {
        let (color, icon, text) = match status {
            LiveStatus::Live => (gpui::green(), "🔴", "直播中"),
            LiveStatus::Offline => (gpui::gray(), "⚫", "未开播"),
            LiveStatus::Playback => (gpui::yellow(), "🟡", "轮播中"),
        };

        div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .bg(color.opacity(0.1))
            .text_color(color)
            .rounded_md()
            .child(
                div()
                    .text_xs()
                    .child(icon),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .child(text),
            )
    }

    fn render_recording_status(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .bg(gpui::red().opacity(0.1))
            .text_color(gpui::red())
            .rounded_md()
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
                    .font_weight(FontWeight::MEDIUM)
                    .child("录制中"),
            )
    }

    fn render_auto_record(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .bg(gpui::green().opacity(0.1))
            .text_color(gpui::green())
            .rounded_md()
            .child(
                div()
                    .text_xs()
                    .child("🔄"),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .child("自动录制"),
            )
    }

    fn render_platform(&self, platform: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let (color, icon) = match platform {
            "抖音" => (gpui::black(), "🎵"),
            "B站" => (gpui::pink(), "📺"),
            "斗鱼" => (gpui::orange(), "🐟"),
            "虎牙" => (gpui::blue(), "🐅"),
            _ => (gpui::gray(), "🎮"),
        };

        div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .bg(color.opacity(0.1))
            .text_color(color)
            .rounded_md()
            .child(
                div()
                    .text_xs()
                    .child(icon),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .child(platform),
            )
    }
}