//! SOOP 可选账号登录配置。

use crate::ui::widgets::Section;
use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_component::{IconName, Sizable};

pub struct SoopCredentialsCard {
    username_input: Entity<InputState>,
    password_input: Entity<InputState>,
    on_save: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
}

impl SoopCredentialsCard {
    pub fn new(username_input: Entity<InputState>, password_input: Entity<InputState>) -> Self {
        Self {
            username_input,
            password_input,
            on_save: None,
        }
    }

    pub fn on_save(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_save = Some(Box::new(handler));
        self
    }
}

impl IntoElement for SoopCredentialsCard {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        div().w_full().child(SoopCredentialsInner {
            username_input: self.username_input,
            password_input: self.password_input,
            on_save: self.on_save,
        })
    }
}

#[derive(IntoElement)]
struct SoopCredentialsInner {
    username_input: Entity<InputState>,
    password_input: Entity<InputState>,
    on_save: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
}

impl RenderOnce for SoopCredentialsInner {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let username_input = self.username_input;
        let password_input = self.password_input;
        let on_save = self.on_save;

        Section::new_with_icon("SOOP 账号认证", IconName::Settings).child(
            div()
                .p(px(20.0))
                .rounded(px(12.0))
                .bg(theme.secondary)
                .border_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("公开房间通常可匿名录制。需要登录权限时，可配置有效的 SOOP Global Cookie（平台填写 soop_global），或填写用户名和密码。19+ 房间还要求账号已完成成人认证并有观看权限。密码保存在 MageKit 本地配置中。"),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(div().text_sm().child("SOOP 用户名"))
                        .child(Input::new(&username_input).small()),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(div().text_sm().child("SOOP 密码"))
                        .child(Input::new(&password_input).small().mask_toggle()),
                )
                .child(
                    div().flex().justify_end().child({
                        let mut button = Button::new("save-soop-credentials")
                            .small()
                            .primary()
                            .label("保存账号");
                        if let Some(handler) = on_save {
                            button = button.on_click(move |_event, window, cx| {
                                handler(window, cx);
                            });
                        }
                        button
                    }),
                ),
        )
    }
}
