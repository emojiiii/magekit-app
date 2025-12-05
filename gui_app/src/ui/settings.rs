//! 设置界面组件
//!
//! 提供应用程序设置的用户界面，包括下载设置、工具设置、主题设置等

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::*;
use gpui_component::button::{Button, ButtonVariants};
use magekit_shared::types::{AppConfig, DownloadConfig, ToolsConfig, UiConfig, AdvancedConfig, UpdateChannel, LogLevel};
use magekit_shared::types::Theme as AppTheme;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::path::PathBuf;

/// 设置面板选项卡
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    /// 下载设置
    Download,
    /// 工具设置
    Tools,
    /// 界面设置
    Appearance,
    /// 高级设置
    Advanced,
}

impl SettingsTab {
    fn label(&self) -> &'static str {
        match self {
            SettingsTab::Download => "下载",
            SettingsTab::Tools => "工具",
            SettingsTab::Appearance => "外观",
            SettingsTab::Advanced => "高级",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            SettingsTab::Download => "⬇️",
            SettingsTab::Tools => "🔧",
            SettingsTab::Appearance => "🎨",
            SettingsTab::Advanced => "⚙️",
        }
    }
}

/// 设置变更事件
#[derive(Debug, Clone)]
pub enum SettingsChange {
    /// 下载路径变更
    DownloadPath(PathBuf),
    /// 并发数变更
    MaxConcurrent(usize),
    /// 默认格式变更
    DefaultFormat(String),
    /// 嵌入元数据变更
    EmbedMetadata(bool),
    /// 嵌入缩略图变更
    EmbedThumbnail(bool),
    /// 自动更新变更
    AutoUpdate(bool),
    /// 更新通道变更
    UpdateChannel(UpdateChannel),
    /// 主题变更
    ThemeChange(AppTheme),
    /// 语言变更
    Language(String),
    /// 显示通知变更
    ShowNotifications(bool),
    /// 日志级别变更
    LogLevel(LogLevel),
    /// 速度限制变更
    SpeedLimit(Option<u64>),
    /// 重试次数变更
    RetryTimes(u32),
}

/// 设置回调类型
pub type SettingsCallback = Arc<dyn Fn(SettingsChange) + Send + Sync + 'static>;

/// 设置视图组件
pub struct SettingsView {
    /// 当前配置
    config: Arc<RwLock<AppConfig>>,
    /// 当前选中的选项卡
    active_tab: SettingsTab,
    /// 设置变更回调
    _on_change: Option<SettingsCallback>,
    /// 是否有未保存的更改
    _has_changes: bool,
}

impl SettingsView {
    /// 创建新的设置视图
    pub fn new(config: Arc<RwLock<AppConfig>>) -> Self {
        Self {
            config,
            active_tab: SettingsTab::Download,
            _on_change: None,
            _has_changes: false,
        }
    }

    /// 设置变更回调
    pub fn on_change(mut self, callback: SettingsCallback) -> Self {
        self._on_change = Some(callback);
        self
    }

