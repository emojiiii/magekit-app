//! 轮询配置组件

use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::input::InputState;
use std::sync::Arc;

use super::PollingConfig;

/// 轮询配置面板组件
pub struct PollingConfigPanel {
    config: PollingConfig,
    interval_input: Entity<InputState>,
    max_concurrent_input: Entity<InputState>,
    on_config_change: Option<Arc<dyn Fn(PollingConfig, &mut Window, &mut Context<Self>)>>,
}

impl PollingConfigPanel {
    pub fn new(config: PollingConfig) -> Self {
        Self {
            config: config.clone(),
            interval_input: Entity::new(|_| InputState::new().value(config.interval.to_string())),
            max_concurrent_input: Entity::new(|_| InputState::new().value(config.max_concurrent_recordings.to_string())),
            on_config_change: None,
        }
    }

    pub fn on_config_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(PollingConfig, &mut Window, &mut Context<Self>) + 'static,
    {
        self.on_config_change = Some(Arc::new(callback));
        self
    }

    fn update_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // 从输入框获取值
        let interval_str = self.interval_input.read(cx).value();
        let max_concurrent_str = self.max_concurrent_input.read(cx).value();

        let interval = interval_str.parse().unwrap_or(self.config.interval);
        let max_concurrent = max_concurrent_str.parse().unwrap_or(self.config.max_concurrent_recordings);

        // 更新配置
        self.config.interval = interval;
        self.config.max_concurrent_recordings = max_concurrent;

        // 触发回调
        if let Some(ref callback) = self.on_config_change {
            callback(self.config.clone(), window, cx);
        }
    }

    fn toggle_auto_record(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.config.auto_record = !self.config.auto_record;
        self.update_config(window, cx);
    }

    fn toggle_notify_on_live(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.config.notify_on_live = !self.config.notify_on_live;
        self.update_config(window, cx);
    }

    fn toggle_notify_on_offline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.config.notify_on_offline = !self.config.notify_on_offline;
        self.update_config(window, cx);
    }
}

impl Render for PollingConfigPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg_color = cx.theme().background;
        let card_bg = cx.theme().surface;
        let border_color = cx.theme().border;
        let title_color = cx.theme().foreground;
        let desc_color = cx.theme().muted_foreground;

        div()
            .flex()
            .flex_col()
            .p_4()
            .bg(card_bg)
            .border_1()
            .border_color(border_color)
            .rounded_lg()
            .gap_4()
            // 标题
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(title_color)
                            .child("⚙️ 轮询配置"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(desc_color)
                            .child("设置直播间状态检查和自动录制选项"),
                    ),
            )
            // 配置选项
            .child(
                div()
                    .grid()
                    .grid_cols_2()
                    .gap_4()
                    // 轮询间隔
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(title_color)
                                    .child("轮询间隔"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        gpui_component::input::Input::new("interval")
                                            .state(&self.interval_input)
                                            .placeholder("60")
                                            .size(gpui_component::input::InputSize::Sm)
                                            .on_change(cx.listener(|this, _event, window, cx| {
                                                this.update_config(window, cx);
                                            })),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(desc_color)
                                            .child("秒"),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(desc_color)
                                    .child("检查直播间状态的时间间隔"),
                            ),
                    )
                    // 最大并发录制数
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(title_color)
                                    .child("最大并发录制"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        gpui_component::input::Input::new("max-concurrent")
                                            .state(&self.max_concurrent_input)
                                            .placeholder("3")
                                            .size(gpui_component::input::InputSize::Sm)
                                            .on_change(cx.listener(|this, _event, window, cx| {
                                                this.update_config(window, cx);
                                            })),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(desc_color)
                                            .child("个"),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(desc_color)
                                    .child("同时录制的最大直播间数量"),
                            ),
                    ),
            )
            // 开关选项
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
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
                                            .text_sm()
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
                                    .checked(self.config.auto_record)
                                    .on_change(cx.listener(|this, checked, window, cx| {
                                        this.config.auto_record = checked;
                                        this.update_config(window, cx);
                                    })),
                            ),
                    )
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
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(title_color)
                                            .child("开播通知"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(desc_color)
                                            .child("直播间开播时发送通知"),
                                    ),
                            )
                            .child(
                                gpui_component::switch::Switch::new("notify-on-live")
                                    .checked(self.config.notify_on_live)
                                    .on_change(cx.listener(|this, checked, window, cx| {
                                        this.config.notify_on_live = checked;
                                        this.update_config(window, cx);
                                    })),
                            ),
                    )
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
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(title_color)
                                            .child("下播通知"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(desc_color)
                                            .child("直播结束时发送通知"),
                                    ),
                            )
                            .child(
                                gpui_component::switch::Switch::new("notify-on-offline")
                                    .checked(self.config.notify_on_offline)
                                    .on_change(cx.listener(|this, checked, window, cx| {
                                        this.config.notify_on_offline = checked;
                                        this.update_config(window, cx);
                                    })),
                            ),
                    ),
            )
    }
}