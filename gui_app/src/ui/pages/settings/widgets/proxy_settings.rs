//! 代理设置组件

use crate::ui::widgets::Section;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_component::{Icon, IconName, Sizable};

/// 代理模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProxyMode {
    /// 不使用代理
    #[default]
    None,
    /// 使用系统代理
    System,
    /// 自定义代理
    Custom,
}

/// 代理测试状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProxyTestStatus {
    #[default]
    Idle,
    Testing,
    Success,
    Failed,
}

/// 代理设置卡片
#[derive(IntoElement)]
pub struct ProxySettingsCard {
    mode: ProxyMode,
    proxy_input: Option<Entity<InputState>>,
    test_status: ProxyTestStatus,
    on_none_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_system_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_custom_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_test_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl ProxySettingsCard {
    pub fn new(mode: ProxyMode) -> Self {
        Self {
            mode,
            proxy_input: None,
            test_status: ProxyTestStatus::Idle,
            on_none_click: None,
            on_system_click: None,
            on_custom_click: None,
            on_test_click: None,
        }
    }

    pub fn proxy_input(mut self, input: Entity<InputState>) -> Self {
        self.proxy_input = Some(input);
        self
    }

    pub fn test_status(mut self, status: ProxyTestStatus) -> Self {
        self.test_status = status;
        self
    }

    pub fn on_mode_change(
        mut self,
        handler: impl Fn(ProxyMode, &mut Window, &mut App) + 'static,
    ) -> Self {
        let handler1 = std::sync::Arc::new(handler);
        let handler2 = handler1.clone();
        let handler3 = handler1.clone();

        self.on_none_click = Some(Box::new(move |_ev, window, cx| {
            handler1(ProxyMode::None, window, cx);
        }));
        self.on_system_click = Some(Box::new(move |_ev, window, cx| {
            handler2(ProxyMode::System, window, cx);
        }));
        self.on_custom_click = Some(Box::new(move |_ev, window, cx| {
            handler3(ProxyMode::Custom, window, cx);
        }));
        self
    }

    pub fn on_test(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_test_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for ProxySettingsCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let card_bg = theme.secondary;
        let border_color = theme.border;
        let title_color = theme.foreground;
        let muted_color = theme.muted_foreground;
        // 使用 hsla 创建颜色以支持 opacity
        let success_color = gpui::hsla(142.0 / 360.0, 0.71, 0.45, 1.0); // green-500
        let error_color = gpui::hsla(0.0 / 360.0, 0.84, 0.60, 1.0); // red-500

        let current_mode = self.mode;
        let test_status = self.test_status;

        // 创建三个模式按钮
        let mut none_btn = Button::new("proxy-mode-none").label("不使用代理");
        if current_mode == ProxyMode::None {
            none_btn = none_btn.primary();
        } else {
            none_btn = none_btn.ghost();
        }

        let mut system_btn = Button::new("proxy-mode-system").label("系统代理");
        if current_mode == ProxyMode::System {
            system_btn = system_btn.primary();
        } else {
            system_btn = system_btn.ghost();
        }

        let mut custom_btn = Button::new("proxy-mode-custom").label("自定义");
        if current_mode == ProxyMode::Custom {
            custom_btn = custom_btn.primary();
        } else {
            custom_btn = custom_btn.ghost();
        }

        // 添加点击事件
        if let Some(handler) = self.on_none_click {
            none_btn = none_btn.on_click(handler);
        }
        if let Some(handler) = self.on_system_click {
            system_btn = system_btn.on_click(handler);
        }
        if let Some(handler) = self.on_custom_click {
            custom_btn = custom_btn.on_click(handler);
        }

        // 测试按钮
        let mut test_btn = Button::new("proxy-test").small().label(match test_status {
            ProxyTestStatus::Idle => "测试连接",
            ProxyTestStatus::Testing => "测试中...",
            ProxyTestStatus::Success => "连接成功",
            ProxyTestStatus::Failed => "连接失败",
        });

        if test_status == ProxyTestStatus::Testing {
            test_btn = test_btn.ghost();
        } else if test_status == ProxyTestStatus::Success {
            test_btn = test_btn.primary();
        } else if test_status == ProxyTestStatus::Failed {
            test_btn = test_btn.danger();
        } else {
            test_btn = test_btn.outline();
        }

        if let Some(handler) = self.on_test_click {
            test_btn = test_btn.on_click(handler);
        }

        Section::new_with_icon("网络代理", IconName::Globe).child(
            div()
                .p(px(20.0))
                .rounded(px(12.0))
                .bg(card_bg)
                .border_1()
                .border_color(border_color)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(16.0))
                        // 代理模式选择
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .child(
                                            Icon::new(IconName::Settings2).text_color(title_color),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .text_color(title_color)
                                                        .child("代理模式"),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(muted_color)
                                                        .child("选择网络代理的使用方式"),
                                                ),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(8.0))
                                        .child(none_btn)
                                        .child(system_btn)
                                        .child(custom_btn),
                                ),
                        )
                        // 自定义代理输入框（仅当选择自定义时显示）
                        .when(current_mode == ProxyMode::Custom, |el| {
                            el.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(
                                                Icon::new(IconName::ExternalLink)
                                                    .text_color(title_color),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(title_color)
                                                            .child("代理地址"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(muted_color)
                                                            .child("支持 HTTP/HTTPS/SOCKS5 代理"),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(8.0))
                                            .items_center()
                                            .child(div().flex_1().when_some(
                                                self.proxy_input.clone(),
                                                |el, input| {
                                                    el.child(Input::new(&input).cleanable(true))
                                                },
                                            ))
                                            .child(test_btn),
                                    ),
                            )
                        })
                        // 系统代理提示
                        .when(current_mode == ProxyMode::System, |el| {
                            el.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .p(px(12.0))
                                    .rounded(px(8.0))
                                    .bg(theme.accent.opacity(0.1))
                                    .child(
                                        Icon::new(IconName::Info).small().text_color(theme.accent),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.accent)
                                            .child("将自动使用系统代理设置"),
                                    ),
                            )
                        })
                        // 测试状态提示
                        .when(
                            current_mode == ProxyMode::Custom
                                && test_status == ProxyTestStatus::Success,
                            |el| {
                                el.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .p(px(12.0))
                                        .rounded(px(8.0))
                                        .bg(success_color.opacity(0.1))
                                        .child(
                                            Icon::new(IconName::CircleCheck)
                                                .small()
                                                .text_color(success_color),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(success_color)
                                                .child("代理连接正常"),
                                        ),
                                )
                            },
                        )
                        .when(
                            current_mode == ProxyMode::Custom
                                && test_status == ProxyTestStatus::Failed,
                            |el| {
                                el.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .p(px(12.0))
                                        .rounded(px(8.0))
                                        .bg(error_color.opacity(0.1))
                                        .child(
                                            Icon::new(IconName::CircleX)
                                                .small()
                                                .text_color(error_color),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(error_color)
                                                .child("无法连接到代理服务器"),
                                        ),
                                )
                            },
                        ),
                ),
        )
    }
}
