//! Add managed Streamlink controls without coupling Python runtime state to yt-dlp/FFmpeg.
use crate::app::AppState;
use gpui::*;
use gpui_component::{ActiveTheme, Disableable};
use gpui_component::button::{Button, ButtonVariants};
use live_recorder::streamlink_runtime;
use std::sync::Arc;

pub struct ToolsPage {
    original: Entity<super::page::ToolsPage>,
    app_state: Arc<AppState>,
    status: String,
    busy: bool,
    installed: bool,
}

impl ToolsPage {
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let original = cx.new(|cx| super::page::ToolsPage::new(app_state.clone(), window, cx));
        let mut page = Self { original, app_state, status: "正在读取 Streamlink 状态…".into(), busy: false, installed: false };
        page.runtime_action("status", cx);
        page
    }

    fn runtime_action(&mut self, action: &'static str, cx: &mut Context<Self>) {
        if self.busy { return; }
        self.busy = true;
        self.status = match action {
            "install" => "正在安装独立 Python/Streamlink 环境；首次安装需要网络…",
            "update" => "正在新环境中更新并验证 Streamlink；已有录制不受影响…",
            _ => "正在读取 Streamlink 状态…",
        }.into();
        cx.notify();
        let runtime = self.app_state.runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime.spawn(async move {
                match action {
                    "install" => streamlink_runtime::ensure().await.map(Some),
                    "update" => streamlink_runtime::update().await.map(Some),
                    _ => streamlink_runtime::status().await,
                }
            }).await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(Ok(Some(info))) => {
                        this.installed = true;
                        this.status = format!("Streamlink {} · {} · 独立 Python 3.12", info.streamlink_version, info.uv_version);
                    }
                    Ok(Ok(None)) => {
                        this.installed = false;
                        this.status = "尚未安装；首次使用直播功能会自动安装，也可在此提前安装。".into();
                    }
                    Ok(Err(error)) => this.status = format!("操作失败：{error}"),
                    Err(_) => this.status = "运行环境管理任务异常结束，请重试。".into(),
                }
                cx.notify();
            });
        }).detach();
    }
}

impl Render for ToolsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let controls = div().flex().flex_col().gap_2().p_4().m_4().rounded_lg()
            .border_1().border_color(cx.theme().border)
            .child(div().font_semibold().child("Streamlink 直播引擎"))
            .child(div().text_sm().child(self.status.clone()))
            .child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child("仅使用应用自己的 Python 环境；更新后新任务使用新版本，正在录制的任务继续使用旧版本。"))
            .child(div().flex().gap_2()
                .child(Button::new("streamlink-install").label("安装").primary()
                    .disabled(self.busy || self.installed)
                    .on_click(cx.listener(|this, _, _, cx| this.runtime_action("install", cx))))
                .child(Button::new("streamlink-update").label("更新 / 修复")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.runtime_action("update", cx))))
                .child(Button::new("streamlink-status").label("刷新状态")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.runtime_action("status", cx)))));
        div().flex().flex_col().size_full().child(controls)
            .child(div().flex_1().min_h_0().child(self.original.clone()))
    }
}
