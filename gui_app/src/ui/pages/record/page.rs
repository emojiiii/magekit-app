//! 录制页面主组件 - 完整重写版本
//!
//! 功能：
//! - 添加、删除、监控直播间
//! - 自动检测平台并验证支持性
//! - 持久化保存监控的房间到 AppConfig
//! - 正确的文件路径格式: {base}/record/{平台}/{主播名}/{主播名}_{时间}.ts
//! - 支持 TS 录制和录制后转码

use crate::app::AppState;
use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::ActiveTheme;
use gpui_component::WindowExt;
use gpui_component::notification::Notification;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_component::v_flex;
use gpui_component::Sizable;
use live_recorder::{LiveRecorder, RecordConfig, error::RecorderError};
use magekit_shared::types::{
    MonitoredRoom, LiveRoomStatus, LiveRecordConfig, LiveRecordQuality, RecordingTask
};
use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;
use uuid::Uuid;
use chrono::Utc;
use parking_lot::RwLock;


/// 运行时房间状态（用于 UI 显示）
#[derive(Clone)]
struct RuntimeRoomState {
    status: LiveRoomStatus,
    is_recording: bool,
    current_task: Option<RecordingTask>,
    last_error: Option<String>,
    /// 封面图 URL
    cover_url: Option<String>,
    /// 直播标题
    title: Option<String>,
}

impl Default for RuntimeRoomState {
    fn default() -> Self {
        Self {
            status: LiveRoomStatus::Unknown,
            is_recording: false,
            current_task: None,
            last_error: None,
            cover_url: None,
            title: None,
        }
    }
}

/// 录制页面组件
pub struct RecordingPage {
    app_state: Arc<AppState>,
    live_recorder: Arc<LiveRecorder>,
    url_input: Entity<InputState>,
    
    /// 持久化的监控房间列表（从 AppConfig 加载）
    monitored_rooms: Vec<MonitoredRoom>,
    /// 运行时状态（不持久化）
    room_states: HashMap<Uuid, RuntimeRoomState>,
    /// 录制配置（从 AppConfig 加载）
    record_config: LiveRecordConfig,
    
    /// UI 状态
    is_loading: bool,
    monitoring_enabled: bool,
    
    /// 监控定时器 ID
    check_task_running: Arc<RwLock<bool>>,
}

