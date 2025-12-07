//! 工具管理页面主组件

use crate::app::{AppState, ToolStatus};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::Disableable;
use gpui_component::button::{Button, ButtonVariants};
use magekit_shared::ToolType;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::widgets::{DownloadProgress, ToolHintCard, ToolInfo, ToolInstallState};

/// 工具管理页面
pub struct ToolsPage {
    app_state: Arc<AppState>,
    tools: Vec<ToolInfo>,
    is_checking: bool,
    error_message: Option<String>,
    initial_check_done: bool,
}

impl ToolsPage {
    pub fn new(app_state: Arc<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 不在构造函数中检测工具状态，延迟到第一次渲染时
        let tools = vec![ToolInfo::yt_dlp(), ToolInfo::ffmpeg()];

        let page = Self {
            app_state,
            tools,
            is_checking: false,
            error_message: None,
            initial_check_done: false,
        };

        // 延迟检测工具状态
        cx.spawn(async move |this, cx| {
            // 等待一小段时间让 UI 先渲染
            Timer::after(std::time::Duration::from_millis(100)).await;

            let _ = this.update(cx, |this, cx| {
                this.check_tools_status(cx);
            });
        })
        .detach();

        page
    }

    fn check_tools_status(&mut self, cx: &mut Context<Self>) {
        self.is_checking = true;
        self.error_message = None;

        tracing::info!("🔍 刷新工具状态（后台线程）...");

        let app_state = self.app_state.clone();

        // 在后台线程中执行工具检测，避免阻塞主线程
        cx.spawn(async move |this, cx| {
            // 使用 smol::unblock 在后台线程中执行同步检测
            let (yt_dlp_status, ffmpeg_status) = smol::unblock(move || {
                let yt_dlp = app_state.check_tool_status_sync(ToolType::YtDlp);
                let ffmpeg = app_state.check_tool_status_sync(ToolType::Ffmpeg);
                (yt_dlp, ffmpeg)
            })
            .await;

            tracing::info!("🔍 yt-dlp 状态: {:?}", yt_dlp_status);
            tracing::info!("🔍 ffmpeg 状态: {:?}", ffmpeg_status);

            // 更新 UI
            let _ = this.update(cx, |this, cx| {
                for tool in &mut this.tools {
                    let status = match tool.tool_type {
                        ToolType::YtDlp => &yt_dlp_status,
                        ToolType::Ffmpeg => &ffmpeg_status,
                    };

                    tool.state = match status {
                        ToolStatus::NotInstalled => ToolInstallState::NotInstalled,
                        ToolStatus::Installed { version, is_system } => {
                            ToolInstallState::Installed {
                                version: version.clone(),
                                is_system: *is_system,
                            }
                        }
                    };
                }

                this.is_checking = false;
                tracing::info!("🔍 工具状态刷新完成");
                cx.notify();
            });
        })
        .detach();
    }

