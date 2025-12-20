//! 应用布局组件
//!
//! 提供带有侧边栏导航的应用主布局

use crate::app::GlobalAppState;
use crate::ui::pages::{HomePage, RecordingPage, SettingsPage, ToolsPage};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::TitleBar;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::scroll::ScrollableElement;
use gpui_component::*;
use gpui_router::{IntoLayout, NavLink, Outlet, use_location};

// ============================================================================
// 页面包装器组件
// 这些组件使用 RenderOnce 在路由渲染时创建 Entity
// ============================================================================

/// 首页包装器 - 创建 HomePage Entity
#[derive(IntoElement)]
pub struct HomePageWrapper;

impl RenderOnce for HomePageWrapper {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let app_state = cx.global::<GlobalAppState>().0.clone();
        cx.new(|cx| HomePage::new(app_state, window, cx))
    }
}

/// 工具页包装器 - 创建 ToolsPage Entity
#[derive(IntoElement)]
pub struct ToolsPageWrapper;

impl RenderOnce for ToolsPageWrapper {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let app_state = cx.global::<GlobalAppState>().0.clone();
        cx.new(|cx| ToolsPage::new(app_state, window, cx))
    }
}

/// 设置页包装器 - 创建 SettingsPage Entity
#[derive(IntoElement)]
pub struct SettingsPageWrapper;

impl RenderOnce for SettingsPageWrapper {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let app_state = cx.global::<GlobalAppState>().0.clone();
        cx.new(|cx| SettingsPage::new(app_state, window, cx))
    }
}

/// 创建首页包装器
pub fn stateful_home_page() -> impl IntoElement {
    HomePageWrapper
}

/// 创建工具页包装器
pub fn stateful_tools_page() -> impl IntoElement {
    ToolsPageWrapper
}

/// 创建设置页包装器
pub fn stateful_settings_page() -> impl IntoElement {
    SettingsPageWrapper
}

/// 录制页包装器 - 创建 RecordingPage Entity
#[derive(IntoElement)]
pub struct RecordingPageWrapper;

impl RenderOnce for RecordingPageWrapper {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let app_state = cx.global::<GlobalAppState>().0.clone();
        cx.new(|cx| RecordingPage::new(app_state, window, cx))
    }
}

/// 创建录制页包装器
pub fn stateful_recording_page() -> impl IntoElement {
    RecordingPageWrapper
}

/// 应用布局结构体 - 必须实现 IntoLayout trait
#[derive(IntoElement, IntoLayout)]
pub struct AppLayout {
    /// Outlet 用于渲染子路由内容
    outlet: Outlet,
}

impl AppLayout {
    pub fn new() -> Self {
        Self {
            outlet: Outlet::new(),
        }
    }
}

impl RenderOnce for AppLayout {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // 从主题获取颜色
        let title_color = cx.theme().foreground;
        let content_bg = cx.theme().background;
        let sidebar_bg = cx.theme().sidebar;
        let sidebar_text = cx.theme().sidebar_foreground;
        let sidebar_hover = cx.theme().sidebar_accent;
        let sidebar_hover_text = cx.theme().sidebar_accent_foreground;
        let border_color = cx.theme().border;