    /// 切换选项卡
    pub fn set_tab(&mut self, tab: SettingsTab) {
        self.active_tab = tab;
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let active_tab = self.active_tab;
        
        // 获取配置快照
        let config_snapshot = if let Ok(config) = self.config.try_read() {
            Some(config.clone())
        } else {
            None
        };
        
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            // 标题栏
            .child(
                div()
                    .h(px(56.0))
                    .px(px(24.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .child("设置")
                    )
                    .child(
                        Button::new("close-settings")
                            .ghost()
                            .icon(IconName::Close)
                    )
            )
            // 主内容区域
            .child(
                div()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    // 侧边栏选项卡
                    .child(
                        div()
                            .w(px(180.0))
                            .border_r_1()
                            .border_color(theme.border)
                            .flex()
                            .flex_col()
                            .p(px(12.0))
                            .gap(px(4.0))
                            .child(render_tab_item(SettingsTab::Download, active_tab, &theme))
                            .child(render_tab_item(SettingsTab::Tools, active_tab, &theme))
                            .child(render_tab_item(SettingsTab::Appearance, active_tab, &theme))
                            .child(render_tab_item(SettingsTab::Advanced, active_tab, &theme))
                    )
                    // 内容区域
                    .child(
                        div()
                            .flex_1()
                            .p(px(24.0))
                            .overflow_hidden()
                            .child(
                                if let Some(config) = config_snapshot {
                                    match active_tab {
                                        SettingsTab::Download => render_download_settings(&config.download, theme).into_any_element(),
                                        SettingsTab::Tools => render_tools_settings(&config.tools, theme).into_any_element(),
                                        SettingsTab::Appearance => render_appearance_settings(&config.ui, theme).into_any_element(),
                                        SettingsTab::Advanced => render_advanced_settings(&config.advanced, theme).into_any_element(),
                                    }
                                } else {
                                    render_loading_state(theme).into_any_element()
                                }
                            )
                    )
            )
            // 底部按钮栏
            .child(
                div()
                    .h(px(64.0))
                    .px(px(24.0))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(12.0))
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        Button::new("reset-settings")
                            .ghost()
                            .label("重置默认")
                    )
                    .child(
                        Button::new("save-settings")
                            .primary()
                            .label("保存")
                    )
            )
    }
}

/// 渲染选项卡项
fn render_tab_item(tab: SettingsTab, active: SettingsTab, theme: &gpui_component::Theme) -> impl IntoElement {
    let is_active = tab == active;
    let accent_color = theme.accent;
    let foreground = theme.foreground;
    let muted_foreground = theme.muted_foreground;
    
    div()
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .when(is_active, |this| {
            this.bg(accent_color)
        })
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .text_sm()
                .child(tab.icon().to_string())
        )
        .child(
            div()
                .text_sm()
                .font_weight(if is_active { FontWeight::MEDIUM } else { FontWeight::NORMAL })
                .text_color(if is_active { foreground } else { muted_foreground })
                .child(tab.label().to_string())
        )
}

/// 渲染下载设置
fn render_download_settings(config: &DownloadConfig, theme: &gpui_component::Theme) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(24.0))
        // 下载路径
        .child(render_setting_section(
            "下载路径",
            "设置视频下载的默认保存位置",
            div()
                .flex()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .px(px(12.0))
                        .py(px(8.0))
                        .border_1()
                        .border_color(theme.border)
                        .rounded(px(6.0))
                        .bg(theme.background)
                        .text_color(theme.foreground)
                        .text_sm()
                        .overflow_hidden()
                        .child(config.default_output_path.to_string_lossy().to_string())
                )
                .child(
                    Button::new("browse-path")
                        .small()
                        .icon(IconName::Folder)
                ),
            theme,
        ))
        // 并发下载数
        .child(render_setting_section(
            "并发下载数",
            "同时下载的最大任务数量",
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    Button::new("dec-concurrent")
                        .ghost()
                        .small()
                        .icon(IconName::Minus)
                )
                .child(
                    div()
                        .w(px(48.0))
                        .text_center()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child(format!("{}", config.max_concurrent_downloads))
                )
                .child(
                    Button::new("inc-concurrent")
                        .ghost()
                        .small()
                        .icon(IconName::Plus)
                ),
            theme,
        ))
        // 默认格式
        .child(render_setting_section(
            "默认格式",
            "下载视频的默认质量格式",
            div()
                .px(px(12.0))
                .py(px(8.0))
                .border_1()
                .border_color(theme.border)
                .rounded(px(6.0))
                .bg(theme.background)
                .text_color(theme.foreground)
                .text_sm()
                .child(format_display_name(&config.default_format)),
            theme,
        ))
        // 嵌入选项
        .child(render_setting_section(
            "嵌入选项",
            "下载时自动嵌入的内容",
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(render_checkbox_item("嵌入元数据", config.embed_metadata, theme))
                .child(render_checkbox_item("嵌入缩略图", config.embed_thumbnail, theme)),
            theme,
        ))
}