    fn install_tool(&mut self, tool_type: ToolType, cx: &mut Context<Self>) {
        // 设置初始下载状态
        for tool in &mut self.tools {
            if tool.tool_type == tool_type {
                tool.state = ToolInstallState::Downloading(DownloadProgress {
                    downloaded: 0,
                    total: 0,
                    speed: 0,
                });
                break;
            }
        }
        cx.notify();

        let app_state = self.app_state.clone();

        // 创建共享的进度变量
        let progress_downloaded = Arc::new(AtomicU64::new(0));
        let progress_total = Arc::new(AtomicU64::new(0));
        let progress_speed = Arc::new(AtomicU64::new(0));

        // 克隆用于回调
        let pd = progress_downloaded.clone();
        let pt = progress_total.clone();
        let ps = progress_speed.clone();

        // 进度回调
        let progress_callback: Arc<dyn Fn(u64, u64, u64) + Send + Sync> =
            Arc::new(move |downloaded, total, speed| {
                pd.store(downloaded, Ordering::Relaxed);
                pt.store(total, Ordering::Relaxed);
                ps.store(speed, Ordering::Relaxed);
            });

        // 在后台线程中运行安装
        let handle = app_state.install_tool_with_progress(tool_type, progress_callback);

        // 使用 cx.spawn 来轮询检查安装结果和更新进度
        cx.spawn(async move |this, cx| {
            // 轮询等待后台线程完成，每 100ms 检查一次
            loop {
                if handle.is_finished() {
                    break;
                }

                // 更新进度 UI
                let downloaded = progress_downloaded.load(Ordering::Relaxed);
                let total = progress_total.load(Ordering::Relaxed);
                let speed = progress_speed.load(Ordering::Relaxed);

                let _ = this.update(cx, |this, cx| {
                    for tool in &mut this.tools {
                        if tool.tool_type == tool_type {
                            // 如果 speed == u64::MAX，表示下载完成，进入安装阶段
                            if speed == u64::MAX {
                                tool.state = ToolInstallState::Installing;
                            } else {
                                tool.state = ToolInstallState::Downloading(DownloadProgress {
                                    downloaded,
                                    total,
                                    speed,
                                });
                            }
                            break;
                        }
                    }
                    cx.notify();
                });

                // 使用 GPUI 的 Timer 来异步等待
                Timer::after(std::time::Duration::from_millis(100)).await;
            }

            // 在后台线程中获取安装结果和检测状态，避免阻塞主线程
            let (result, status): (anyhow::Result<()>, ToolStatus) = smol::unblock(move || {
                let result = handle
                    .join()
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("安装线程崩溃")));
                let status = app_state.check_tool_status_sync(tool_type);
                (result, status)
            })
            .await;

            // 更新 UI
            let _ = this.update(cx, |this, cx| {
                for tool in &mut this.tools {
                    if tool.tool_type == tool_type {
                        match &result {
                            Ok(_) => {
                                tool.state = match &status {
                                    ToolStatus::NotInstalled => ToolInstallState::NotInstalled,
                                    ToolStatus::Installed { version, is_system } => {
                                        ToolInstallState::Installed {
                                            version: version.clone(),
                                            is_system: *is_system,
                                        }
                                    }
                                };
                                tracing::info!("✅ 安装成功: {}", tool.name);
                            }
                            Err(e) => {
                                tool.state = ToolInstallState::Failed(e.to_string());
                                this.error_message =
                                    Some(format!("安装 {} 失败: {}", tool.name, e));
                                tracing::error!("❌ 安装失败: {}: {}", tool.name, e);
                            }
                        }
                        break;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn delete_tool(&mut self, tool_type: ToolType, _window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!("🗑️ 删除工具: {:?}", tool_type);

        // 获取工具名称用于日志
        let tool_name = self
            .tools
            .iter()
            .find(|t| t.tool_type == tool_type)
            .map(|t| t.name)
            .unwrap_or("unknown");

        match self.app_state.delete_tool_sync(tool_type) {
            Ok(_) => {
                // 更新工具状态
                for tool in &mut self.tools {
                    if tool.tool_type == tool_type {
                        tool.state = ToolInstallState::NotInstalled;
                        break;
                    }
                }
                tracing::info!("✅ 工具 {} 删除成功", tool_name);
            }
            Err(e) => {
                self.error_message = Some(format!("删除失败: {}", e));
                tracing::error!("❌ 工具 {} 删除失败: {}", tool_name, e);
            }
        }
        cx.notify();
    }

    fn install_all_tools(&mut self, cx: &mut Context<Self>) {
        // 设置所有未安装工具为安装中状态
        for tool in &mut self.tools {
            if matches!(
                tool.state,
                ToolInstallState::NotInstalled | ToolInstallState::Failed(_)
            ) {
                tool.state = ToolInstallState::Installing;
            }
        }
        cx.notify();

        let app_state = self.app_state.clone();

        // 在后台线程中运行安装，使用 JoinHandle 来避免阻塞
        let handle = app_state.install_all_tools_in_background();

        // 使用 cx.spawn 来轮询检查安装结果
        cx.spawn(async move |this, cx| {
            // 轮询等待后台线程完成，每 100ms 检查一次
            loop {
                if handle.is_finished() {
                    break;
                }
                Timer::after(std::time::Duration::from_millis(100)).await;
            }

            // 获取安装结果
            let result = handle
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("安装线程崩溃")));

            // 安装完成后重新检测所有工具状态
            let yt_dlp_status = app_state.check_tool_status_sync(ToolType::YtDlp);
            let ffmpeg_status = app_state.check_tool_status_sync(ToolType::Ffmpeg);

            // 更新 UI
            let _ = this.update(cx, |this, cx| {
                if let Err(ref e) = result {
                    this.error_message = Some(format!("安装工具失败: {}", e));
                }

                for tool in &mut this.tools {
                    let status = match tool.tool_type {
                        ToolType::YtDlp => &yt_dlp_status,
                        ToolType::Ffmpeg => &ffmpeg_status,
                    };

                    tool.state = match status {
                        ToolStatus::NotInstalled => {
                            if this.error_message.is_some() {
                                ToolInstallState::Failed("安装失败".to_string())
                            } else {
                                ToolInstallState::NotInstalled
                            }
                        }
                        ToolStatus::Installed { version, is_system } => {
                            ToolInstallState::Installed {
                                version: version.clone(),
                                is_system: *is_system,
                            }
                        }
                    };
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn refresh_status(&mut self, cx: &mut Context<Self>) {
        self.check_tools_status(cx);
    }
}

impl Render for ToolsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let any_not_installed = self.tools.iter().any(|t| {
            matches!(
                t.state,
                ToolInstallState::NotInstalled | ToolInstallState::Failed(_)
            )
        });
        let any_installing = self.tools.iter().any(|t| {
            matches!(
                t.state,
                ToolInstallState::Installing | ToolInstallState::Downloading(_)
            )
        });

        let bg_color = cx.theme().background;

        div()
            .id("tools-page")
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(bg_color)
            .child(
                // 可滚动内容区域
                div()
                    .id("tools-scroll-container")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .p(px(24.0))
                            .gap(px(24.0))
                            // 页面标题
                            .child(self.render_header(cx, any_not_installed, any_installing))
                            // 错误消息
                            .when_some(self.error_message.clone(), |el, msg| {
                                el.child(self.render_error_message(msg))
                            })
                            // 工具卡片
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(12.0))
                                    .children(self.render_tool_cards(cx)),
                            )
                            // 关于工具
                            .child(ToolHintCard),
                    ),
            )
    }
}

impl ToolsPage {
    fn render_header(
        &mut self,
        cx: &mut Context<Self>,
        any_not_installed: bool,
        any_installing: bool,
    ) -> impl IntoElement {
        let title_color = cx.theme().foreground;
        let muted_color = cx.theme().muted_foreground;

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
                            .text_color(title_color)
                            .child("工具管理"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted_color)
                            .child("管理下载所需的依赖工具"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        Button::new("refresh-tools")
                            .ghost()
                            .label("刷新状态")
                            .disabled(self.is_checking)
                            .on_click(cx.listener(|this, _ev, _window, cx| {
                                this.refresh_status(cx);
                            })),
                    )
                    .child(
                        Button::new("install-all")
                            .primary()
                            .label("一键安装全部")
                            .disabled(!any_not_installed || any_installing || self.is_checking)
                            .on_click(cx.listener(|this, _ev, _window, cx| {
                                this.install_all_tools(cx);
                            })),
                    ),
            )
    }

    fn render_error_message(&self, msg: String) -> impl IntoElement {
        div()
            .p(px(12.0))
            .rounded(px(8.0))
            .bg(Hsla::from(rgb(0x450a0a)).opacity(0.5))
            .border_1()
            .border_color(rgb(0xef4444))
            .child(div().text_sm().text_color(rgb(0xef4444)).child(msg))
    }

    fn render_tool_cards(&mut self, cx: &mut Context<Self>) -> Vec<impl IntoElement> {
        // 获取主题颜色
        let card_bg = cx.theme().secondary;
        let border_color = cx.theme().border;
        let icon_bg = cx.theme().muted;
        let title_color = cx.theme().foreground;
        let muted_color = cx.theme().muted_foreground;
        let success_color = cx.theme().success;
        let danger_color = cx.theme().danger;
        let primary_color = cx.theme().primary;

        self.tools
            .iter()
            .enumerate()
            .map(|(idx, tool)| {
                let tool_type = tool.tool_type;
                let is_busy = matches!(
                    tool.state,
                    ToolInstallState::Installing | ToolInstallState::Downloading(_)
                );
                let tool_name: SharedString = tool.name.into();
                let tool_desc: SharedString = tool.description.into();
                let tool_icon = tool.icon;

                // 状态文本和颜色
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
                        let text: SharedString = format!(
                            "下载中 {:.1}% - {} / {} @ {}",
                            progress.percent(),
                            progress.downloaded_str(),
                            progress.total_str(),
                            progress.speed_str()
                        )
                        .into();
                        (text, primary_color)
                    }
                    ToolInstallState::Installing => ("安装中...".into(), primary_color),
                    ToolInstallState::Failed(_) => ("安装失败".into(), danger_color),
                };

                let status_bg: Hsla = status_color.opacity(0.15);

                // 是否可删除（只有非系统安装的可以删除）
                let can_delete = matches!(
                    &tool.state,
                    ToolInstallState::Installed {
                        is_system: false,
                        ..
                    }
                );

                // 进度信息
                let progress_percent = match &tool.state {
                    ToolInstallState::Downloading(progress) => Some(progress.percent()),
                    _ => None,
                };

                let btn_id: SharedString = format!("install-{}", idx).into();
                let delete_btn_id: SharedString = format!("delete-{}", idx).into();

                div()
                    .id(SharedString::from(format!("tool-card-{}", idx)))
                    .p(px(20.0))
                    .rounded(px(12.0))
                    .bg(card_bg)
                    .border_1()
                    .border_color(border_color)
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
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
                                                div()
                                                    .text_sm()
                                                    .text_color(muted_color)
                                                    .child(tool_desc),
                                            ),
                                    ),
                            )
                            .child(
                                // 右侧：操作按钮
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    // 删除按钮 (仅非系统安装的工具显示)
                                    .when(can_delete, |el| {
                                        el.child(
                                            Button::new(delete_btn_id)
                                                .label("删除")
                                                .danger()
                                                .on_click(cx.listener(
                                                    move |this, _ev, window, cx| {
                                                        tracing::info!(
                                                            "🗑️ 点击删除按钮: {:?}",
                                                            tool_type
                                                        );
                                                        this.delete_tool(tool_type, window, cx);
                                                    },
                                                )),
                                        )
                                    })
                                    // 安装/更新按钮
                                    .child({
                                        let (btn_label, btn_disabled) = match &tool.state {
                                            ToolInstallState::NotInstalled
                                            | ToolInstallState::Failed(_) => ("安装", false),
                                            ToolInstallState::Installed { .. } => {
                                                ("检查更新", false)
                                            }
                                            ToolInstallState::Installing
                                            | ToolInstallState::Downloading(_) => {
                                                ("下载中...", true)
                                            }
                                            ToolInstallState::Unknown => ("...", true),
                                        };
                                        Button::new(btn_id)
                                            .label(btn_label)
                                            .primary()
                                            .disabled(btn_disabled || is_busy)
                                            .on_click(cx.listener(move |this, _ev, _window, cx| {
                                                tracing::info!("🔧 点击安装按钮: {:?}", tool_type);
                                                this.install_tool(tool_type, cx);
                                            }))
                                    }),
                            ),
                    )
                    // 进度条 (下载时显示)
                    .when_some(progress_percent, |el, percent| {
                        el.child(
                            div()
                                .w_full()
                                .h(px(4.0))
                                .bg(icon_bg)
                                .rounded(px(2.0))
                                .overflow_hidden()
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(percent / 100.0))
                                        .bg(primary_color)
                                        .rounded(px(2.0)),
                                ),
                        )
                    })
            })
            .collect()
    }
}