        // 获取当前路由用于高亮激活状态
        let location = use_location(cx);
        let current_path = location.pathname.clone();

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                // 自定义 TitleBar
                TitleBar::new().child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size_full()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(div().text_xl().child("🎬"))
                                .child(
                                    div()
                                        .text_lg()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(title_color)
                                        .child("MageKit"),
                                ),
                        ),
                ),
            )
            .child(
                // 主内容区域
                div()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    .child(
                        // 自定义侧边栏
                        div()
                            .w(px(200.0))
                            .flex()
                            .flex_col()
                            .bg(sidebar_bg)
                            .border_r_1()
                            .border_color(border_color)
                            .child(
                                // 顶部 Header
                                div().p(px(16.0)).child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(sidebar_text)
                                        .child("导航"),
                                ),
                            )
                            .child(
                                // 导航列表
                                div()
                                    .flex()
                                    .flex_1()
                                    .flex_col()
                                    .p(px(8.0))
                                    .gap(px(4.0))
                                    .child(create_nav_item(
                                        "/",
                                        "🏠",
                                        "首页",
                                        &current_path,
                                        sidebar_text,
                                        sidebar_hover,
                                        sidebar_hover_text,
                                    ))
                                    .child(create_nav_item(
                                        "/record",
                                        "🎥",
                                        "录制",
                                        &current_path,
                                        sidebar_text,
                                        sidebar_hover,
                                        sidebar_hover_text,
                                    ))
                                    .child(create_nav_item(
                                        "/capture",
                                        "🌐",
                                        "嗅探",
                                        &current_path,
                                        sidebar_text,
                                        sidebar_hover,
                                        sidebar_hover_text,
                                    ))
                                    .child(create_nav_item(
                                        "/channel",
                                        "👥",
                                        "频道",
                                        &current_path,
                                        sidebar_text,
                                        sidebar_hover,
                                        sidebar_hover_text,
                                    ))
                                    .child(create_nav_item(
                                        "/tasks",
                                        "📋",
                                        "任务",
                                        &current_path,
                                        sidebar_text,
                                        sidebar_hover,
                                        sidebar_hover_text,
                                    ))
                                    .child(create_nav_item(
                                        "/tools",
                                        "🔧",
                                        "工具",
                                        &current_path,
                                        sidebar_text,
                                        sidebar_hover,
                                        sidebar_hover_text,
                                    )),
                            )
                            .child(
                                // Footer - 设置和主题
                                div()
                                    .flex()
                                    .flex_col()
                                    .p(px(12.0))
                                    .gap(px(8.0))
                                    .border_t_1()
                                    .border_color(border_color)
                                    .child(create_nav_item(
                                        "/settings",
                                        "⚙️",
                                        "设置",
                                        &current_path,
                                        sidebar_text,
                                        sidebar_hover,
                                        sidebar_hover_text,
                                    ))
                                    .child(
                                        Button::new("theme-toggle")
                                            .ghost()
                                            .label("💡 切换主题")
                                            .on_click(|_, _, _| {
                                                // TODO: 实现主题切换逻辑
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        // 内容区域 - Outlet 用于渲染子路由
                        div()
                            .id("main-content-area")
                            .flex_1()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .bg(content_bg)
                            .child(self.outlet),
                    ),
            )
    }
}

// 创建导航项辅助函数
fn create_nav_item(
    path: &'static str,
    icon: &'static str,
    label: &'static str,
    current_path: &str,
    text_color: Hsla,
    hover_bg: Hsla,
    hover_text: Hsla,
) -> impl IntoElement {
    let is_active = current_path == path;

    NavLink::new().to(path).child(
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(6.0))
            .when(is_active, |this| this.bg(hover_bg).text_color(hover_text))
            .when(!is_active, |this| this.text_color(text_color))
            .hover(move |this| this.bg(hover_bg).text_color(hover_text))
            .child(div().text_base().child(icon))
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label)),
    )
}

