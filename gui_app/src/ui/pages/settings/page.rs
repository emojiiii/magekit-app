//! 设置页面主组件

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::input::InputState;
use crate::app::AppState;
use std::sync::Arc;
use super::widgets::{
    DownloadSettingsCard,
    AdvancedSettingsCard,
    AboutSection,
};

/// 设置页面
pub struct SettingsPage {
    #[allow(dead_code)]
    app_state: Arc<AppState>,
    // 下载设置
    download_path_input: Entity<InputState>,
    max_concurrent: usize,
    
    // 高级设置
    auto_check_updates: bool,
    debug_mode: bool,
}

impl SettingsPage {
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            app_state,
            download_path_input: cx.new(|cx| InputState::new(window, cx).placeholder("~/Downloads/MageKit")),
            max_concurrent: 3,
            auto_check_updates: true,
            debug_mode: false,
        }
    }
    
    fn increment_concurrent(&mut self, _cx: &mut Context<Self>) {
        if self.max_concurrent < 10 {
            self.max_concurrent += 1;
        }
    }
    
    fn decrement_concurrent(&mut self, _cx: &mut Context<Self>) {
        if self.max_concurrent > 1 {
            self.max_concurrent -= 1;
        }
    }
    
    fn toggle_auto_check_updates(&mut self, enabled: bool, _cx: &mut Context<Self>) {
        self.auto_check_updates = enabled;
    }
    
    fn toggle_debug_mode(&mut self, enabled: bool, _cx: &mut Context<Self>) {
        self.debug_mode = enabled;
    }
    
    fn browse_download_path(&mut self, _cx: &mut Context<Self>) {
        // TODO: 实现文件夹选择对话框
        println!("Browse download path clicked");
    }
}

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("settings-page")
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                // 可滚动内容区域
                div()
                    .id("settings-scroll-container")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .p(px(24.0))
                            .gap(px(24.0))
                            // 页面标题
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.0))
                                    .child(
                                        div()
                                            .text_2xl()
                                            .child("⚙️")
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .text_2xl()
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(rgb(0xfafafa))
                                                    .child("设置")
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(rgb(0xa1a1aa))
                                                    .child("自定义应用程序行为和偏好")
                                            )
                                    )
                            )
                            // 下载设置
                            .child(
                                DownloadSettingsCard::new(&self.download_path_input, self.max_concurrent)
                                    .on_browse(cx.listener(|this, _ev, _window, cx| {
                                        this.browse_download_path(cx);
                                    }))
                                    .on_increment(cx.listener(|this, _ev, _window, cx| {
                                        this.increment_concurrent(cx);
                                    }))
                                    .on_decrement(cx.listener(|this, _ev, _window, cx| {
                                        this.decrement_concurrent(cx);
                                    }))
                            )
                            // 高级设置
                            .child(
                                AdvancedSettingsCard::new(self.auto_check_updates, self.debug_mode)
                                    .on_auto_check_change(cx.listener(|this, enabled: &bool, _window, cx| {
                                        this.toggle_auto_check_updates(*enabled, cx);
                                    }))
                                    .on_debug_mode_change(cx.listener(|this, enabled: &bool, _window, cx| {
                                        this.toggle_debug_mode(*enabled, cx);
                                    }))
                            )
                            // 关于
                            .child(AboutSection)
                    )
            )
    }
}
