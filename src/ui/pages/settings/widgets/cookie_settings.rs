//! Cookie 设置组件
//!
//! 支持为不同平台配置 Cookie，用于访问需要登录的内容

use crate::ui::widgets::Section;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_component::select::{Select, SelectItem, SelectState};
use gpui_component::{Disableable, IconName, Sizable};
use magekit_shared::PlatformCookie;
use std::rc::Rc;

pub const CUSTOM_COOKIE_PLATFORM: &str = "__custom_domain__";

#[derive(Clone)]
pub struct CookiePlatformOption {
    label: SharedString,
    key: String,
}

impl CookiePlatformOption {
    fn new(label: &str, key: &str) -> Self {
        Self {
            label: label.to_owned().into(),
            key: key.to_owned(),
        }
    }
}

impl SelectItem for CookiePlatformOption {
    type Value = String;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.key
    }
}

pub fn cookie_platform_options() -> Vec<CookiePlatformOption> {
    [
        ("抖音", "douyin"),
        ("Bilibili", "bilibili"),
        ("YouTube", "youtube"),
        ("X / Twitter", "twitter"),
        ("Instagram", "instagram"),
        ("TikTok", "tiktok"),
        ("微博", "weibo"),
        ("小红书", "xiaohongshu"),
        ("斗鱼", "douyu"),
        ("虎牙", "huya"),
        ("Twitch", "twitch"),
        ("SOOP 韩国", "sooplive"),
        ("SOOP Global", "soop_global"),
        ("AfreecaTV", "afreeca"),
        ("自定义域名…", CUSTOM_COOKIE_PLATFORM),
    ]
    .into_iter()
    .map(|(label, key)| CookiePlatformOption::new(label, key))
    .collect()
}

/// 将常见旧平台名和域名配置映射到同一标识，避免更新时重复添加。
pub fn cookie_platform_identity(platform: &str) -> String {
    let raw = platform
        .trim()
        .to_ascii_lowercase()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_owned();
    let value = raw
        .split(|character| matches!(character, '/' | '?' | '#'))
        .next()
        .unwrap_or_default()
        .trim_start_matches("www.")
        .trim_start_matches('.')
        .to_owned();

    match value.as_str() {
        "douyin" | "douyin.com" | "iesdouyin.com" => "douyin".into(),
        "bilibili" | "bilibili.com" | "b23.tv" => "bilibili".into(),
        "youtube" | "youtube.com" | "youtu.be" => "youtube".into(),
        "twitter" | "x" | "twitter.com" | "x.com" => "twitter".into(),
        "instagram" | "instagram.com" => "instagram".into(),
        "tiktok" | "tiktok.com" => "tiktok".into(),
        "weibo" | "weibo.com" => "weibo".into(),
        "xiaohongshu" | "xiaohongshu.com" | "xhs.link" => "xiaohongshu".into(),
        "douyu" | "douyu.com" => "douyu".into(),
        "huya" | "huya.com" => "huya".into(),
        "twitch" | "twitch.tv" => "twitch".into(),
        "soop" | "sooplive" | "sooplive.co.kr" => "sooplive".into(),
        "soop_global" | "sooplive.com" => "soop_global".into(),
        "afreeca" | "afreecatv.com" => "afreeca".into(),
        _ => value,
    }
}

/// Cookie 设置卡片
pub struct CookieSettingsCard {
    /// Cookie 列表
    cookies: Vec<PlatformCookie>,
    /// 平台下拉选择
    platform_select: Entity<SelectState<Vec<CookiePlatformOption>>>,
    /// 自定义平台域名输入框
    custom_platform_input: Entity<InputState>,
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
        platform_select: Entity<SelectState<Vec<CookiePlatformOption>>>,
        custom_platform_input: Entity<InputState>,
        cookie_input: Entity<InputState>,
    ) -> Self {
        Self {
            cookies,
            platform_select,
            custom_platform_input,
            cookie_input,
            on_add: None,
            on_delete: None,
            on_toggle: None,
        }
    }

    pub fn on_add(
        mut self,
        handler: impl Fn(PlatformCookie, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_add = Some(Box::new(handler));
        self
    }

    pub fn on_delete(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_delete = Some(Rc::new(handler));
        self
    }

    pub fn on_toggle(
        mut self,
        handler: impl Fn(usize, bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_toggle = Some(Rc::new(handler));
        self
    }
}

impl IntoElement for CookieSettingsCard {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        let cookies = self.cookies;
        let platform_select = self.platform_select;
        let custom_platform_input = self.custom_platform_input;
        let cookie_input = self.cookie_input;
        let on_add = self.on_add;
        let on_delete = self.on_delete;
        let on_toggle = self.on_toggle;

        div().w_full().flex().flex_col().gap_4().child(
            // 使用闭包渲染内部内容
            CookieSettingsInner {
                cookies,
                platform_select,
                custom_platform_input,
                cookie_input,
                on_add,
                on_delete,
                on_toggle,
            },
        )
    }
}

