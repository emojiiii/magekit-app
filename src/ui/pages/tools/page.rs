//! 工具管理页面主组件

use crate::app::{AppState, ToolStatus};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_kit::component::Disableable;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::separator::Separator;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable, StyledExt};
use magekit_shared::{ToolType, UpdateChannel};
use magekit_tool_manager::UpdateInfo;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::widgets::{DownloadProgress, ToolHintCard, ToolInfo, ToolInstallState};

/// 工具管理页面
pub struct ToolsPage {
    app_state: Arc<AppState>,
    runtime_controls: Option<Entity<super::streamlink_page::RuntimeControls>>,
    tools: Vec<ToolInfo>,
    is_checking: bool,
    is_checking_updates: bool,
    update_info: Option<UpdateInfo>,
    error_message: Option<String>,
    initial_check_done: bool,
}

impl ToolsPage {
    pub fn new(app_state: Arc<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 不在构造函数中检测工具状态，延迟到第一次渲染时
        let tools = vec![ToolInfo::yt_dlp(), ToolInfo::ffmpeg()];

        let page = Self {
            app_state,
            runtime_controls: None,
            tools,
            is_checking: false,
            is_checking_updates: false,
            update_info: None,
            error_message: None,
            initial_check_done: false,
        };

        // 延迟检测工具状态
        cx.spawn(async move |this, cx| {
            // 等待一小段时间让 UI 先渲染
            smol::Timer::after(std::time::Duration::from_millis(100)).await;

            let _ = this.update(cx, |this, cx| {
                if !this.initial_check_done {
                    this.initial_check_done = true;
                    this.check_tools_status(cx);
                    this.check_tool_updates(cx);
                }
            });
        })
        .detach();

        page
    }

    pub(super) fn set_runtime_controls(
        &mut self,
        controls: Entity<super::streamlink_page::RuntimeControls>,
        cx: &mut Context<Self>,
    ) {
        self.runtime_controls = Some(controls);
        cx.notify();
    }

