//! 工具管理面板组件
//!
//! 提供工具（yt-dlp, ffmpeg）的状态显示、版本检查和更新功能

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::*;
use std::sync::Arc;

/// 工具信息
#[derive(Debug, Clone)]
pub struct ToolInfo {
    /// 工具名称
    pub name: String,
    /// 工具描述
    pub description: String,
    /// 当前版本
    pub current_version: Option<String>,
    /// 最新版本
    pub latest_version: Option<String>,
    /// 工具状态
    pub status: ToolState,
    /// 安装路径
    pub install_path: Option<String>,
    /// 图标
    pub icon: &'static str,
}

/// 工具状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolState {
    /// 未安装
    NotInstalled,
    /// 已安装
    Installed,
    /// 检查中
    Checking,
    /// 更新中
    Updating,
    /// 有可用更新
    UpdateAvailable,
    /// 安装中
    Installing,
    /// 错误
    Error,
}

impl ToolState {
    fn label(&self) -> &'static str {
        match self {
            ToolState::NotInstalled => "未安装",
            ToolState::Installed => "已安装",
            ToolState::Checking => "检查中...",
            ToolState::Updating => "更新中...",
            ToolState::UpdateAvailable => "有更新",
            ToolState::Installing => "安装中...",
            ToolState::Error => "错误",
        }
    }

    fn is_busy(&self) -> bool {
        matches!(
            self,
            ToolState::Checking | ToolState::Updating | ToolState::Installing
        )
    }
}

/// 工具面板事件
#[derive(Debug, Clone)]
pub enum ToolPanelEvent {
    /// 检查更新
    CheckUpdate(String),
    /// 安装工具
    Install(String),
    /// 更新工具
    Update(String),
    /// 重新安装
    Reinstall(String),
    /// 打开安装目录
    OpenFolder(String),
    /// 刷新所有
    RefreshAll,
}

/// 工具面板回调
pub type ToolPanelCallback = Arc<dyn Fn(ToolPanelEvent) + Send + Sync + 'static>;

/// 工具管理面板
pub struct ToolPanel {
    /// 工具列表
    tools: Vec<ToolInfo>,
    /// 事件回调
    _on_event: Option<ToolPanelCallback>,
    /// 是否正在刷新
    _is_refreshing: bool,
}

impl ToolPanel {
    /// 创建新的工具面板
    pub fn new() -> Self {
        Self {
            tools: vec![
                ToolInfo {
                    name: "yt-dlp".to_string(),
                    description: "视频下载引擎，支持 1000+ 网站".to_string(),
                    current_version: None,
                    latest_version: None,
                    status: ToolState::Checking,
                    install_path: None,
                    icon: "📥",
                },
                ToolInfo {
                    name: "FFmpeg".to_string(),
                    description: "音视频处理工具，用于格式转换和合并".to_string(),
                    current_version: None,
                    latest_version: None,
                    status: ToolState::Checking,
                    install_path: None,
                    icon: "🎬",
                },
            ],
            _on_event: None,
            _is_refreshing: false,
        }
    }

    /// 设置事件回调
    pub fn on_event(mut self, callback: ToolPanelCallback) -> Self {
        self._on_event = Some(callback);
        self
    }

    /// 更新工具信息
    pub fn update_tool(&mut self, name: &str, info: ToolInfo) {
        if let Some(tool) = self.tools.iter_mut().find(|t| t.name == name) {
            *tool = info;
        }
    }

    /// 设置工具状态
    pub fn set_tool_state(&mut self, name: &str, state: ToolState) {
        if let Some(tool) = self.tools.iter_mut().find(|t| t.name == name) {
            tool.status = state;
        }
    }

    /// 设置工具版本
    pub fn set_tool_version(
        &mut self,
        name: &str,
        current: Option<String>,
        latest: Option<String>,
    ) {
        if let Some(tool) = self.tools.iter_mut().find(|t| t.name == name) {
            tool.current_version = current;
            tool.latest_version = latest;

            // 自动更新状态
            if tool.current_version.is_none() {
                tool.status = ToolState::NotInstalled;
            } else if tool.latest_version.is_some() && tool.current_version != tool.latest_version {
                tool.status = ToolState::UpdateAvailable;
            } else {
                tool.status = ToolState::Installed;
            }
        }
    }
}

impl Default for ToolPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for ToolPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let border = theme.border;
        let background = theme.background;
        let foreground = theme.foreground;
        let muted_foreground = theme.muted_foreground;
        let secondary = theme.secondary;
        let success = theme.success;
        let warning = theme.warning;
        let danger = theme.danger;
        let primary = theme.primary;
        let accent = theme.accent;

        let tools_cloned = self.tools.clone();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(background)
            // 标题栏
            .child(
                div()
                    .h(px(48.0))
                    .px(px(16.0))
                    .border_b_1()
                    .border_color(border)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(foreground)
                            .child("🔧 工具管理"),
                    )
                    .child(
                        Button::new("refresh-all")
                            .ghost()
                            .small()
                            .icon(IconName::LoaderCircle)
                            .label("刷新"),
                    ),
            )
            // 工具列表
            .child(
                div()
                    .flex_1()
                    .p(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .children(tools_cloned.into_iter().map(move |tool| {
                        render_tool_card_static(
                            tool,
                            border,
                            secondary,
                            foreground,
                            muted_foreground,
                            success,
                            warning,
                            danger,
                            primary,
                            accent,
                        )
                    })),
            )
            // 底部说明
            .child(
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .border_t_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted_foreground)
                            .child("💡 MageKit 会自动管理这些工具，确保它们保持最新版本"),
                    ),
            )
    }
}