/// Cookie 设置内部组件 (需要 cx 访问主题)
#[derive(IntoElement)]
struct CookieSettingsInner {
    cookies: Vec<PlatformCookie>,
    platform_select: Entity<SelectState<Vec<CookiePlatformOption>>>,
    custom_platform_input: Entity<InputState>,
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
        let platform_select = self.platform_select;
        let custom_platform_input = self.custom_platform_input;
        let cookie_input = self.cookie_input;
        let on_add = self.on_add;
        let on_delete = self.on_delete;
        let on_toggle = self.on_toggle;
        let selected_platform = platform_select.read(cx).selected_value().cloned();
        let custom_platform_selected = selected_platform.as_deref() == Some(CUSTOM_COOKIE_PLATFORM);
        let soop_korea_selected = selected_platform.as_deref() == Some("sooplive");
        let target_platform = match selected_platform.as_deref() {
            Some(CUSTOM_COOKIE_PLATFORM) => {
                custom_platform_input.read(cx).value().trim().to_owned()
            }
            Some(platform) => platform.to_owned(),
            None => String::new(),
        };
        let is_update = !target_platform.is_empty()
            && cookies.iter().any(|cookie| {
                cookie_platform_identity(&cookie.platform)
                    == cookie_platform_identity(&target_platform)
            });

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
                        .child("选择平台后粘贴 Cookie。已有平台会显示「更新 Cookie」，保存后替换原值；其他平台可选择自定义域名。SOOP Global Cookie 请选择对应选项。")
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
                                // 平台选择
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
                                                .child("平台")
                                        )
                                        .child(Select::new(&platform_select)
                                            .small()
                                            .w_full()
                                            .placeholder("选择平台")
                                            .search_placeholder("搜索平台"))
                                )
                                .when(custom_platform_selected, |this| {
                                    this.child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .child(div().text_sm().child("自定义平台域名"))
                                            .child(
                                                Input::new(&custom_platform_input)
                                                    .small()
                                                    .cleanable(true),
                                            ),
                                    )
                                })
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
                                        .when(soop_korea_selected, |this| {
                                            this.child(
                                                div()
                                                    .text_xs()
                                                    .text_color(muted_color)
                                                    .child("SOOP 登录接口使用 sooplive.com；韩国 Cookie 也会用于该平台的官方认证接口"),
                                            )
                                        })
                                )
                                // 添加按钮
                                .child(
                                    div()
                                        .flex()
                                        .justify_end()
                                        .child({
                                            let cookie_input_clone = cookie_input.clone();
                                            let platform = target_platform.clone();
                                            let mut btn = Button::new("add-cookie")
                                                .small()
                                                .primary()
                                                .icon(if is_update { IconName::Check } else { IconName::Plus })
                                                .label(if is_update { "更新 Cookie" } else { "添加 Cookie" })
                                                .disabled(
                                                    platform.is_empty()
                                                        || (custom_platform_selected
                                                            && !platform.contains('.'))
                                                        || cookie_input.read(cx).value().trim().is_empty(),
                                                );
                                            if let Some(handler) = on_add {
                                                btn = btn.on_click(move |_ev, window, cx| {
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
                                        .child("💡 常见平台可直接选择；自定义选项填写域名，例如 example.com")
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
                                .child(platform),
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
                                    .child("已禁用"),
                            )
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted_color)
                        .truncate()
                        .child(cookie_preview),
                ),
        )
        .child(
            // 操作按钮
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                // 启用/禁用按钮
                .child({
                    let mut btn =
                        Button::new(SharedString::from(format!("toggle-cookie-{}", index)))
                            .small()
                            .ghost()
                            .icon(if is_enabled {
                                IconName::Check
                            } else {
                                IconName::Close
                            });

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
                    let mut btn =
                        Button::new(SharedString::from(format!("delete-cookie-{}", index)))
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
                }),
        )
}