impl RecordingPage {
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 创建 URL 输入框
        let url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入直播间地址（支持抖音、B站、虎牙、斗鱼、快手、SOOP）")
        });

        // 创建 live_recorder 实例
        let live_recorder = Arc::new(LiveRecorder::new());

        // 从配置加载数据
        let config = app_state.config.blocking_read().clone();
        let monitored_rooms = config.monitored_rooms.clone();
        let record_config = config.live_record.clone();
        
        // 初始化运行时状态
        let mut room_states = HashMap::new();
        for room in &monitored_rooms {
            room_states.insert(room.id, RuntimeRoomState::default());
        }

        let page = Self {
            app_state,
            live_recorder,
            url_input,
            monitored_rooms,
            room_states,
            record_config,
            is_loading: false,
            monitoring_enabled: true,
            check_task_running: Arc::new(RwLock::new(false)),
        };
        
        // 启动监控任务
        page.start_monitoring_task(cx);
        
        page
    }

    /// 启动监控任务（定期检查房间状态）
    fn start_monitoring_task(&self, cx: &mut Context<Self>) {
        let check_interval = self.record_config.check_interval;
        let check_running = self.check_task_running.clone();
        
        // 检查是否已在运行
        if *check_running.read() {
            return;
        }
        *check_running.write() = true;
        
        tracing::info!("🚀 启动监控任务，检测间隔: {} 秒", check_interval);
        
        // 立即执行一次检测
        self.check_all_rooms(cx);
    }

    /// 检查所有房间状态
    fn check_all_rooms(&self, cx: &mut Context<Self>) {
        let rooms: Vec<_> = self.monitored_rooms
            .iter()
            .filter(|r| r.monitoring_enabled)
            .cloned()
            .collect();
        
        if rooms.is_empty() {
            return;
        }
        
        let live_recorder = self.live_recorder.clone();
        let runtime = self.app_state.runtime.clone();
        
        tracing::info!("🔍 开始检查 {} 个房间状态...", rooms.len());
        
        cx.spawn(async move |this, cx| {
            for room in rooms {
                let recorder = live_recorder.clone();
                let url = room.url.clone();
                let room_id = room.id;
                
                // 在 tokio runtime 中执行
                let result = runtime.spawn(async move {
                    recorder.check_room_status(&url).await
                }).await;
                
                match result {
                    Ok(Ok(room_info)) => {
                        let status = match room_info.status {
                            live_recorder::types::LiveStatus::Live => LiveRoomStatus::Live,
                            live_recorder::types::LiveStatus::Offline => LiveRoomStatus::Offline,
                            live_recorder::types::LiveStatus::Playback => LiveRoomStatus::Playback,
                            live_recorder::types::LiveStatus::Unknown => LiveRoomStatus::Unknown,
                        };
                        
                        let _ = this.update(cx, |this, cx| {
                            if let Some(state) = this.room_states.get_mut(&room_id) {
                                state.status = status;
                                state.last_error = None;
                            }
                            
                            // 更新最后检查时间
                            if let Some(room) = this.monitored_rooms.iter_mut().find(|r| r.id == room_id) {
                                room.last_checked = Some(Utc::now());
                            }
                            
                            cx.notify();
                        });
                    }
                    Ok(Err(e)) => {
                        let error = format!("{}", e);
                        let _ = this.update(cx, |this, cx| {
                            if let Some(state) = this.room_states.get_mut(&room_id) {
                                state.status = LiveRoomStatus::Error(error.clone());
                                state.last_error = Some(error);
                            }
                            cx.notify();
                        });
                    }
                    Err(e) => {
                        let error = format!("任务执行失败: {}", e);
                        let _ = this.update(cx, |this, cx| {
                            if let Some(state) = this.room_states.get_mut(&room_id) {
                                state.status = LiveRoomStatus::Error(error.clone());
                                state.last_error = Some(error);
                            }
                            cx.notify();
                        });
                    }
                }
            }
            
            tracing::info!("✅ 房间状态检查完成");
        }).detach();
    }

    /// 保存配置到文件
    fn save_config(&self) {
        let mut config = self.app_state.config.blocking_write();
        config.monitored_rooms = self.monitored_rooms.clone();
        config.live_record = self.record_config.clone();
        
        // 保存到文件（需要克隆，因为 save_app_config 需要 &AppConfig）
        let config_clone = config.clone();
        drop(config); // 释放锁
        
        if let Err(e) = magekit_shared::utils::save_app_config(&config_clone) {
            tracing::error!("❌ 保存配置失败: {}", e);
        } else {
            tracing::info!("✅ 配置已保存");
        }
    }

    /// 显示添加直播间对话框
    fn show_add_room_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url_input = self.url_input.clone();
        let this = cx.entity().clone();

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let url_input = url_input.clone();
            let this = this.clone();

            dialog
                .title("添加直播间")
                .h(px(320.0))
                .child(
                    v_flex()
                        .gap_4()
                        .child(
                            div()
                                .text_sm()
                                .child("请输入直播间链接")
                        )
                        .child(Input::new(&url_input).cleanable(true))
                        .child(
                            div()
                                .text_xs()
                                .text_color(gpui::rgb(0x888888))
                                .child("支持的平台：抖音直播、B站直播、虎牙直播、斗鱼直播、快手直播、SOOP")
                        )
                )
                .footer(move |_, _, _, _| {
                    let url_input = url_input.clone();
                    let this = this.clone();

                    vec![
                        Button::new("confirm-add")
                            .primary()
                            .label("添加")
                            .on_click(move |_event, window, cx| {
                                let url = url_input.read(cx).text().to_string().trim().to_string();
                                
                                if !url.is_empty() {
                                    let this = this.clone();
                                    let _ = this.update(cx, |this, cx| {
                                        this.add_room(url, window, cx);
                                    });
                                }
                                
                                window.close_dialog(cx);
                            }),
                        Button::new("cancel-add")
                            .label("取消")
                            .on_click(|_event, window, cx| {
                                window.close_dialog(cx);
                            }),
                    ]
                })
        });
    }

    /// 添加直播间
    fn add_room(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        // 检查是否已存在
        if self.monitored_rooms.iter().any(|r| r.url == url) {
            window.push_notification(
                Notification::warning("该直播间已在监控列表中"),
                cx,
            );
            return;
        }

        self.is_loading = true;
        cx.notify();
        
        let live_recorder = self.live_recorder.clone();
        let runtime = self.app_state.runtime.clone();

        tracing::info!("🚀 开始添加直播间: {}", url);

        window.push_notification(
            Notification::info("正在获取直播间信息..."),
            cx,
        );

        cx.spawn(async move |this, cx| {
            let url_clone = url.clone();
            
            // 首先尝试获取平台处理器（验证平台支持性）
            let result = runtime.spawn(async move {
                live_recorder.check_room_status(&url_clone).await
            }).await;

            match result {
                Ok(Ok(room_info)) => {
                    tracing::info!("✅ 成功获取直播间信息: {} - {}", room_info.anchor_name, room_info.title);

                    // 从 URL 推断平台
                    let platform = if url.contains("douyin") || url.contains("live.douyin") {
                        "抖音直播".to_string()
                    } else if url.contains("bilibili") || url.contains("live.bilibili") {
                        "B站直播".to_string()
                    } else if url.contains("huya") {
                        "虎牙直播".to_string()
                    } else if url.contains("douyu") {
                        "斗鱼直播".to_string()
                    } else if url.contains("kuaishou") || url.contains("live.kuaishou") {
                        "快手直播".to_string()
                    } else if url.contains("soop") || url.contains("afreeca") {
                        "SOOP".to_string()
                    } else {
                        "未知平台".to_string()
                    };

                    let monitored_room = MonitoredRoom::new(
                        url,
                        platform,
                        room_info.room_id.clone(),
                        room_info.anchor_name.clone(),
                    );
                    
                    let room_id = monitored_room.id;
                    
                    let status = match room_info.status {
                        live_recorder::types::LiveStatus::Live => LiveRoomStatus::Live,
                        live_recorder::types::LiveStatus::Offline => LiveRoomStatus::Offline,
                        live_recorder::types::LiveStatus::Playback => LiveRoomStatus::Playback,
                        live_recorder::types::LiveStatus::Unknown => LiveRoomStatus::Unknown,
                    };
                    let cover_url = room_info.cover_url.clone();
                    let title = if room_info.title.is_empty() { None } else { Some(room_info.title.clone()) };

                    let _ = this.update(cx, |this, cx| {
                        this.monitored_rooms.push(monitored_room);
                        this.room_states.insert(room_id, RuntimeRoomState {
                            status,
                            is_recording: false,
                            current_task: None,
                            last_error: None,
                            cover_url,
                            title,
                        });
                        this.is_loading = false;
                        this.save_config();
                        cx.notify();
                    });

                    tracing::info!("🎉 成功添加直播间: {} - {}", room_info.anchor_name, room_info.title);
                }
                Ok(Err(e)) => {
                    let error_msg = Self::format_error(&e);
                    tracing::error!("❌ 获取直播间信息失败: {}", error_msg);

                    let _ = this.update(cx, |this, cx| {
                        this.is_loading = false;
                        cx.notify();
                    });
                }
                Err(e) => {
                    let error_msg = format!("任务执行失败: {}", e);
                    tracing::error!("❌ {}", error_msg);

                    let _ = this.update(cx, |this, cx| {
                        this.is_loading = false;
                        cx.notify();
                    });
                }
            }
        }).detach();
    }

    /// 格式化错误消息（友好显示）
    fn format_error(error: &RecorderError) -> String {
        match error {
            RecorderError::UnsupportedPlatform(url) => {
                format!("不支持的平台: {}\n支持的平台: 抖音、B站、虎牙、斗鱼、快手、SOOP", url)
            }
            RecorderError::RoomNotFound(room_id) => {
                format!("直播间不存在: {}", room_id)
            }
            RecorderError::HttpError(e) => {
                format!("网络错误: {}", e)
            }
            RecorderError::JsonError(e) => {
                format!("解析错误: {}", e)
            }
            RecorderError::InvalidResponseFormat(msg) => {
                format!("响应格式错误: {}", msg)
            }
            RecorderError::AuthenticationRequired(msg) => {
                format!("需要登录: {}\n请在设置中配置 Cookie", msg)
            }
            RecorderError::AuthenticationFailed(msg) => {
                format!("认证失败: {}", msg)
            }
            RecorderError::NetworkTimeout => {
                "网络超时".to_string()
            }
            _ => format!("{}", error)
        }
    }

    /// 生成录制输出路径
    fn generate_output_path(&self, room: &MonitoredRoom) -> PathBuf {
        let download_path = self.app_state.config.blocking_read().download.default_output_path.clone();
        let record_base = download_path.join("record");
        
        // 格式: {base}/record/{平台}/{主播名}/{主播名}_{时间}.ts
        let platform_dir = record_base.join(&room.platform);
        let anchor_dir = platform_dir.join(&room.anchor_name);
        
        let timestamp = Utc::now().format("%Y-%m-%d_%H-%M-%S_%3f");
        let filename = format!("{}_{}.{}", room.anchor_name, timestamp, self.record_config.record_format);
        
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
                Notification::warning("直播间未开播，无法录制"),
                cx,
            );
            return;
        }

        // 检查是否已在录制
        if state.is_recording {
            window.push_notification(
                Notification::warning("该直播间已在录制中"),
                cx,
            );
            return;
        }

        let output_path = self.generate_output_path(&room);
        let live_recorder = self.live_recorder.clone();
        let runtime = self.app_state.runtime.clone();
        let url = room.url.clone();
        let anchor_name = room.anchor_name.clone();
        
        // 创建输出目录
        if let Some(parent) = output_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::error!("❌ 无法创建输出目录: {}", e);
                window.push_notification(
                    Notification::error(&format!("无法创建输出目录: {}", e)),
                    cx,
                );
                return;
            }
        }

        // 更新状态
        if let Some(state) = self.room_states.get_mut(&room_id) {
            state.is_recording = true;
            state.current_task = Some(RecordingTask {
                id: Uuid::new_v4(),
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
            Notification::success(&format!("开始录制: {}", anchor_name)),
            cx,
        );

        tracing::info!("🎥 开始录制: {} -> {:?}", anchor_name, output_path);

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
            timeout: self.record_config.reconnect_delay,
            max_duration: None,
            include_danmaku: false,
            proxy: None,
            headers: std::collections::HashMap::new(),
        };

        cx.spawn(async move |this, cx| {
            let result = runtime.spawn(async move {
                live_recorder.start_recording(&url, config).await
            }).await;

            match result {
                Ok(Ok(_handle)) => {
                    tracing::info!("✅ 录制任务已启动: {}", anchor_name);
                    // 录制任务会在后台运行，我们需要监控它的进度
                }
                Ok(Err(e)) => {
                    let error_msg = format!("录制失败: {}", e);
                    tracing::error!("❌ {}", error_msg);
                    
                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.is_recording = false;
                            state.current_task = None;
                            state.last_error = Some(error_msg.clone());
                        }
                        cx.notify();
                    });
                    
                    tracing::error!("❌ 录制失败: {}", error_msg);
                }
                Err(e) => {
                    let error_msg = format!("任务执行失败: {}", e);
                    tracing::error!("❌ {}", error_msg);
                    
                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.is_recording = false;
                            state.current_task = None;
                        }
                        cx.notify();
                    });
                }
            }
        }).detach();
    }

    /// 停止录制
    fn stop_recording(&mut self, room_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.room_states.get_mut(&room_id) {
            state.is_recording = false;
            
            // 检查是否需要转码
            if self.record_config.auto_transcode {
                if let Some(task) = &state.current_task {
                    // TODO: 启动转码任务
                    tracing::info!("📦 即将转码: {:?}", task.output_path);
                }
            }
            
            state.current_task = None;
        }

        let anchor_name = self.monitored_rooms
            .iter()
            .find(|r| r.id == room_id)
            .map(|r| r.anchor_name.clone())
            .unwrap_or_default();

        window.push_notification(
            Notification::success(&format!("停止录制: {}", anchor_name)),
            cx,
        );
        cx.notify();
    }

    /// 删除直播间
    fn remove_room(&mut self, room_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        // 检查是否正在录制
        if let Some(state) = self.room_states.get(&room_id) {
            if state.is_recording {
                window.push_notification(
                    Notification::warning("请先停止录制再删除"),
                    cx,
                );
                return;
            }
        }

        if let Some(index) = self.monitored_rooms.iter().position(|r| r.id == room_id) {
            self.monitored_rooms.remove(index);
            self.room_states.remove(&room_id);
            self.save_config();
            
            window.push_notification(
                Notification::info("直播间已移除"),
                cx,
            );
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

    /// 刷新房间状态
    fn refresh_room(&mut self, room_id: Uuid, cx: &mut Context<Self>) {
        let room = match self.monitored_rooms.iter().find(|r| r.id == room_id) {
            Some(r) => r.clone(),
            None => return,
        };

        // 设置为检查中状态
        if let Some(state) = self.room_states.get_mut(&room_id) {
            state.status = LiveRoomStatus::Checking;
        }
        cx.notify();

        let live_recorder = self.live_recorder.clone();
        let runtime = self.app_state.runtime.clone();
        let url = room.url.clone();

        cx.spawn(async move |this, cx| {
            // 获取完整的流信息（包括封面图和标题）
            let result = runtime.spawn(async move {
                live_recorder.get_stream_info(&url).await
            }).await;

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
                    let title = if room_info.title.is_empty() { None } else { Some(room_info.title.clone()) };
                    let anchor_name = room_info.anchor_name.clone();

                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.status = status;
                            state.last_error = None;
                            state.cover_url = cover_url;
                            state.title = title;
                        }
                        // 更新主播名称（如果之前是 Unknown）
                        if let Some(room) = this.monitored_rooms.iter_mut().find(|r| r.id == room_id) {
                            room.last_checked = Some(Utc::now());
                            if room.anchor_name == "Unknown" || room.anchor_name.starts_with("Unknown-") {
                                room.anchor_name = anchor_name;
                                // 保存更新
                                this.save_config();
                            }
                        }
                        cx.notify();
                    });
                }
                Ok(Err(e)) => {
                    let error = format!("{}", e);
                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.status = LiveRoomStatus::Error(error.clone());
                            state.last_error = Some(error);
                        }
                        cx.notify();
                    });
                }
                Err(e) => {
                    let error = format!("任务执行失败: {}", e);
                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.status = LiveRoomStatus::Error(error.clone());
                            state.last_error = Some(error);
                        }
                        cx.notify();
                    });
                }
            }
        }).detach();
    }

    /// 显示设置弹窗
    fn show_settings_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = self.record_config.clone();

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let config = config.clone();

            dialog
                .title("📹 录制设置")
                .h(px(520.0))
                .w(px(480.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .p_4()
                        // === 录制格式 ===
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
                                                .child("🎬 录制格式")
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(gpui::rgb(0x888888))
                                                .child("ts|mkv|flv|mp4 (ts 最稳定)")
                                        )
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(gpui::rgb(0x3b82f6))
                                        .child(config.record_format.to_uppercase())
                                )
                        )
                        // === 视频质量 ===
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
                                                .child("📊 视频质量")
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(gpui::rgb(0x888888))
                                                .child("原画|蓝光|超清|高清|标清")
                                        )
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(gpui::rgb(0x22c55e))
                                        .child(config.quality.display_name())
                                )
                        )
                        // === 检测间隔 ===
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
                                                .child("⏱️ 检测间隔")
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(gpui::rgb(0x888888))
                                                .child("检测直播状态的时间间隔")
                                        )
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(format!("{} 秒", config.check_interval))
                                )
                        )
                        // === 分段录制 ===
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
                                                .child("📁 分段录制")
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(gpui::rgb(0x888888))
                                                .child("视频分段时间（秒）")
                                        )
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(if let Some(dur) = config.segment_duration {
                                            format!("{} 秒", dur)
                                        } else {
                                            "关闭".to_string()
                                        })
                                )
                        )
                        // === 重试设置 ===
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
                                                .child("🔄 重试设置")
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(gpui::rgb(0x888888))
                                                .child("断流后自动重连")
                                        )
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .child(format!("{} 次 / {} 秒", config.retry_count, config.reconnect_delay))
                                )
                        )
                        // === 自动转码 ===
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
                                                .child("🔄 录制后自动转码")
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(gpui::rgb(0x888888))
                                                .child("录制完成后自动转为 mp4 格式")
                                        )
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(if config.auto_transcode { gpui::rgb(0x22c55e) } else { gpui::rgb(0x6b7280) })
                                        .child(if config.auto_transcode { "开启" } else { "关闭" })
                                )
                        )
                        // === 保存路径 ===
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
                                                .child("📂 保存路径")
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(gpui::rgb(0x888888))
                                                .child("相对于下载路径")
                                        )
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(gpui::rgb(0x9ca3af))
                                        .max_w(px(200.0))
                                        .overflow_x_hidden()
                                        .text_ellipsis()
                                        .child(config.output_base_path.to_string_lossy().to_string())
                                )
                        )
                )
                .footer(move |_, _, _, _| {
                    vec![
                        Button::new("close-settings")
                            .label("关闭")
                            .on_click(|_event, window, cx| {
                                window.close_dialog(cx);
                            }),
                    ]
                })
        });
    }

    /// 更新录制配置
    fn update_record_config<F>(&mut self, f: F, cx: &mut Context<Self>)
    where
        F: FnOnce(&mut LiveRecordConfig),
    {
        f(&mut self.record_config);
        self.save_record_config();
        cx.notify();
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

    /// 渲染房间卡片（带封面图的美化版本）
    fn render_room_card(&self, room: &MonitoredRoom, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let card_bg = theme.secondary;
        let border_color = theme.border;
        let title_color = theme.foreground;
        let desc_color = theme.muted_foreground;

        let room_id = room.id;
        let state = self.room_states.get(&room_id).cloned().unwrap_or_default();
        let is_live = state.status == LiveRoomStatus::Live;
        let is_recording = state.is_recording;
        let is_monitoring = room.monitoring_enabled;
        let cover_url = state.cover_url.clone();
        let title = state.title.clone();

        // 根据状态获取颜色
        let status_color = match &state.status {
            LiveRoomStatus::Live => gpui::rgb(0x22c55e),      // 绿色
            LiveRoomStatus::Recording => gpui::rgb(0xef4444), // 红色
            LiveRoomStatus::Playback => gpui::rgb(0xeab308),  // 黄色
            LiveRoomStatus::Checking => gpui::rgb(0x3b82f6),  // 蓝色
            LiveRoomStatus::Error(_) => gpui::rgb(0xef4444),  // 红色
            _ => gpui::rgb(0x6b7280),                         // 灰色
        };

        // 平台颜色映射
        let platform_color = match room.platform.as_str() {
            "douyin" => gpui::rgb(0x000000),  // 黑色
            "bilibili" => gpui::rgb(0xfb7299), // B站粉
            "huya" => gpui::rgb(0xff9600),    // 虎牙橙
            "douyu" => gpui::rgb(0xff5d23),   // 斗鱼橙
            "kuaishou" => gpui::rgb(0xff4906), // 快手橙
            "soop" => gpui::rgb(0x5b6edc),    // SOOP 蓝紫
            _ => gpui::rgb(0x6366f1),         // 默认紫色
        };

        // 平台显示名称
        let platform_display = match room.platform.as_str() {
            "douyin" => "抖音",
            "bilibili" => "B站",
            "huya" => "虎牙",
            "douyu" => "斗鱼",
            "kuaishou" => "快手",
            "soop" => "SOOP",
            _ => &room.platform,
        };

        div()
            .flex()
            .p_3()
            .bg(card_bg)
            .border_1()
            .border_color(border_color)
            .rounded_xl()
            .gap_3()
            .hover(|el| el.bg(theme.muted).cursor_pointer())
            // 封面图区域
            .child(
                div()
                    .relative()
                    .w(px(120.0))
                    .h(px(68.0))
                    .bg(gpui::rgb(0x1a1a1a))
                    .rounded_lg()
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .flex_shrink_0()
                    // 显示封面图或占位符
                    .when_some(cover_url.clone(), |el, url| {
                        el.child(
                            img(url)
                                .size_full()
                                .object_fit(ObjectFit::Cover),
                        )
                    })
                    .when(cover_url.is_none(), |el| {
                        el.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .text_2xl()
                                        .child("📺"),
                                ),
                        )
                    })
                    // 直播状态角标
                    .when(is_live, |el| {
                        el.child(
                            div()
                                .absolute()
                                .top(px(4.0))
                                .left(px(4.0))
                                .px(px(6.0))
                                .py(px(2.0))
                                .bg(gpui::rgb(0xef4444))
                                .rounded(px(4.0))
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(gpui::white())
                                .child("LIVE"),
                        )
                    })
                    // 录制中指示器
                    .when(is_recording, |el| {
                        el.child(
                            div()
                                .absolute()
                                .bottom(px(4.0))
                                .left(px(4.0))
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .px(px(6.0))
                                .py(px(2.0))
                                .bg(gpui::rgba(0x00000099))
                                .rounded(px(4.0))
                                .child(
                                    div()
                                        .w(px(6.0))
                                        .h(px(6.0))
                                        .bg(gpui::rgb(0xef4444))
                                        .rounded_full(),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(gpui::white())
                                        .child("REC"),
                                ),
                        )
                    }),
            )
            // 信息区域
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex_col()
                    .gap(px(4.0))
                    .min_w_0()
                    // 主播名 + 平台标签
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(title_color)
                                    .overflow_x_hidden()
                                    .text_ellipsis()
                                    .child(room.anchor_name.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .bg(platform_color)
                                    .text_color(gpui::white())
                                    .rounded(px(4.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .flex_shrink_0()
                                    .child(platform_display.to_string()),
                            )
                            .when(!is_monitoring, |el| {
                                el.child(
                                    div()
                                        .text_xs()
                                        .px(px(6.0))
                                        .py(px(2.0))
                                        .bg(gpui::rgb(0x6b7280))
                                        .text_color(gpui::white())
                                        .rounded(px(4.0))
                                        .flex_shrink_0()
                                        .child("已暂停"),
                                )
                            }),
                    )
                    // 直播标题
                    .when_some(title.clone(), |el, t| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(desc_color)
                                .overflow_x_hidden()
                                .text_ellipsis()
                                .child(t),
                        )
                    })
                    // 状态和房间号
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            // 状态指示点 + 文字
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .w(px(6.0))
                                            .h(px(6.0))
                                            .bg(status_color)
                                            .rounded_full(),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(desc_color)
                                            .child(state.status.display_name()),
                                    ),
                            )
                            // 房间号
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(gpui::rgb(0x9ca3af))
                                    .child(format!("#{}", room.room_id)),
                            ),
                    )
                    // 错误信息
                    .when_some(state.last_error.clone(), |el, error| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(gpui::rgb(0xef4444))
                                .overflow_x_hidden()
                                .text_ellipsis()
                                .child(format!("⚠️ {}", error)),
                        )
                    }),
            )
            // 操作按钮
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .flex_shrink_0()
                    // 刷新按钮
                    .child(
                        Button::new(SharedString::from(format!("refresh-{}", room_id)))
                            .icon(gpui_component::IconName::Replace)
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.refresh_room(room_id, cx);
                            }))
                    )
                    // 录制/停止按钮
                    .when(is_live && !is_recording, |el| {
                        el.child(
                            Button::new(SharedString::from(format!("start-{}", room_id)))
                                .label("录制")
                                .xsmall()
                                .on_click(cx.listener(move |this, _event, window, cx| {
                                    this.start_recording(room_id, window, cx);
                                }))
                        )
                    })
                    .when(is_recording, |el| {
                        el.child(
                            Button::new(SharedString::from(format!("stop-{}", room_id)))
                                .icon(gpui_component::IconName::CircleX)
                                .label("停止")
                                .xsmall()
                                .danger()
                                .on_click(cx.listener(move |this, _event, window, cx| {
                                    this.stop_recording(room_id, window, cx);
                                }))
                        )
                    })
                    // 打开直播间
                    .child({
                        let room_url = room.url.clone();
                        Button::new(SharedString::from(format!("open-{}", room_id)))
                            .icon(gpui_component::IconName::ExternalLink)
                            .label("打开")
                            .xsmall()
                            .ghost()
                            .on_click(move |_event, _window, _cx| {
                                if let Err(e) = open::that(&room_url) {
                                    tracing::error!("❌ 无法打开浏览器: {}", e);
                                }
                            })
                    })
                    // 监控开关
                    .child(
                        Button::new(SharedString::from(format!("monitor-{}", room_id)))
                            .icon(if is_monitoring { gpui_component::IconName::Minus } else { gpui_component::IconName::ArrowRight })
                            .label(if is_monitoring { "暂停" } else { "监控" })
                            .xsmall()
                            .ghost()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.toggle_monitoring(room_id, cx);
                            }))
                    )
                    // 刷新按钮
                    .child(
                        Button::new(SharedString::from(format!("refresh-{}", room_id)))
                            .icon(gpui_component::IconName::Replace)
                            .label("刷新")
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.refresh_room(room_id, cx);
                            }))
                    )
                    // 删除按钮
                    .child(
                        Button::new(SharedString::from(format!("remove-{}", room_id)))
                            .icon(gpui_component::IconName::Delete)
                            .label("删除")
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(move |this, _event, window, cx| {
                                this.remove_room(room_id, window, cx);
                            }))
                    ),
            )
    }
}