    fn check_tools_status(&mut self, cx: &mut Context<Self>) {
        if self.is_checking {
            return;
        }
        self.is_checking = true;
        self.error_message = None;

        tracing::debug!("🔍 刷新工具状态（后台线程）...");

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

            tracing::debug!("🔍 yt-dlp 状态: {:?}", yt_dlp_status);
            tracing::debug!("🔍 ffmpeg 状态: {:?}", ffmpeg_status);

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
                tracing::debug!("🔍 工具状态刷新完成");
                cx.notify();
            });
        })
        .detach();
    }

    fn check_tool_updates(&mut self, cx: &mut Context<Self>) {
        if self.is_checking_updates {
            return;
        }
        self.is_checking_updates = true;

        let app_state = self.app_state.clone();

        cx.spawn(async move |this, cx| {
            let result =
                smol::unblock(move || app_state.check_for_tool_updates_sync(UpdateChannel::Stable))
                    .await;

            let _ = this.update(cx, |this, cx| {
                this.is_checking_updates = false;
                match result {
                    Ok(info) => {
                        this.update_info = Some(info);
                    }
                    Err(e) => {
                        this.error_message =
                            Some(crate::i18n::format("检查更新失败: {}", &[format!("{}", e)]));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn install_tool(&mut self, tool_type: ToolType, cx: &mut Context<Self>) {
        // ffmpeg 当前没有可靠的下载进度（macOS/Linux 走系统包管理器），直接展示“安装中”。
        if tool_type == ToolType::Ffmpeg {
            for tool in &mut self.tools {
                if tool.tool_type == tool_type {
                    tool.state = ToolInstallState::Installing;
                    break;
                }
            }
            cx.notify();

            let app_state = self.app_state.clone();
            let handle = app_state.install_tool_in_background(tool_type);

            cx.spawn(async move |this, cx| {
                loop {
                    if handle.is_finished() {
                        break;
                    }
                    smol::Timer::after(std::time::Duration::from_millis(100)).await;
                }

                let (result, status): (anyhow::Result<()>, ToolStatus) = smol::unblock(move || {
                    let result = handle.join().unwrap_or_else(|_| {
                        Err(anyhow::anyhow!(crate::i18n::format("安装线程崩溃", &[])))
                    });
                    let status = app_state.check_tool_status_sync(tool_type);
                    (result, status)
                })
                .await;

                let refresh_updates = result.is_ok();
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
                                    this.error_message = Some(crate::i18n::format(
                                        "安装 {} 失败: {}",
                                        &[format!("{}", tool.name), format!("{}", e)],
                                    ));
                                    tracing::error!("❌ 安装失败: {}: {}", tool.name, e);
                                }
                            }
                            break;
                        }
                    }

                    // 安装成功后刷新一次更新信息
                    if refresh_updates {
                        this.check_tool_updates(cx);
                    }
                    cx.notify();
                });
            })
            .detach();

            return;
        }

        // yt-dlp：展示下载进度
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

                // 使用 smol 的 Timer 来异步等待
                smol::Timer::after(std::time::Duration::from_millis(100)).await;
            }

            // 在后台线程中获取安装结果和检测状态，避免阻塞主线程
            let (result, status): (anyhow::Result<()>, ToolStatus) = smol::unblock(move || {
                let result = handle.join().unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(crate::i18n::format("安装线程崩溃", &[])))
                });
                let status = app_state.check_tool_status_sync(tool_type);
                (result, status)
            })
            .await;

            let refresh_updates = result.is_ok();
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
                                this.error_message = Some(crate::i18n::format(
                                    "安装 {} 失败: {}",
                                    &[format!("{}", tool.name), format!("{}", e)],
                                ));
                                tracing::error!("❌ 安装失败: {}: {}", tool.name, e);
                            }
                        }
                        break;
                    }
                }
                // 安装成功后刷新一次更新信息
                if refresh_updates {
                    this.check_tool_updates(cx);
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
                self.error_message = Some(crate::i18n::format("删除失败: {}", &[format!("{}", e)]));
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
                smol::Timer::after(std::time::Duration::from_millis(100)).await;
            }

            // 获取安装结果
            let result = handle
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!(crate::i18n::format("安装线程崩溃", &[]))));
            let refresh_updates = result.is_ok();

            // 安装完成后重新检测所有工具状态
            let yt_dlp_status = app_state.check_tool_status_sync(ToolType::YtDlp);
            let ffmpeg_status = app_state.check_tool_status_sync(ToolType::Ffmpeg);

            // 更新 UI
            let _ = this.update(cx, |this, cx| {
                if let Err(ref e) = result {
                    this.error_message =
                        Some(crate::i18n::format("安装工具失败: {}", &[format!("{}", e)]));
                }

                for tool in &mut this.tools {
                    let status = match tool.tool_type {
                        ToolType::YtDlp => &yt_dlp_status,
                        ToolType::Ffmpeg => &ffmpeg_status,
                    };

                    tool.state = match status {
                        ToolStatus::NotInstalled => {
                            if this.error_message.is_some() {
                                ToolInstallState::Failed(crate::i18n::tr("安装失败").to_string())
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
                if refresh_updates {
                    this.check_tool_updates(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn refresh_status(&mut self, cx: &mut Context<Self>) {
        self.check_tools_status(cx);
        self.check_tool_updates(cx);
    }
}

impl Render for ToolsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                            .w_full()
                            .max_w(px(1200.0))
                            .mx_auto()
                            .p(px(32.0))
                            .gap(px(28.0))
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
                                    .when(window.bounds().size.width < px(1100.0), |el| {
                                        el.flex_col()
                                    })
                                    .gap(px(16.0))
                                    .children(self.render_tool_cards(cx)),
                            )
                            // YouTube 与直播录制运行环境
                            .when_some(self.runtime_controls.clone(), |el, controls| {
                                el.child(controls)
                            })
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
        let has_updates = self
            .update_info
            .as_ref()
            .map(|info| info.has_updates())
            .unwrap_or(false);

        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(30.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(title_color)
                            .child(crate::i18n::tr("工具管理")),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted_color)
                            .child(crate::i18n::tr("管理下载所需的依赖工具")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        div()
                            .relative()
                            .child(
                                Button::new("check-updates")
                                    .ghost()
                                    .label(if self.is_checking_updates {
                                        crate::i18n::tr("检查更新中...")
                                    } else {
                                        crate::i18n::tr("检查更新")
                                    })
                                    .disabled(self.is_checking_updates)
                                    .loading(self.is_checking_updates)
                                    .on_click(cx.listener(|this, _ev, _window, cx| {
                                        this.check_tool_updates(cx);
                                    })),
                            )
                            .when(has_updates, |el| {
                                el.child(
                                    div()
                                        .absolute()
                                        .top(px(-2.0))
                                        .right(px(-2.0))
                                        .w(px(8.0))
                                        .h(px(8.0))
                                        .bg(cx.theme().danger)
                                        .rounded_full(),
                                )
                            }),
                    )
                    .child(
                        Button::new("refresh-tools")
                            .ghost()
                            .icon(IconName::RefreshCw)
                            .label(crate::i18n::tr("刷新"))
                            .disabled(self.is_checking)
                            .loading(self.is_checking)
                            .on_click(cx.listener(|this, _ev, _window, cx| {
                                this.refresh_status(cx);
                            })),
                    )
                    .child(
                        Button::new("install-all")
                            .primary()
                            .label(crate::i18n::tr("一键安装全部"))
                            .disabled(!any_not_installed || any_installing || self.is_checking)
                            .on_click(cx.listener(|this, _ev, _window, cx| {
                                this.install_all_tools(cx);
                            })),
                    ),
            )
    }

    fn render_error_message(&self, msg: String) -> impl IntoElement {
        Alert::error("tool-operation-error", crate::i18n::text(&msg))
    }

    fn render_tool_cards(&mut self, cx: &mut Context<Self>) -> Vec<impl IntoElement> {
        let update_info = self.update_info.clone();
        self.tools
            .iter()
            .enumerate()
            .map(|(idx, tool)| {
                let tool_type = tool.tool_type;
                let is_busy = matches!(
                    tool.state,
                    ToolInstallState::Installing | ToolInstallState::Downloading(_)
                );
                let tool_update = update_info.as_ref().and_then(|info| match tool_type {
                    ToolType::YtDlp => info.yt_dlp_update.as_ref(),
                    ToolType::Ffmpeg => info.ffmpeg_update.as_ref(),
                });
                let has_update = tool_update.is_some();
                let can_delete = matches!(
                    tool.state,
                    ToolInstallState::Installed {
                        is_system: false,
                        ..
                    }
                );
                let (status, variant, detail) = match &tool.state {
                    ToolInstallState::Unknown => (
                        crate::i18n::tr("检查中..."),
                        TagVariant::Secondary,
                        String::new(),
                    ),
                    ToolInstallState::NotInstalled => (
                        crate::i18n::tr("未安装"),
                        TagVariant::Secondary,
                        String::new(),
                    ),
                    ToolInstallState::Installed { version, is_system } => {
                        let detail = version
                            .as_ref()
                            .map(|v| {
                                if *is_system {
                                    crate::i18n::format("v{} (系统)", &[v.clone()])
                                } else {
                                    format!("v{v}")
                                }
                            })
                            .unwrap_or_default();
                        if let Some(update) = tool_update {
                            (
                                crate::i18n::tr("可更新"),
                                TagVariant::Warning,
                                crate::i18n::format(
                                    "v{} → v{} 可更新{}",
                                    &[update.current.clone(), update.latest.clone(), String::new()],
                                ),
                            )
                        } else {
                            (crate::i18n::tr("已安装"), TagVariant::Success, detail)
                        }
                    }
                    ToolInstallState::Downloading(progress) => (
                        crate::i18n::tr("下载中..."),
                        TagVariant::Info,
                        crate::i18n::format(
                            "下载中 {:.1}% - {} / {} @ {}",
                            &[
                                format!("{:.1}", progress.percent()),
                                progress.downloaded_str(),
                                progress.total_str(),
                                progress.speed_str(),
                            ],
                        ),
                    ),
                    ToolInstallState::Installing => (
                        crate::i18n::tr("安装中..."),
                        TagVariant::Info,
                        String::new(),
                    ),
                    ToolInstallState::Failed(error) => (
                        crate::i18n::tr("安装失败"),
                        TagVariant::Danger,
                        crate::i18n::text(error).to_string(),
                    ),
                };
                let (btn_label, btn_disabled, action) = match &tool.state {
                    ToolInstallState::NotInstalled | ToolInstallState::Failed(_) => {
                        (crate::i18n::tr("安装"), false, "install")
                    }
                    ToolInstallState::Installed { .. } if has_update => {
                        (crate::i18n::tr("更新"), false, "install")
                    }
                    ToolInstallState::Installed { .. } if tool_type == ToolType::Ffmpeg => {
                        (crate::i18n::tr("重新检测"), false, "refresh")
                    }
                    ToolInstallState::Installed { .. } => {
                        (crate::i18n::tr("检查更新"), false, "check_updates")
                    }
                    ToolInstallState::Unknown => (crate::i18n::tr("检查中..."), true, "none"),
                    _ => (crate::i18n::tr("下载中..."), true, "none"),
                };
                GroupBox::new()
                    .id(("tool-card", idx))
                    .outline()
                    .flex_1()
                    .min_w(px(300.0))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .justify_between()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .size_11()
                                            .rounded(cx.theme().radius)
                                            .bg(cx.theme().secondary)
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                Icon::new(match tool_type {
                                                    ToolType::YtDlp => {
                                                        gpui_kit::assets::IconName::ArrowDown
                                                    }
                                                    ToolType::Ffmpeg => {
                                                        gpui_kit::assets::IconName::Film
                                                    }
                                                })
                                                .size_6(),
                                            ),
                                    )
                                    .child(div().text_lg().font_semibold().child(tool.name)),
                            )
                            .child(
                                Tag::new()
                                    .with_variant(variant)
                                    .outline()
                                    .small()
                                    .child(status),
                            ),
                    )
                    .child(
                        div()
                            .min_h(px(44.0))
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(crate::i18n::text(tool.description)),
                    )
                    .child(
                        div()
                            .min_h(px(20.0))
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(detail),
                    )
                    .when_some(
                        match &tool.state {
                            ToolInstallState::Downloading(p) => Some(p),
                            _ => None,
                        },
                        |card, progress| {
                            card.child(
                                Progress::new(("tool-progress", idx))
                                    .value(progress.percent())
                                    .loading(progress.total == 0)
                                    .accessibility_label(tool.name)
                                    .small(),
                            )
                        },
                    )
                    .child(Separator::horizontal())
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                Button::new(("install-tool", idx))
                                    .label(btn_label)
                                    .when(action == "install", |button| button.primary())
                                    .when(action != "install", |button| button.outline())
                                    .disabled(btn_disabled || is_busy)
                                    .loading(is_busy)
                                    .on_click(cx.listener(move |this, _, _, cx| match action {
                                        "install" => this.install_tool(tool_type, cx),
                                        "refresh" => this.refresh_status(cx),
                                        "check_updates" => this.check_tool_updates(cx),
                                        _ => {}
                                    })),
                            )
                            .when(can_delete, |row| {
                                row.child(
                                    Button::new(("delete-tool", idx))
                                        .ghost()
                                        .label(crate::i18n::tr("删除"))
                                        .icon(gpui_kit::assets::IconName::Trash)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.delete_tool(tool_type, window, cx)
                                        })),
                                )
                            }),
                    )
            })
            .collect()
    }
}
