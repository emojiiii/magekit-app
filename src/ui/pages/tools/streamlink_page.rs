//! Add managed Streamlink controls without coupling Python runtime state to yt-dlp/FFmpeg.
use crate::app::AppState;
use crate::i18n::Message;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants};
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Sizable, StyledExt};
use live_recorder::streamlink_runtime;
use magekit_tool_manager::deno_runtime;
use std::sync::Arc;

pub struct ToolsPage {
    original: Entity<super::page::ToolsPage>,
}

pub struct RuntimeControls {
    app_state: Arc<AppState>,
    status: Message,
    busy: bool,
    installed: bool,
    deno_status: Message,
    deno_busy: bool,
    deno_installed: bool,
    deno_error: bool,
    runtime_error: bool,
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
            status: Message::plain("正在读取 Streamlink 状态…").into(),
            busy: false,
            installed: false,
            deno_status: Message::plain("正在检查 Deno JavaScript runtime…").into(),
            deno_busy: false,
            deno_installed: false,
            deno_error: false,
            runtime_error: false,
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
        self.deno_error = false;
        self.deno_status = match action {
            "install" => Message::plain("正在下载并校验 Deno 官方 runtime…").into(),
            "update" => Message::plain("正在更新 / 修复 Deno runtime…").into(),
            _ => Message::plain("正在检查 Deno 状态…").into(),
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
                        this.deno_status = Message::new(
                            if info.managed {
                                "Deno {} · MageKit 管理"
                            } else {
                                "Deno {} · 系统 PATH"
                            },
                            &[info.version.to_string()],
                        );
                    }
                    Ok(Ok(Some(info))) => {
                        this.deno_installed = false;
                        this.deno_status = Message::new("检测到 Deno {}，版本过旧；YouTube EJS 需要 Deno 2.3 或更新版本。", &[format!("{}", info.version)]);
                    }
                    Ok(Ok(None)) => {
                        this.deno_installed = false;
                        this.deno_status =
                            Message::plain("未找到兼容的 Deno；YouTube 解析/下载可能缺少格式，首次解析会自动安装。")
                                .into();
                    }
                    Ok(Err(error)) => {
                        this.deno_error = true;
                        this.deno_status = Message::new("Deno 操作失败：{error}", &[format!("{}", error)]);
                    }
                    Err(_) => {
                        this.deno_error = true;
                        this.deno_status = Message::plain("Deno runtime 管理任务异常结束，请重试。").into();
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
        self.runtime_error = false;
        self.status = match action {
            "install" => Message::plain("正在安装独立 Python/Streamlink 环境；首次安装需要网络…"),
            "update" => Message::plain("正在新环境中更新并验证 Streamlink；已有录制不受影响…"),
            _ => Message::plain("正在读取 Streamlink 状态…"),
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
                        this.status = Message::new(
                            "Streamlink {} · {} · 独立 Python 3.12",
                            &[
                                format!("{}", info.streamlink_version),
                                format!("{}", info.uv_version),
                            ],
                        );
                    }
                    Ok(Ok(None)) => {
                        this.installed = false;
                        this.status = Message::plain(
                            "尚未安装；首次使用直播功能会自动安装，也可在此提前安装。",
                        )
                        .into();
                    }
                    Ok(Err(error)) => {
                        this.runtime_error = true;
                        this.status = Message::new("操作失败：{error}", &[format!("{}", error)])
                    }
                    Err(_) => {
                        this.runtime_error = true;
                        this.status = Message::plain("运行环境管理任务异常结束，请重试。").into()
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for RuntimeControls {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let deno_controls = GroupBox::new().id("deno-card").fill().flex_1().min_w_0()
            .child(div().flex().items_center().justify_between().gap_3()
                .child(div().flex().items_center().gap_2().font_semibold()
                    .child(Icon::new(IconName::SquareTerminal).size_5()).child("Deno"))
                .child(Tag::new().outline().small().with_variant(if self.deno_installed { TagVariant::Success } else { TagVariant::Secondary })
                    .child(crate::i18n::tr(if self.deno_installed { "已安装" } else { "未安装" }))))
            .child(div().text_sm().text_color(cx.theme().muted_foreground)
                .child(crate::i18n::tr("YouTube JavaScript runtime · Deno")))
            .child(if self.deno_error {
                Alert::error("deno-error", self.deno_status.render()).into_any_element()
            } else {
                div().text_sm().child(self.deno_status.render()).into_any_element()
            })
            .child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child(crate::i18n::tr("yt-dlp 已包含 EJS 脚本；Deno 负责执行 YouTube JS challenge。首次解析时会自动检查。")))
            .child(div().flex().flex_wrap().gap_2()
                .when(!self.deno_installed, |row| row.child(Button::new("deno-install").label(crate::i18n::tr("安装"))
                    .primary().disabled(self.deno_busy).loading(self.deno_busy)
                    .on_click(cx.listener(|this, _, _, cx| this.deno_action("install", cx)))))
                .child(Button::new("deno-update").label(crate::i18n::tr("更新 / 修复")).outline().disabled(self.deno_busy)
                    .on_click(cx.listener(|this, _, _, cx| this.deno_action("update", cx))))
                .child(Button::new("deno-status").icon(IconName::RefreshCw).label(crate::i18n::tr("刷新状态"))
                    .ghost().disabled(self.deno_busy).on_click(cx.listener(|this, _, _, cx| this.deno_action("status", cx)))));
        let streamlink_controls =
            GroupBox::new()
                .id("streamlink-card")
                .fill()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .font_semibold()
                                .child(Icon::new(gpui_kit::assets::IconName::Video).size_5())
                                .child("Streamlink"),
                        )
                        .child(
                            Tag::new()
                                .outline()
                                .small()
                                .with_variant(if self.installed {
                                    TagVariant::Success
                                } else {
                                    TagVariant::Secondary
                                })
                                .child(crate::i18n::tr(if self.installed {
                                    "已安装"
                                } else {
                                    "未安装"
                                })),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(crate::i18n::tr("Streamlink 直播引擎")),
                )
                .child(if self.runtime_error {
                    Alert::error("streamlink-error", self.status.render()).into_any_element()
                } else {
                    div()
                        .text_sm()
                        .child(self.status.render())
                        .into_any_element()
                })
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(crate::i18n::tr(
                            "使用应用独立 Python 环境；更新只影响新录制任务。",
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .when(!self.installed, |row| {
                            row.child(
                                Button::new("streamlink-install")
                                    .label(crate::i18n::tr("安装"))
                                    .primary()
                                    .disabled(self.busy)
                                    .loading(self.busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.runtime_action("install", cx)
                                    })),
                            )
                        })
                        .child(
                            Button::new("streamlink-update")
                                .label(crate::i18n::tr("更新 / 修复"))
                                .outline()
                                .disabled(self.busy)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.runtime_action("update", cx)),
                                ),
                        )
                        .child(
                            Button::new("streamlink-status")
                                .icon(IconName::RefreshCw)
                                .label(crate::i18n::tr("刷新状态"))
                                .ghost()
                                .disabled(self.busy)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.runtime_action("status", cx)),
                                ),
                        ),
                );
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .text_base()
                    .font_semibold()
                    .child(crate::i18n::tr("YouTube 与直播运行环境")),
            )
            .child(
                div()
                    .flex()
                    .items_stretch()
                    .gap_4()
                    .when(window.bounds().size.width < px(1100.0), |row| {
                        row.flex_col()
                    })
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
