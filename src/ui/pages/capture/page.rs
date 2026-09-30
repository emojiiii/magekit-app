//! M3U8 嗅探页面

use crate::app::{
    AppState, CaptureEvent, CaptureFilterType, CaptureRequest, CaptureResourceType, M3u8Stream,
};
use crate::ui::pages::capture::widgets::{CaptureRowTheme, capture_row};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::empty::{Empty, EmptyDescription, EmptyHeader, EmptyTitle};
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::scroll::{Scrollbar, ScrollbarAxis};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, VirtualListScrollHandle, v_virtual_list,
};
use magekit_shared::DownloadOptions;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

const DEFAULT_FFMPEG_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// 嗅探页面
pub struct CapturePage {
    app_state: Arc<AppState>,
    url_input: Entity<InputState>,
    browser_input: Entity<InputState>,
    headless: bool,
    filter_type: CaptureFilterType,
    is_running: bool,
    captured: Vec<M3u8Stream>,
    logs: Vec<String>,
    cancel_tx: Option<tokio::sync::oneshot::Sender<()>>,
    output_dir: String,
    download_status: HashMap<String, EnqueueStatus>,
    session_generation: u64,
    last_error: Option<String>,
    scroll_handle: VirtualListScrollHandle,
}

const CAPTURE_ITEM_HEIGHT: f32 = 110.0;

#[derive(Clone)]
enum EnqueueStatus {
    Submitting,
    Queued,
    Failed(String),
}

impl EnqueueStatus {
    fn label(&self) -> String {
        match self {
            Self::Submitting => crate::i18n::tr("提交中...").into(),
            Self::Queued => crate::i18n::tr("已加入任务").into(),
            Self::Failed(error) => crate::i18n::format("失败: {}", &[error.clone()]),
        }
    }
    fn blocks_submission(&self) -> bool {
        matches!(self, Self::Submitting | Self::Queued)
    }
}

fn valid_capture_url(value: &str) -> bool {
    AppState::validate_media_url(value).is_ok()
}

impl CapturePage {
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url_input = cx.new(|cx| {
            crate::i18n::input("输入要嗅探的网页 URL，支持频道/播放页", window, cx)
                .clean_on_escape()
        });