/// 首页 - 下载面板 + 快速操作
pub fn home_page() -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .p(px(24.0))
        .gap(px(24.0))
        .child(
            // 页面标题
            div().flex().items_center().justify_between().child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0xfafafa))
                            .child("视频下载"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xa1a1aa))
                            .child("粘贴视频链接，一键下载"),
                    ),
            ),
        )
        .child(
            // 演示模式提示
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .p(px(12.0))
                .bg(rgb(0x422006))
                .border_1()
                .border_color(rgb(0x854d0e))
                .rounded(px(8.0))
                .child(div().text_sm().child("⚠️"))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xfef08a))
                        .child("演示模式：输入框和按钮暂不可用，完整功能需要集成后端服务"),
                ),
        )
        .child(
            // URL 输入卡片
            div()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .p(px(20.0))
                .bg(rgb(0x18181b))
                .border_1()
                .border_color(rgb(0x3f3f46))
                .rounded(px(12.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0xfafafa))
                        .child("视频链接"),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(12.0))
                        .child(
                            // 模拟输入框（静态展示）
                            div()
                                .flex_1()
                                .h(px(40.0))
                                .px(px(12.0))
                                .flex()
                                .items_center()
                                .bg(rgb(0x09090b))
                                .border_1()
                                .border_color(rgb(0x3f3f46))
                                .rounded(px(8.0))
                                .text_sm()
                                .text_color(rgb(0x71717a))
                                .child("粘贴 YouTube、Bilibili 等视频链接..."),
                        )
                        .child(Button::new("paste-btn").label("粘贴"))
                        .child(Button::new("download-btn").primary().label("下载")),
                ),
        )
        .child(
            // 快速设置
            div()
                .flex()
                .gap(px(16.0))
                .child(
                    // 质量选择
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .p(px(16.0))
                        .bg(rgb(0x18181b))
                        .border_1()
                        .border_color(rgb(0x3f3f46))
                        .rounded(px(12.0))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(0xfafafa))
                                .child("🎬 视频质量"),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(8.0))
                                .child(render_quality_option("最佳", true))
                                .child(render_quality_option("1080p", false))
                                .child(render_quality_option("720p", false))
                                .child(render_quality_option("480p", false)),
                        ),
                )
                .child(
                    // 输出设置
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .p(px(16.0))
                        .bg(rgb(0x18181b))
                        .border_1()
                        .border_color(rgb(0x3f3f46))
                        .rounded(px(12.0))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(0xfafafa))
                                .child("📁 保存位置"),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .h(px(36.0))
                                        .px(px(12.0))
                                        .flex()
                                        .items_center()
                                        .bg(rgb(0x09090b))
                                        .border_1()
                                        .border_color(rgb(0x3f3f46))
                                        .rounded(px(6.0))
                                        .text_sm()
                                        .text_color(rgb(0xa1a1aa))
                                        .overflow_hidden()
                                        .child("~/Downloads"),
                                )
                                .child(Button::new("browse").small().label("浏览")),
                        ),
                ),
        )
        .child(
            // 下载选项
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .p(px(16.0))
                .bg(rgb(0x18181b))
                .border_1()
                .border_color(rgb(0x3f3f46))
                .rounded(px(12.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0xfafafa))
                        .child("⚙️ 下载选项"),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(24.0))
                        .child(render_checkbox_option("嵌入元数据", true))
                        .child(render_checkbox_option("嵌入缩略图", false))
                        .child(render_checkbox_option("下载字幕", false))
                        .child(render_checkbox_option("仅提取音频", false)),
                ),
        )
}

/// 渲染质量选项按钮
fn render_quality_option(label: &str, selected: bool) -> impl IntoElement {
    let bg_color = if selected {
        rgb(0x3b82f6)
    } else {
        rgb(0x27272a)
    };
    let text_color = if selected {
        rgb(0xfafafa)
    } else {
        rgb(0xa1a1aa)
    };

    div()
        .px(px(12.0))
        .py(px(6.0))
        .bg(bg_color)
        .rounded(px(6.0))
        .text_sm()
        .text_color(text_color)
        .cursor_pointer()
        .hover(|this| this.opacity(0.8))
        .child(label.to_string())
}

/// 渲染复选框选项
fn render_checkbox_option(label: &str, checked: bool) -> impl IntoElement {
    let check_bg = if checked {
        rgb(0x3b82f6)
    } else {
        rgb(0x27272a)
    };
    let check_border = if checked {
        rgb(0x3b82f6)
    } else {
        rgb(0x52525b)
    };

    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .cursor_pointer()
        .child(
            div()
                .w(px(16.0))
                .h(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(check_bg)
                .border_1()
                .border_color(check_border)
                .rounded(px(4.0))
                .when(checked, |this| {
                    this.child(div().text_xs().text_color(rgb(0xfafafa)).child("✓"))
                }),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xa1a1aa))
                .child(label.to_string()),
        )
}

