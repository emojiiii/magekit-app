//! 主题设置组件

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::ActiveTheme;
use magekit_shared::types::Theme;
use crate::ui::widgets::Section;
use std::sync::Arc;

/// 主题设置卡片
#[derive(IntoElement)]
pub struct ThemeSettingsCard {
    current_theme: Theme,
    on_theme_change: Option<Arc<dyn Fn(&Theme, &mut Window, &mut App) + Send + Sync>>,
}

impl ThemeSettingsCard {
    pub fn new(current_theme: Theme) -> Self {
        Self {
            current_theme,
            on_theme_change: None,
        }
    }

    pub fn on_theme_change(mut self, handler: impl Fn(&Theme, &mut Window, &mut App) + Send + Sync + 'static) -> Self {
        self.on_theme_change = Some(Arc::new(handler));
        self
    }
}

impl RenderOnce for ThemeSettingsCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let current_theme = self.current_theme.clone();
        let on_change = self.on_theme_change;
        
        // 检测当前是否是暗色模式
        let is_dark = cx.theme().mode.is_dark();
        
        // 根据主题选择颜色
        let bg_color = if is_dark { rgb(0x18181b) } else { rgb(0xffffff) };
        let border_color = if is_dark { rgb(0x3f3f46) } else { rgb(0xe4e4e7) };
        let text_color = if is_dark { rgb(0xfafafa) } else { rgb(0x18181b) };
        let muted_color = if is_dark { rgb(0x71717a) } else { rgb(0xa1a1aa) };
        let selected_bg = rgb(0x3b82f6);
        let unselected_bg = if is_dark { rgb(0x27272a) } else { rgb(0xf4f4f5) };
        
        Section::new("外观设置")
            .icon("🎨")
            .description("自定义应用程序的外观主题")
            .child(
                div()
                    .p(px(16.0))
                    .rounded(px(12.0))
                    .bg(bg_color)
                    .border_1()
                    .border_color(border_color)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(text_color)
                                    .child("主题模式")
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted_color)
                                    .child("选择应用程序的外观主题")
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.0))
                                    .mt(px(4.0))
                                    // 跟随系统
                                    .child({
                                        let is_selected = matches!(current_theme, Theme::System);
                                        let handler = on_change.clone();
                                        ThemeButton::new("system", "🖥️", "跟随系统")
                                            .selected(is_selected)
                                            .selected_bg(selected_bg)
                                            .unselected_bg(unselected_bg)
                                            .text_color(text_color)
                                            .when_some(handler, |btn, h| {
                                                btn.on_click(move |_, window, cx| h(&Theme::System, window, cx))
                                            })
                                    })
                                    // 浅色模式
                                    .child({
                                        let is_selected = matches!(current_theme, Theme::Light);
                                        let handler = on_change.clone();
                                        ThemeButton::new("light", "☀️", "浅色")
                                            .selected(is_selected)
                                            .selected_bg(selected_bg)
                                            .unselected_bg(unselected_bg)
                                            .text_color(text_color)
                                            .when_some(handler, |btn, h| {
                                                btn.on_click(move |_, window, cx| h(&Theme::Light, window, cx))
                                            })
                                    })
                                    // 深色模式
                                    .child({
                                        let is_selected = matches!(current_theme, Theme::Dark);
                                        let handler = on_change.clone();
                                        ThemeButton::new("dark", "🌙", "深色")
                                            .selected(is_selected)
                                            .selected_bg(selected_bg)
                                            .unselected_bg(unselected_bg)
                                            .text_color(text_color)
                                            .when_some(handler, |btn, h| {
                                                btn.on_click(move |_, window, cx| h(&Theme::Dark, window, cx))
                                            })
                                    })
                            )
                    )
            )
    }
}

/// 主题选择按钮
#[derive(IntoElement)]
struct ThemeButton {
    id: SharedString,
    icon: &'static str,
    label: &'static str,
    selected: bool,
    selected_bg: Rgba,
    unselected_bg: Rgba,
    text_color: Rgba,
    on_click: Option<Arc<dyn Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync>>,
}

impl ThemeButton {
    fn new(id: impl Into<SharedString>, icon: &'static str, label: &'static str) -> Self {
        Self {
            id: id.into(),
            icon,
            label,
            selected: false,
            selected_bg: rgb(0x3b82f6),
            unselected_bg: rgb(0x27272a),
            text_color: rgb(0xfafafa),
            on_click: None,
        }
    }
    
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    
    fn selected_bg(mut self, color: Rgba) -> Self {
        self.selected_bg = color;
        self
    }
    
    fn unselected_bg(mut self, color: Rgba) -> Self {
        self.unselected_bg = color;
        self
    }
    
    fn text_color(mut self, color: Rgba) -> Self {
        self.text_color = color;
        self
    }
    
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(handler));
        self
    }
}

impl RenderOnce for ThemeButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let bg = if self.selected { self.selected_bg } else { self.unselected_bg };
        let border_color = if self.selected { self.selected_bg } else { self.unselected_bg };
        let text_color = if self.selected { rgb(0xffffff) } else { self.text_color };
        
        let mut el = div()
            .id(self.id)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .px(px(16.0))
            .py(px(12.0))
            .rounded(px(8.0))
            .bg(bg)
            .border_2()
            .border_color(border_color)
            .cursor_pointer()
            .child(div().text_lg().child(self.icon))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(text_color)
                    .child(self.label)
            );
        
        if let Some(handler) = self.on_click {
            el = el.on_click(move |e, window, cx| handler(e, window, cx));
        }
        
        el
    }
}
