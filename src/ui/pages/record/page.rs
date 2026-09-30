//! 录制页面主组件 - 完整重写版本
//!
//! 功能：
//! - 添加、删除、监控直播间
//! - 自动检测平台并验证支持性
//! - 持久化保存监控的房间到 AppConfig
//! - 正确的文件路径格式: {base}/record/{平台}/{主播名}/{主播名}_{时间}.ts
//! - 支持 TS 录制和录制后转码

use crate::app::AppState;
use chrono::Utc;
use futures_util::{StreamExt, stream::FuturesUnordered};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_kit::component::Sizable;
use gpui_kit::component::WindowExt;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::empty::{
    Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle,
};
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::radio::RadioGroup;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{ActiveTheme, Disableable};
use gpui_kit::component::{Icon, IconName, h_flex, v_flex};
use live_recorder::{
    LiveRecorder, RecordConfig, RecordStatus, error::RecorderError, recorder::RecordingHandle,
};
use magekit_shared::types::{
    LiveRecordConfig, LiveRecordQuality, LiveRoomStatus, MonitoredRoom, RecordingTask,
};
use parking_lot::RwLock;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

static CONFIG_SAVE_GENERATION: AtomicU64 = AtomicU64::new(0);

fn cover_image_source(source: String) -> gpui::ImageSource {
    if source.starts_with("http://") || source.starts_with("https://") {
        source.into()
    } else {
        PathBuf::from(source).into()
    }
}

fn is_placeholder_anchor_name(name: &str) -> bool {
    matches!(name.trim(), "" | "Unknown" | "获取中..." | "获取失败") || name.starts_with("Unknown-")
}

/// 运行时房间状态（用于 UI 显示）
struct RuntimeRoomState {
    status: LiveRoomStatus,
    is_recording: bool,
    current_task: Option<RecordingTask>,
    last_error: Option<String>,
    /// 封面图 URL
    cover_url: Option<String>,
    /// 本次运行已经尝试获取过一次平台分享封面。
    cover_lookup_attempted: bool,
    /// 直播标题
    title: Option<String>,
    /// 最近一次 SOOP 房态查询得到的短期频道元数据（仅本次运行使用）。
    soop_hint: Option<Value>,
    soop_prefetch_inflight: bool,
    /// 录制句柄（用于停止录制）
    recording_handle: Option<Arc<TokioMutex<RecordingHandle>>>,
}

impl Clone for RuntimeRoomState {
    fn clone(&self) -> Self {
        Self {
            status: self.status.clone(),
            is_recording: self.is_recording,
            current_task: self.current_task.clone(),
            last_error: self.last_error.clone(),
            cover_url: self.cover_url.clone(),
            cover_lookup_attempted: self.cover_lookup_attempted,
            title: self.title.clone(),
            soop_hint: self.soop_hint.clone(),
            soop_prefetch_inflight: self.soop_prefetch_inflight,
            recording_handle: self.recording_handle.clone(),
        }
    }
}

impl Default for RuntimeRoomState {
    fn default() -> Self {
        Self {
            status: LiveRoomStatus::Unknown,
            is_recording: false,
            current_task: None,
            last_error: None,
            cover_url: None,
            cover_lookup_attempted: false,
            title: None,
            soop_hint: None,
            soop_prefetch_inflight: false,
            recording_handle: None,
        }
    }
}

/// 录制页面组件
pub struct RecordingPage {
    app_state: Arc<AppState>,
    url_input: Entity<InputState>,
    room_search: Entity<InputState>,
    room_filter: RoomFilter,
    room_sort: RoomSort,
    room_view: RoomView,
    room_scroll: ScrollHandle,

    /// 持久化的监控房间列表（从 AppConfig 加载）
    monitored_rooms: Vec<MonitoredRoom>,
    /// 运行时状态（不持久化）
    room_states: HashMap<Uuid, RuntimeRoomState>,
    /// 录制配置（从 AppConfig 加载）
    record_config: LiveRecordConfig,

    /// UI 状态
    is_loading: bool,
    monitoring_enabled: bool,

    /// 最后一次添加房间的错误
    last_add_error: Option<String>,
    pending_toasts: Vec<(ToastLevel, String)>,