/// 任务页面
pub fn tasks_page() -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .p(px(24.0))
        .gap(px(24.0))
        .child(
            // 页面标题
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_2xl()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0xfafafa))
                                .child("下载任务"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0xa1a1aa))
                                .child("管理所有下载任务"),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(Button::new("refresh").ghost().small().label("刷新"))
                        .child(Button::new("clear").ghost().small().label("清除已完成")),
                ),
        )
        .child(
            // 任务统计
            div()
                .flex()
                .gap(px(16.0))
                .child(render_stat_card("进行中", "0", "🔄", rgb(0x3b82f6).into()))
                .child(render_stat_card("已完成", "0", "✅", rgb(0x22c55e).into()))
                .child(render_stat_card("失败", "0", "❌", rgb(0xef4444).into()))
                .child(render_stat_card("等待中", "0", "⏳", rgb(0xeab308).into())),
        )
        .child(
            // 空状态
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(16.0))
                .child(div().text_3xl().child("📭"))
                .child(
                    div()
                        .text_lg()
                        .text_color(rgb(0xa1a1aa))
                        .child("暂无下载任务"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x71717a))
                        .child("在下载页面添加视频链接开始下载"),
                )
                .child(
                    NavLink::new()
                        .to("/")
                        .child(Button::new("go-download").primary().label("去下载")),
                ),
        )
}

/// 渲染统计卡片
fn render_stat_card(
    label: &'static str,
    value: &'static str,
    icon: &'static str,
    color: Hsla,
) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(16.0))
        .bg(rgb(0x18181b))
        .border_1()
        .border_color(rgb(0x3f3f46))
        .rounded(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(div().text_lg().child(icon))
                .child(div().text_sm().text_color(rgb(0xa1a1aa)).child(label)),
        )
        .child(
            div()
                .text_2xl()
                .font_weight(FontWeight::BOLD)
                .text_color(color)
                .child(value),
        )
}

/// 工具页面
pub fn tools_page() -> impl IntoElement {
    // 工具状态 - 标记为未安装，因为这是静态页面无法检测实际状态
    // 实际状态检测需要在有状态的组件中实现
    let yt_dlp_installed = false;
    let ffmpeg_installed = false;

    div().flex().flex_col().size_full().child(
        div()
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scrollbar()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p(px(24.0))
                    .gap(px(24.0))
                    .child(
                        // 页面标题
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .text_2xl()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0xfafafa))
                                            .child("工具管理"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(0xa1a1aa))
                                            .child("管理 yt-dlp 和 ffmpeg 工具"),
                                    ),
                            )
                            .child(Button::new("check-all").label("检查更新")),
                    )
                    .child(
                        // 提示信息
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .p(px(12.0))
                            .bg(rgb(0x27272a))
                            .rounded(px(8.0))
                            .child(div().text_sm().child("💡"))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0xa1a1aa))
                                    .child("此页面为静态展示，实际工具状态检测需要连接后端服务"),
                            ),
                    )
                    .child(
                        // 工具卡片
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(16.0))
                            .child(render_tool_card(
                                "yt-dlp",
                                "视频下载核心工具，支持数千个网站",
                                "📥",
                                None,
                                yt_dlp_installed,
                            ))
                            .child(render_tool_card(
                                "ffmpeg",
                                "音视频处理工具，用于格式转换和合并",
                                "🎞️",
                                None,
                                ffmpeg_installed,
                            )),
                    ),
            ),
    )
}

/// 渲染工具卡片
fn render_tool_card(
    name: &'static str,
    description: &'static str,
    icon: &'static str,
    version: Option<&'static str>,
    installed: bool,
) -> impl IntoElement {
    let status_color: Hsla = if installed {
        rgb(0x22c55e).into()
    } else {
        rgb(0xef4444).into()
    };
    let status_bg: Hsla = status_color.opacity(0.2);
    let status_text = if installed { "已安装" } else { "未安装" };

    // 为每个工具创建唯一的按钮ID
    let update_id: SharedString = format!("update-{}", name).into();
    let reinstall_id: SharedString = format!("reinstall-{}", name).into();
    let install_id: SharedString = format!("install-{}", name).into();

    div()
        .flex()
        .items_center()
        .justify_between()
        .p(px(20.0))
        .bg(rgb(0x18181b))
        .border_1()
        .border_color(rgb(0x3f3f46))
        .rounded(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(16.0))
                .child(
                    div()
                        .w(px(48.0))
                        .h(px(48.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(0x27272a))
                        .rounded(px(12.0))
                        .text_2xl()
                        .child(icon),
                )
                .child(
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
                                        .text_color(rgb(0xfafafa))
                                        .child(name.to_string()),
                                )
                                .child(
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
                            div()
                                .text_sm()
                                .text_color(rgb(0xa1a1aa))
                                .child(description.to_string()),
                        )
                        .when_some(version, |this, v| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x71717a))
                                    .child(format!("版本: {}", v)),
                            )
                        }),
                ),
        )
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .when(installed, |this| {
                    this.child(Button::new(update_id).ghost().small().label("更新"))
                        .child(Button::new(reinstall_id).ghost().small().label("重装"))
                })
                .when(!installed, |this| {
                    this.child(Button::new(install_id).primary().small().label("安装"))
                }),
        )
}