impl ToolPanel {
    /// 渲染工具卡片 - 空实现，使用静态函数替代
    fn _placeholder(&self) {}
}

/// 渲染工具卡片（静态函数）
fn render_tool_card_static(
    tool: ToolInfo,
    border: Hsla,
    secondary: Hsla,
    foreground: Hsla,
    muted_foreground: Hsla,
    success: Hsla,
    warning: Hsla,
    danger: Hsla,
    primary: Hsla,
    _accent: Hsla,
) -> impl IntoElement {
    let status = tool.status;
    let tool_name = tool.name.clone();
    let icon = tool.icon;
    let description = tool.description.clone();
    let current_version = tool.current_version.clone();
    let latest_version = tool.latest_version.clone();
    let install_path = tool.install_path.clone();

    div()
        .p(px(16.0))
        .border_1()
        .border_color(border)
        .rounded(px(12.0))
        .bg(secondary)
        .flex()
        .flex_col()
        .gap(px(12.0))
        // 工具头部
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
                        .child(div().text_2xl().child(icon))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_base()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(foreground)
                                        .child(tool_name.clone()),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(muted_foreground)
                                        .child(description),
                                ),
                        ),
                )
                .child(render_status_badge_static(
                    status, success, warning, danger, primary,
                )),
        )
        // 版本信息
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(16.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_sm()
                                .text_color(muted_foreground)
                                .child("当前版本:"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(foreground)
                                .child(
                                    current_version
                                        .clone()
                                        .unwrap_or_else(|| "未安装".to_string()),
                                ),
                        ),
                )
                .when(
                    latest_version.is_some() && latest_version != current_version,
                    |this| {
                        this.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(muted_foreground)
                                        .child("最新版本:"),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(success)
                                        .child(latest_version.clone().unwrap_or_default()),
                                ),
                        )
                    },
                ),
        )
        // 安装路径
        .when_some(install_path.clone(), |this, path| {
            this.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted_foreground)
                            .child("安装位置:"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(foreground)
                            .overflow_hidden()
                            .child(path),
                    ),
            )
        })
        // 操作按钮
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(render_action_buttons_static(&tool_name, status)),
        )
}

/// 渲染状态标签（静态函数）
fn render_status_badge_static(
    status: ToolState,
    success: Hsla,
    warning: Hsla,
    danger: Hsla,
    primary: Hsla,
) -> impl IntoElement {
    let (bg_color, text_color, label) = match status {
        ToolState::NotInstalled => (danger.opacity(0.1), danger, "未安装"),
        ToolState::Installed => (success.opacity(0.1), success, "已安装"),
        ToolState::Checking => (primary.opacity(0.1), primary, "检查中..."),
        ToolState::Updating => (primary.opacity(0.1), primary, "更新中..."),
        ToolState::UpdateAvailable => (warning.opacity(0.1), warning, "有更新"),
        ToolState::Installing => (primary.opacity(0.1), primary, "安装中..."),
        ToolState::Error => (danger.opacity(0.1), danger, "错误"),
    };

    div()
        .px(px(10.0))
        .py(px(4.0))
        .rounded(px(6.0))
        .bg(bg_color)
        .flex()
        .items_center()
        .gap(px(6.0))
        .when(status.is_busy(), |this| {
            this.child(
                Icon::new(IconName::LoaderCircle)
                    .size(px(14.0))
                    .text_color(text_color),
            )
        })
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(text_color)
                .child(label),
        )
}

/// 渲染操作按钮（静态函数）
fn render_action_buttons_static(tool_name: &str, status: ToolState) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(match status {
            ToolState::NotInstalled => {
                Button::new(SharedString::from(format!("install-{}", tool_name)))
                    .primary()
                    .small()
                    .label("安装")
                    .into_any_element()
            }
            ToolState::Installed => Button::new(SharedString::from(format!("check-{}", tool_name)))
                .ghost()
                .small()
                .label("检查更新")
                .into_any_element(),
            ToolState::UpdateAvailable => {
                Button::new(SharedString::from(format!("update-{}", tool_name)))
                    .primary()
                    .small()
                    .label("更新")
                    .into_any_element()
            }
            ToolState::Checking | ToolState::Updating | ToolState::Installing => {
                Button::new(SharedString::from(format!("wait-{}", tool_name)))
                    .ghost()
                    .small()
                    .label("请稍候...")
                    .disabled(true)
                    .into_any_element()
            }
            ToolState::Error => Button::new(SharedString::from(format!("retry-{}", tool_name)))
                .ghost()
                .small()
                .label("重试")
                .into_any_element(),
        })
        .when(
            status == ToolState::Installed || status == ToolState::UpdateAvailable,
            |this| {
                this.child(
                    Button::new(SharedString::from(format!("folder-{}", tool_name)))
                        .ghost()
                        .small()
                        .icon(IconName::Folder),
                )
            },
        )
}
