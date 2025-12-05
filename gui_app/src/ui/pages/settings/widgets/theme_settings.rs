//! 主题设置组件 - 使用 gpui-component 内置主题系统

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::{ActiveTheme, ThemeRegistry};
use crate::ui::widgets::Section;
use std::sync::Arc;

/// 主题设置卡片
#[derive(IntoElement)]
pub struct ThemeSettingsCard {
    current_theme_name: SharedString,
    on_theme_change: Option<Arc<dyn Fn(&SharedString, &mut Window, &mut App) + Send + Sync>>,
}

impl ThemeSettingsCard {
    pub fn new(current_theme_name: impl Into<SharedString>) -> Self {
        Self {
            current_theme_name: current_theme_name.into(),
            on_theme_change: None,
        }
    }

    pub fn on_theme_change(mut self, handler: impl Fn(&SharedString, &mut Window, &mut App) + Send + Sync + 'static) -> Self {
        self.on_theme_change = Some(Arc::new(handler));
        self
    }
}

impl RenderOnce for ThemeSettingsCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let current_theme_name = self.current_theme_name.clone();
        let on_change = self.on_theme_change;
        
        // 获取主题颜色
        let theme = cx.theme();
        let card_bg = theme.secondary;
        let border_color = theme.border;
        let text_color = theme.foreground;
        let muted_color = theme.muted_foreground;
        let _accent_color = theme.accent;
        
        // 获取所有可用主题
        let themes = ThemeRegistry::global(cx).sorted_themes();
        
        Section::new("🎨 外观")
            .child(
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
                            // 当前主题信息
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.0))
                                    .child(
                                        div()
                                            .text_2xl()
                                            .child("🖌️")
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(text_color)
                                                    .child(format!("当前: {}", current_theme_name))
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(muted_color)
                                                    .child(format!("{} 个主题可用", themes.len()))
                                            )
                                    )
                            )
                            // 主题网格
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(8.0))
                                    .children(
                                        themes.iter().map(|theme_config| {
                                            let theme_name = theme_config.name.clone();
                                            let is_selected = theme_name == current_theme_name;
                                            let is_dark = theme_config.mode.is_dark();
                                            let handler = on_change.clone();
                                            
                                            ThemeButton::new(theme_name.clone())
                                                .selected(is_selected)
                                                .is_dark(is_dark)
                                                .when_some(handler, move |btn, h| {
                                                    let name = theme_name.clone();
                                                    btn.on_click(move |_, window, cx| h(&name, window, cx))
                                                })
                                        })
                                    )
                            )
                    )
            )
    }
}

/// 主题选择按钮
#[derive(IntoElement)]
struct ThemeButton {
    name: SharedString,
    selected: bool,
    is_dark: bool,
    on_click: Option<Arc<dyn Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync>>,
}

impl ThemeButton {
    fn new(name: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            selected: false,
            is_dark: false,
            on_click: None,
        }
    }
    
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    
    fn is_dark(mut self, is_dark: bool) -> Self {
        self.is_dark = is_dark;
        self
    }
    
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(handler));
        self
    }
}

impl RenderOnce for ThemeButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let accent = cx.theme().accent;
        
        let bg = if self.selected {
            accent
        } else {
            cx.theme().background
        };
        
        let border_color = if self.selected {
            accent
        } else {
            cx.theme().border
        };
        
        let text_color = if self.selected {
            cx.theme().accent_foreground
        } else {
            cx.theme().foreground
        };
        
        // 用 emoji 替代 Icon
        let icon = if self.is_dark { "🌙" } else { "☀️" };
        
        let mut el = div()
            .id(SharedString::from(format!("theme-{}", self.name)))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .bg(bg)
            .border_1()
            .border_color(border_color)
            .cursor_pointer()
            .hover(|style| style.opacity(0.8))
            .child(
                div()
                    .text_sm()
                    .child(icon)
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(if self.selected { FontWeight::BOLD } else { FontWeight::MEDIUM })
                    .text_color(text_color)
                    .text_ellipsis()
                    .max_w(px(80.0))
                    .overflow_hidden()
                    .child(self.name.clone())
            );
        
        if let Some(handler) = self.on_click {
            el = el.on_click(move |e, window, cx| handler(e, window, cx));
        }
        
        el
    }
}