    /// 监控定时器 ID
    check_task_running: Arc<RwLock<bool>>,
    /// 上一轮房间状态检查完成前，不启动新一轮。
    check_cycle_running: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoomFilter {
    All,
    Recording,
    Live,
    Offline,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoomSort {
    Activity,
    Name,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoomView {
    Grid,
    List,
}

#[derive(Debug, Clone, Copy)]
enum ToastLevel {
    Success,
    Info,
    Warning,
    Error,
}

struct CheckCycleGuard(Arc<AtomicBool>);

impl Drop for CheckCycleGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl RecordingPage {
    fn push_toast(&mut self, level: ToastLevel, message: impl Into<String>) {
        self.pending_toasts.push((level, message.into()));
    }

    fn resolve_ffmpeg_path(&self) -> Option<PathBuf> {
        let bundled = self
            .app_state
            .tool_manager
            .storage
            .get_tool_path(magekit_shared::ToolType::Ffmpeg);
        if bundled.exists() {
            return Some(bundled);
        }
        magekit_shared::resolve_ffmpeg_path().or_else(|| which::which("ffmpeg").ok())
    }

    fn transcode_target_path(&self, input_path: &PathBuf) -> Option<PathBuf> {
        if !self.record_config.auto_transcode {
            return None;
        }

        let target_ext = self
            .record_config
            .transcode_format
            .as_deref()
            .unwrap_or("mp4");

        let input_ext = input_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        if input_ext.eq_ignore_ascii_case(target_ext) {
            return None;
        }

        Some(input_path.with_extension(target_ext))
    }

    fn start_transcode_in_background(
        &mut self,
        room_id: Uuid,
        input_path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let output_path = match self.transcode_target_path(&input_path) {
            Some(p) => p,
            None => return,
        };

        let ffmpeg = match self.resolve_ffmpeg_path() {
            Some(p) => p,
            None => {
                self.push_toast(
                    ToastLevel::Warning,
                    crate::i18n::tr("未找到 ffmpeg，跳过自动转码"),
                );
                cx.notify();
                return;
            }
        };

        if let Some(state) = self.room_states.get_mut(&room_id) {
            if let Some(ref mut task) = state.current_task {
                task.status = magekit_shared::types::RecordingTaskStatus::Transcoding;
            }
        }
        cx.notify();

        let runtime = self.app_state.runtime.clone();
        cx.spawn(async move |this, cx| {
            let ffmpeg = ffmpeg.clone();
            let output_path_clone = output_path.clone();
            let input_path_clone = input_path.clone();

            let result = runtime
                .spawn(async move {
                    let mut cmd = magekit_shared::create_tokio_command(&ffmpeg);
                    cmd.arg("-hide_banner")
                        .arg("-loglevel")
                        .arg("warning")
                        .arg("-y")
                        .arg("-i")
                        .arg(&input_path_clone)
                        .arg("-c")
                        .arg("copy");

                    let out_ext = output_path_clone
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    if out_ext.eq_ignore_ascii_case("mp4") {
                        cmd.arg("-bsf:a").arg("aac_adtstoasc");
                        cmd.arg("-movflags").arg("+faststart");
                    }

                    let output = cmd.arg(&output_path_clone).output().await?;

                    let mut delete_error = None;
                    if output.status.success() && input_path_clone != output_path_clone {
                        if let Err(e) = tokio::fs::remove_file(&input_path_clone).await {
                            delete_error = Some(e.to_string());
                        }
                    }

                    Ok::<(std::process::Output, Option<String>), std::io::Error>((
                        output,
                        delete_error,
                    ))
                })
                .await;

            let _ = this.update(cx, |page, cx| {
                match result {
                    Ok(Ok((output, delete_error))) => {
                        if output.status.success() {
                            if let Some(state) = page.room_states.get_mut(&room_id) {
                                if let Some(ref mut task) = state.current_task {
                                    task.status =
                                        magekit_shared::types::RecordingTaskStatus::Completed;
                                    task.output_path = output_path.clone();
                                }
                            }
                            if let Some(err) = delete_error {
                                page.push_toast(
                                    ToastLevel::Warning,
                                    crate::i18n::format(
                                        "已转码但清理源文件失败：{}",
                                        &[format!("{}", err)],
                                    ),
                                );
                            }
                        } else {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            page.push_toast(
                                ToastLevel::Error,
                                crate::i18n::format(
                                    "转码失败：{}",
                                    &[format!("{}", stderr.trim())],
                                ),
                            );
                            if let Some(state) = page.room_states.get_mut(&room_id) {
                                if let Some(ref mut task) = state.current_task {
                                    task.status =
                                        magekit_shared::types::RecordingTaskStatus::Failed(
                                            stderr.trim().to_string(),
                                        );
                                }
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        page.push_toast(
                            ToastLevel::Error,
                            crate::i18n::format("启动 ffmpeg 失败：{}", &[format!("{}", e)]),
                        );
                        if let Some(state) = page.room_states.get_mut(&room_id) {
                            if let Some(ref mut task) = state.current_task {
                                task.status = magekit_shared::types::RecordingTaskStatus::Failed(
                                    e.to_string(),
                                );
                            }
                        }
                    }
                    Err(e) => {
                        page.push_toast(
                            ToastLevel::Error,
                            crate::i18n::format("转码任务失败：{}", &[format!("{}", e)]),
                        );
                        if let Some(state) = page.room_states.get_mut(&room_id) {
                            if let Some(ref mut task) = state.current_task {
                                task.status = magekit_shared::types::RecordingTaskStatus::Failed(
                                    e.to_string(),
                                );
                            }
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn format_record_duration(seconds: u64) -> String {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;

        if hours > 0 {
            format!("{:02}:{:02}:{:02}", hours, minutes, secs)
        } else {
            format!("{:02}:{:02}", minutes, secs)
        }
    }

    fn platform_record_dir_name(platform: &str) -> &str {
        match platform {
            // 兼容旧配置（曾保存为中文平台名）
            "抖音直播" | "douyin" => "抖音直播",
            "B站直播" | "bilibili" => "B站直播",
            "虎牙直播" | "huya" => "虎牙直播",
            "斗鱼直播" | "douyu" => "斗鱼直播",
            "快手直播" | "kuaishou" => "快手直播",
            "SOOP" | "soop" => "SOOP",
            other => other,
        }
    }

    fn sanitize_path_component(raw: &str) -> String {
        let mut s = raw
            .chars()
            .map(|c| match c {
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
                c if c.is_control() => '_',
                c => c,
            })
            .collect::<String>();

        s = s.trim().trim_matches('.').to_string();

        if s.is_empty() { "_".to_string() } else { s }
    }

    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 创建 URL 输入框
        let url_input = cx.new(|cx| {
            crate::i18n::input(
                "输入直播间地址（抖音使用原生录制，其他支持的平台使用 Streamlink）",
                window,
                cx,
            )
        });

        let room_search = cx.new(|cx| crate::i18n::input("搜索主播或房间", window, cx));
        cx.subscribe(&room_search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.room_scroll.set_offset(point(px(0.0), px(0.0)));
            }
            cx.notify();
        })
        .detach();

        // 从配置加载数据
        let config = app_state.config.blocking_read().clone();
        let monitored_rooms = config.monitored_rooms.clone();
        let record_config = config.live_record.clone();

        // 初始化运行时状态：先显示历史封面，等本次状态刷新拿到新封面后再替换。
        // 历史 URL 即使已经过期，也会由 img 的 fallback 安全降级为占位图。
        let mut room_states = HashMap::new();
        for room in &monitored_rooms {
            room_states.insert(
                room.id,
                RuntimeRoomState {
                    status: LiveRoomStatus::Unknown,
                    is_recording: false,
                    current_task: None,
                    last_error: None,
                    cover_url: room.cached_cover_url.clone(),
                    // A disk cache is a useful placeholder, not proof that the platform's
                    // best cover has been checked during this application session.
                    cover_lookup_attempted: false,
                    title: room.cached_title.clone(),
                    soop_hint: None,
                    soop_prefetch_inflight: false,
                    recording_handle: None,
                },
            );
        }

        let page = Self {
            app_state,
            url_input,
            room_search,
            room_filter: RoomFilter::All,
            room_sort: RoomSort::Activity,
            room_view: RoomView::Grid,
            room_scroll: ScrollHandle::new(),
            monitored_rooms,
            room_states,
            record_config,
            is_loading: false,
            monitoring_enabled: true,
            last_add_error: None,
            pending_toasts: Vec::new(),
            check_task_running: Arc::new(RwLock::new(false)),
            check_cycle_running: Arc::new(AtomicBool::new(false)),
        };

        // 启动监控任务
        page.start_monitoring_task(cx);

        page
    }

    /// 创建混合录制器，并应用可选的 SOOP 登录信息。
    fn current_live_recorder(&self) -> Arc<LiveRecorder> {
        let (username, password, proxy) = self
            .app_state
            .config
            .try_read()
            .map(|config| {
                (
                    config.live_record.soop_username.clone(),
                    config.live_record.soop_password.clone(),
                    config.advanced.proxy.as_ref().map(|proxy| {
                        if proxy.url.trim().eq_ignore_ascii_case("system") {
                            live_recorder::proxy::system_proxy_url()
                                .unwrap_or_else(|| "system".to_string())
                        } else {
                            proxy.url.clone()
                        }
                    }),
                )
            })
            .unwrap_or_else(|_| {
                (
                    self.record_config.soop_username.clone(),
                    self.record_config.soop_password.clone(),
                    None,
                )
            });
        Arc::new(
            LiveRecorder::new()
                .with_soop_credentials(username, password)
                .with_proxy(proxy),
        )
    }

    /// 仅对手动录制的在线 SOOP 房间预热所选流，不阻塞房态更新。
    fn prefetch_soop_stream_background(
        &mut self,
        room_id: Uuid,
        hint: Value,
        cx: &mut Context<Self>,
    ) {
        let Some(url) = self
            .monitored_rooms
            .iter()
            .find(|room| room.id == room_id)
            .map(|room| room.url.clone())
        else {
            return;
        };
        let Some(state) = self.room_states.get_mut(&room_id) else {
            return;
        };
        if state.soop_prefetch_inflight || state.is_recording {
            return;
        }
        state.soop_prefetch_inflight = true;

        let checked_at = hint["checked_at"].clone();
        let quality = match self.record_config.quality {
            LiveRecordQuality::Original => live_recorder::types::VideoQuality::Original,
            LiveRecordQuality::Blue => live_recorder::types::VideoQuality::Blue,
            LiveRecordQuality::Ultra => live_recorder::types::VideoQuality::Ultra,
            LiveRecordQuality::High => live_recorder::types::VideoQuality::High,
            LiveRecordQuality::Standard => live_recorder::types::VideoQuality::Standard,
        };
        let recorder = self.current_live_recorder();
        let runtime = self.app_state.runtime.clone();
        let app_state = self.app_state.clone();
        cx.spawn(async move |this, cx| {
            let cookies = app_state.config.blocking_read().advanced.cookies.clone();
            let result = runtime
                .spawn(async move {
                    recorder
                        .prepare_soop_stream_with_cookies(&url, quality, &cookies, hint)
                        .await
                })
                .await;
            let _ = this.update(cx, |page, cx| {
                let Some(state) = page.room_states.get_mut(&room_id) else {
                    return;
                };
                state.soop_prefetch_inflight = false;
                if state.is_recording
                    || state.status != LiveRoomStatus::Live
                    || state.soop_hint.as_ref().map(|hint| &hint["checked_at"]) != Some(&checked_at)
                {
                    return;
                }
                if let Ok(Ok(prepared_hint)) = result {
                    state.soop_hint = Some(prepared_hint);
                    tracing::info!("⚡ SOOP 手动录制流已预热: room_id={room_id}");
                    cx.notify();
                } else {
                    tracing::debug!("⚠️ SOOP 流预热未完成: room_id={room_id}");
                }
            });
        })
        .detach();
    }

    /// 返回录制页当前使用的引擎。
    fn current_engine_label(&self) -> &'static str {
        crate::i18n::tr("抖音原生 / 其他平台 Streamlink")
    }

    /// 启动监控任务（定期检查房间状态）
    fn start_monitoring_task(&self, cx: &mut Context<Self>) {
        let check_running = self.check_task_running.clone();

        // 检查是否已在运行
        if *check_running.read() {
            return;
        }
        *check_running.write() = true;

        tracing::debug!("🚀 启动监控任务");

        cx.spawn(async move |this, cx| {
            loop {
                // 组件被销毁时 update 会失败，用于退出后台循环
                let result = this.update(cx, |this, cx| {
                    this.check_all_rooms(cx);
                    this.record_config.check_interval.max(10)
                });

                let interval_secs = match result {
                    Ok(v) => v,
                    Err(_) => break,
                };

                smol::Timer::after(std::time::Duration::from_secs(interval_secs)).await;
            }

            *check_running.write() = false;
            tracing::debug!("🛑 监控任务已退出");
        })
        .detach();
    }

    /// 检查所有房间状态
    fn check_all_rooms(&mut self, cx: &mut Context<Self>) {
        let rooms: Vec<_> = self
            .monitored_rooms
            .iter()
            .filter(|r| r.monitoring_enabled)
            .map(|room| {
                let (fetch_cover, was_live) = self
                    .room_states
                    .get(&room.id)
                    .map(|state| {
                        (
                            !state.cover_lookup_attempted,
                            state.status == LiveRoomStatus::Live
                                || state.status == LiveRoomStatus::Recording
                                || state.is_recording,
                        )
                    })
                    .unwrap_or((room.cached_cover_url.is_none(), false));
                (room.clone(), fetch_cover, was_live)
            })
            .collect();

        if rooms.is_empty() {
            return;
        }

        let check_cycle_running = self.check_cycle_running.clone();
        if check_cycle_running.swap(true, Ordering::AcqRel) {
            tracing::debug!("⏳ 上一轮房间状态检查尚未完成，跳过本次轮询");
            return;
        }

        let live_recorder = self.current_live_recorder();
        let runtime = self.app_state.runtime.clone();
        let app_state = self.app_state.clone();

        for (room, _, _) in &rooms {
            if let Some(state) = self.room_states.get_mut(&room.id) {
                if !state.is_recording {
                    state.status = LiveRoomStatus::Checking;
                }
                state.last_error = None;
            }
        }
        cx.notify();

        tracing::info!("🔍 开始检查 {} 个房间状态...", rooms.len());

        let cycle_guard = CheckCycleGuard(check_cycle_running);
        cx.spawn(async move |this, cx| {
            let _cycle_guard = cycle_guard;

            // 读取 Cookie 配置
            let cookies = {
                let config = app_state.config.blocking_read();
                config.advanced.cookies.clone()
            };

            // 限制并发，避免大量 Python worker 同时启动造成窗口卡顿。
            // 慢周期结束前由 guard 阻止下一轮重叠启动。
            const MAX_CONCURRENT_CHECKS: usize = 4;
            let mut pending_rooms = VecDeque::from(rooms);
            let mut checks = FuturesUnordered::new();
            while !pending_rooms.is_empty() || !checks.is_empty() {
                while checks.len() < MAX_CONCURRENT_CHECKS {
                    let Some((room, fetch_cover, was_live)) = pending_rooms.pop_front() else {
                        break;
                    };
                    let recorder = live_recorder.clone();
                    let url = room.url.clone();
                    let room_id = room.id;
                    let cookies_clone = cookies.clone();
                    let runtime = runtime.clone();

                    checks.push(async move {
                        // 在 Tokio runtime 中执行；已有封面时跳过额外的网页请求。
                        let result = runtime
                            .spawn(async move {
                                recorder
                                    .check_room_status_with_cookies_and_cover(
                                        &url,
                                        &cookies_clone,
                                        fetch_cover,
                                    )
                                    .await
                            })
                            .await;
                        (room_id, fetch_cover, was_live, result)
                    });
                }

                let Some((room_id, fetch_cover, was_live, result)) = checks.next().await else {
                    break;
                };
                match result {
                    Ok(Ok(room_info)) => {
                        let status = match room_info.status {
                            live_recorder::types::LiveStatus::Live => LiveRoomStatus::Live,
                            live_recorder::types::LiveStatus::Offline => LiveRoomStatus::Offline,
                            live_recorder::types::LiveStatus::Playback => LiveRoomStatus::Playback,
                            live_recorder::types::LiveStatus::Unknown => LiveRoomStatus::Unknown,
                        };

                        // 提取房间信息用于缓存
                        let title = if room_info.title.is_empty() {
                            None
                        } else {
                            Some(room_info.title.clone())
                        };
                        let cover_url = room_info.cover_url.clone();
                        let anchor_name = room_info.anchor_name.clone();
                        let is_live = status == LiveRoomStatus::Live;
                        let soop_hint = room_info.extra.get("soop_hint").cloned();

                        // 调试日志
                        tracing::info!("📦 房间 {} 状态刷新: {:?}", room_id, status);
                        tracing::debug!("   - 状态: {:?}", status);
                        tracing::debug!("   - 标题: {:?}", title);
                        tracing::debug!("   - 封面: {:?}", cover_url);
                        tracing::debug!("   - 主播: {}", room_info.anchor_name);

                        let _ = this.update(cx, |this, cx| {
                            let is_recording = this
                                .room_states
                                .get(&room_id)
                                .map(|s| s.is_recording)
                                .unwrap_or(false);

                            // 更新运行时状态
                            if let Some(state) = this.room_states.get_mut(&room_id) {
                                state.status = status;
                                state.last_error = None;
                                state.cover_lookup_attempted |= fetch_cover;
                                if !is_live {
                                    state.soop_hint = None;
                                } else if soop_hint.is_some() {
                                    state.soop_hint = soop_hint.clone();
                                }
                                // 更新标题和封面
                                if title.is_some() {
                                    state.title = title.clone();
                                }
                                // 只有拿到新封面时才替换历史缓存；暂时没有封面或探测失败时保留旧图过渡。
                                if let Some(source) = &cover_url {
                                    cover_image_source(source.clone()).remove_asset(cx);
                                    state.cover_url = cover_url.clone();
                                }
                            }

                            // 更新持久化的房间信息（缓存标题、封面等）
                            let mut need_save = false;
                            if let Some(room) =
                                this.monitored_rooms.iter_mut().find(|r| r.id == room_id)
                            {
                                room.last_checked = Some(Utc::now());

                                // 添加时探测失败会留下占位名称；后续状态轮询成功后补回主播名。
                                if is_placeholder_anchor_name(&room.anchor_name)
                                    && !anchor_name.trim().is_empty()
                                {
                                    room.anchor_name = anchor_name.clone();
                                    need_save = true;
                                }

                                // 同步标题到缓存
                                if title.is_some() && room.cached_title != title {
                                    tracing::debug!(
                                        "📝 更新房间 {} 缓存标题: {:?} -> {:?}",
                                        room_id,
                                        room.cached_title,
                                        title
                                    );
                                    room.cached_title = title;
                                    need_save = true;
                                }
                                // 只有新封面有效时才替换缓存，避免一次无封面响应把历史图清掉。
                                if cover_url.is_some() && room.cached_cover_url != cover_url {
                                    tracing::debug!(
                                        "🖼️ 更新房间 {} 缓存封面: {:?}",
                                        room_id,
                                        cover_url
                                    );
                                    room.cached_cover_url = cover_url.clone();
                                    need_save = true;
                                }
                                // 更新最后直播时间
                                if is_live && (!was_live || room.last_live_at.is_none()) {
                                    room.last_live_at = Some(Utc::now());
                                    need_save = true;
                                }

                                tracing::debug!("📊 房间 {} need_save={}", room_id, need_save);
                            } else {
                                tracing::warn!("⚠️ 找不到房间 {} 在 monitored_rooms 中", room_id);
                            }

                            // 保存配置（如果有更新）
                            if need_save {
                                this.save_config_background(cx);
                            }

                            // 如果开启了自动录制且变为直播状态，自动开始录制（自动录制与监控分离）
                            let global_auto_record_enabled = this.record_config.auto_record;
                            let room_auto_record_enabled = this
                                .monitored_rooms
                                .iter()
                                .find(|r| r.id == room_id)
                                .map(|r| r.auto_record)
                                .unwrap_or(false);

                            if is_live
                                && global_auto_record_enabled
                                && room_auto_record_enabled
                                && !is_recording
                            {
                                tracing::info!("🎬 自动录制检测到直播，开始录制: {}", room_id);
                                this.start_recording_background(room_id, cx);
                            } else if is_live
                                && !is_recording
                                && let Some(hint) = soop_hint.clone()
                            {
                                this.prefetch_soop_stream_background(room_id, hint, cx);
                            }

                            cx.notify();
                        });
                    }
                    Ok(Err(e)) => {
                        let error = format!("{}", e);
                        tracing::warn!("⚠️ 房间 {} 状态刷新失败: {}", room_id, error);
                        let _ = this.update(cx, |this, cx| {
                            if let Some(state) = this.room_states.get_mut(&room_id) {
                                state.status = LiveRoomStatus::Error(error.clone());
                                state.last_error = Some(error);
                                state.cover_lookup_attempted |= fetch_cover;
                            }
                            cx.notify();
                        });
                    }
                    Err(e) => {
                        let error = crate::i18n::format("任务执行失败: {}", &[format!("{}", e)]);
                        tracing::warn!("⚠️ 房间 {} 状态检查任务失败: {}", room_id, error);
                        let _ = this.update(cx, |this, cx| {
                            if let Some(state) = this.room_states.get_mut(&room_id) {
                                state.status = LiveRoomStatus::Error(error.clone());
                                state.last_error = Some(error);
                                state.cover_lookup_attempted |= fetch_cover;
                            }
                            cx.notify();
                        });
                    }
                }
            }

            tracing::debug!("✅ 房间状态检查完成");
        })
        .detach();
    }

    /// 在配置锁内合并本页字段并写盘，避免后台旧快照覆盖其他页面的新设置。
    fn save_config(&self) {
        CONFIG_SAVE_GENERATION.fetch_add(1, Ordering::AcqRel);
        let mut config = self.app_state.config.blocking_write();
        let previous = config.clone();
        config.monitored_rooms = self.monitored_rooms.clone();
        let credentials = (
            config.live_record.soop_username.clone(),
            config.live_record.soop_password.clone(),
        );
        config.live_record = self.record_config.clone();
        (
            config.live_record.soop_username,
            config.live_record.soop_password,
        ) = credentials;
        if let Err(error) = magekit_shared::save_app_config(&config) {
            *config = previous;
            tracing::error!("Failed to save recording settings: {}", error);
        }
    }

    /// 轮询持久化只携带录制字段，获取共享配置锁后再合并最新配置。
    fn save_config_background(&self, cx: &mut Context<Self>) {
        let app_state = self.app_state.clone();
        let rooms = self.monitored_rooms.clone();
        let record_config = self.record_config.clone();
        let generation = CONFIG_SAVE_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
        cx.spawn(async move |_this, _cx| {
            smol::unblock(move || {
                let mut config = app_state.config.blocking_write();
                if CONFIG_SAVE_GENERATION.load(Ordering::Acquire) != generation {
                    return;
                }
                let previous = config.clone();
                config.monitored_rooms = rooms;
                let credentials = (
                    config.live_record.soop_username.clone(),
                    config.live_record.soop_password.clone(),
                );
                config.live_record = record_config;
                (
                    config.live_record.soop_username,
                    config.live_record.soop_password,
                ) = credentials;
                if let Err(error) = magekit_shared::save_app_config(&config) {
                    *config = previous;
                    tracing::error!("Failed to save recording settings: {}", error);
                }
            })
            .await;
        })
        .detach();
    }

    /// 显示添加直播间对话框
    fn show_add_room_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url_input = self.url_input.clone();
        let this = cx.entity().clone();

        window.open_dialog(cx, move |dialog, _window, cx| {
            let url_input = url_input.clone();
            let this = this.clone();
            let value = url_input.read(cx).value();
            let valid = recording_platform(value.trim()).is_some();
            let show_validation = !value.trim().is_empty() && !valid;

            dialog
                .title(crate::i18n::tr("添加直播间"))
                .child(
                    v_flex()
                        .gap_4()
                        .child(div().text_sm().child(crate::i18n::tr("请输入直播间链接")))
                        .child(Input::new(&url_input).cleanable(true))
                        .when(show_validation, |el| el.child(Alert::error("room-url-invalid", crate::i18n::tr("请输入支持的直播平台的完整 HTTP 或 HTTPS 链接"))))
                        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(
                            crate::i18n::tr("抖音使用原生录制；B站、虎牙、斗鱼、SOOP 等平台使用 Streamlink；快手暂不支持"),
                        )),
                )
                .footer({
                    let url_input = url_input.clone();
                    let this = this.clone();

                    DialogFooter::new().children([
                        Button::new("confirm-add").primary().disabled(!valid).label(crate::i18n::tr("添加")).on_click(
                            move |_event, window, cx| {
                                let url = url_input.read(cx).text().to_string().trim().to_string();

                                let added = this.update(cx, |this, cx| this.add_room(url, window, cx));
                                if added {
                                    url_input.update(cx, |input, cx| input.set_value("", window, cx));
                                    window.close_dialog(cx);
                                }
                            },
                        ),
                        Button::new("cancel-add")
                            .label(crate::i18n::tr("取消"))
                            .on_click(|_event, window, cx| {
                                window.close_dialog(cx);
                            }),
                    ])
                })
        });
    }

    /// 添加直播间 - 立即添加到列表，后台获取信息
    fn add_room(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(platform) = recording_platform(&url) else {
            window.push_notification(
                Notification::error(crate::i18n::tr(
                    "请输入支持的直播平台的完整 HTTP 或 HTTPS 链接",
                )),
                cx,
            );
            return false;
        };
        // 检查是否已存在
        if self.monitored_rooms.iter().any(|r| r.url == url) {
            window.push_notification(
                Notification::warning(crate::i18n::tr("该直播间已在监控列表中")),
                cx,
            );
            return false;
        }

        tracing::info!("🚀 开始添加直播间: {}", url);

        // 立即创建房间并添加到列表
        let mut monitored_room = MonitoredRoom::new(
            url.clone(),
            platform.to_string(),
            String::new(),           // room_id 稍后获取
            "获取中...".to_string(), // anchor_name 稍后获取
        );
        // 录制策略：默认只监控，不自动录制（避免“仅想看状态却自动开录”的误触发）
        monitored_room.auto_record = false;

        let room_id = monitored_room.id;

        // 添加到列表，状态为 Unknown（加载中）
        self.monitored_rooms.push(monitored_room);
        self.room_filter = RoomFilter::All;
        self.room_sort = RoomSort::Activity;
        self.room_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.room_scroll.set_offset(point(px(0.0), px(0.0)));
        self.room_states.insert(
            room_id,
            RuntimeRoomState {
                status: LiveRoomStatus::Checking,
                is_recording: false,
                current_task: None,
                last_error: None,
                cover_url: None,
                cover_lookup_attempted: false,
                title: None,
                soop_hint: None,
                soop_prefetch_inflight: false,
                recording_handle: None,
            },
        );
        self.save_config();
        cx.notify();

        // 后台获取房间详细信息
        let live_recorder = self.current_live_recorder();
        let runtime = self.app_state.runtime.clone();
        let app_state = self.app_state.clone();

        tracing::info!("📡 获取直播间信息请求: url={}", url);

        cx.spawn(async move |this, cx| {
            let url_for_log = url.clone();
            let url_for_request = url.clone();

            // 读取 Cookie 配置
            let cookies = {
                let config = app_state.config.blocking_read();
                config.advanced.cookies.clone()
            };

            let result = runtime
                .spawn(async move {
                    live_recorder
                        .check_room_status_with_cookies(&url_for_request, &cookies)
                        .await
                })
                .await;

            match result {
                Ok(Ok(room_info)) => {
                    tracing::info!(
                        "✅ 获取直播间信息成功: url={} room_id={} anchor={} title={} status={:?}",
                        url_for_log,
                        room_info.room_id,
                        room_info.anchor_name,
                        room_info.title,
                        room_info.status
                    );

                    let status = match room_info.status {
                        live_recorder::types::LiveStatus::Live => LiveRoomStatus::Live,
                        live_recorder::types::LiveStatus::Offline => LiveRoomStatus::Offline,
                        live_recorder::types::LiveStatus::Playback => LiveRoomStatus::Playback,
                        live_recorder::types::LiveStatus::Unknown => LiveRoomStatus::Unknown,
                    };
                    let cover_url = room_info.cover_url.clone();
                    let title = if room_info.title.is_empty() {
                        None
                    } else {
                        Some(room_info.title.clone())
                    };
                    let anchor_name = room_info.anchor_name.clone();
                    let real_room_id = room_info.room_id.clone();

                    let _ = this.update(cx, |this, cx| {
                        // 更新房间信息
                        if let Some(room) =
                            this.monitored_rooms.iter_mut().find(|r| r.id == room_id)
                        {
                            room.room_id = real_room_id;
                            room.anchor_name = anchor_name;
                            room.cached_title = title.clone();
                            room.cached_cover_url = cover_url.clone();
                        }

                        // 更新运行时状态
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.status = status;
                            state.last_error = None;
                            if let Some(source) = &cover_url {
                                cover_image_source(source.clone()).remove_asset(cx);
                            }
                            state.cover_url = cover_url;
                            state.cover_lookup_attempted = true;
                            state.title = title;
                        }

                        this.save_config();
                        cx.notify();
                    });
                }
                Ok(Err(e)) => {
                    let error_msg = Self::format_error(&e);
                    tracing::error!(
                        "❌ 获取直播间信息失败: url={}, err={}",
                        url_for_log,
                        error_msg
                    );

                    let _ = this.update(cx, |this, cx| {
                        // 更新房间状态为错误，但保留在列表中
                        if let Some(room) =
                            this.monitored_rooms.iter_mut().find(|r| r.id == room_id)
                        {
                            room.anchor_name = "获取失败".to_string();
                        }

                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.status = LiveRoomStatus::Error(error_msg.clone());
                            state.last_error = Some(error_msg);
                        }

                        this.save_config();
                        cx.notify();
                    });
                }
                Err(e) => {
                    let error_msg = crate::i18n::format("任务执行失败: {}", &[format!("{}", e)]);
                    tracing::error!(
                        "❌ 获取直播间信息失败: url={}, err={}",
                        url_for_log,
                        error_msg
                    );

                    let _ = this.update(cx, |this, cx| {
                        if let Some(room) =
                            this.monitored_rooms.iter_mut().find(|r| r.id == room_id)
                        {
                            room.anchor_name = "获取失败".to_string();
                        }

                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.status = LiveRoomStatus::Error(error_msg.clone());
                            state.last_error = Some(error_msg);
                        }

                        this.save_config();
                        cx.notify();
                    });
                }
            }
        })
        .detach();
        true
    }

    /// 格式化错误消息（友好显示）
    fn format_error(error: &RecorderError) -> String {
        match error {
            RecorderError::UnsupportedPlatform(url) => crate::i18n::format(
                "当前录制器不支持该地址: {}\n抖音使用原生录制；其他支持的平台由 Streamlink 插件解析",
                &[format!("{}", url)],
            ),
            RecorderError::RoomNotFound(room_id) => {
                crate::i18n::format("直播间不存在: {}", &[format!("{}", room_id)])
            }
            RecorderError::HttpError(e) => crate::i18n::format("网络错误: {}", &[format!("{}", e)]),
            RecorderError::JsonError(e) => crate::i18n::format("解析错误: {}", &[format!("{}", e)]),
            RecorderError::InvalidResponseFormat(msg) => {
                crate::i18n::format("响应格式错误: {}", &[format!("{}", msg)])
            }
            RecorderError::AuthenticationRequired(msg) => crate::i18n::format(
                "需要登录: {}\n请在设置中配置平台账号或 Cookie",
                &[format!("{}", msg)],
            ),
            RecorderError::AuthenticationFailed(msg) => {
                crate::i18n::format("认证失败: {}", &[format!("{}", msg)])
            }
            RecorderError::NetworkTimeout => crate::i18n::tr("网络超时").to_string(),
            _ => format!("{}", error),
        }
    }

    /// 生成录制输出路径
    fn generate_output_path(&self, room: &MonitoredRoom) -> PathBuf {
        let download_path = self
            .app_state
            .config
            .blocking_read()
            .download
            .default_output_path
            .clone();
        let record_base = download_path.join(&self.record_config.output_base_path);

        // 格式: {base}/record/{平台}/{主播名}/{主播名}_{时间}.ts
        let platform_dir = record_base.join(Self::platform_record_dir_name(&room.platform));
        let safe_anchor_name = Self::sanitize_path_component(&room.anchor_name);
        let anchor_dir = platform_dir.join(&safe_anchor_name);

        let timestamp = Utc::now().format("%Y-%m-%d_%H-%M-%S_%3f");
        let filename = format!(
            "{}_{}.{}",
            safe_anchor_name, timestamp, self.record_config.record_format
        );

        anchor_dir.join(filename)
    }

    /// 开始录制
    fn start_recording(&mut self, room_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let room = match self.monitored_rooms.iter().find(|r| r.id == room_id) {
            Some(r) => r.clone(),
            None => return,
        };

        let state = self.room_states.get(&room_id).cloned().unwrap_or_default();

        // 检查是否正在直播
        if state.status != LiveRoomStatus::Live {
            window.push_notification(
                Notification::warning(crate::i18n::tr("直播间未开播，无法录制")),
                cx,
            );
            return;
        }

        // 检查是否已在录制
        if state.is_recording {
            window.push_notification(
                Notification::warning(crate::i18n::tr("该直播间已在录制中")),
                cx,
            );
            return;
        }

        let output_path = self.generate_output_path(&room);
        let live_recorder = self.current_live_recorder();
        let runtime = self.app_state.runtime.clone();
        let app_state = self.app_state.clone();
        let url = room.url.clone();
        let anchor_name = room.anchor_name.clone();
        let soop_hint = self
            .room_states
            .get(&room_id)
            .and_then(|state| state.soop_hint.clone());

        // 创建输出目录
        if let Some(parent) = output_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::error!("❌ 无法创建输出目录: {}", e);
                window.push_notification(
                    Notification::error(&crate::i18n::format(
                        "无法创建输出目录: {}",
                        &[format!("{}", e)],
                    )),
                    cx,
                );
                return;
            }
        }

        let task_id = Uuid::new_v4();
        // 更新状态
        if let Some(state) = self.room_states.get_mut(&room_id) {
            state.is_recording = true;
            state.current_task = Some(RecordingTask {
                id: task_id,
                room_id,
                output_path: output_path.clone(),
                start_time: Utc::now(),
                duration: 0,
                recorded_bytes: 0,
                status: magekit_shared::types::RecordingTaskStatus::Recording,
            });
        }
        cx.notify();

        window.push_notification(
            Notification::info(&crate::i18n::format(
                "正在连接直播流: {}",
                &[format!("{}", anchor_name)],
            )),
            cx,
        );

        tracing::info!(
            "🎥 开始录制: anchor={} url={} -> {:?}",
            anchor_name,
            url,
            output_path
        );

        // 创建录制配置
        let config = RecordConfig {
            output_path_template: output_path.to_string_lossy().to_string(),
            format: self.record_config.record_format.clone(),
            quality: match self.record_config.quality {
                LiveRecordQuality::Original => live_recorder::types::VideoQuality::Original,
                LiveRecordQuality::Blue => live_recorder::types::VideoQuality::Blue,
                LiveRecordQuality::Ultra => live_recorder::types::VideoQuality::Ultra,
                LiveRecordQuality::High => live_recorder::types::VideoQuality::High,
                LiveRecordQuality::Standard => live_recorder::types::VideoQuality::Standard,
            },
            segment_duration: self.record_config.segment_duration,
            retry_count: self.record_config.retry_count,
            reconnect_delay: self.record_config.reconnect_delay,
            timeout: 30,
            max_duration: None,
            include_danmaku: false,
            proxy: None,
            headers: std::collections::HashMap::new(),
        };

        cx.spawn(async move |this, cx| {
            // 读取 Cookie 配置
            let cookies = {
                let config = app_state.config.blocking_read();
                config.advanced.cookies.clone()
            };

            let result = runtime
                .spawn(async move {
                    live_recorder
                        .start_recording_with_cookies_and_hint(&url, config, &cookies, soop_hint)
                        .await
                })
                .await;

            match result {
                Ok(Ok(handle)) => {
                    tracing::info!("✅ 录制任务已启动: {}", anchor_name);
                    let _ = this.update(cx, |this, cx| {
                        let current = this.room_states.get(&room_id).is_some_and(|state| {
                            state.is_recording
                                && state
                                    .current_task
                                    .as_ref()
                                    .is_some_and(|task| task.id == task_id)
                        });
                        if current {
                            // 保存 handle 以便后续停止录制和获取进度。
                            let handle = Arc::new(TokioMutex::new(handle));
                            let handle_clone = handle.clone();
                            if let Some(state) = this.room_states.get_mut(&room_id) {
                                state.recording_handle = Some(handle);
                            }
                            this.start_progress_monitor(room_id, task_id, handle_clone, cx);
                        } else {
                            // 启动期间已停止或重开；丢弃旧 handle 会通知旧 worker 停止。
                            tracing::info!("⏹️ 丢弃已过期的录制任务: room_id={}", room_id);
                        }
                        cx.notify();
                    });
                }
                Ok(Err(e)) => {
                    let error_msg = crate::i18n::format("录制失败: {}", &[format!("{}", e)]);
                    tracing::error!("❌ {}", error_msg);

                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            if !state
                                .current_task
                                .as_ref()
                                .is_some_and(|task| task.id == task_id)
                            {
                                return;
                            }
                            state.is_recording = false;
                            state.last_error = Some(error_msg.clone());
                            state.recording_handle = None;
                            if let Some(ref mut task) = state.current_task {
                                task.status = magekit_shared::types::RecordingTaskStatus::Failed(
                                    error_msg.clone(),
                                );
                            }
                        }
                        cx.notify();
                    });

                    tracing::error!("❌ 录制失败: {}", error_msg);
                }
                Err(e) => {
                    let error_msg = crate::i18n::format("任务执行失败: {}", &[format!("{}", e)]);
                    tracing::error!("❌ {}", error_msg);

                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            if !state
                                .current_task
                                .as_ref()
                                .is_some_and(|task| task.id == task_id)
                            {
                                return;
                            }
                            state.is_recording = false;
                            state.recording_handle = None;
                            state.last_error = Some(error_msg.clone());
                            if let Some(ref mut task) = state.current_task {
                                task.status = magekit_shared::types::RecordingTaskStatus::Failed(
                                    error_msg.clone(),
                                );
                            }
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// 停止录制
    fn stop_recording(&mut self, room_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let anchor_name = self
            .monitored_rooms
            .iter()
            .find(|r| r.id == room_id)
            .map(|r| r.anchor_name.clone())
            .unwrap_or_default();
        tracing::info!(
            "⏹️ 请求停止录制: room_id={} anchor={}",
            room_id,
            anchor_name
        );

        let (handle, output_path, task_id) = if let Some(state) = self.room_states.get_mut(&room_id)
        {
            state.is_recording = false;
            let output_path = state.current_task.as_ref().map(|t| t.output_path.clone());
            let task_id = state.current_task.as_ref().map(|t| t.id);
            (state.recording_handle.take(), output_path, task_id)
        } else {
            (None, None, None)
        };

        if let Some(handle) = handle {
            let runtime = self.app_state.runtime.clone();
            let anchor_name_for_toast = anchor_name.clone();
            let output_path_for_toast = output_path.clone();

            cx.spawn(async move |this, cx| {
                let stop_result = runtime
                    .spawn(async move {
                        let mut h = handle.lock().await;
                        h.stop().await
                    })
                    .await;

                match stop_result {
                    Ok(Ok(_)) => {
                        let _ = this.update(cx, move |page, cx| {
                            if !page.room_states.get(&room_id).is_some_and(|state| {
                                task_id.is_some_and(|id| {
                                    state
                                        .current_task
                                        .as_ref()
                                        .is_some_and(|task| task.id == id)
                                })
                            }) {
                                return;
                            }
                            if let Some(input_path) = output_path_for_toast {
                                let recorded_bytes =
                                    std::fs::metadata(&input_path).map(|m| m.len()).unwrap_or(0);

                                if recorded_bytes == 0 {
                                    tracing::error!(
                                        "❌ 录制已停止但没有媒体数据: room_id={}",
                                        room_id
                                    );
                                    page.push_toast(
                                        ToastLevel::Error,
                                        crate::i18n::format(
                                            "录制已停止但未写入任何数据：{}",
                                            &[format!("{}", anchor_name_for_toast)],
                                        ),
                                    );
                                    if let Some(state) = page.room_states.get_mut(&room_id) {
                                        state.last_error =
                                            Some(crate::i18n::tr("未写入任何数据").to_string());
                                        if let Some(ref mut task) = state.current_task {
                                            task.status =
                                                magekit_shared::types::RecordingTaskStatus::Failed(
                                                    crate::i18n::tr("未写入任何数据").to_string(),
                                                );
                                        }
                                    }
                                    cx.notify();
                                    return;
                                }

                                if page.transcode_target_path(&input_path).is_some() {
                                    page.push_toast(
                                        ToastLevel::Info,
                                        crate::i18n::format(
                                            "录制已停止，后台转码中：{}",
                                            &[format!("{}", anchor_name_for_toast)],
                                        ),
                                    );
                                    page.start_transcode_in_background(room_id, input_path, cx);
                                } else if let Some(state) = page.room_states.get_mut(&room_id) {
                                    state.last_error = None;
                                    if let Some(ref mut task) = state.current_task {
                                        task.status =
                                            magekit_shared::types::RecordingTaskStatus::Completed;
                                    }
                                    page.push_toast(
                                        ToastLevel::Success,
                                        crate::i18n::format(
                                            "录制已停止：{}",
                                            &[format!("{}", anchor_name_for_toast)],
                                        ),
                                    );
                                }
                            }

                            cx.notify();
                        });
                    }
                    Ok(Err(e)) => {
                        let err = e.to_string();
                        tracing::error!("❌ 停止录制失败: room_id={} error={}", room_id, err);
                        let _ = this.update(cx, move |page, cx| {
                            if !page.room_states.get(&room_id).is_some_and(|state| {
                                task_id.is_some_and(|id| {
                                    state
                                        .current_task
                                        .as_ref()
                                        .is_some_and(|task| task.id == id)
                                })
                            }) {
                                return;
                            }
                            page.push_toast(
                                ToastLevel::Error,
                                crate::i18n::format(
                                    "停止录制失败：{}（{}）",
                                    &[format!("{}", anchor_name_for_toast), format!("{}", err)],
                                ),
                            );
                            if let Some(state) = page.room_states.get_mut(&room_id) {
                                state.last_error = Some(err.clone());
                                if let Some(ref mut task) = state.current_task {
                                    task.status =
                                        magekit_shared::types::RecordingTaskStatus::Failed(err);
                                }
                            }
                            cx.notify();
                        });
                    }
                    Err(e) => {
                        let err = e.to_string();
                        tracing::error!("❌ 停止录制任务失败: room_id={} error={}", room_id, err);
                        let _ = this.update(cx, move |page, cx| {
                            if !page.room_states.get(&room_id).is_some_and(|state| {
                                task_id.is_some_and(|id| {
                                    state
                                        .current_task
                                        .as_ref()
                                        .is_some_and(|task| task.id == id)
                                })
                            }) {
                                return;
                            }
                            page.push_toast(
                                ToastLevel::Error,
                                crate::i18n::format(
                                    "停止录制任务失败：{}（{}）",
                                    &[format!("{}", anchor_name_for_toast), format!("{}", err)],
                                ),
                            );
                            if let Some(state) = page.room_states.get_mut(&room_id) {
                                state.last_error = Some(err.clone());
                                if let Some(ref mut task) = state.current_task {
                                    task.status =
                                        magekit_shared::types::RecordingTaskStatus::Failed(err);
                                }
                            }
                            cx.notify();
                        });
                    }
                }
            })
            .detach();
        }

        window.push_notification(
            Notification::info(&crate::i18n::format(
                "正在停止录制：{}",
                &[format!("{}", anchor_name)],
            )),
            cx,
        );
        cx.notify();
    }

    /// 将 worker 的最终状态同步到录制页，不能把进度通道关闭误判为成功。
    fn finalize_monitored_recording(
        &mut self,
        room_id: Uuid,
        status: RecordStatus,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let mut error = error.or_else(|| match &status {
            RecordStatus::Error(message) => Some(message.clone()),
            _ => None,
        });
        let completed = matches!(&status, RecordStatus::Completed | RecordStatus::Stopped);
        let input_path = self.room_states.get(&room_id).and_then(|state| {
            state
                .current_task
                .as_ref()
                .map(|task| task.output_path.clone())
        });
        if error.is_none()
            && completed
            && input_path.as_ref().is_some_and(|path| {
                std::fs::metadata(path)
                    .map(|metadata| metadata.len() == 0)
                    .unwrap_or(true)
            })
        {
            error = Some(crate::i18n::tr("录制结束但未写入媒体数据").to_string());
        }

        if let Some(state) = self.room_states.get_mut(&room_id) {
            state.is_recording = false;
            state.recording_handle = None;
            state.last_error = error.clone();
            if let Some(task) = state.current_task.as_mut() {
                task.status = match error.as_ref() {
                    Some(message) => {
                        magekit_shared::types::RecordingTaskStatus::Failed(message.clone())
                    }
                    None if matches!(&status, RecordStatus::Stopped) => {
                        magekit_shared::types::RecordingTaskStatus::Cancelled
                    }
                    None => magekit_shared::types::RecordingTaskStatus::Completed,
                };
            }
        }

        if let Some(message) = error {
            tracing::error!("❌ 录制失败: room_id={} error={}", room_id, message);
            self.push_toast(
                ToastLevel::Error,
                crate::i18n::format("录制失败：{}", &[format!("{}", message)]),
            );
        } else if completed
            && let Some(input_path) = input_path
            && self.transcode_target_path(&input_path).is_some()
        {
            self.push_toast(ToastLevel::Info, crate::i18n::tr("录制已完成，后台转码中"));
            self.start_transcode_in_background(room_id, input_path, cx);
        }

        cx.notify();
    }

    /// 启动进度监控任务，定期获取录制进度并更新 UI
    fn start_progress_monitor(
        &self,
        room_id: Uuid,
        task_id: Uuid,
        handle: Arc<TokioMutex<RecordingHandle>>,
        cx: &mut Context<Self>,
    ) {
        let runtime = self.app_state.runtime.clone();

        cx.spawn(async move |this, cx| {
            loop {
                // 每秒获取一次进度
                smol::Timer::after(std::time::Duration::from_secs(1)).await;

                // 检查是否还在录制
                let still_recording = this
                    .update(cx, |this, _cx| {
                        this.room_states
                            .get(&room_id)
                            .map(|s| {
                                s.is_recording
                                    && s.current_task
                                        .as_ref()
                                        .is_some_and(|task| task.id == task_id)
                            })
                            .unwrap_or(false)
                    })
                    .ok()
                    .unwrap_or(false);

                if !still_recording {
                    tracing::info!("📊 房间已退出录制状态，进度监控停止: room_id={}", room_id);
                    break;
                }

                // 获取进度；终态到达后继续等待 worker 真正退出，拿到准确错误。
                let handle = handle.clone();
                let progress = runtime
                    .spawn(async move {
                        let mut h = handle.lock().await;
                        let progress = h.get_progress().await;
                        let terminal = progress
                            .as_ref()
                            .map(|value| {
                                matches!(
                                    &value.status,
                                    RecordStatus::Completed
                                        | RecordStatus::Stopped
                                        | RecordStatus::Error(_)
                                )
                            })
                            .unwrap_or(true);
                        let completion_error = if terminal {
                            h.wait_for_completion()
                                .await
                                .err()
                                .map(|error| error.to_string())
                        } else {
                            None
                        };
                        (progress, h.status().clone(), completion_error)
                    })
                    .await;

                match progress {
                    Ok((Some(progress), status, completion_error)) => {
                        if matches!(
                            &status,
                            RecordStatus::Completed
                                | RecordStatus::Stopped
                                | RecordStatus::Error(_)
                        ) {
                            let error = completion_error.or(progress.error.clone());
                            let _ = this.update(cx, |this, cx| {
                                if this.room_states.get(&room_id).is_some_and(|state| {
                                    state.is_recording
                                        && state
                                            .current_task
                                            .as_ref()
                                            .is_some_and(|task| task.id == task_id)
                                }) {
                                    this.finalize_monitored_recording(room_id, status, error, cx);
                                }
                            });
                            break;
                        }

                        let _ = this.update(cx, |this, cx| {
                            let mut first_media = false;
                            if let Some(state) = this.room_states.get_mut(&room_id) {
                                if let Some(ref mut task) = state.current_task
                                    && state.is_recording
                                    && task.id == task_id
                                {
                                    first_media = task.recorded_bytes == 0 && progress.size > 0;
                                    task.recorded_bytes = progress.size;
                                    // 首个输出字节到达后才开始计为录制时长，避免连接阶段显示假 REC 计时。
                                    if progress.size > 0 {
                                        task.duration = chrono::Utc::now()
                                            .signed_duration_since(task.start_time)
                                            .num_seconds()
                                            .max(0)
                                            as u64;
                                    }
                                }
                            }
                            if first_media {
                                tracing::info!("✅ 录制收到首个媒体数据: room_id={}", room_id);
                                this.push_toast(
                                    ToastLevel::Success,
                                    crate::i18n::tr("直播流已连接，开始写入数据"),
                                );
                            }
                            cx.notify();
                        });
                    }
                    Ok((None, status, completion_error)) => {
                        let error = completion_error.or_else(|| match &status {
                            RecordStatus::Error(message) => Some(message.clone()),
                            _ => Some(
                                crate::i18n::tr("Streamlink worker 未返回最终状态").to_string(),
                            ),
                        });
                        tracing::error!(
                            "❌ 录制进度通道关闭: room_id={} error={:?}",
                            room_id,
                            error
                        );
                        let _ = this.update(cx, |this, cx| {
                            if this.room_states.get(&room_id).is_some_and(|state| {
                                state.is_recording
                                    && state
                                        .current_task
                                        .as_ref()
                                        .is_some_and(|task| task.id == task_id)
                            }) {
                                this.finalize_monitored_recording(room_id, status, error, cx);
                            }
                        });
                        break;
                    }
                    Err(e) => {
                        let error =
                            crate::i18n::format("获取录制进度失败：{}", &[format!("{}", e)]);
                        tracing::error!("❌ {}", error);
                        let _ = this.update(cx, |this, cx| {
                            if this.room_states.get(&room_id).is_some_and(|state| {
                                state.is_recording
                                    && state
                                        .current_task
                                        .as_ref()
                                        .is_some_and(|task| task.id == task_id)
                            }) {
                                this.finalize_monitored_recording(
                                    room_id,
                                    RecordStatus::Error(error.clone()),
                                    Some(error),
                                    cx,
                                );
                            }
                        });
                        break;
                    }
                }
            }
        })
        .detach();
    }

    /// 后台自动开始录制（无需 window，用于监控自动触发）
    fn start_recording_background(&mut self, room_id: Uuid, cx: &mut Context<Self>) {
        let room = match self.monitored_rooms.iter().find(|r| r.id == room_id) {
            Some(r) => r.clone(),
            None => return,
        };

        let state = self.room_states.get(&room_id).cloned().unwrap_or_default();

        // 检查是否正在直播
        if state.status != LiveRoomStatus::Live {
            return;
        }

        // 检查是否已在录制
        if state.is_recording {
            return;
        }

        let output_path = self.generate_output_path(&room);
        let live_recorder = self.current_live_recorder();
        let runtime = self.app_state.runtime.clone();
        let app_state = self.app_state.clone();
        let url = room.url.clone();
        let anchor_name = room.anchor_name.clone();
        let soop_hint = self
            .room_states
            .get(&room_id)
            .and_then(|state| state.soop_hint.clone());

        // 创建输出目录
        if let Some(parent) = output_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::error!("❌ 无法创建输出目录: {}", e);
                return;
            }
        }

        let task_id = Uuid::new_v4();
        // 更新状态
        if let Some(state) = self.room_states.get_mut(&room_id) {
            state.is_recording = true;
            state.current_task = Some(RecordingTask {
                id: task_id,
                room_id,
                output_path: output_path.clone(),
                start_time: Utc::now(),
                duration: 0,
                recorded_bytes: 0,
                status: magekit_shared::types::RecordingTaskStatus::Recording,
            });
        }
        cx.notify();

        tracing::info!("🎥 自动开始录制: {} -> {:?}", anchor_name, output_path);

        // 创建录制配置
        let config = RecordConfig {
            output_path_template: output_path.to_string_lossy().to_string(),
            format: self.record_config.record_format.clone(),
            quality: match self.record_config.quality {
                LiveRecordQuality::Original => live_recorder::types::VideoQuality::Original,
                LiveRecordQuality::Blue => live_recorder::types::VideoQuality::Blue,
                LiveRecordQuality::Ultra => live_recorder::types::VideoQuality::Ultra,
                LiveRecordQuality::High => live_recorder::types::VideoQuality::High,
                LiveRecordQuality::Standard => live_recorder::types::VideoQuality::Standard,
            },
            segment_duration: self.record_config.segment_duration,
            retry_count: self.record_config.retry_count,
            reconnect_delay: self.record_config.reconnect_delay,
            timeout: 30,
            max_duration: None,
            include_danmaku: false,
            proxy: None,
            headers: std::collections::HashMap::new(),
        };

        cx.spawn(async move |this, cx| {
            // 读取 Cookie 配置
            let cookies = {
                let config = app_state.config.blocking_read();
                config.advanced.cookies.clone()
            };

            let result = runtime
                .spawn(async move {
                    live_recorder
                        .start_recording_with_cookies_and_hint(&url, config, &cookies, soop_hint)
                        .await
                })
                .await;

            match result {
                Ok(Ok(handle)) => {
                    tracing::info!("✅ 自动录制任务已启动: {}", anchor_name);
                    let _ = this.update(cx, |this, cx| {
                        let current = this.room_states.get(&room_id).is_some_and(|state| {
                            state.is_recording
                                && state
                                    .current_task
                                    .as_ref()
                                    .is_some_and(|task| task.id == task_id)
                        });
                        if current {
                            let handle = Arc::new(TokioMutex::new(handle));
                            let handle_clone = handle.clone();
                            if let Some(state) = this.room_states.get_mut(&room_id) {
                                state.recording_handle = Some(handle);
                            }
                            this.start_progress_monitor(room_id, task_id, handle_clone, cx);
                        } else {
                            tracing::info!("⏹️ 丢弃已过期的自动录制任务: room_id={}", room_id);
                        }
                        cx.notify();
                    });
                }
                Ok(Err(e)) => {
                    let error_msg = crate::i18n::format("自动录制失败: {}", &[format!("{}", e)]);
                    tracing::error!("❌ {}", error_msg);

                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            if !state
                                .current_task
                                .as_ref()
                                .is_some_and(|task| task.id == task_id)
                            {
                                return;
                            }
                            state.is_recording = false;
                            state.current_task = None;
                            state.last_error = Some(error_msg.clone());
                            state.recording_handle = None;
                        }
                        cx.notify();
                    });
                }
                Err(e) => {
                    let error_msg = crate::i18n::format("任务执行失败: {}", &[format!("{}", e)]);
                    tracing::error!("❌ {}", error_msg);

                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            if !state
                                .current_task
                                .as_ref()
                                .is_some_and(|task| task.id == task_id)
                            {
                                return;
                            }
                            state.is_recording = false;
                            state.current_task = None;
                            state.recording_handle = None;
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    fn confirm_remove_room(&mut self, room_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .room_states
            .get(&room_id)
            .is_some_and(|state| state.is_recording)
        {
            window.push_notification(
                Notification::warning(crate::i18n::tr("请先停止录制再删除")),
                cx,
            );
            return;
        }
        let Some(room) = self.monitored_rooms.iter().find(|room| room.id == room_id) else {
            return;
        };
        let name = room.anchor_name.clone();
        let entity = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let entity = entity.clone();
            dialog
                .title(crate::i18n::tr("移除直播间"))
                .child(crate::i18n::format(
                    "移除 {} 的监控配置？已录制的文件会保留。",
                    &[name.clone()],
                ))
                .footer(
                    DialogFooter::new().children([
                        Button::new("cancel-remove-room")
                            .label(crate::i18n::tr("取消"))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                        Button::new("confirm-remove-room")
                            .danger()
                            .label(crate::i18n::tr("移除"))
                            .on_click(move |_, window, cx| {
                                let _ = entity
                                    .update(cx, |this, cx| this.remove_room(room_id, window, cx));
                                window.close_dialog(cx);
                            }),
                    ]),
                )
        });
    }

    /// 删除直播间
    fn remove_room(&mut self, room_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        // 检查是否正在录制
        if let Some(state) = self.room_states.get(&room_id) {
            if state.is_recording {
                window.push_notification(
                    Notification::warning(crate::i18n::tr("请先停止录制再删除")),
                    cx,
                );
                return;
            }
        }

        if let Some(index) = self.monitored_rooms.iter().position(|r| r.id == room_id) {
            self.monitored_rooms.remove(index);
            self.room_states.remove(&room_id);
            self.save_config();

            window.push_notification(Notification::info(crate::i18n::tr("直播间已移除")), cx);
            cx.notify();
        }
    }

    /// 切换监控状态
    fn toggle_monitoring(&mut self, room_id: Uuid, cx: &mut Context<Self>) {
        if let Some(room) = self.monitored_rooms.iter_mut().find(|r| r.id == room_id) {
            room.monitoring_enabled = !room.monitoring_enabled;
            self.save_config();
            cx.notify();
        }
    }

    /// 切换自动录制（仅影响“监控检测到开播后自动开始录制”，不影响手动点击录制）
    fn toggle_auto_record(&mut self, room_id: Uuid, cx: &mut Context<Self>) {
        if let Some(room) = self.monitored_rooms.iter_mut().find(|r| r.id == room_id) {
            room.auto_record = !room.auto_record;
            self.save_config();
            cx.notify();
        }
    }

    /// 刷新房间状态
    fn refresh_room(&mut self, room_id: Uuid, cx: &mut Context<Self>) {
        let room = match self.monitored_rooms.iter().find(|r| r.id == room_id) {
            Some(r) => r.clone(),
            None => return,
        };

        // Ignore repeat activation until this room's current request finishes.
        if let Some(state) = self.room_states.get_mut(&room_id) {
            if state.status == LiveRoomStatus::Checking {
                return;
            }
            state.status = LiveRoomStatus::Checking;
        }
        cx.notify();

        let live_recorder = self.current_live_recorder();
        let runtime = self.app_state.runtime.clone();
        let url = room.url.clone();
        let app_state = self.app_state.clone();

        tracing::info!("🔄 开始刷新房间: {} ({})", room.anchor_name, room_id);

        cx.spawn(async move |this, cx| {
            // 读取 Cookie 配置
            let cookies = {
                let config = app_state.config.blocking_read();
                config.advanced.cookies.clone()
            };

            // 获取完整的流信息（包括封面图和标题）
            let result = runtime
                .spawn(async move {
                    live_recorder
                        .get_stream_info_with_cookies(&url, &cookies)
                        .await
                })
                .await;

            match result {
                Ok(Ok(stream_info)) => {
                    let room_info = stream_info.room;
                    let status = match room_info.status {
                        live_recorder::types::LiveStatus::Live => LiveRoomStatus::Live,
                        live_recorder::types::LiveStatus::Offline => LiveRoomStatus::Offline,
                        live_recorder::types::LiveStatus::Playback => LiveRoomStatus::Playback,
                        live_recorder::types::LiveStatus::Unknown => LiveRoomStatus::Unknown,
                    };
                    let cover_url = room_info.cover_url.clone();
                    let title = if room_info.title.is_empty() {
                        None
                    } else {
                        Some(room_info.title.clone())
                    };
                    let anchor_name = room_info.anchor_name.clone();
                    let is_live = status == LiveRoomStatus::Live;

                    // 调试日志
                    tracing::info!("📦 刷新房间 {} 结果:", room_id);
                    tracing::info!("   - 状态: {:?}", status);
                    tracing::info!("   - 标题: {:?}", title);
                    tracing::info!("   - 封面: {:?}", cover_url);
                    tracing::info!("   - 主播: {}", anchor_name);

                    let _ = this.update(cx, |this, cx| {
                        // 更新运行时状态
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.status = status;
                            state.last_error = None;
                            state.cover_lookup_attempted = true;
                            if !is_live {
                                state.soop_hint = None;
                            }
                            if title.is_some() {
                                state.title = title.clone();
                            }
                            if let Some(source) = &cover_url {
                                cover_image_source(source.clone()).remove_asset(cx);
                                state.cover_url = cover_url.clone();
                            }
                        }

                        // 更新持久化的房间信息（缓存标题、封面等）
                        let mut need_save = false;
                        if let Some(room) =
                            this.monitored_rooms.iter_mut().find(|r| r.id == room_id)
                        {
                            room.last_checked = Some(Utc::now());

                            // 首次获取失败时使用的占位文字必须能被后续成功刷新替换。
                            if is_placeholder_anchor_name(&room.anchor_name)
                                && !anchor_name.trim().is_empty()
                            {
                                tracing::info!(
                                    "📝 更新房间 {} 主播名: {} -> {}",
                                    room_id,
                                    room.anchor_name,
                                    anchor_name
                                );
                                room.anchor_name = anchor_name;
                                need_save = true;
                            }

                            // 同步标题到缓存
                            if title.is_some() && room.cached_title != title {
                                tracing::info!(
                                    "📝 更新房间 {} 缓存标题: {:?} -> {:?}",
                                    room_id,
                                    room.cached_title,
                                    title
                                );
                                room.cached_title = title;
                                need_save = true;
                            }

                            // 只有新封面有效时才替换缓存，保留历史封面作为过渡。
                            if cover_url.is_some() && room.cached_cover_url != cover_url {
                                tracing::info!("🖼️ 更新房间 {} 缓存封面: {:?}", room_id, cover_url);
                                room.cached_cover_url = cover_url.clone();
                                need_save = true;
                            }

                            // 更新最后直播时间
                            if is_live {
                                room.last_live_at = Some(Utc::now());
                                need_save = true;
                            }
                        }

                        // 保存配置（如果有更新）
                        if need_save {
                            tracing::info!("💾 保存房间 {} 的缓存更新", room_id);
                            this.save_config_background(cx);
                        }

                        cx.notify();
                    });
                }
                Ok(Err(e)) => {
                    let error = format!("{}", e);
                    tracing::error!("❌ 刷新房间 {} 失败: {}", room_id, error);
                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.status = LiveRoomStatus::Error(error.clone());
                            state.last_error = Some(error);
                        }
                        cx.notify();
                    });
                }
                Err(e) => {
                    let error = crate::i18n::format("任务执行失败: {}", &[format!("{}", e)]);
                    tracing::error!("❌ 刷新房间 {} 任务失败: {}", room_id, error);
                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.status = LiveRoomStatus::Error(error.clone());
                            state.last_error = Some(error);
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// 显示设置弹窗（可编辑版本）
    fn show_settings_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = self.record_config.clone();
        let this = cx.entity().clone();

        // 格式选项
        let format_options = ["ts", "mkv", "flv", "mp4"];
        let format_index = format_options
            .iter()
            .position(|&f| f == config.record_format.as_str())
            .unwrap_or(0);

        // 质量选项
        let quality_options = [
            LiveRecordQuality::Original,
            LiveRecordQuality::Blue,
            LiveRecordQuality::Ultra,
            LiveRecordQuality::High,
            LiveRecordQuality::Standard,
        ];
        let quality_index = quality_options
            .iter()
            .position(|q| *q == config.quality)
            .unwrap_or(0);

        // 使用 Arc<RwLock> 存储选择状态（因为 Dialog 闭包需要 Fn）
        let selected_format = Arc::new(RwLock::new(format_index));
        let selected_quality = Arc::new(RwLock::new(quality_index));
        let auto_transcode = Arc::new(RwLock::new(config.auto_transcode));
        let auto_record = Arc::new(RwLock::new(config.auto_record));

        // 在 open_dialog 之前创建所有 InputState
        let interval_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("60")
                .default_value(config.check_interval.to_string())
        });
        let segment_input = cx.new(|cx| {
            crate::i18n::input("留空关闭", window, cx).default_value(
                config
                    .segment_duration
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
            )
        });
        let retry_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("3")
                .default_value(config.retry_count.to_string())
        });
        let reconnect_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("10")
                .default_value(config.reconnect_delay.to_string())
        });
        let path_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("record")
                .default_value(config.output_base_path.to_string_lossy().to_string())
        });

        // Clone for closures
        let interval_input_clone = interval_input.clone();
        let segment_input_clone = segment_input.clone();
        let retry_input_clone = retry_input.clone();
        let reconnect_input_clone = reconnect_input.clone();
        let path_input_clone = path_input.clone();
        let selected_format_clone = selected_format.clone();
        let selected_quality_clone = selected_quality.clone();
        let auto_transcode_clone = auto_transcode.clone();
        let auto_record_clone = auto_record.clone();

        window.open_dialog(cx, move |dialog, _window, cx| {
            let selected_format = selected_format.clone();
            let selected_quality = selected_quality.clone();
            let auto_transcode = auto_transcode.clone();
            let auto_record = auto_record.clone();
            let theme = cx.theme();
            let border_color = theme.border;
            let muted_fg = theme.muted_foreground;

            dialog
                .title(crate::i18n::tr("录制设置"))
                .overlay(true)
                .h(px(580.0))
                .w(px(500.0))
                .child({
                    let selected_format = selected_format.clone();
                    let selected_quality = selected_quality.clone();
                    let auto_transcode = auto_transcode.clone();

                    div()
                        .id("settings-scroll")
                        .overflow_y_scroll()
                        .p_5()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_5()
                                // === 输出设置分组 ===
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_4()
                                        .p_4()
                                        .rounded_lg()
                                        .border_1()
                                        .border_color(border_color)
                                        // 分组标题
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .pb_2()
                                                .border_b_1()
                                                .border_color(border_color)
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .child(
                                                            gpui_kit::component::Icon::new(
                                                                gpui_kit::component::IconName::Folder,
                                                            )
                                                            .size(px(16.0))
                                                            .text_color(theme.foreground),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_base()
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .child(crate::i18n::tr("输出设置")),
                                                        )
                                                )
                                        )
                                        // 录制格式
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .child(crate::i18n::tr("录制格式"))
                                                )
                                                .child({
                                                    let selected_format = selected_format.clone();
                                                    let current_idx = *selected_format.read();
                                                    RadioGroup::horizontal("format-radio")
                                                        .children(["TS", "MKV", "FLV", "MP4"])
                                                        .selected_index(Some(current_idx))
                                                        .on_click({
                                                            let selected_format = selected_format.clone();
                                                            move |idx: &usize, _window, _cx| {
                                                                *selected_format.write() = *idx;
                                                            }
                                                        })
                                                })
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(muted_fg)
                                                        .child(crate::i18n::tr("TS 最稳定，MP4 兼容性最好"))
                                                )
                                        )
                                        // 视频质量
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .child(crate::i18n::tr("视频质量"))
                                                )
                                                .child({
                                                    let selected_quality = selected_quality.clone();
                                                    let current_idx = *selected_quality.read();
                                                    RadioGroup::horizontal("quality-radio")
                                                        .children([crate::i18n::tr("原画"), crate::i18n::tr("蓝光"), crate::i18n::tr("超清"), crate::i18n::tr("高清"), crate::i18n::tr("标清")])
                                                        .selected_index(Some(current_idx))
                                                        .on_click({
                                                            let selected_quality = selected_quality.clone();
                                                            move |idx: &usize, _window, _cx| {
                                                                *selected_quality.write() = *idx;
                                                            }
                                                        })
                                                })
                                        )
                                        // 自动转码开关
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .pt_2()
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .gap_1()
                                                        .child(
                                                            div()
                                                                .text_sm()
                                                                .font_weight(FontWeight::MEDIUM)
                                                                .child(crate::i18n::tr("录制后自动转码"))
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(muted_fg)
                                                                .child(crate::i18n::tr("录制完成后自动转为 MP4 格式"))
                                                        )
                                                )
                                                .child({
                                                    let auto_transcode = auto_transcode.clone();
                                                    let checked = *auto_transcode.read();
                                                    Switch::new("auto-transcode")
                                                        .checked(checked)
                                                        .on_click({
                                                            let auto_transcode = auto_transcode.clone();
                                                            move |checked: &bool, _window, _cx| {
                                                                *auto_transcode.write() = *checked;
                                                            }
                                                        })
                                                })
                                        )
                                )
                                // === 监控设置分组 ===
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_4()
                                        .p_4()
                                        .rounded_lg()
                                        .border_1()
                                        .border_color(border_color)
                                        // 分组标题
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .pb_2()
                                                .border_b_1()
                                                .border_color(border_color)
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .child(
                                                            gpui_kit::component::Icon::new(
                                                                gpui_kit::component::IconName::Search,
                                                            )
                                                            .size(px(16.0))
                                                            .text_color(theme.foreground),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_base()
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .child(crate::i18n::tr("监控设置")),
                                                        )
                                                )
                                        )
                                        // 检测间隔
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .gap_1()
                                                        .child(
                                                            div()
                                                                .text_sm()
                                                                .font_weight(FontWeight::MEDIUM)
                                                                .child(crate::i18n::tr("检测间隔"))
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(muted_fg)
                                                                .child(crate::i18n::tr("检查直播间状态的时间间隔（最小 10 秒）"))
                                                        )
                                                )
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .child(
                                                            div()
                                                                .w(px(80.0))
                                                                .child(Input::new(&interval_input).small())
                                                        )
                                                        .child(
                                                            div()
                                                                .text_sm()
                                                                .text_color(muted_fg)
                                                                .child(crate::i18n::tr("秒"))
                                                        )
                                                )
                                        )
                                        // 自动录制（全局开关）
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .gap_1()
                                                        .child(
                                                            div()
                                                                .text_sm()
                                                                .font_weight(FontWeight::MEDIUM)
                                                                .child(crate::i18n::tr("自动录制")),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(muted_fg)
                                                                .child(crate::i18n::tr("检测到开播时自动开始录制（与监控分离）")),
                                                        ),
                                                )
                                                .child({
                                                    let auto_record = auto_record.clone();
                                                    let checked = *auto_record.read();
                                                    Switch::new("record-auto-record")
                                                        .checked(checked)
                                                        .on_click({
                                                            let auto_record = auto_record.clone();
                                                            move |checked: &bool, _window, _cx| {
                                                                *auto_record.write() = *checked;
                                                            }
                                                        })
                                                }),
                                        )
                                        // 重试设置
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .child(crate::i18n::tr("重试/超时设置"))
                                                )
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap_3()
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .gap_1()
                                                                .child(
                                                                    div()
                                                                        .w(px(50.0))
                                                                        .child(Input::new(&retry_input).small())
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_sm()
                                                                        .text_color(muted_fg)
                                                                        .child(crate::i18n::tr("次"))
                                                                )
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .gap_1()
                                                                .child(
                                                                    div()
                                                                        .text_sm()
                                                                        .text_color(muted_fg)
                                                                        .child(crate::i18n::tr("超时"))
                                                                )
                                                                .child(
                                                                    div()
                                                                        .w(px(50.0))
                                                                        .child(Input::new(&reconnect_input).small())
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_sm()
                                                                        .text_color(muted_fg)
                                                                        .child(crate::i18n::tr("秒"))
                                                                )
                                                        )
                                                )
                                        )
                                )
                                // === 高级设置分组 ===
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_4()
                                        .p_4()
                                        .rounded_lg()
                                        .border_1()
                                        .border_color(border_color)
                                        // 分组标题
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .pb_2()
                                                .border_b_1()
                                                .border_color(border_color)
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .child(
                                                            gpui_kit::component::Icon::new(
                                                                gpui_kit::component::IconName::Settings2,
                                                            )
                                                            .size(px(16.0))
                                                            .text_color(theme.foreground),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_base()
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .child(crate::i18n::tr("高级设置")),
                                                        )
                                                )
                                        )
                                        // 分段录制
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .gap_1()
                                                        .child(
                                                            div()
                                                                .text_sm()
                                                                .font_weight(FontWeight::MEDIUM)
                                                                .child(crate::i18n::tr("分段录制"))
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(muted_fg)
                                                                .child(crate::i18n::tr("按固定时长分割文件（留空关闭）"))
                                                        )
                                                )
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .child(
                                                            div()
                                                                .w(px(80.0))
                                                                .child(Input::new(&segment_input).small())
                                                        )
                                                        .child(
                                                            div()
                                                                .text_sm()
                                                                .text_color(muted_fg)
                                                                .child(crate::i18n::tr("秒"))
                                                        )
                                                )
                                        )
                                        // 保存路径
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .child(crate::i18n::tr("保存路径"))
                                                )
                                                .child(Input::new(&path_input).small())
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(muted_fg)
                                                        .child(crate::i18n::tr("相对于下载路径的保存目录"))
                                                )
                                        )
                                )
                        )
                })
                .footer({
                    let interval_input = interval_input_clone.clone();
                    let segment_input = segment_input_clone.clone();
                    let retry_input = retry_input_clone.clone();
                    let reconnect_input = reconnect_input_clone.clone();
                    let path_input = path_input_clone.clone();
                    let selected_format = selected_format_clone.clone();
                    let selected_quality = selected_quality_clone.clone();
                    let auto_transcode = auto_transcode_clone.clone();
                    let auto_record = auto_record_clone.clone();
                    let this = this.clone();

                    DialogFooter::new().children([
                        Button::new("save-settings")
                            .primary()
                            .label(crate::i18n::tr("保存"))
                            .on_click(move |_event, window, cx| {
                                // 从 RadioGroup 获取选择
                                let format_options = ["ts", "mkv", "flv", "mp4"];
                                let quality_options = [
                                    LiveRecordQuality::Original,
                                    LiveRecordQuality::Blue,
                                    LiveRecordQuality::Ultra,
                                    LiveRecordQuality::High,
                                    LiveRecordQuality::Standard,
                                ];
                                let format_idx = *selected_format.read();
                                let quality_idx = *selected_quality.read();
                                let transcode = *auto_transcode.read();
                                let global_auto_record = *auto_record.read();
                                let format = format_options[format_idx].to_string();
                                let quality = quality_options[quality_idx].clone();
                                // 从输入框读取值
                                let interval = interval_input.read(cx).text().to_string()
                                    .trim().parse::<u64>().unwrap_or(60).max(10);
                                let segment_str = segment_input.read(cx).text().to_string();
                                let segment = segment_str.trim().parse::<u64>().ok();
                                let retry = retry_input.read(cx).text().to_string()
                                    .trim().parse::<u32>().unwrap_or(3).max(1);
                                let reconnect = reconnect_input.read(cx).text().to_string()
                                    .trim().parse::<u64>().unwrap_or(10).clamp(1, 60);
                                let path = path_input.read(cx).text().to_string().trim().to_string();
                                // 更新配置
                                let _ = this.update(cx, |page, cx| {
                                    page.record_config.record_format = format;
                                    page.record_config.quality = quality;
                                    page.record_config.auto_transcode = transcode;
                                    page.record_config.auto_record = global_auto_record;
                                    page.record_config.check_interval = interval;
                                    page.record_config.segment_duration = segment;
                                    page.record_config.retry_count = retry;
                                    page.record_config.reconnect_delay = reconnect;
                                    page.record_config.output_base_path = PathBuf::from(
                                        if path.is_empty() {
                                            "record".to_string()
                                        } else {
                                            path
                                        }
                                    );
                                    page.save_record_config();
                                    cx.notify();
                                });
                                window.push_notification(
                                    Notification::success(crate::i18n::tr("设置已保存")),
                                    cx,
                                );
                                window.close_dialog(cx);
                            }),
                        Button::new("close-settings")
                            .label(crate::i18n::tr("取消"))
                            .on_click(|_event, window, cx| {
                                window.close_dialog(cx);
                            }),
                    ])
                })
        });
    }

    /// 保存录制配置到 AppConfig
    fn save_record_config(&self) {
        // 直接使用已有的 save_config 方法
        self.save_config();
    }

    /// 打开直播间（在默认浏览器中）
    fn open_room_in_browser(&self, room: &MonitoredRoom) {
        if let Err(e) = open::that(&room.url) {
            tracing::error!("❌ 无法打开浏览器: {}", e);
        }
    }

    fn room_menu_button(&self, room: &MonitoredRoom, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
        let entity = cx.entity().downgrade();
        let id = room.id;
        let room_url = room.url.clone();
        let room_label = format!("#{}", room.room_id);
        let monitoring = room.monitoring_enabled;
        let automatic = room.auto_record;
        let recording = self.room_states.get(&id).is_some_and(|s| s.is_recording);
        let checking = self
            .room_states
            .get(&id)
            .is_some_and(|s| s.status == LiveRoomStatus::Checking);
        let global_auto = self.record_config.auto_record;
        Button::new(SharedString::from(format!("room-menu-{id}")))
            .ghost()
            .small()
            .icon(gpui_kit::assets::IconName::Settings2)
            .tooltip(crate::i18n::tr("房间设置"))
            .accessibility_label(crate::i18n::format(
                "房间设置: {}",
                &[room.anchor_name.clone()],
            ))
            .dropdown_menu(move |menu, _, _| {
                let monitor_entity = entity.clone();
                let auto_entity = entity.clone();
                let refresh_entity = entity.clone();
                let remove_entity = entity.clone();
                let open_url = room_url.clone();
                menu.item(PopupMenuItem::label(room_label.clone()))
                    .item(
                        PopupMenuItem::new(crate::i18n::tr("打开"))
                            .icon(gpui_kit::assets::IconName::ExternalLink)
                            .on_click(move |_, _, _| {
                                if let Err(error) = open::that(&open_url) {
                                    tracing::error!("Failed to open room: {error}");
                                }
                            }),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new(crate::i18n::tr("监控"))
                            .checked(monitoring)
                            .on_click(move |_, _, cx| {
                                let _ = monitor_entity
                                    .update(cx, |this, cx| this.toggle_monitoring(id, cx));
                            }),
                    )
                    .item(
                        PopupMenuItem::new(crate::i18n::tr("自动录制"))
                            .checked(automatic)
                            .on_click(move |_, _, cx| {
                                let _ = auto_entity
                                    .update(cx, |this, cx| this.toggle_auto_record(id, cx));
                            }),
                    )
                    .when(!global_auto, |menu| {
                        menu.item(PopupMenuItem::label(crate::i18n::tr("自动录制:全局关")))
                    })
                    .separator()
                    .item(
                        PopupMenuItem::new(crate::i18n::tr("刷新"))
                            .icon(gpui_kit::assets::IconName::RefreshCw)
                            .disabled(checking)
                            .on_click(move |_, _, cx| {
                                let _ =
                                    refresh_entity.update(cx, |this, cx| this.refresh_room(id, cx));
                            }),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new(crate::i18n::tr("移除"))
                            .icon(gpui_kit::assets::IconName::Trash)
                            .disabled(recording)
                            .on_click(move |_, window, cx| {
                                let _ = remove_entity.update(cx, |this, cx| {
                                    this.confirm_remove_room(id, window, cx)
                                });
                            }),
                    )
            })
    }

    fn render_room_card(
        &self,
        room: &MonitoredRoom,
        list: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let room_id = room.id;
        let state = self.room_states.get(&room_id).cloned().unwrap_or_default();
        let is_live = state.status == LiveRoomStatus::Live;
        let is_recording = state.is_recording;
        let anchor_name = if is_placeholder_anchor_name(&room.anchor_name) {
            crate::i18n::text(&room.anchor_name)
        } else {
            room.anchor_name.clone()
        };
        let platform = match room.platform.as_str() {
            "douyin" | "抖音直播" => crate::i18n::tr("抖音"),
            "bilibili" | "B站直播" => crate::i18n::tr("B站"),
            "huya" | "虎牙直播" => crate::i18n::tr("虎牙"),
            "douyu" | "斗鱼直播" => crate::i18n::tr("斗鱼"),
            "kuaishou" | "快手直播" => crate::i18n::tr("快手"),
            "soop" | "SOOP" => "SOOP",
            _ => &room.platform,
        };
        let (status, variant) = if is_recording {
            (
                state
                    .current_task
                    .as_ref()
                    .filter(|t| t.recorded_bytes > 0)
                    .map(|t| {
                        format!(
                            "{} {}",
                            crate::i18n::tr("录制中"),
                            Self::format_record_duration(t.duration)
                        )
                    })
                    .unwrap_or_else(|| crate::i18n::tr("连接中").into()),
                TagVariant::Danger,
            )
        } else {
            (
                crate::i18n::text(state.status.display_name()),
                match state.status {
                    LiveRoomStatus::Live => TagVariant::Success,
                    LiveRoomStatus::Checking => TagVariant::Info,
                    LiveRoomStatus::Error(_) => TagVariant::Danger,
                    LiveRoomStatus::Playback => TagVariant::Warning,
                    _ => TagVariant::Secondary,
                },
            )
        };
        let placeholder_color = theme.muted_foreground;
        let placeholder = move || {
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(placeholder_color)
                .child(Icon::new(gpui_kit::assets::IconName::Radio).size(px(28.0)))
                .child(div().text_xs().child(crate::i18n::tr("暂无封面")))
                .into_any_element()
        };
        let cover = div()
            .relative()
            .flex_shrink_0()
            .overflow_hidden()
            .bg(theme.muted)
            .when(list, |el| el.w(px(128.0)).h(px(76.0)))
            .when(!list, |el| el.w_full().aspect_ratio(16.0 / 9.0))
            .when_some(state.cover_url.clone(), |el, source| {
                el.child(
                    img(cover_image_source(source))
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .object_fit(ObjectFit::ScaleDown)
                        .with_loading(|| Spinner::new().into_any_element())
                        .with_fallback(placeholder),
                )
            })
            .when(state.cover_url.is_none(), |el| el.child(placeholder()))
            .when(!list, |el| {
                el.child(
                    h_flex()
                        .absolute()
                        .top_2()
                        .left_2()
                        .right_2()
                        .justify_between()
                        .gap_1()
                        .child(Tag::secondary().small().child(platform.to_string()))
                        .child(
                            Tag::new()
                                .with_variant(variant)
                                .small()
                                .child(status.clone()),
                        ),
                )
            });
        let primary =
            if is_recording {
                Button::new(SharedString::from(format!("stop-{room_id}")))
                    .danger()
                    .small()
                    .icon(gpui_kit::assets::IconName::Square)
                    .label(crate::i18n::tr("停止"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.stop_recording(room_id, window, cx)
                    }))
            } else if is_live {
                Button::new(SharedString::from(format!("start-{room_id}")))
                    .primary()
                    .small()
                    .icon(gpui_kit::assets::IconName::Circle)
                    .label(crate::i18n::tr("录制"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.start_recording(room_id, window, cx)
                    }))
            } else {
                let url = room.url.clone();
                Button::new(SharedString::from(format!("open-{room_id}")))
                    .outline()
                    .small()
                    .icon(gpui_kit::assets::IconName::ExternalLink)
                    .label(crate::i18n::tr("打开"))
                    .on_click(move |_, _, _| {
                        if let Err(error) = open::that(&url) {
                            tracing::error!("Failed to open room: {error}");
                        }
                    })
            };
        let subtitle = state
            .title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .map(|title| format!("{title} · #{}", room.room_id))
            .unwrap_or_else(|| format!("#{}", room.room_id));
        let footer = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .p_3()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(anchor_name),
                            )
                            .when(
                                room.monitoring_enabled
                                    && room.auto_record
                                    && self.record_config.auto_record,
                                |el| {
                                    el.child(
                                        Tag::primary()
                                            .outline()
                                            .small()
                                            .child(crate::i18n::tr("自动")),
                                    )
                                },
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .truncate()
                            .child(subtitle),
                    )
                    .when(list, |el| {
                        el.child(
                            h_flex()
                                .gap_2()
                                .child(Tag::secondary().small().child(platform.to_string()))
                                .child(
                                    Tag::new()
                                        .with_variant(variant)
                                        .small()
                                        .child(status.clone()),
                                ),
                        )
                    }),
            )
            .child(primary)
            .child(self.room_menu_button(room, cx));
        GroupBox::new()
            .id(SharedString::from(format!("room-card-{room_id}")))
            .fill()
            .min_w_0()
            .content_style(
                div()
                    .p_0()
                    .gap_0()
                    .border_1()
                    .border_color(theme.border)
                    .overflow_hidden()
                    .style()
                    .clone(),
            )
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .when(!list, |el| el.flex_col())
                    .child(cover)
                    .child(footer),
            )
            .when_some(state.last_error.as_ref(), |el, error| {
                el.child(
                    div()
                        .px_3()
                        .pb_2()
                        .text_xs()
                        .text_color(theme.danger)
                        .truncate()
                        .child(crate::i18n::text(error)),
                )
            })
    }
}