        cx.subscribe_in(&url_input, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.start_capture(window, cx);
            }
        })
        .detach();

        let browser_input = cx.new(|cx| {
            crate::i18n::input("可选：自定义浏览器可执行路径 (Chrome/Edge)", window, cx)
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
            filter_type: CaptureFilterType::Media, // 默认视频+音频
            is_running: false,
            captured: Vec::new(),
            logs: Vec::new(),
            cancel_tx: None,
            output_dir,
            download_status: HashMap::new(),
            session_generation: 0,
            last_error: None,
            scroll_handle: VirtualListScrollHandle::new(),
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
            self.append_log(crate::i18n::tr("已在运行中，先停止再重新开始").into());
            cx.notify();
            return;
        }

        let url = self.url_input.read(cx).value().trim().to_string();
        if !valid_capture_url(&url) {
            self.last_error = Some(crate::i18n::tr("请输入完整的 HTTP 或 HTTPS 网页地址").into());
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
        self.last_error = None;
        self.session_generation = self.session_generation.wrapping_add(1);
        let generation = self.session_generation;
        self.is_running = true;
        cx.notify();

        let request = CaptureRequest {
            target_url: url.clone(),
            custom_browser_path: browser.clone(),
            headless: self.headless,
            timeout: Duration::from_secs(45),
            filter_type: self.filter_type,
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
                    let accepted = this
                        .update(cx, |this, cx| {
                            if this.session_generation != generation || !this.is_running {
                                // 启动期间已经停止；不要复活旧会话。
                                if let Some(cancel) = cancel_tx {
                                    let _ = cancel.send(());
                                }
                                return false;
                            }
                            this.cancel_tx = cancel_tx;
                            this.append_log(crate::i18n::tr("正在捕获网页媒体资源").into());
                            cx.notify();
                            true
                        })
                        .unwrap_or(false);
                    if !accepted {
                        return;
                    }

                    while let Some(evt) = rx.recv().await {
                        let current = this
                            .update(cx, |this, cx| {
                                if this.session_generation != generation {
                                    return false;
                                }
                                this.handle_event(evt);
                                cx.notify();
                                true
                            })
                            .unwrap_or(false);
                        if !current {
                            break;
                        }
                    }
                    let _ = this.update(cx, |this, cx| {
                        if this.session_generation != generation {
                            return;
                        }
                        this.is_running = false;
                        this.cancel_tx = None;
                        cx.notify();
                    });
                }
                Err(err) => {
                    let _ = this.update(cx, |this, cx| {
                        if this.session_generation != generation {
                            return;
                        }
                        this.is_running = false;
                        this.last_error = Some(err.to_string());
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    fn stop_capture(&mut self) {
        self.session_generation = self.session_generation.wrapping_add(1);
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
        self.is_running = false;
        self.append_log(crate::i18n::tr("⏹️ 已请求停止").into());
    }

    fn handle_event(&mut self, event: CaptureEvent) {
        match event {
            CaptureEvent::Log(msg) => self.append_log(msg),
            CaptureEvent::Found(item) => {
                if !self.captured.contains(&item) {
                    // 添加调试日志
                    tracing::info!(
                        "🔍 捕获资源: {} | 类型: {:?} | MIME: {:?}",
                        magekit_shared::redact_url_for_log(&item.url),
                        item.resource_type,
                        item.mime_type
                    );

                    self.append_log(format!(
                        "✅ {:?}: {}",
                        item.resource_type,
                        magekit_shared::redact_url_for_log(&item.url)
                    ));
                    self.captured.push(item);
                }
            }
            CaptureEvent::Finished => {
                self.is_running = false;
                self.append_log("🎉 嗅探结束".into());
            }
            CaptureEvent::Error(err) => {
                self.last_error = Some(err.clone());
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

    /// 根据当前筛选器过滤资源
    fn filter_resources(&self) -> Vec<M3u8Stream> {
        self.captured
            .iter()
            .filter(|item| {
                // 首先只保留媒体类型（视频/音频/图片）
                if !matches!(
                    item.resource_type,
                    CaptureResourceType::Video
                        | CaptureResourceType::Audio
                        | CaptureResourceType::Image
                ) {
                    return false;
                }

                // 然后根据用户选择的筛选器过滤
                match self.filter_type {
                    CaptureFilterType::All => true,
                    CaptureFilterType::Video => item.resource_type == CaptureResourceType::Video,
                    CaptureFilterType::Audio => item.resource_type == CaptureResourceType::Audio,
                    CaptureFilterType::Image => item.resource_type == CaptureResourceType::Image,
                    CaptureFilterType::Media => {
                        matches!(
                            item.resource_type,
                            CaptureResourceType::Video | CaptureResourceType::Audio
                        )
                    }
                }
            })
            .cloned()
            .collect()
    }

    fn download_link(&mut self, url: String, cx: &mut Context<Self>) {
        // 状态检查位于处理入口，不能只依赖按钮的上一次渲染状态。
        if self
            .download_status
            .get(&url)
            .is_some_and(EnqueueStatus::blocks_submission)
        {
            return;
        }
        self.download_status
            .insert(url.clone(), EnqueueStatus::Submitting);
        cx.notify();

        let app_state = self.app_state.clone();
        let captured_item = self.captured.iter().find(|i| i.url == url).cloned();

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

                // 注意：标题已在 ToolManager.quick_enqueue_download 中从 URL 提取
                // 这里不再手动设置 output_template 和 task_title

                // 不再手动设置 download_url 或 ffmpeg_url，让 quick_enqueue_download 自动判断下载策略
                // 只需要传递 headers（用于需要认证的流媒体）

                // 设置 ffmpeg headers（放在 -i 之前），优先透传嗅探到的真实请求头，再补齐常见头
                let mut header_lines = Vec::new();
                let mut seen = HashSet::new();

                tracing::debug!(
                    "🔧 开始构造 headers，captured_item 存在: {}",
                    captured_item.is_some()
                );

                if let Some(item) = captured_item.clone() {
                    if let Some(headers) = item.headers {
                        tracing::debug!("📦 从 captured_item 获取到 {} 个 headers", headers.len());
                        for (k, v) in headers {
                            magekit_shared::validate_http_header(&k, &v)?;
                            let key_lower = k.to_ascii_lowercase();
                            if seen.insert(key_lower) {
                                header_lines.push(format!("{}: {}", k, v));
                            }
                        }
                    } else {
                        tracing::warn!("⚠️ captured_item.headers 为 None");
                    }
                    if let Some(referer) = item.referer {
                        magekit_shared::validate_http_header("Referer", &referer)?;
                        if seen.insert("referer".into()) {
                            header_lines.push(format!("Referer: {}", referer));
                        }
                        if seen.insert("origin".into()) {
                            if let Ok(u) = Url::parse(&referer) {
                                if let Some(host) = u.host_str() {
                                    let origin = format!("Origin: {}://{}", u.scheme(), host);
                                    tracing::debug!("🌐 构造 Origin: {}", origin);
                                    header_lines.push(origin);
                                }
                            }
                        }
                    } else {
                        tracing::warn!("⚠️ captured_item.referer 为 None");
                    }
                } else {
                    tracing::warn!("⚠️ captured_item 为 None，无法获取真实 headers");
                }

                // 兜底补齐缺失的常用头（避免覆盖已抓到的）
                let defaults = [
                    ("user-agent", format!("User-Agent: {}", DEFAULT_FFMPEG_UA)),
                    ("accept", "Accept: */*".to_string()),
                    (
                        "accept-encoding",
                        "Accept-Encoding: gzip, deflate, br".to_string(),
                    ),
                    (
                        "accept-language",
                        "Accept-Language: zh-CN,zh;q=0.9,en;q=0.8".to_string(),
                    ),
                    ("cache-control", "Cache-Control: no-cache".to_string()),
                    ("pragma", "Pragma: no-cache".to_string()),
                ];
                for (k, v) in defaults {
                    if seen.insert(k.to_string()) {
                        header_lines.push(v);
                    }
                }

                // 设置 headers（会被 ffmpeg 使用）
                let header_block = format!("{}\r\n", header_lines.join("\r\n"));
                // Cookie、Authorization 和签名 URL 绝不写入日志。
                tracing::debug!(
                    header_count = header_lines.len(),
                    "Prepared capture request headers"
                );
                options.ffmpeg_args.push("-headers".to_string());
                options.ffmpeg_args.push(header_block);
                tracing::info!(
                    "✅ ffmpeg_args 已设置，总长度: {}",
                    options.ffmpeg_args.len()
                );

                runtime.block_on(async { app_state.start_download(&url_clone, options).await })
            })
            .await;

            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(_) => {
                        this.download_status
                            .insert(url.clone(), EnqueueStatus::Queued);
                    }
                    Err(err) => {
                        this.download_status
                            .insert(url.clone(), EnqueueStatus::Failed(err.to_string()));
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
        // 完成列表构造后，再读取主题
        let theme = cx.theme();
        let row_theme = CaptureRowTheme {
            bg: theme.background,
            border: theme.border,
            text: theme.foreground,
            muted: theme.muted_foreground,
        };

        // 使用筛选后的资源列表
        let captured_data: Vec<(M3u8Stream, Option<EnqueueStatus>)> = self
            .filter_resources()
            .into_iter()
            .map(|item| {
                let status = self.download_status.get(&item.url).cloned();
                (item, status)
            })
            .collect();

        let captured_data_rc: Rc<Vec<(M3u8Stream, Option<EnqueueStatus>)>> =
            Rc::new(captured_data.clone());
        let item_sizes: Rc<Vec<Size<Pixels>>> = Rc::new(
            captured_data
                .iter()
                .map(|_| size(px(800.0), px(CAPTURE_ITEM_HEIGHT)))
                .collect(),
        );

        let scroll_handle = self.scroll_handle.clone();
        let row_theme_copy = row_theme;
        let entity = cx.entity().clone();
        let capture_list = v_virtual_list(
            cx.entity().clone(),
            "capture-list",
            item_sizes,
            move |_page, visible_range, _window, _cx| {
                let row_theme = row_theme_copy;
                let captured_data = captured_data_rc.clone();
                let entity = entity.clone();
                visible_range
                    .filter_map(move |ix| {
                        let captured_data = captured_data.clone();
                        let entity = entity.clone();
                        captured_data.get(ix).cloned().map(move |(item, status)| {
                            let url = item.url.clone();
                            let status_label = status.as_ref().map(EnqueueStatus::label);
                            let blocked = status
                                .as_ref()
                                .is_some_and(EnqueueStatus::blocks_submission);
                            let queued = matches!(status, Some(EnqueueStatus::Queued));
                            let submitting = matches!(status, Some(EnqueueStatus::Submitting));
                            let entity_for_btn = entity.clone();
                            let action = Button::new(("capture-download", ix))
                                .primary()
                                .small()
                                .label(if queued {
                                    crate::i18n::tr("已加入任务")
                                } else {
                                    crate::i18n::tr("加入下载")
                                })
                                .loading(submitting)
                                .disabled(blocked)
                                .on_click(move |_, _, cx| {
                                    let _ = entity_for_btn.update(cx, |this, cx| {
                                        this.download_link(url.clone(), cx);
                                    });
                                });
                            capture_row(item, status_label, row_theme, action)
                        })
                    })
                    .collect::<Vec<_>>()
            },
        )
        .track_scroll(&scroll_handle);

        let capture_body: AnyElement = if captured_data.is_empty() {
            Empty::new()
                .header(
                    EmptyHeader::new()
                        .title(EmptyTitle::new().child(crate::i18n::tr("尚未捕获到媒体资源")))
                        .description(EmptyDescription::new().child(if self.is_running {
                            crate::i18n::tr("正在监听网页请求，可随时停止；已捕获的资源会保留")
                        } else {
                            crate::i18n::tr("输入网页地址并开始抓取，选择资源后加入下载任务")
                        })),
                )
                .into_any_element()
        } else {
            div()
                .relative()
                .flex_1()
                .w_full()
                .overflow_hidden()
                .rounded(px(12.0))
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right(px(12.0))
                        .bottom_0()
                        .p(px(8.0))
                        .border_1()
                        .border_color(theme.border)
                        .rounded(px(12.0))
                        .child(capture_list),
                )
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .bottom_0()
                        .w(px(12.0))
                        .child(Scrollbar::new(&self.scroll_handle).axis(ScrollbarAxis::Vertical)),
                )
                .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .id("capture-page")
            .overflow_y_scroll()
            .gap(px(16.0))
            .p(px(20.0))
            .child(
                // 标题
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_3()
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
                                    .child(crate::i18n::tr("M3U8 嗅探")),
                            )
                            .child(div().text_sm().text_color(theme.muted_foreground).child(
                                crate::i18n::tr("启动浏览器抓包，实时展示捕获到的 m3u8 链接"),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(
                                Button::new("start-capture")
                                    .primary()
                                    .disabled(self.is_running)
                                    .label(crate::i18n::tr("开始抓取"))
                                    .on_click(cx.listener(|this, _event, window, cx| {
                                        this.start_capture(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("stop-capture")
                                    .ghost()
                                    .disabled(!self.is_running)
                                    .label(crate::i18n::tr("停止"))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.stop_capture();
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .when_some(self.last_error.clone(), |el, error| {
                el.child(Alert::error("capture-error", error))
            })
            .when(self.is_running, |el| {
                el.child(Alert::info(
                    "capture-running",
                    crate::i18n::tr("正在监听网页请求，可随时停止；已捕获的资源会保留"),
                ))
            })
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
                            .child(crate::i18n::tr("抓取设置")),
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
                                    .child(crate::i18n::tr("目标 URL")),
                            )
                            .child(Input::new(&self.url_input).disabled(self.is_running)),
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
                                    .child(crate::i18n::tr("自定义浏览器路径 (可选)")),
                            )
                            .child(Input::new(&self.browser_input).disabled(self.is_running)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                Checkbox::new("headless-toggle")
                                    .disabled(self.is_running)
                                    .checked(self.headless)
                                    .label(crate::i18n::tr("使用 Headless 模式"))
                                    .on_click(cx.listener(|this, checked, _window, cx| {
                                        this.headless = *checked;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child(crate::i18n::tr("无头模式可减少资源占用")),
                            ),
                    ),
            )
            .child(
                // 捕获结果
                div()
                    .min_h(px(320.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .p(px(16.0))
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(12.0))
                    .child(
                        // 标题和操作栏
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.foreground)
                                    .child(crate::i18n::format(
                                        "捕获的资源 ({})",
                                        &[format!("{}", self.captured.len())],
                                    )),
                            )
                            .child(
                                Button::new("refresh-capture")
                                    .ghost()
                                    .small()
                                    .label(crate::i18n::tr("清空"))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.captured.clear();
                                        this.download_status.clear();
                                        this.scroll_handle = VirtualListScrollHandle::new();
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        // 资源类型筛选（移到这里）
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .p(px(8.0))
                            .bg(theme.muted.opacity(0.05))
                            .border_1()
                            .border_color(theme.border.opacity(0.5))
                            .rounded(px(8.0))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.muted_foreground)
                                    .child(crate::i18n::tr("筛选类型：")),
                            )
                            .child(
                                Button::new("filter-media")
                                    .when(self.filter_type == CaptureFilterType::Media, |btn| {
                                        btn.primary()
                                    })
                                    .when(self.filter_type != CaptureFilterType::Media, |btn| {
                                        btn.ghost()
                                    })
                                    .xsmall()
                                    .label(crate::i18n::tr("媒体"))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.filter_type = CaptureFilterType::Media;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("filter-video")
                                    .when(self.filter_type == CaptureFilterType::Video, |btn| {
                                        btn.primary()
                                    })
                                    .when(self.filter_type != CaptureFilterType::Video, |btn| {
                                        btn.ghost()
                                    })
                                    .xsmall()
                                    .label(crate::i18n::tr("视频"))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.filter_type = CaptureFilterType::Video;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("filter-audio")
                                    .when(self.filter_type == CaptureFilterType::Audio, |btn| {
                                        btn.primary()
                                    })
                                    .when(self.filter_type != CaptureFilterType::Audio, |btn| {
                                        btn.ghost()
                                    })
                                    .xsmall()
                                    .label(crate::i18n::tr("音频"))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.filter_type = CaptureFilterType::Audio;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("filter-image")
                                    .when(self.filter_type == CaptureFilterType::Image, |btn| {
                                        btn.primary()
                                    })
                                    .when(self.filter_type != CaptureFilterType::Image, |btn| {
                                        btn.ghost()
                                    })
                                    .xsmall()
                                    .label(crate::i18n::tr("图片"))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.filter_type = CaptureFilterType::Image;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("filter-all")
                                    .when(self.filter_type == CaptureFilterType::All, |btn| {
                                        btn.primary()
                                    })
                                    .when(self.filter_type != CaptureFilterType::All, |btn| {
                                        btn.ghost()
                                    })
                                    .xsmall()
                                    .label(crate::i18n::tr("全部"))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.filter_type = CaptureFilterType::All;
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
                            .child(capture_body),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{EnqueueStatus, valid_capture_url};
    #[test]
    fn only_web_urls_are_accepted() {
        assert!(valid_capture_url("https://example.com/watch?v=1"));
        for value in [
            "",
            "example.com",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "--no-sandbox",
        ] {
            assert!(!valid_capture_url(value));
        }
    }
    #[test]
    fn enqueue_state_blocks_repeated_clicks_without_localized_text_comparison() {
        assert!(EnqueueStatus::Submitting.blocks_submission());
        assert!(EnqueueStatus::Queued.blocks_submission());
        assert!(!EnqueueStatus::Failed("network".into()).blocks_submission());
    }
}