/// 渲染工具设置
fn render_tools_settings(config: &ToolsConfig, theme: &gpui_component::Theme) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(24.0))
        // 自动更新
        .child(render_setting_section(
            "自动更新",
            "自动检查并更新下载工具",
            render_checkbox_item("启用自动更新", config.auto_update, theme),
            theme,
        ))
        // 更新通道
        .child(render_setting_section(
            "更新通道",
            "选择工具的更新版本类型",
            div()
                .flex()
                .gap(px(8.0))
                .child(render_radio_item("稳定版", matches!(config.update_channel, UpdateChannel::Stable), theme))
                .child(render_radio_item("开发版", matches!(config.update_channel, UpdateChannel::Nightly), theme)),
            theme,
        ))
        // yt-dlp 版本
        .child(render_setting_section(
            "yt-dlp",
            "视频下载引擎",
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.foreground)
                        .child(config.yt_dlp_version.clone().unwrap_or_else(|| "未安装".to_string()))
                )
                .child(
                    Button::new("update-ytdlp")
                        .ghost()
                        .small()
                        .label("检查更新")
                ),
            theme,
        ))
        // ffmpeg 版本
        .child(render_setting_section(
            "FFmpeg",
            "音视频处理工具",
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.foreground)
                        .child(config.ffmpeg_version.clone().unwrap_or_else(|| "未安装".to_string()))
                )
                .child(
                    Button::new("update-ffmpeg")
                        .ghost()
                        .small()
                        .label("检查更新")
                ),
            theme,
        ))
}

/// 渲染外观设置
fn render_appearance_settings(config: &UiConfig, theme: &gpui_component::Theme) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(24.0))
        // 主题选择
        .child(render_setting_section(
            "主题",
            "选择应用程序的外观主题",
            div()
                .flex()
                .gap(px(8.0))
                .child(render_theme_card("浅色", "☀️", matches!(config.theme, AppTheme::Light), theme))
                .child(render_theme_card("深色", "🌙", matches!(config.theme, AppTheme::Dark), theme))
                .child(render_theme_card("跟随系统", "💻", matches!(config.theme, AppTheme::System), theme)),
            theme,
        ))
        // 语言
        .child(render_setting_section(
            "语言",
            "选择应用程序的界面语言",
            div()
                .px(px(12.0))
                .py(px(8.0))
                .border_1()
                .border_color(theme.border)
                .rounded(px(6.0))
                .bg(theme.background)
                .text_color(theme.foreground)
                .text_sm()
                .child(language_display_name(&config.language)),
            theme,
        ))
        // 通知设置
        .child(render_setting_section(
            "通知",
            "控制应用程序的通知行为",
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(render_checkbox_item("显示下载完成通知", config.show_notifications, theme))
                .child(render_checkbox_item("最小化到系统托盘", config.minimize_to_tray, theme)),
            theme,
        ))
}

/// 渲染高级设置
fn render_advanced_settings(config: &AdvancedConfig, theme: &gpui_component::Theme) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(24.0))
        // 日志级别
        .child(render_setting_section(
            "日志级别",
            "设置应用程序的日志详细程度",
            div()
                .flex()
                .gap(px(8.0))
                .child(render_radio_item("错误", matches!(config.log_level, LogLevel::Error), theme))
                .child(render_radio_item("警告", matches!(config.log_level, LogLevel::Warn), theme))
                .child(render_radio_item("信息", matches!(config.log_level, LogLevel::Info), theme))
                .child(render_radio_item("调试", matches!(config.log_level, LogLevel::Debug), theme)),
            theme,
        ))
        // 速度限制
        .child(render_setting_section(
            "速度限制",
            "限制下载速度（留空表示不限制）",
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .w(px(120.0))
                        .px(px(12.0))
                        .py(px(8.0))
                        .border_1()
                        .border_color(theme.border)
                        .rounded(px(6.0))
                        .bg(theme.background)
                        .text_color(theme.foreground)
                        .text_sm()
                        .child(
                            config.speed_limit
                                .map(|s| format_speed(s))
                                .unwrap_or_else(|| "不限制".to_string())
                        )
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("MB/s")
                ),
            theme,
        ))
        // 重试次数
        .child(render_setting_section(
            "重试次数",
            "下载失败时的最大重试次数",
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    Button::new("dec-retry")
                        .ghost()
                        .small()
                        .icon(IconName::Minus)
                )
                .child(
                    div()
                        .w(px(48.0))
                        .text_center()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child(format!("{}", config.retry_times))
                )
                .child(
                    Button::new("inc-retry")
                        .ghost()
                        .small()
                        .icon(IconName::Plus)
                ),
            theme,
        ))
        // 超时设置
        .child(render_setting_section(
            "连接超时",
            "网络请求的超时时间",
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .w(px(80.0))
                        .px(px(12.0))
                        .py(px(8.0))
                        .border_1()
                        .border_color(theme.border)
                        .rounded(px(6.0))
                        .bg(theme.background)
                        .text_color(theme.foreground)
                        .text_sm()
                        .child(format!("{}", config.timeout.as_secs()))
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("秒")
                ),
            theme,
        ))
}