impl Render for RecordingPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg_color = theme.background;
        let title_color = theme.foreground;
        let desc_color = theme.muted_foreground;

        let rooms = self.monitored_rooms.clone();
        let live_count = self.room_states.values().filter(|s| s.status == LiveRoomStatus::Live).count();
        let recording_count = self.room_states.values().filter(|s| s.is_recording).count();

        div()
            .id("recording-page")
            .size_full()
            .overflow_y_scroll()
            .bg(bg_color)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p_6()
                    .gap_6()
                    // 页面标题
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
                                            .text_2xl()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(title_color)
                                            .child("🎥 直播录制"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(desc_color)
                                            .child(format!(
                                                "监控 {} 个房间 · {} 个直播中 · {} 个录制中",
                                                rooms.len(),
                                                live_count,
                                                recording_count
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_3()
                                    .child(
                                        Button::new("refresh-all")
                                            .icon(gpui_component::IconName::Replace)
                                            .ghost()
                                            .on_click(cx.listener(|this, _event, _window, cx| {
                                                this.check_all_rooms(cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("add-room")
                                            .icon(gpui_component::IconName::Plus)
                                            .label("添加直播间")
                                            .on_click(cx.listener(|this, _event, window, cx| {
                                                this.show_add_room_dialog(window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("config")
                                            .icon(gpui_component::IconName::Settings2)
                                            .label("设置")
                                            .ghost()
                                            .on_click(cx.listener(|this, _event, window, cx| {
                                                this.show_settings_dialog(window, cx);
                                            })),
                                    ),
                            ),
                    )
                    // 加载指示器
                    .when(self.is_loading, |el| {
                        el.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .p_4()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(desc_color)
                                        .child("正在加载...")
                                )
                        )
                    })
                    // 直播间列表
                    .child(
                        if rooms.is_empty() {
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .p(px(80.0))
                                .gap_4()
                                .child(
                                    div()
                                        .text_2xl()
                                        .text_color(desc_color)
                                        .child("📺"),
                                )
                                .child(
                                    div()
                                        .text_lg()
                                        .text_color(desc_color)
                                        .child("暂无监控的直播间"),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(desc_color)
                                        .child("点击上方「添加直播间」按钮开始监控"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(desc_color)
                                        .child("支持：抖音、B站、虎牙、斗鱼、快手、SOOP"),
                                )
                                .into_any_element()
                        } else {
                            div()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .children(
                                    rooms.iter().map(|room| {
                                        self.render_room_card(room, cx).into_any_element()
                                    })
                                )
                                .into_any_element()
                        }
                    ),
            )
    }
}
