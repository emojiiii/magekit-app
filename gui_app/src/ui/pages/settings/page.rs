//! 设置页面主组件

use gpui::*;
use gpui_component::input::InputState;
use crate::app::AppState;
use std::sync::Arc;
use std::path::PathBuf;
use super::widgets::{
    DownloadSettingsCard,
    AdvancedSettingsCard,
    AboutSection,
};

/// 设置页面
pub struct SettingsPage {
    app_state: Arc<AppState>,
    // 下载设置
    download_path_input: Entity<InputState>,
    download_path: String,
    max_concurrent: usize,
    embed_metadata: bool,
    embed_thumbnail: bool,
    
    // 高级设置
    auto_check_updates: bool,
    debug_mode: bool,
    
    // 是否有未保存的更改
    has_changes: bool,
}

impl SettingsPage {
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 从 AppState 加载配置
        let config = app_state.config();
        let download_path = config.download.default_output_path.to_string_lossy().to_string();
        
        Self {
            app_state,
            download_path_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("~/Downloads/MageKit")
                    .default_value(&download_path)
            }),
            download_path,
            max_concurrent: config.download.max_concurrent_downloads,
            embed_metadata: config.download.embed_metadata,
            embed_thumbnail: config.download.embed_thumbnail,
            auto_check_updates: config.tools.auto_update,
            debug_mode: matches!(config.advanced.log_level, magekit_shared::LogLevel::Debug | magekit_shared::LogLevel::Trace),
            has_changes: false,
        }
    }
    
    fn increment_concurrent(&mut self, cx: &mut Context<Self>) {
        if self.max_concurrent < 10 {
            self.max_concurrent += 1;
            self.has_changes = true;
            self.save_settings(cx);
        }
    }
    
    fn decrement_concurrent(&mut self, cx: &mut Context<Self>) {
        if self.max_concurrent > 1 {
            self.max_concurrent -= 1;
            self.has_changes = true;
            self.save_settings(cx);
        }
    }
    
    fn toggle_auto_check_updates(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.auto_check_updates = enabled;
        self.has_changes = true;
        self.save_settings(cx);
    }
    
    fn toggle_debug_mode(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.debug_mode = enabled;
        self.has_changes = true;
        self.save_settings(cx);
    }
    
    fn browse_download_path(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // 使用 rfd 打开文件夹选择对话框
        let current_path = self.download_path.clone();
        
        cx.spawn(async move |this, cx| {
            // 在后台线程中打开文件对话框
            let selected_path: Option<PathBuf> = smol::unblock(move || {
                let dialog = rfd::FileDialog::new()
                    .set_title("选择下载目录")
                    .set_directory(&current_path);
                dialog.pick_folder()
            }).await;
            
            if let Some(path) = selected_path {
                let path_str = path.to_string_lossy().to_string();
                let _ = this.update(cx, |this, cx| {
                    this.download_path = path_str.clone();
                    this.has_changes = true;
                    // 更新输入框
                    this.download_path_input.update(cx, |_state, _cx| {
                        // InputState 可能需要不同的方法来设置值
                        // 这里暂时只更新内部状态
                    });
                    this.save_settings(cx);
                    cx.notify();
                });
            }
        }).detach();
    }
    
    /// 保存设置到 AppState
    fn save_settings(&mut self, cx: &mut Context<Self>) {
        let app_state = self.app_state.clone();
        let download_path = PathBuf::from(&self.download_path);
        let max_concurrent = self.max_concurrent;
        let embed_metadata = self.embed_metadata;
        let embed_thumbnail = self.embed_thumbnail;
        let auto_check_updates = self.auto_check_updates;
        let debug_mode = self.debug_mode;
        
        cx.spawn(async move |_this, _cx| {
            // 获取当前配置并更新
            smol::unblock(move || {
                let mut config = app_state.config();
                config.download.default_output_path = download_path;
                config.download.max_concurrent_downloads = max_concurrent;
                config.download.embed_metadata = embed_metadata;
                config.download.embed_thumbnail = embed_thumbnail;
                config.tools.auto_update = auto_check_updates;
                config.advanced.log_level = if debug_mode {
                    magekit_shared::LogLevel::Debug
                } else {
                    magekit_shared::LogLevel::Info
                };
                
                // 使用 runtime 保存配置
                app_state.runtime.block_on(async {
                    let _ = app_state.update_config(config).await;
                });
            }).await;
        }).detach();
        
        self.has_changes = false;
        cx.notify();
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
                                    .on_browse(cx.listener(|this, _ev, window, cx| {
                                        this.browse_download_path(window, cx);
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
