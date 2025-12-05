//! 主题设置组件 - 使用 gpui-component 内置主题系统

use gpui::*;
use gpui_component::{ActiveTheme, ThemeRegistry, IconName};
use gpui_component::button::Button;
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
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
        
        // 构建主题选项列表
        let theme_names: Vec<SharedString> = themes.iter().map(|t| t.name.clone()).collect();
        
        // 找到当前主题的索引
        let _selected_idx = theme_names.iter().position(|n| n == &current_theme_name).unwrap_or(0);
        
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
                            // 主题选择下拉框
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
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
                                                            .child("主题")
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(muted_color)
                                                            .child(format!("{} 个主题可用", themes.len()))
                                                    )
                                            )
                                    )
                                    .child({
                                        // 使用带下拉菜单的按钮
                                        Button::new("theme-selector")
                                            .outline()
                                            .icon(IconName::ChevronDown)
                                            .label(current_theme_name.clone())
                                            .w(px(200.0))
                                            .dropdown_menu(move |menu, _window, _cx| {
                                                let mut m = menu;
                                                for name in theme_names.iter() {
                                                    let theme_name = name.clone();
                                                    let is_current = theme_name == current_theme_name;
                                                    let handler = on_change.clone();
                                                    let click_name = theme_name.clone();
                                                    
                                                    let item = PopupMenuItem::new(theme_name)
                                                        .checked(is_current)
                                                        .on_click(move |_, window, cx| {
                                                            if let Some(h) = &handler {
                                                                h(&click_name, window, cx);
                                                            }
                                                        });
                                                    m = m.item(item);
                                                }
                                                m
                                            })
                                    })
                            )
                    )
            )
    }
}
