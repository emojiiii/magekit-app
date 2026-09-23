//! 高级设置组件

use crate::ui::widgets::Section;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::switch::Switch;
use gpui_component::{Icon, IconName};

/// 高级设置数据
#[derive(Debug, Clone)]
pub struct AdvancedSettingsData {
    pub auto_check_updates: bool,
    pub debug_mode: bool,
}

impl Default for AdvancedSettingsData {
    fn default() -> Self {
        Self {
            auto_check_updates: true,
            debug_mode: false,
        }
    }
}

/// 高级设置卡片
#[derive(IntoElement)]
pub struct AdvancedSettingsCard {
    settings: AdvancedSettingsData,
    on_toggle_updates: Option<Box<dyn Fn(&bool, &mut Window, &mut App) + 'static>>,
    on_toggle_debug: Option<Box<dyn Fn(&bool, &mut Window, &mut App) + 'static>>,
}

impl AdvancedSettingsCard {
    pub fn new(auto_check_updates: bool, debug_mode: bool) -> Self {
        Self {
            settings: AdvancedSettingsData {
                auto_check_updates,
                debug_mode,
            },
            on_toggle_updates: None,
            on_toggle_debug: None,
        }
    }

    pub fn on_auto_check_change(
        mut self,
        handler: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_toggle_updates = Some(Box::new(handler));
        self
    }

    pub fn on_debug_mode_change(
        mut self,
        handler: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_toggle_debug = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for AdvancedSettingsCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let auto_check = self.settings.auto_check_updates;
        let debug_mode = self.settings.debug_mode;

        let theme = cx.theme();
        let card_bg = theme.secondary;
        let border_color = theme.border;

        Section::new_with_icon("高级", IconName::Settings).child(
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
                        // 自动检查更新
                        .child(
                            SettingsToggleItem::new("auto-updates", "自动检查更新")
                                .description("启动时自动检查工具和应用更新")
                                .icon(IconName::Loader)
                                .checked(auto_check)
                                .border_bottom(true)
                                .when_some(self.on_toggle_updates, |el, handler| {
                                    el.on_toggle(move |checked, window, cx| {
                                        handler(&checked, window, cx)
                                    })
                                }),
                        )
                        // 调试模式
                        .child(
                            SettingsToggleItem::new("debug-mode", "调试模式")
                                .description("启用详细日志和调试信息")
                                .icon(IconName::Inspector)
                                .checked(debug_mode)
                                .when_some(self.on_toggle_debug, |el, handler| {
                                    el.on_toggle(move |checked, window, cx| {
                                        handler(&checked, window, cx)
                                    })
                                }),
                        ),
                ),
        )
    }
}

/// 设置开关项组件
#[derive(IntoElement)]
pub struct SettingsToggleItem {
    id: String,
    label: String,
    description: Option<String>,
    icon: Option<IconName>,
    checked: bool,
    border_bottom: bool,
    on_toggle: Option<Box<dyn Fn(bool, &mut Window, &mut App) + 'static>>,
}

impl SettingsToggleItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            icon: None,
            checked: false,
            border_bottom: false,
            on_toggle: None,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn border_bottom(mut self, border: bool) -> Self {
        self.border_bottom = border;
        self
    }

    pub fn on_toggle(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for SettingsToggleItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let checked = self.checked;
        let border_color = cx.theme().border;
        let title_color = cx.theme().foreground;
        let muted_color = cx.theme().muted_foreground;

        div()
            .flex()
            .items_center()
            .justify_between()
            .py(px(14.0))
            .when(self.border_bottom, |el| {
                el.border_b_1().border_color(border_color)
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    // 图标
                    .when_some(self.icon, |el, icon| {
                        el.child(Icon::new(icon).text_color(title_color))
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(title_color)
                                    .child(self.label),
                            )
                            .when_some(self.description, |el, desc| {
                                el.child(div().text_xs().text_color(muted_color).child(desc))
                            }),
                    ),
            )
            .child({
                let switch_id: SharedString = self.id.into();
                let mut switch = Switch::new(switch_id).checked(checked);
                if let Some(handler) = self.on_toggle {
                    switch =
                        switch.on_click(move |_checked, window, cx| handler(!checked, window, cx));
                }
                switch
            })
    }
}
