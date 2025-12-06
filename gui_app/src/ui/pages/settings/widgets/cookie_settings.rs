//! Cookie 设置组件
//!
//! 支持为不同平台配置 Cookie，用于访问需要登录的内容

use crate::ui::widgets::Section;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_component::{IconName, Sizable};
use magekit_shared::PlatformCookie;
use std::rc::Rc;

/// Cookie 设置卡片
pub struct CookieSettingsCard {
    /// Cookie 列表
    cookies: Vec<PlatformCookie>,
    /// 平台输入框
    platform_input: Entity<InputState>,
    /// Cookie 输入框
    cookie_input: Entity<InputState>,
    /// 添加回调
    on_add: Option<Box<dyn Fn(PlatformCookie, &mut Window, &mut App) + 'static>>,
    /// 删除回调
    on_delete: Option<Rc<dyn Fn(usize, &mut Window, &mut App) + 'static>>,
    /// 切换启用回调
    on_toggle: Option<Rc<dyn Fn(usize, bool, &mut Window, &mut App) + 'static>>,
}

impl CookieSettingsCard {
    pub fn new(
        cookies: Vec<PlatformCookie>,
        platform_input: Entity<InputState>,
        cookie_input: Entity<InputState>,
    ) -> Self {
        Self {
            cookies,
            platform_input,
            cookie_input,
            on_add: None,
            on_delete: None,
            on_toggle: None,
        }
    }

    pub fn on_add(mut self, handler: impl Fn(PlatformCookie, &mut Window, &mut App) + 'static) -> Self {
        self.on_add = Some(Box::new(handler));
        self
    }

    pub fn on_delete(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_delete = Some(Rc::new(handler));
        self
    }

    pub fn on_toggle(mut self, handler: impl Fn(usize, bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(handler));
        self
    }
}

impl IntoElement for CookieSettingsCard {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        let cookies = self.cookies;
        let platform_input = self.platform_input;
        let cookie_input = self.cookie_input;
        let on_add = self.on_add;
        let on_delete = self.on_delete;
        let on_toggle = self.on_toggle;

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                // 使用闭包渲染内部内容
                CookieSettingsInner {
                    cookies,
                    platform_input,
                    cookie_input,
                    on_add,
                    on_delete,
                    on_toggle,
                }
            )
    }
}

/// Cookie 设置内部组件 (需要 cx 访问主题)
#[derive(IntoElement)]
struct CookieSettingsInner {
    cookies: Vec<PlatformCookie>,
    platform_input: Entity<InputState>,
    cookie_input: Entity<InputState>,
    on_add: Option<Box<dyn Fn(PlatformCookie, &mut Window, &mut App) + 'static>>,
    on_delete: Option<Rc<dyn Fn(usize, &mut Window, &mut App) + 'static>>,
    on_toggle: Option<Rc<dyn Fn(usize, bool, &mut Window, &mut App) + 'static>>,
}

impl RenderOnce for CookieSettingsInner {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let card_bg = theme.secondary;
        let border_color = theme.border;
        let title_color = theme.foreground;
        let muted_color = theme.muted_foreground;

        let cookies = self.cookies;
        let platform_input = self.platform_input;
        let cookie_input = self.cookie_input;
        let on_add = self.on_add;
        let on_delete = self.on_delete;
        let on_toggle = self.on_toggle;