/// 设置页面
pub fn settings_page() -> impl IntoElement {
    div().flex().flex_col().size_full().child(
        div()
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scrollbar()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p(px(24.0))
                    .gap(px(24.0))
                    .child(
                        // 页面标题
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0xfafafa))
                                    .child("设置"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0xa1a1aa))
                                    .child("自定义应用程序设置"),
                            ),
                    )
                    .child(
                        // 演示模式提示
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .p(px(12.0))
                            .bg(rgb(0x422006))
                            .border_1()
                            .border_color(rgb(0x854d0e))
                            .rounded(px(8.0))
                            .child(div().text_sm().child("⚠️"))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0xfef08a))
                                    .child("演示模式：设置项仅供展示，修改功能需要集成配置服务"),
                            ),
                    )
                    .child(
                        // 下载设置
                        render_settings_section(
                            "下载设置",
                            "download",
                            "⬇️",
                            vec![
                                ("默认保存路径", "~/Downloads"),
                                ("最大并发数", "3"),
                                ("默认视频质量", "最佳质量"),
                            ],
                        ),
                    )
                    .child(
                        // 工具设置
                        render_settings_section(
                            "工具设置",
                            "tool",
                            "🔧",
                            vec![("自动检查更新", "开启"), ("更新通道", "稳定版")],
                        ),
                    )
                    .child(
                        // 界面设置
                        render_settings_section(
                            "界面设置",
                            "ui",
                            "🎨",
                            vec![("主题", "深色"), ("语言", "简体中文"), ("显示通知", "开启")],
                        ),
                    )
                    .child(
                        // 高级设置
                        render_settings_section(
                            "高级设置",
                            "advanced",
                            "⚙️",
                            vec![
                                ("日志级别", "Info"),
                                ("代理设置", "无"),
                                ("速度限制", "无限制"),
                            ],
                        ),
                    ),
            ),
    )
}

/// 渲染设置分组
fn render_settings_section(
    title: &'static str,
    section_id: &'static str,
    icon: &'static str,
    items: Vec<(&'static str, &'static str)>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .p(px(20.0))
        .bg(rgb(0x18181b))
        .border_1()
        .border_color(rgb(0x3f3f46))
        .rounded(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(div().text_lg().child(icon))
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0xfafafa))
                        .child(title),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .children(
                    items
                        .into_iter()
                        .enumerate()
                        .map(move |(idx, (label, value))| {
                            render_settings_item(section_id, idx, label, value)
                        }),
                ),
        )
}

/// 渲染设置项
fn render_settings_item(
    section_id: &'static str,
    idx: usize,
    label: &'static str,
    value: &'static str,
) -> impl IntoElement {
    let item_id: SharedString = format!("setting-{}-{}", section_id, idx).into();

    div()
        .id(item_id)
        .flex()
        .items_center()
        .justify_between()
        .py(px(8.0))
        .px(px(4.0))
        .rounded(px(4.0))
        .border_b_1()
        .border_color(rgb(0x27272a))
        .cursor_pointer()
        .hover(|this| this.bg(rgb(0x27272a)))
        .child(div().text_sm().text_color(rgb(0xa1a1aa)).child(label))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(div().text_sm().text_color(rgb(0xfafafa)).child(value))
                .child(div().text_xs().text_color(rgb(0x71717a)).child(">")),
        )
}

/// 404 页面
pub fn not_found_page() -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .items_center()
        .justify_center()
        .gap(px(16.0))
        .child(div().text_3xl().child("🔍"))
        .child(
            div()
                .text_2xl()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(0xfafafa))
                .child("页面未找到"),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xa1a1aa))
                .child("请检查URL是否正确"),
        )
        .child(
            NavLink::new()
                .to("/")
                .child(Button::new("go-home").primary().label("返回首页")),
        )
}
