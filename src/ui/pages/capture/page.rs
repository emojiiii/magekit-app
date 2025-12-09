//! M3U8 嗅探页面

use crate::app::{AppState, CaptureEvent, CaptureRequest, M3u8Stream};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputState};
use gpui_component::{ActiveTheme, Disableable, Sizable, StyledExt};
use gpui_router::NavLink;
use magekit_shared::DownloadOptions;
use magekit_shared::utils::sanitize_filename;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// 嗅探页面
pub struct CapturePage {
    app_state: Arc<AppState>,
    url_input: Entity<InputState>,
    browser_input: Entity<InputState>,
    headless: bool,
    is_running: bool,
    captured: Vec<M3u8Stream>,
    logs: Vec<String>,
    cancel_tx: Option<tokio::sync::oneshot::Sender<()>>,
    output_dir: String,
    download_status: HashMap<String, String>,
}

impl CapturePage {
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入要嗅探的网页 URL，支持频道/播放页")
                .clean_on_escape()
        });

        let browser_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("可选：自定义浏览器可执行路径 (Chrome/Edge)")
                .clean_on_escape()
        });

        let output_dir = app_state
            .config()
            .download
            .default_output_path
            .to_string_lossy()
            .to_string();

        Self {
            app_state,
            url_input,
            browser_input,
            headless: true,
            is_running: false,
            captured: Vec::new(),
            logs: Vec::new(),
            cancel_tx: None,
            output_dir,
            download_status: HashMap::new(),
        }
    }

    fn item_title_from_list(&self, url: &str) -> Option<String> {
        self.captured
            .iter()
            .find(|it| it.url == url)
            .and_then(|it| it.title.clone())
    }

    fn start_capture(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_running {
            self.append_log("已在运行中，先停止再重新开始".into());
            cx.notify();
            return;
        }

        let url = self.url_input.read(cx).value().trim().to_string();
        if url.is_empty() {
            self.append_log("请输入要嗅探的 URL".into());
            cx.notify();
            return;
        }

        let browser_path = self.browser_input.read(cx).value().trim().to_string();
        let browser = if browser_path.is_empty() {
            None
        } else {
            Some(PathBuf::from(browser_path))
        };

        self.captured.clear();
        self.logs.clear();
        self.is_running = true;
        cx.notify();

        let request = CaptureRequest {
            target_url: url.clone(),
            custom_browser_path: browser.clone(),
            headless: self.headless,
            timeout: Duration::from_secs(45),
        };

        let app_state = self.app_state.clone();
        cx.spawn(async move |this, cx| {
            // 在阻塞上下文中调用 tokio 任务
            let result = smol::unblock(move || {
                let runtime = app_state.runtime.clone();
                runtime.block_on(async { app_state.start_m3u8_capture(request).await })
            })
            .await;

            match result {
                Ok(session) => {
                    let (mut rx, cancel_tx) = session.split();
                    let _ = this.update(cx, |this, cx| {
                        this.cancel_tx = cancel_tx;
                        this.append_log(format!("🔍 开始嗅探 {}", url));
                        this.is_running = true;
                        cx.notify();
                    });

                    while let Some(evt) = rx.recv().await {
                        let _ = this.update(cx, |this, cx| {
                            this.handle_event(evt);
                            cx.notify();
                        });
                    }
                    let _ = this.update(cx, |this, cx| {
                        this.is_running = false;
                        cx.notify();
                    });
                }
                Err(err) => {
                    let _ = this.update(cx, |this, cx| {
                        this.is_running = false;
                        this.append_log(format!("❌ 启动失败: {}", err));
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    fn stop_capture(&mut self) {
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
        self.is_running = false;
        self.append_log("⏹️ 已请求停止".into());
    }

    fn handle_event(&mut self, event: CaptureEvent) {
        match event {
            CaptureEvent::Log(msg) => self.append_log(msg),
            CaptureEvent::Found(item) => {
                if !self.captured.contains(&item) {
                    self.append_log(format!("✅ 捕获到 m3u8: {}", item.url));
                    self.captured.push(item);
                }
            }
            CaptureEvent::Finished => {
                self.is_running = false;
                self.append_log("🎉 嗅探结束".into());
            }
            CaptureEvent::Error(err) => {
                self.append_log(format!("❌ 嗅探错误: {}", err));
                self.is_running = false;
            }
        }
    }

    fn append_log(&mut self, msg: String) {
        self.logs.push(msg);
        const MAX_LOGS: usize = 300;
        if self.logs.len() > MAX_LOGS {
            let overflow = self.logs.len() - MAX_LOGS;
            self.logs.drain(0..overflow);
        }
    }

    fn download_link(&mut self, url: String, cx: &mut Context<Self>) {
        // 标记提交中，避免重复点击
        self.download_status
            .insert(url.clone(), "提交中...".to_string());
        cx.notify();

        let app_state = self.app_state.clone();
        let title_hint = self.item_title_from_list(&url);

        cx.spawn(async move |this, cx| {
            let url_clone = url.clone();
            let result = smol::unblock(move || {
                let runtime = app_state.runtime.clone();
                let mut options = DownloadOptions::default();
                let cfg = app_state.config();
                options.output_path = cfg.download.default_output_path.clone();
                options.format_id = cfg.download.default_format.clone();
                options.embed_metadata = cfg.download.embed_metadata;
                options.embed_thumbnail = cfg.download.embed_thumbnail;
                options.extract_audio = cfg.download.auto_extract_audio;
                if let Some(t) = title_hint.clone() {
                    options.output_template = Some(format!("{}.%(ext)s", sanitize_filename(&t)));
                }
                options.task_title = title_hint.clone();
                // 使用 ffmpeg 拉流
                options.ffmpeg_url = Some(url_clone.clone());
                runtime.block_on(async { app_state.start_download(&url_clone, options).await })
            })
            .await;

            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(_) => {
                        this.download_status
                            .insert(url.clone(), "已加入任务".to_string());
                    }
                    Err(err) => {
                        this.download_status
                            .insert(url.clone(), format!("失败: {}", err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for CapturePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 先构造捕获列表，完成对 cx 的可变借用
        let capture_items: Vec<AnyElement> = if self.captured.is_empty() {
            vec![
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_1()
                    .text_color(cx.theme().muted_foreground)
                    .child("尚未捕获到 m3u8 链接")
                    .into_any_element(),
            ]
        } else {
            let output_dir = self.output_dir.clone();
            self.captured
                .iter()
                .enumerate()
                .map(|(idx, item)| {
                    let status = self.download_status.get(&item.url).cloned();
                    render_capture_row(idx, item, status, cx, output_dir.clone()).into_any_element()
                })
                .collect()
        };

        // 完成列表构造后，再读取主题
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .size_full()
            .gap(px(16.0))
            .p(px(20.0))
            .child(
                // 标题
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
                                    .text_color(theme.foreground)
                                    .child("M3U8 嗅探"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child("启动浏览器抓包，实时展示捕获到的 m3u8 链接"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(
                                Button::new("start-capture")
                                    .primary()
                                    .disabled(self.is_running)
                                    .label("开始抓取")
                                    .on_click(cx.listener(|this, _event, window, cx| {
                                        this.start_capture(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("stop-capture")
                                    .ghost()
                                    .disabled(!self.is_running)
                                    .label("停止")
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.stop_capture();
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                // 配置区
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .p(px(16.0))
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(12.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .child("抓取设置"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.foreground)
                                    .child("目标 URL"),
                            )
                            .child(Input::new(&self.url_input)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.foreground)
                                    .child("自定义浏览器路径 (可选)"),
                            )
                            .child(Input::new(&self.browser_input)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                Checkbox::new("headless-toggle")
                                    .checked(self.headless)
                                    .label("使用 Headless 模式")
                                    .on_click(cx.listener(|this, checked, _window, cx| {
                                        this.headless = *checked;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child("无头模式可减少资源占用，某些站点需要关闭无头"),
                            ),
                    ),
            )
            .child(
                // 捕获结果
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .p(px(12.0))
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.foreground)
                                    .child("捕获的 m3u8"),
                            )
                            .child(
                                Button::new("refresh-capture")
                                    .ghost()
                                    .small()
                                    .label("清空")
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.captured.clear();
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .flex_1()
                            .scrollable(Axis::Vertical)
                            .children(capture_items),
                    ),
            )
    }
}

fn render_capture_row(
    idx: usize,
    item: &M3u8Stream,
    status: Option<String>,
    cx: &mut Context<CapturePage>,
    output_dir: String,
) -> impl IntoElement {
    let theme = cx.theme();
    let url = item.url.clone();
    let label = item.title.clone().unwrap_or_else(|| {
        url.replace('/', "/\u{200b}")
            .replace('?', "?\u{200b}")
            .replace('&', "&\u{200b}")
    });

    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .p(px(10.0))
        .bg(theme.background)
        .border_1()
        .border_color(theme.border)
        .rounded(px(8.0))
        .child(div().text_sm().text_color(theme.foreground).child(label))
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .items_center()
                .child(
                    NavLink::new().to("/tasks").child(
                        Button::new(("download", idx))
                            .primary()
                            .small()
                            .label("加入下载")
                            .disabled(matches!(status.as_deref(), Some("提交中...")))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.download_link(url.clone(), cx);
                            })),
                    ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("输出目录: {}", output_dir)),
                )
                .when_some(status, |row, s| {
                    row.child(div().text_xs().text_color(theme.foreground).child(s))
                }),
        )
}