        Section::new_with_icon("平台 Cookie", IconName::Inbox).child(
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
                        // 说明文字
                        .child(
                            div()
                                .text_sm()
                                .text_color(muted_color)
                                .child("为需要登录的平台配置 Cookie，以访问受限内容。Cookie 可从浏览器开发者工具中获取。")
                        )
                        // 添加新 Cookie 区域
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .p(px(12.0))
                                .rounded(px(8.0))
                                .bg(theme.background)
                                .border_1()
                                .border_color(border_color)
                                // 平台输入
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.0))
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(title_color)
                                                .child("平台名称")
                                        )
                                        .child(
                                            Input::new(&platform_input)
                                                .small()
                                                .cleanable(true)
                                        )
                                )
                                // Cookie 输入
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.0))
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(title_color)
                                                .child("Cookie 内容")
                                        )
                                        .child(
                                            Input::new(&cookie_input)
                                                .small()
                                                .cleanable(true)
                                        )
                                )
                                // 添加按钮
                                .child(
                                    div()
                                        .flex()
                                        .justify_end()
                                        .child({
                                            let platform_input_clone = platform_input.clone();
                                            let cookie_input_clone = cookie_input.clone();
                                            let mut btn = Button::new("add-cookie")
                                                .small()
                                                .primary()
                                                .icon(IconName::Plus)
                                                .label("添加");
                                            
                                            if let Some(handler) = on_add {
                                                btn = btn.on_click(move |_ev, window, cx| {
                                                    let platform = platform_input_clone.read(cx).value().to_string();
                                                    let cookie = cookie_input_clone.read(cx).value().to_string();
                                                    
                                                    if !platform.trim().is_empty() && !cookie.trim().is_empty() {
                                                        let new_cookie = PlatformCookie::new(
                                                            platform.trim().to_string(),
                                                            cookie.trim().to_string(),
                                                        );
                                                        handler(new_cookie, window, cx);
                                                    }
                                                });
                                            }
                                            btn
                                        })
                                )
                        )
                        // Cookie 列表或空状态
                        .child({
                            let is_empty = cookies.is_empty();
                            let cookies_len = cookies.len();
                            if is_empty {
                                // 空状态
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .py(px(24.0))
                                    .text_sm()
                                    .text_color(muted_color)
                                    .child("暂无配置的 Cookie")
                            } else {
                                // Cookie 列表
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(title_color)
                                            .child(format!("已配置的 Cookie ({})", cookies_len))
                                    )
                                    .children(
                                        cookies.into_iter().enumerate().map(|(index, cookie)| {
                                            render_cookie_item(
                                                index,
                                                cookie,
                                                &on_delete,
                                                &on_toggle,
                                                card_bg,
                                                border_color,
                                                title_color,
                                                muted_color,
                                            )
                                        })
                                    )
                            }
                        })
                        // 常用平台提示
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .pt(px(8.0))
                                .border_t_1()
                                .border_color(border_color)
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted_color)
                                        .child("💡 常用平台名称: bilibili, youtube, twitter, instagram")
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted_color)
                                        .child("获取 Cookie: 浏览器 F12 → Application/存储 → Cookies → 复制相关 Cookie")
                                )
                        )
                )
        )
    }
}

/// 渲染单个 Cookie 项
fn render_cookie_item(
    index: usize,
    cookie: PlatformCookie,
    on_delete: &Option<Rc<dyn Fn(usize, &mut Window, &mut App) + 'static>>,
    on_toggle: &Option<Rc<dyn Fn(usize, bool, &mut Window, &mut App) + 'static>>,
    _card_bg: Hsla,
    border_color: Hsla,
    title_color: Hsla,
    muted_color: Hsla,
) -> impl IntoElement {
    let is_enabled = cookie.enabled;
    let platform = cookie.platform.clone();
    let cookie_preview = if cookie.cookie.len() > 50 {
        format!("{}...", &cookie.cookie[..50])
    } else {
        cookie.cookie.clone()
    };

    div()
        .id(SharedString::from(format!("cookie-item-{}", index)))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .p(px(12.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(border_color)
        .when(is_enabled, |el| el.opacity(1.0))
        .when(!is_enabled, |el| el.opacity(0.5))
        .child(
            // 平台信息
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(title_color)
                                .child(platform)
                        )
                        .when(!is_enabled, |el| {
                            el.child(
                                div()
                                    .text_xs()
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(gpui::hsla(0.0, 0.0, 0.5, 0.2))
                                    .text_color(muted_color)
                                    .child("已禁用")
                            )
                        })
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted_color)
                        .truncate()
                        .child(cookie_preview)
                )
        )
        .child(
            // 操作按钮
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                // 启用/禁用按钮
                .child({
                    let mut btn = Button::new(SharedString::from(format!("toggle-cookie-{}", index)))
                        .small()
                        .ghost()
                        .icon(if is_enabled { IconName::Check } else { IconName::Close });

                    if let Some(handler) = on_toggle {
                        let handler = Rc::clone(handler);
                        btn = btn.on_click(move |_ev, window, cx| {
                            handler(index, !is_enabled, window, cx);
                        });
                    }
                    btn
                })
                // 删除按钮
                .child({
                    let mut btn = Button::new(SharedString::from(format!("delete-cookie-{}", index)))
                        .small()
                        .danger()
                        .ghost()
                        .icon(IconName::Delete);

                    if let Some(handler) = on_delete {
                        let handler = Rc::clone(handler);
                        btn = btn.on_click(move |_ev, window, cx| {
                            handler(index, window, cx);
                        });
                    }
                    btn
                })
        )
}
