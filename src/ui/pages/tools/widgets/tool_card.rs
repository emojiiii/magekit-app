//! 工具卡片组件

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::Disableable;
use gpui_component::button::{Button, ButtonVariants};
use magekit_shared::ToolType;

/// 下载进度信息
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadProgress {
    /// 已下载字节数
    pub downloaded: u64,
    /// 总字节数
    pub total: u64,
    /// 下载速度 (bytes/s)
    pub speed: u64,
}

impl DownloadProgress {
    /// 计算下载百分比
    pub fn percent(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.downloaded as f32 / self.total as f32) * 100.0
        }
    }

    /// 格式化已下载大小
    pub fn downloaded_str(&self) -> String {
        format_bytes(self.downloaded)
    }

    /// 格式化总大小
    pub fn total_str(&self) -> String {
        format_bytes(self.total)
    }

    /// 格式化下载速度
    pub fn speed_str(&self) -> String {
        format!("{}/s", format_bytes(self.speed))
    }
}

/// 格式化字节数
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// 工具安装状态
#[derive(Debug, Clone, PartialEq)]
pub enum ToolInstallState {
    Unknown,
    NotInstalled,
    /// 已安装 (version: 版本号, is_system: 是否是系统安装)
    Installed {
        version: Option<String>,
        is_system: bool,
    },
    /// 下载中 (带进度信息)
    Downloading(DownloadProgress),
    /// 安装中 (下载完成，正在安装)
    Installing,
    #[allow(dead_code)]
    Failed(String),
}

impl Default for ToolInstallState {
    fn default() -> Self {
        Self::Unknown
    }
}

/// 工具信息
#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub tool_type: ToolType,
    pub state: ToolInstallState,
    pub icon: &'static str,
}

impl ToolInfo {
    pub fn yt_dlp() -> Self {
        Self {
            name: "yt-dlp",
            description: "强大的视频下载工具，支持 YouTube、Bilibili 等众多平台",
            tool_type: ToolType::YtDlp,
            state: ToolInstallState::Unknown,
            icon: "📥",
        }
    }

    pub fn ffmpeg() -> Self {
        Self {
            name: "FFmpeg",
            description: "音视频处理工具，用于格式转换和视频合并",
            tool_type: ToolType::Ffmpeg,
            state: ToolInstallState::Unknown,
            icon: "🎞️",
        }
    }
}

/// 工具卡片组件
#[derive(IntoElement)]
pub struct ToolCard<F>
where
    F: Fn(&ToolType, &mut Window, &mut App) + 'static,
{
    tool: ToolInfo,
    index: usize,
    on_action: Option<F>,
}

impl<F> ToolCard<F>
where
    F: Fn(&ToolType, &mut Window, &mut App) + 'static,
{
    pub fn new(tool: ToolInfo, index: usize) -> Self {
        Self {
            tool,
            index,
            on_action: None,
        }
    }

    pub fn on_action(mut self, handler: F) -> Self {
        self.on_action = Some(handler);
        self
    }
}

impl<F> RenderOnce for ToolCard<F>
where
    F: Fn(&ToolType, &mut Window, &mut App) + 'static,
{
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tool = self.tool;
        let tool_type = tool.tool_type;
        let is_installing = matches!(
            tool.state,
            ToolInstallState::Installing | ToolInstallState::Downloading(_)
        );
        let tool_name: SharedString = tool.name.into();
        let tool_desc: SharedString = tool.description.into();
        let tool_icon = tool.icon;

        // 使用主题颜色
        let card_bg = cx.theme().secondary;
        let border_color = cx.theme().border;
        let title_color = cx.theme().foreground;
        let muted_color = cx.theme().muted_foreground;
        let icon_bg = cx.theme().muted;
        let success_color = cx.theme().success;
        let danger_color = cx.theme().danger;
        let primary_color = cx.theme().primary;

        let (status_text, status_color): (SharedString, Hsla) = match &tool.state {
            ToolInstallState::Unknown => ("检查中...".into(), muted_color),
            ToolInstallState::NotInstalled => ("未安装".into(), danger_color),
            ToolInstallState::Installed { version, is_system } => {
                let text: SharedString = match (version, is_system) {
                    (Some(v), true) => format!("v{} (系统)", v).into(),
                    (Some(v), false) => format!("v{}", v).into(),
                    (None, true) => "已安装 (系统)".into(),
                    (None, false) => "已安装".into(),
                };
                (text, success_color)
            }
            ToolInstallState::Downloading(progress) => {
                let text: SharedString = format!("下载中 {:.1}%", progress.percent()).into();
                (text, primary_color)
            }
            ToolInstallState::Installing => ("安装中...".into(), primary_color),
            ToolInstallState::Failed(_) => ("安装失败".into(), danger_color),
        };

        let status_bg: Hsla = status_color.opacity(0.15);

        let (btn_label, btn_disabled) = match &tool.state {
            ToolInstallState::NotInstalled | ToolInstallState::Failed(_) => ("安装", false),
            ToolInstallState::Installed { .. } => ("检查更新", false),
            ToolInstallState::Installing | ToolInstallState::Downloading(_) => ("下载中...", true),
            ToolInstallState::Unknown => ("...", true),
        };

        let btn_id: SharedString = format!("install-{}", self.index).into();
        let on_action = self.on_action;

        div()
            .p(px(20.0))
            .rounded(px(12.0))
            .bg(card_bg)
            .border_1()
            .border_color(border_color)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        // 左侧：图标和信息
                        div()
                            .flex()
                            .items_center()
                            .gap(px(16.0))
                            .child(
                                // 图标
                                div()
                                    .w(px(48.0))
                                    .h(px(48.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(icon_bg)
                                    .rounded(px(12.0))
                                    .text_2xl()
                                    .child(tool_icon),
                            )
                            .child(
                                // 名称和描述
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(
                                                div()
                                                    .text_base()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(title_color)
                                                    .child(tool_name),
                                            )
                                            .child(
                                                // 状态标签
                                                div()
                                                    .px(px(8.0))
                                                    .py(px(2.0))
                                                    .bg(status_bg)
                                                    .rounded(px(4.0))
                                                    .text_xs()
                                                    .text_color(status_color)
                                                    .child(status_text),
                                            ),
                                    )
                                    .child(
                                        div().text_sm().text_color(muted_color).child(tool_desc),
                                    ),
                            ),
                    )
                    .child(
                        // 右侧：操作按钮
                        Button::new(btn_id)
                            .label(btn_label)
                            .primary()
                            .disabled(btn_disabled || is_installing)
                            .when_some(on_action, |btn, handler| {
                                btn.on_click(move |_ev, window, cx| {
                                    handler(&tool_type, window, cx);
                                })
                            }),
                    ),
            )
    }
}

/// 工具提示卡片
#[derive(IntoElement)]
pub struct ToolHintCard;

impl RenderOnce for ToolHintCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let hint_bg = cx.theme().muted;
        let title_color = cx.theme().foreground;
        let muted_color = cx.theme().muted_foreground;

        div().p(px(16.0)).rounded(px(12.0)).bg(hint_bg).child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(title_color)
                        .child("💡 关于工具"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted_color)
                        .child("这些工具是视频下载功能所必需的。首次使用时会自动从官方源下载。"),
                ),
        )
    }
}
