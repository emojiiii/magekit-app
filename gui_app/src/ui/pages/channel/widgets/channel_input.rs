//! 频道 URL 输入卡片组件

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Disableable, Icon, IconName};

/// 频道输入卡片
pub struct ChannelInputCard {
    /// 输入框实体
    input: Entity<gpui_component::input::InputState>,
    /// 是否正在解析
    is_loading: bool,
    /// 解析按钮点击回调
    on_parse: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
}

impl ChannelInputCard {
    pub fn new(input: Entity<gpui_component::input::InputState>) -> Self {
        Self {
            input,
            is_loading: false,
            on_parse: None,
        }
    }
    
    pub fn loading(mut self, is_loading: bool) -> Self {
        self.is_loading = is_loading;
        self
    }
    
    pub fn on_parse(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_parse = Some(Box::new(callback));
        self
    }
}

impl IntoElement for ChannelInputCard {
    type Element = Stateful<Div>;
    
    fn into_element(self) -> Self::Element {
        let input = self.input;
        let is_loading = self.is_loading;
        let on_parse = self.on_parse;
        
        div()
            .id("channel-input-card")
            .w_full()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(gpui::hsla(0.0, 0.0, 0.5, 0.2))
            .bg(gpui::hsla(0.0, 0.0, 0.5, 0.05))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    // 标题
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Icon::new(IconName::Folder)
                                    .size_4()
                            )
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("频道/播放列表")
                            )
                    )
                    // 说明文字
                    .child(
                        div()
                            .text_sm()
                            .text_color(gpui::hsla(0.0, 0.0, 0.5, 0.7))
                            .child("支持 YouTube 频道、播放列表、Bilibili UP主空间等")
                    )
                    // 输入框和按钮
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .child(input.clone())
                            )
                            .child(
                                Button::new("parse-channel")
                                    .primary()
                                    .label(if is_loading { "解析中..." } else { "解析" })
                                    .icon(if is_loading { IconName::LoaderCircle } else { IconName::Search })
                                    .disabled(is_loading)
                                    .when_some(on_parse, |btn, callback| {
                                        btn.on_click(move |_, window, cx| {
                                            callback(window, cx);
                                        })
                                    })
                            )
                    )
            )
    }
}
