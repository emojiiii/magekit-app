//! 设置页面主组件

use gpui::*;
use gpui_component::{ActiveTheme, Theme, ThemeRegistry};
use crate::app::AppState;
use magekit_shared::types::Theme as AppTheme;
use std::sync::Arc;
use std::path::PathBuf;
use super::widgets::{
    DownloadSettingsCard,
    AdvancedSettingsCard,
    AboutSection,
    ThemeSettingsCard,
};

/// 设置页面
pub struct SettingsPage {
    app_state: Arc<AppState>,
    // 下载设置
    download_path: String,
    max_concurrent: usize,
    embed_metadata: bool,
    embed_thumbnail: bool,
    
    // 外观设置 - 存储主题名称
    theme_name: SharedString,
    
    // 高级设置
    auto_check_updates: bool,
    debug_mode: bool,
    
    // 是否有未保存的更改
    has_changes: bool,
}

impl SettingsPage {
    pub fn new(app_state: Arc<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 从 AppState 加载配置
        let config = app_state.config();
        let download_path = config.download.default_output_path.to_string_lossy().to_string();
        
        // 获取当前主题名称
        let theme_name: SharedString = cx.theme().theme_name().clone();
        
        Self {
            app_state,
            download_path,
            max_concurrent: config.download.max_concurrent_downloads,
            embed_metadata: config.download.embed_metadata,
            embed_thumbnail: config.download.embed_thumbnail,
            theme_name,
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
    
    fn set_theme(&mut self, theme_name: &SharedString, _window: &mut Window, cx: &mut Context<Self>) {
        self.theme_name = theme_name.clone();
        self.has_changes = true;
        
        // 从 ThemeRegistry 获取主题配置并应用
        if let Some(theme_config) = ThemeRegistry::global(cx)
            .themes()
            .get(theme_name)
            .cloned()
        {
            Theme::global_mut(cx).apply_config(&theme_config);
            cx.refresh_windows();
        }
        
        self.save_settings(cx);
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
                
                // 更新 SettingsPage
                let _ = this.update(cx, |this, cx| {
                    this.download_path = path_str;
                    this.has_changes = true;
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
        let theme_name = self.theme_name.to_string();
        
        cx.spawn(async move |_this, _cx| {
            // 获取当前配置并更新
            smol::unblock(move || {
                let mut config = app_state.config();
                config.download.default_output_path = download_path;
                config.download.max_concurrent_downloads = max_concurrent;
                config.download.embed_metadata = embed_metadata;
                config.download.embed_thumbnail = embed_thumbnail;
                config.tools.auto_update = auto_check_updates;
                // 保存主题名称到配置
                config.ui.theme = AppTheme::Custom(magekit_shared::types::ThemeConfig {
                    name: theme_name.clone(),
                    mode: if theme_name.to_lowercase().contains("dark") 
                        || theme_name.to_lowercase().contains("night")
                        || theme_name.to_lowercase().contains("noir") {
                        magekit_shared::types::ThemeMode::Dark
                    } else {
                        magekit_shared::types::ThemeMode::Light
                    },
                });
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
        // 使用主题颜色
        let theme = cx.theme();
        let title_color = theme.foreground;
        let desc_color = theme.muted_foreground;
        
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
                            .gap(px(20.0))
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
                                                    .text_xl()
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(title_color)
                                                    .child("设置")
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(desc_color)
                                                    .child("自定义应用程序行为和偏好")
                                            )
                                    )
                            )
                            // 外观设置（主题切换）
                            .child(
                                ThemeSettingsCard::new(self.theme_name.clone())
                                    .on_theme_change(cx.listener(|this, theme_name: &SharedString, window, cx| {
                                        this.set_theme(theme_name, window, cx);
                                    }))
                            )
                            // 下载设置
                            .child(
                                DownloadSettingsCard::new(&self.download_path, self.max_concurrent)
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