impl Render for RecordingPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
        use gpui_kit::component::tab::{Tab, TabBar};
        if let Some(error) = self.last_add_error.take() {
            window.push_notification(
                Notification::error(crate::i18n::format("添加失败: {}", &[error])),
                cx,
            );
        }
        while let Some((level, message)) = self.pending_toasts.pop() {
            window.push_notification(
                match level {
                    ToastLevel::Success => Notification::success(&message),
                    ToastLevel::Info => Notification::info(&message),
                    ToastLevel::Warning => Notification::warning(&message),
                    ToastLevel::Error => Notification::error(&message),
                },
                cx,
            );
        }
        let counts = [
            RoomFilter::All,
            RoomFilter::Recording,
            RoomFilter::Live,
            RoomFilter::Offline,
        ]
        .map(|filter| {
            self.monitored_rooms
                .iter()
                .filter(|room| room_matches_filter(self.room_states.get(&room.id), filter))
                .count()
        });
        let query = self.room_search.read(cx).value().to_lowercase();
        let mut rooms: Vec<_> = self
            .monitored_rooms
            .iter()
            .filter(|room| {
                let state = self.room_states.get(&room.id);
                room_matches_filter(state, self.room_filter)
                    && room_matches_query(
                        room,
                        state.and_then(|state| state.title.as_deref()),
                        &query,
                    )
            })
            .cloned()
            .collect();
        rooms.sort_by(|a, b| {
            match self.room_sort {
                RoomSort::Name => a
                    .anchor_name
                    .to_lowercase()
                    .cmp(&b.anchor_name.to_lowercase()),
                RoomSort::Activity => {
                    let priority = |room: &MonitoredRoom| {
                        self.room_states.get(&room.id).map_or(0, |s| {
                            if s.is_recording {
                                2
                            } else if s.status == LiveRoomStatus::Live {
                                1
                            } else {
                                0
                            }
                        })
                    };
                    priority(b).cmp(&priority(a)).then_with(|| {
                        b.last_live_at
                            .or(b.last_checked)
                            .unwrap_or(b.added_at)
                            .cmp(&a.last_live_at.or(a.last_checked).unwrap_or(a.added_at))
                    })
                }
            }
            .then_with(|| a.id.cmp(&b.id))
        });
        let width = window.bounds().size.width.as_f32();
        let columns = if self.room_view == RoomView::List {
            1
        } else if width >= 1440.0 {
            4
        } else if width >= 1100.0 {
            3
        } else if width >= 760.0 {
            2
        } else {
            1
        };
        let sort_entity = cx.entity().downgrade();
        let list = self.room_view == RoomView::List;
        let filtered_count = rooms.len();
        v_flex()
            .id("recording-page")
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .flex_shrink_0()
                    .flex_wrap()
                    .gap_3()
                    .px_6()
                    .py_4()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        h_flex()
                            .gap_3()
                            .flex_1()
                            .min_w(px(180.0))
                            .child(
                                div()
                                    .text_size(px(22.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(crate::i18n::tr("直播录制")),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(crate::i18n::format(
                                        "{} 个房间",
                                        &[counts[0].to_string()],
                                    )),
                            ),
                    )
                    .child(
                        div().w(px(250.0)).child(
                            Input::new(&self.room_search)
                                .prefix(Icon::new(gpui_kit::assets::IconName::Search).size_4())
                                .cleanable(true),
                        ),
                    )
                    .child(
                        Button::new("recording-settings")
                            .outline()
                            .icon(gpui_kit::assets::IconName::Settings2)
                            .accessibility_label(crate::i18n::tr("录制设置"))
                            .tooltip(crate::i18n::tr("录制设置"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_settings_dialog(window, cx)
                            })),
                    )
                    .child(
                        Button::new("add-room")
                            .primary()
                            .icon(IconName::Plus)
                            .label(crate::i18n::tr("添加直播间"))
                            .disabled(self.is_loading)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_add_room_dialog(window, cx)
                            })),
                    ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .flex_wrap()
                    .justify_between()
                    .gap_2()
                    .px_6()
                    .py_3()
                    .child(
                        TabBar::new("room-filter-tabs")
                            .pill()
                            .small()
                            .selected_index(self.room_filter as usize)
                            .children(
                                ["全部", "录制中", "直播中", "未开播"]
                                    .into_iter()
                                    .enumerate()
                                    .map(|(i, title)| {
                                        Tab::new().label(format!(
                                            "{}  {}",
                                            crate::i18n::tr(title),
                                            counts[i]
                                        ))
                                    }),
                            )
                            .on_click(cx.listener(|this, i: &usize, _, cx| {
                                this.room_filter = [
                                    RoomFilter::All,
                                    RoomFilter::Recording,
                                    RoomFilter::Live,
                                    RoomFilter::Offline,
                                ][*i];
                                cx.notify();
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new("room-sort")
                                    .ghost()
                                    .small()
                                    .icon(IconName::ChevronDown)
                                    .label(crate::i18n::tr(
                                        if self.room_sort == RoomSort::Activity {
                                            "最近活动"
                                        } else {
                                            "主播名称"
                                        },
                                    ))
                                    .dropdown_menu(move |menu, _, _| {
                                        let by_activity = sort_entity.clone();
                                        let by_name = sort_entity.clone();
                                        menu.item(
                                            PopupMenuItem::new(crate::i18n::tr("最近活动"))
                                                .on_click(move |_, _, cx| {
                                                    let _ = by_activity.update(cx, |this, cx| {
                                                        this.room_sort = RoomSort::Activity;
                                                        cx.notify();
                                                    });
                                                }),
                                        )
                                        .item(
                                            PopupMenuItem::new(crate::i18n::tr("主播名称"))
                                                .on_click(move |_, _, cx| {
                                                    let _ = by_name.update(cx, |this, cx| {
                                                        this.room_sort = RoomSort::Name;
                                                        cx.notify();
                                                    });
                                                }),
                                        )
                                    }),
                            )
                            .child(
                                Button::new("room-grid-view")
                                    .small()
                                    .icon(gpui_kit::assets::IconName::LayoutGrid)
                                    .when(!list, |b| b.primary())
                                    .when(list, |b| b.ghost())
                                    .accessibility_label(crate::i18n::tr("网格视图"))
                                    .tooltip(crate::i18n::tr("网格视图"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.room_view = RoomView::Grid;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("room-list-view")
                                    .small()
                                    .icon(gpui_kit::assets::IconName::List)
                                    .when(list, |b| b.primary())
                                    .when(!list, |b| b.ghost())
                                    .accessibility_label(crate::i18n::tr("列表视图"))
                                    .tooltip(crate::i18n::tr("列表视图"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.room_view = RoomView::List;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .id("room-results-scroll")
                    .track_scroll(&self.room_scroll)
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_6()
                    .pb_6()
                    .when(!rooms.is_empty(), |el| {
                        el.child(
                            div()
                                .grid()
                                .grid_cols(columns)
                                .gap_4()
                                .items_start()
                                .children(rooms.iter().map(|room| {
                                    self.render_room_card(room, list, cx).into_any_element()
                                })),
                        )
                    })
                    .when(rooms.is_empty(), |el| {
                        el.child(
                            Empty::new()
                                .py_12()
                                .header(
                                    EmptyHeader::new()
                                        .media(
                                            EmptyMedia::new().child(Icon::new(
                                                gpui_kit::assets::IconName::Radio,
                                            )),
                                        )
                                        .title(EmptyTitle::new().child(crate::i18n::tr(
                                            if counts[0] == 0 {
                                                "暂无直播间"
                                            } else {
                                                "没有匹配的直播间"
                                            },
                                        )))
                                        .description(EmptyDescription::new().child(
                                            crate::i18n::tr(if counts[0] == 0 {
                                                "添加直播间，开始监控与录制"
                                            } else {
                                                "尝试其他关键词或状态筛选"
                                            }),
                                        )),
                                )
                                .when(counts[0] == 0, |empty| {
                                    empty.child(
                                        EmptyContent::new().child(
                                            Button::new("add-first-room")
                                                .primary()
                                                .icon(IconName::Plus)
                                                .label(crate::i18n::tr("添加直播间"))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.show_add_room_dialog(window, cx)
                                                })),
                                        ),
                                    )
                                }),
                        )
                    }),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .justify_between()
                    .px_6()
                    .py_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(crate::i18n::format(
                        "显示 {} / {} 个房间 · {} 个录制中",
                        &[
                            filtered_count.to_string(),
                            counts[0].to_string(),
                            counts[1].to_string(),
                        ],
                    ))
                    .child(crate::i18n::format(
                        "检查间隔: {} 秒",
                        &[self.record_config.check_interval.to_string()],
                    )),
            )
    }
}

fn room_matches_filter(state: Option<&RuntimeRoomState>, filter: RoomFilter) -> bool {
    match filter {
        RoomFilter::All => true,
        RoomFilter::Recording => state.is_some_and(|state| state.is_recording),
        RoomFilter::Live => {
            state.is_some_and(|state| state.status == LiveRoomStatus::Live && !state.is_recording)
        }
        RoomFilter::Offline => state
            .is_some_and(|state| state.status == LiveRoomStatus::Offline && !state.is_recording),
    }
}

fn room_matches_query(room: &MonitoredRoom, title: Option<&str>, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    let aliases = match room.platform.as_str() {
        "douyin" => "抖音",
        "bilibili" => "B站 哔哩哔哩",
        "huya" => "虎牙",
        "douyu" => "斗鱼",
        "kuaishou" => "快手",
        _ => "",
    };
    query.is_empty()
        || [
            &room.anchor_name,
            &room.platform,
            &room.room_id,
            &room.url,
            title.unwrap_or(""),
            aliases,
        ]
        .into_iter()
        .any(|value| value.to_lowercase().contains(query.as_str()))
}

fn recording_platform(value: &str) -> Option<&'static str> {
    let parsed = url::Url::parse(value).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    match magekit_shared::platform_from_url(value) {
        Some(platform @ ("douyin" | "bilibili" | "huya" | "douyu" | "kuaishou")) => Some(platform),
        Some("soop" | "soop_global" | "sooplive" | "sooplive_global") => Some("soop"),
        // Streamlink 支持更多站点；合法未知 host 交给实际后端解析。
        _ => Some("unknown"),
    }
}

#[cfg(test)]
mod review_tests {
    use super::{
        LiveRoomStatus, MonitoredRoom, RoomFilter, RuntimeRoomState, recording_platform,
        room_matches_filter, room_matches_query,
    };

    #[test]
    fn room_search_matches_names_titles_ids_and_platform_aliases() {
        let room = MonitoredRoom::new(
            "https://live.bilibili.com/128".into(),
            "bilibili".into(),
            "128".into(),
            "Aero 山野".into(),
        );
        for query in ["aero", " AERO ", "山野", "B站", "128", "forest"] {
            assert!(
                room_matches_query(&room, Some("Forest radio"), query),
                "{query}"
            );
        }
        assert!(!room_matches_query(&room, None, "another room"));
    }

    #[test]
    fn room_filters_keep_unknown_distinct_and_recording_authoritative() {
        assert!(room_matches_filter(None, RoomFilter::All));
        assert!(!room_matches_filter(None, RoomFilter::Offline));
        let mut state = RuntimeRoomState::default();
        state.status = LiveRoomStatus::Offline;
        assert!(room_matches_filter(Some(&state), RoomFilter::Offline));
        state.is_recording = true;
        assert!(room_matches_filter(Some(&state), RoomFilter::Recording));
        assert!(!room_matches_filter(Some(&state), RoomFilter::Offline));
        state.status = LiveRoomStatus::Live;
        assert!(!room_matches_filter(Some(&state), RoomFilter::Live));
        state.is_recording = false;
        assert!(room_matches_filter(Some(&state), RoomFilter::Live));
    }

    #[test]
    fn recording_urls_use_parsed_hosts() {
        assert_eq!(
            recording_platform("https://live.bilibili.com/123"),
            Some("bilibili")
        );
        for url in [
            "",
            "file:///tmp/huya",
            "https://name:secret@example.com/room",
        ] {
            assert_eq!(recording_platform(url), None);
        }
        for url in [
            "https://www.twitch.tv/example",
            "https://huya.com.evil.example/1",
            "https://evil.example/?next=live.bilibili.com",
        ] {
            assert_eq!(recording_platform(url), Some("unknown"));
        }
    }
}