/// 渲染设置区块
fn render_setting_section(
    title: &str,
    description: &str,
    content: impl IntoElement,
    theme: &gpui_component::Theme,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child(title.to_string())
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(description.to_string())
                )
        )
        .child(content)
}

/// 渲染复选框项
fn render_checkbox_item(label: &str, checked: bool, theme: &gpui_component::Theme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .cursor_pointer()
        .child(
            div()
                .w(px(16.0))
                .h(px(16.0))
                .border_1()
                .border_color(if checked { theme.primary } else { theme.border })
                .rounded(px(4.0))
                .bg(if checked { theme.primary } else { theme.background })
                .flex()
                .items_center()
                .justify_center()
                .when(checked, |this| {
                    this.child(
                        div()
                            .text_color(theme.primary_foreground)
                            .text_xs()
                            .child("✓")
                    )
                })
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.foreground)
                .child(label.to_string())
        )
}

/// 渲染单选项
fn render_radio_item(label: &str, selected: bool, theme: &gpui_component::Theme) -> impl IntoElement {
    div()
        .px(px(12.0))
        .py(px(6.0))
        .border_1()
        .border_color(if selected { theme.primary } else { theme.border })
        .rounded(px(6.0))
        .bg(if selected { theme.accent } else { theme.background })
        .cursor_pointer()
        .child(
            div()
                .text_sm()
                .text_color(if selected { theme.foreground } else { theme.muted_foreground })
                .child(label.to_string())
        )
}

/// 渲染主题卡片
fn render_theme_card(label: &str, icon: &str, selected: bool, theme: &gpui_component::Theme) -> impl IntoElement {
    div()
        .w(px(100.0))
        .p(px(12.0))
        .border_2()
        .border_color(if selected { theme.primary } else { theme.border })
        .rounded(px(8.0))
        .bg(theme.background)
        .cursor_pointer()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .text_2xl()
                .child(icon.to_string())
        )
        .child(
            div()
                .text_sm()
                .text_color(if selected { theme.foreground } else { theme.muted_foreground })
                .child(label.to_string())
        )
}

/// 渲染加载状态
fn render_loading_state(theme: &gpui_component::Theme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size_full()
        .child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("加载中...")
        )
}

/// 格式显示名称
fn format_display_name(format: &str) -> String {
    match format {
        "best" => "最佳质量".to_string(),
        "bestvideo+bestaudio" => "最佳视频+音频".to_string(),
        "bestvideo" => "仅最佳视频".to_string(),
        "bestaudio" => "仅最佳音频".to_string(),
        "worst" => "最低质量".to_string(),
        _ => format.to_string(),
    }
}

/// 语言显示名称
fn language_display_name(lang: &str) -> String {
    match lang {
        "en" => "English".to_string(),
        "zh" => "简体中文".to_string(),
        "zh-TW" => "繁體中文".to_string(),
        "ja" => "日本語".to_string(),
        "ko" => "한국어".to_string(),
        _ => lang.to_string(),
    }
}

/// 格式化速度
fn format_speed(bytes_per_sec: u64) -> String {
    let mb = bytes_per_sec as f64 / 1024.0 / 1024.0;
    format!("{:.1}", mb)
}
