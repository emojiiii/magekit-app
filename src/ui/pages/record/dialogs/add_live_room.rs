//! 添加直播间对话框

use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::input::InputState;
use gpui_component::modal::Modal;
use std::sync::Arc;

use crate::ui::pages::record::widgets::LiveRoomStatus;

/// 添加直播间对话框组件
pub struct AddLiveRoomDialog {
    url_input: Entity<InputState>,
    name_input: Entity<InputState>,
    title_input: Entity<InputState>,
    platform_input: Entity<InputState>,
    quality_input: Entity<InputState>,
    auto_record: bool,
    is_open: bool,
    on_confirm: Option<Arc<dyn Fn(LiveRoomStatus, &mut Window, &mut Context<Self>)>>,
    on_cancel: Option<Arc<dyn Fn(&mut Window, &mut Context<Self>)>>,
}

impl AddLiveRoomDialog {
    pub fn new() -> Self {
        Self {
            url_input: Entity::new(|_| InputState::new().placeholder("请输入直播间链接")),
            name_input: Entity::new(|_| InputState::new().placeholder("主播名称（可选）")),
            title_input: Entity::new(|_| InputState::new().placeholder("直播间标题（可选）")),
            platform_input: Entity::new(|_| InputState::new().value("抖音")),
            quality_input: Entity::new(|_| InputState::new().value("原画")),
            auto_record: false,
            is_open: true,
            on_confirm: None,
            on_cancel: None,
        }
    }

    pub fn on_confirm<F>(mut self, callback: F) -> Self
    where
        F: Fn(LiveRoomStatus, &mut Window, &mut Context<Self>) + 'static,
    {
        self.on_confirm = Some(Arc::new(callback));
        self
    }

    pub fn on_cancel<F>(mut self, callback: F) -> Self
    where
        F: Fn(&mut Window, &mut Context<Self>) + 'static,
    {
        self.on_cancel = Some(Arc::new(callback));
        self
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.is_open = false;
        cx.notify();
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.url_input.read(cx).value().trim();
        if url.is_empty() {
            window.push_notification(
                gpui_component::notification::Notification::error("请输入直播间链接"),
                cx,
            );
            return;
        }

        // 创建直播间状态
        let mut room = LiveRoomStatus::new(
            url.to_string(),
            self.platform_input.read(cx).value().trim().to_string(),
            self.name_input.read(cx).value().trim().to_string(),
            self.title_input.read(cx).value().trim().to_string(),
        );

        room.auto_record = self.auto_record;
        room.record_quality = self.quality_input.read(cx).value().trim().to_string();

        // 如果没有提供主播名称，尝试从URL中提取
        if room.anchor_name.is_empty() {
            room.anchor_name = self.extract_anchor_name(url);
        }

        if let Some(ref callback) = self.on_confirm {
            callback(room, window, cx);
        }

        self.close(cx);
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ref callback) = self.on_cancel {
            callback(window, cx);
        }
        self.close(cx);
    }

    fn toggle_auto_record(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.auto_record = !self.auto_record;
        cx.notify();
    }

    fn extract_anchor_name(&self, url: &str) -> String {
        // 简单的名称提取逻辑
        if url.contains("douyin.com") {
            "抖音主播".to_string()
        } else if url.contains("live.bilibili.com") {
            "B站主播".to_string()
        } else if url.contains("live.douyu.com") {
            "斗鱼主播".to_string()
        } else {
            "未知主播".to_string()
        }
    }
}

impl Render for AddLiveRoomDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.is_open {
            return div().into_any_element();
        }

        let bg_color = cx.theme().background;
        let card_bg = cx.theme().surface;
        let title_color = cx.theme().foreground;
        let desc_color = cx.theme().muted_foreground;

        Modal::new("add-live-room-modal")
            .title("添加直播间")
            .size(gpui_component::modal::ModalSize::Md)
            .show_close_button(true)
            .on_close(cx.listener(|this, _event, window, cx| {
                this.cancel(window, cx);
            }))
            .footer(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .child(
                        gpui_component::button::Button::new("cancel")
                            .label("取消")
                            .style(gpui_component::button::ButtonStyle::Ghost)
                            .size(gpui_component::button::ButtonSize::Sm)
                            .on_click(cx.listener(|this, _event, window, cx| {
                                this.cancel(window, cx);
                            })),
                    )
                    .child(
                        gpui_component::button::Button::new("confirm")
                            .label("添加")
                            .style(gpui_component::button::ButtonStyle::Primary)
                            .size(gpui_component::button::ButtonSize::Sm)
                            .on_click(cx.listener(|this, _event, window, cx| {
                                this.confirm(window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    // URL输入
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(title_color)
                                    .child("直播间链接 *"),
                            )
                            .child(
                                gpui_component::input::Input::new("room-url")
                                    .state(&self.url_input)
                                    .placeholder("https://live.douyin.com/123456")
                                    .size(gpui_component::input::InputSize::Md),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(desc_color)
                                    .child("支持抖音、B站、斗鱼等主流直播平台"),
                            ),
                    )
                    // 主播名称（可选）
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(title_color)
                                    .child("主播名称"),
                            )
                            .child(
                                gpui_component::input::Input::new("anchor-name")
                                    .state(&self.name_input)
                                    .placeholder("主播名称（可选，会自动获取）")
                                    .size(gpui_component::input::InputSize::Md),
                            ),
                    )
                    // 直播间标题（可选）
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(title_color)
                                    .child("直播间标题"),
                            )
                            .child(
                                gpui_component::input::Input::new("room-title")
                                    .state(&self.title_input)
                                    .placeholder("直播间标题（可选，会自动获取）")
                                    .size(gpui_component::input::InputSize::Md),
                            ),
                    )
                    // 高级选项
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(title_color)
                                    .child("高级选项"),
                            )
                            .child(
                                div()
                                    .grid()
                                    .grid_cols_2()
                                    .gap_4()
                                    // 平台
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(title_color)
                                                    .child("平台"),
                                            )
                                            .child(
                                                gpui_component::input::Input::new("platform")
                                                    .state(&self.platform_input)
                                                    .placeholder("抖音")
                                                    .size(gpui_component::input::InputSize::Sm),
                                            ),
                                    )
                                    // 录制质量
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(title_color)
                                                    .child("录制质量"),
                                            )
                                            .child(
                                                gpui_component::input::Input::new("quality")
                                                    .state(&self.quality_input)
                                                    .placeholder("原画")
                                                    .size(gpui_component::input::InputSize::Sm),
                                            ),
                                    ),
                            )
                            // 自动录制开关
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
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(title_color)
                                                    .child("自动录制"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(desc_color)
                                                    .child("检测到开播时自动开始录制"),
                                            ),
                                    )
                                    .child(
                                        gpui_component::switch::Switch::new("auto-record")
                                            .checked(self.auto_record)
                                            .on_change(cx.listener(|this, checked, _window, cx| {
                                                this.auto_record = checked;
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    ),
            )
    }
}