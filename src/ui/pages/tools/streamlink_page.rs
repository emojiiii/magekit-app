//! Add managed Streamlink controls without coupling Python runtime state to yt-dlp/FFmpeg.
use crate::app::AppState;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{ActiveTheme, Disableable, StyledExt};
use live_recorder::streamlink_runtime;
use magekit_tool_manager::deno_runtime;
use std::sync::Arc;

pub struct ToolsPage {
    original: Entity<super::page::ToolsPage>,
}

pub struct RuntimeControls {
    app_state: Arc<AppState>,
    status: String,
    busy: bool,
    installed: bool,
    deno_status: String,
    deno_busy: bool,
    deno_installed: bool,
}

impl ToolsPage {
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let original = cx.new(|cx| super::page::ToolsPage::new(app_state.clone(), window, cx));
        let controls = cx.new(|cx| RuntimeControls::new(app_state, cx));
        original.update(cx, |page, cx| page.set_runtime_controls(controls, cx));
        Self { original }
    }
}

impl RuntimeControls {
    fn new(app_state: Arc<AppState>, cx: &mut Context<Self>) -> Self {
        let mut controls = Self {
            app_state,
            status: "正在读取 Streamlink 状态…".into(),
            busy: false,
            installed: false,
            deno_status: "正在检查 Deno JavaScript runtime…".into(),
            deno_busy: false,
            deno_installed: false,
        };
        controls.runtime_action("status", cx);
        controls.deno_action("status", cx);
        controls
    }

    fn deno_action(&mut self, action: &'static str, cx: &mut Context<Self>) {
        if self.deno_busy {
            return;
        }
        self.deno_busy = true;
        self.deno_status = match action {
            "install" => "正在下载并校验 Deno 官方 runtime…".into(),
            "update" => "正在更新 / 修复 Deno runtime…".into(),
            _ => "正在检查 Deno 状态…".into(),
        };
        cx.notify();

        let runtime = self.app_state.runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    match action {
                        "install" => deno_runtime::ensure().await.map(Some),
                        "update" => deno_runtime::update().await.map(Some),
                        _ => deno_runtime::status().await,
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.deno_busy = false;
                match result {
                    Ok(Ok(Some(info))) if info.supported => {
                        this.deno_installed = true;
                        this.deno_status = format!(
                            "Deno {} · {}",
                            info.version,
                            if info.managed {
                                "MageKit 管理"
                            } else {
                                "系统 PATH"
                            }
                        );
                    }
                    Ok(Ok(Some(info))) => {
                        this.deno_installed = false;
                        this.deno_status = format!(
                            "检测到 Deno {}，版本过旧；YouTube EJS 需要 Deno 2.3 或更新版本。",
                            info.version
                        );
                    }
                    Ok(Ok(None)) => {
                        this.deno_installed = false;
                        this.deno_status =
                            "未找到兼容的 Deno；YouTube 解析/下载可能缺少格式，首次解析会自动安装。"
                                .into();
                    }
                    Ok(Err(error)) => {
                        this.deno_installed = false;
                        this.deno_status = format!("Deno 状态读取失败：{error}");
                    }
                    Err(_) => {
                        this.deno_installed = false;
                        this.deno_status = "Deno runtime 管理任务异常结束，请重试。".into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn runtime_action(&mut self, action: &'static str, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status = match action {
            "install" => "正在安装独立 Python/Streamlink 环境；首次安装需要网络…",
            "update" => "正在新环境中更新并验证 Streamlink；已有录制不受影响…",
            _ => "正在读取 Streamlink 状态…",
        }
        .into();
        cx.notify();
        let runtime = self.app_state.runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    match action {
                        "install" => streamlink_runtime::ensure().await.map(Some),
                        "update" => streamlink_runtime::update().await.map(Some),
                        _ => streamlink_runtime::status().await,
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(Ok(Some(info))) => {
                        this.installed = true;
                        this.status = format!(
                            "Streamlink {} · {} · 独立 Python 3.12",
                            info.streamlink_version, info.uv_version
                        );
                    }
                    Ok(Ok(None)) => {
                        this.installed = false;
                        this.status =
                            "尚未安装；首次使用直播功能会自动安装，也可在此提前安装。".into();
                    }
                    Ok(Err(error)) => this.status = format!("操作失败：{error}"),
                    Err(_) => this.status = "运行环境管理任务异常结束，请重试。".into(),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for RuntimeControls {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let deno_controls = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(16.0))
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .child(div().font_semibold().child("YouTube JavaScript runtime · Deno"))
            .child(div().text_sm().child(self.deno_status.clone()))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("yt-dlp 已包含 EJS 脚本；Deno 负责执行 YouTube JS challenge。首次解析时会自动检查。"),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("deno-install")
                            .label("安装")
                            .primary()
                            .disabled(self.deno_busy || self.deno_installed)
                            .on_click(cx.listener(|this, _, _, cx| this.deno_action("install", cx))),
                    )
                    .child(
                        Button::new("deno-update")
                            .label("更新 / 修复")
                            .disabled(self.deno_busy)
                            .on_click(cx.listener(|this, _, _, cx| this.deno_action("update", cx))),
                    )
                    .child(
                        Button::new("deno-status")
                            .label("刷新状态")
                            .disabled(self.deno_busy)
                            .on_click(cx.listener(|this, _, _, cx| this.deno_action("status", cx))),
                    ),
            );
        let streamlink_controls = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(16.0))
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .child(div().font_semibold().child("Streamlink 直播引擎"))
            .child(div().text_sm().child(self.status.clone()))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("使用应用独立 Python 环境；更新只影响新录制任务。"),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("streamlink-install")
                            .label("安装")
                            .primary()
                            .disabled(self.busy || self.installed)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.runtime_action("install", cx)),
                            ),
                    )
                    .child(
                        Button::new("streamlink-update")
                            .label("更新 / 修复")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.runtime_action("update", cx)),
                            ),
                    )
                    .child(
                        Button::new("streamlink-status")
                            .label("刷新状态")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.runtime_action("status", cx)),
                            ),
                    ),
            );

        div()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_lg()
                    .font_semibold()
                    .child("YouTube 与直播运行环境"),
            )
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.0))
                    .child(deno_controls)
                    .child(streamlink_controls),
            )
    }
}

impl Render for ToolsPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.original.clone())
    }
}
