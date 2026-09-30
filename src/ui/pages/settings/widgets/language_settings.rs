//! 界面语言选择。选项值与显示文案分离，跟随系统不保存解析结果。
use crate::i18n::{Language, tr};
use crate::ui::widgets::Section;
use gpui::*;
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme, IconName};
use std::sync::Arc;

#[derive(IntoElement)]
pub struct LanguageSettingsCard {
    current: Language,
    on_change: Arc<dyn Fn(Language, &mut Window, &mut App)>,
}

fn language_label(language: Language) -> &'static str {
    match language {
        Language::System => tr("跟随系统"),
        // 语言选项始终显示本名，便于从不熟悉的语言切换回来。
        Language::Chinese => "中文",
        Language::English => "English",
    }
}

impl LanguageSettingsCard {
    pub fn new(
        current: Language,
        on_change: impl Fn(Language, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            current,
            on_change: Arc::new(on_change),
        }
    }
}

impl RenderOnce for LanguageSettingsCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let current = self.current;
        let on_change = self.on_change;
        Section::new_with_icon(tr("语言"), IconName::Globe).child(
            div()
                .p_4()
                .rounded_lg()
                .bg(cx.theme().secondary)
                .border_1()
                .border_color(cx.theme().border)
                .flex()
                .items_center()
                .justify_between()
                .gap_4()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr("选择应用程序的界面语言")),
                )
                .child(
                    Button::new("language-selector")
                        .outline()
                        .label(language_label(current))
                        .icon(IconName::ChevronDown)
                        .dropdown_menu(move |menu, _window, _cx| {
                            let mut menu = menu;
                            for language in [Language::System, Language::Chinese, Language::English]
                            {
                                let on_change = on_change.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(language_label(language))
                                        .checked(current == language)
                                        .on_click(move |_, window, cx| {
                                            on_change(language, window, cx)
                                        }),
                                );
                            }
                            menu
                        }),
                ),
        )
    }
}
