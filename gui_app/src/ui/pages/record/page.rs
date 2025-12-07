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
use gpui_component::radio::RadioGroup;
use gpui_component::switch::Switch;
use gpui_component::v_flex;
use gpui_component::Sizable;
use live_recorder::{LiveRecorder, RecordConfig, recorder::RecordingHandle, error::RecorderError};
use magekit_shared::types::{
    MonitoredRoom, LiveRoomStatus, LiveRecordConfig, LiveRecordQuality, RecordingTask
};
use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;
use uuid::Uuid;
use chrono::Utc;
use parking_lot::RwLock;
use tokio::sync::Mutex as TokioMutex;


/// 运行时房间状态（用于 UI 显示）
struct RuntimeRoomState {
    status: LiveRoomStatus,
    is_recording: bool,
    current_task: Option<RecordingTask>,
    last_error: Option<String>,
    /// 封面图 URL
    cover_url: Option<String>,
    /// 直播标题
    title: Option<String>,
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
            title: self.title.clone(),
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
            title: None,
            recording_handle: None,
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
    
    /// 最后一次添加房间的错误
    last_add_error: Option<String>,
    
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
        
        // 初始化运行时状态（从缓存恢复标题和封面）
        let mut room_states = HashMap::new();
        for room in &monitored_rooms {
            room_states.insert(room.id, RuntimeRoomState {
                status: LiveRoomStatus::Unknown,
                is_recording: false,
                current_task: None,
                last_error: None,
                cover_url: room.cached_cover_url.clone(),
                title: room.cached_title.clone(),
                recording_handle: None,
            });
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
            last_add_error: None,
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
                        
                        // 提取房间信息用于缓存
                        let title = if room_info.title.is_empty() { None } else { Some(room_info.title.clone()) };
                        let cover_url = room_info.cover_url.clone();
                        let is_live = status == LiveRoomStatus::Live;
                        
                        // 调试日志
                        tracing::info!("📦 房间 {} 状态检查结果:", room_id);
                        tracing::info!("   - 状态: {:?}", status);
                        tracing::info!("   - 标题: {:?}", title);
                        tracing::info!("   - 封面: {:?}", cover_url);
                        tracing::info!("   - 主播: {}", room_info.anchor_name);
                        
                        let _ = this.update(cx, |this, cx| {
                            let is_recording = this.room_states.get(&room_id)
                                .map(|s| s.is_recording)
                                .unwrap_or(false);
                            
                            // 更新运行时状态
                            if let Some(state) = this.room_states.get_mut(&room_id) {
                                state.status = status;
                                state.last_error = None;
                                // 更新标题和封面
                                if title.is_some() {
                                    state.title = title.clone();
                                }
                                if cover_url.is_some() {
                                    state.cover_url = cover_url.clone();
                                }
                            }
                            
                            // 更新持久化的房间信息（缓存标题、封面等）
                            let mut need_save = false;
                            if let Some(room) = this.monitored_rooms.iter_mut().find(|r| r.id == room_id) {
                                room.last_checked = Some(Utc::now());
                                
                                // 同步标题到缓存
                                if title.is_some() && room.cached_title != title {
                                    tracing::info!("📝 更新房间 {} 缓存标题: {:?} -> {:?}", room_id, room.cached_title, title);
                                    room.cached_title = title;
                                    need_save = true;
                                }
                                // 同步封面到缓存
                                if cover_url.is_some() && room.cached_cover_url != cover_url {
                                    tracing::info!("🖼️ 更新房间 {} 缓存封面: {:?}", room_id, cover_url);
                                    room.cached_cover_url = cover_url;
                                    need_save = true;
                                }
                                // 更新最后直播时间
                                if is_live {
                                    room.last_live_at = Some(Utc::now());
                                    need_save = true;
                                }
                                
                                tracing::info!("📊 房间 {} need_save={}", room_id, need_save);
                            } else {
                                tracing::warn!("⚠️ 找不到房间 {} 在 monitored_rooms 中", room_id);
                            }
                            
                            // 保存配置（如果有更新）
                            if need_save {
                                this.save_config();
                            }
                            
                            // 如果开启了监控且变为直播状态，自动开始录制
                            let is_monitoring = this.monitored_rooms.iter()
                                .find(|r| r.id == room_id)
                                .map(|r| r.monitoring_enabled)
                                .unwrap_or(false);
                            
                            if is_live && is_monitoring && !is_recording {
                                tracing::info!("🎬 监控检测到直播，自动开始录制: {}", room_id);
                                this.start_recording_background(room_id, cx);
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

    /// 添加直播间 - 立即添加到列表，后台获取信息
    fn add_room(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        // 检查是否已存在
        if self.monitored_rooms.iter().any(|r| r.url == url) {
            window.push_notification(
                Notification::warning("该直播间已在监控列表中"),
                cx,
            );
            return;
        }

        tracing::info!("🚀 开始添加直播间: {}", url);

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

        // 立即创建房间并添加到列表
        let monitored_room = MonitoredRoom::new(
            url.clone(),
            platform,
            String::new(), // room_id 稍后获取
            "获取中...".to_string(), // anchor_name 稍后获取
        );
        
        let room_id = monitored_room.id;
        
        // 添加到列表，状态为 Unknown（加载中）
        self.monitored_rooms.push(monitored_room);
        self.room_states.insert(room_id, RuntimeRoomState {
            status: LiveRoomStatus::Unknown,
            is_recording: false,
            current_task: None,
            last_error: Some("正在获取直播间信息...".to_string()),
            cover_url: None,
            title: None,
            recording_handle: None,
        });
        self.save_config();
        cx.notify();
        
        // 后台获取房间详细信息
        let live_recorder = self.live_recorder.clone();
        let runtime = self.app_state.runtime.clone();

        cx.spawn(async move |this, cx| {
            let url_clone = url.clone();
            
            let result = runtime.spawn(async move {
                live_recorder.check_room_status(&url_clone).await
            }).await;

            match result {
                Ok(Ok(room_info)) => {
                    tracing::info!("✅ 成功获取直播间信息: {} - {}", room_info.anchor_name, room_info.title);

                    let status = match room_info.status {
                        live_recorder::types::LiveStatus::Live => LiveRoomStatus::Live,
                        live_recorder::types::LiveStatus::Offline => LiveRoomStatus::Offline,
                        live_recorder::types::LiveStatus::Playback => LiveRoomStatus::Playback,
                        live_recorder::types::LiveStatus::Unknown => LiveRoomStatus::Unknown,
                    };
                    let cover_url = room_info.cover_url.clone();
                    let title = if room_info.title.is_empty() { None } else { Some(room_info.title.clone()) };
                    let anchor_name = room_info.anchor_name.clone();
                    let real_room_id = room_info.room_id.clone();

                    let _ = this.update(cx, |this, cx| {
                        // 更新房间信息
                        if let Some(room) = this.monitored_rooms.iter_mut().find(|r| r.id == room_id) {
                            room.room_id = real_room_id;
                            room.anchor_name = anchor_name;
                            room.cached_title = title.clone();
                            room.cached_cover_url = cover_url.clone();
                        }
                        
                        // 更新运行时状态
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.status = status;
                            state.last_error = None;
                            state.cover_url = cover_url;
                            state.title = title;
                        }
                        
                        this.save_config();
                        cx.notify();
                    });
                }
                Ok(Err(e)) => {
                    let error_msg = Self::format_error(&e);
                    tracing::error!("❌ 获取直播间信息失败: {}", error_msg);

                    let _ = this.update(cx, |this, cx| {
                        // 更新房间状态为错误，但保留在列表中
                        if let Some(room) = this.monitored_rooms.iter_mut().find(|r| r.id == room_id) {
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
                    let error_msg = format!("任务执行失败: {}", e);
                    tracing::error!("❌ {}", error_msg);

                    let _ = this.update(cx, |this, cx| {
                        if let Some(room) = this.monitored_rooms.iter_mut().find(|r| r.id == room_id) {
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
                Ok(Ok(handle)) => {
                    tracing::info!("✅ 录制任务已启动: {}", anchor_name);
                    // 保存 handle 以便后续停止录制和获取进度
                    let handle = Arc::new(TokioMutex::new(handle));
                    let handle_clone = handle.clone();
                    
                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.recording_handle = Some(handle);
                        }
                        cx.notify();
                        
                        // 启动进度监控任务
                        this.start_progress_monitor(room_id, handle_clone, cx);
                    });
                }
                Ok(Err(e)) => {
                    let error_msg = format!("录制失败: {}", e);
                    tracing::error!("❌ {}", error_msg);
                    
                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.is_recording = false;
                            state.current_task = None;
                            state.last_error = Some(error_msg.clone());
                            state.recording_handle = None;
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
                            state.recording_handle = None;
                        }
                        cx.notify();
                    });
                }
            }
        }).detach();
    }

    /// 停止录制
    fn stop_recording(&mut self, room_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        // 获取录制任务信息用于转码
        let (handle, output_path, should_transcode) = if let Some(state) = self.room_states.get_mut(&room_id) {
            state.is_recording = false;
            
            let output_path = state.current_task.as_ref().map(|t| t.output_path.clone());
            let should_transcode = self.record_config.auto_transcode && output_path.is_some();
            
            state.current_task = None;
            (state.recording_handle.take(), output_path, should_transcode)
        } else {
            (None, None, false)
        };

        let anchor_name = self.monitored_rooms
            .iter()
            .find(|r| r.id == room_id)
            .map(|r| r.anchor_name.clone())
            .unwrap_or_default();

        // 异步停止录制并转码
        if let Some(handle) = handle {
            let runtime = self.app_state.runtime.clone();
            let app_state = self.app_state.clone();
            let anchor_name_for_log = anchor_name.clone();
            
            cx.spawn(async move |_this, _cx| {
                // 先停止录制
                let stop_result = runtime.spawn(async move {
                    let mut h = handle.lock().await;
                    h.stop().await
                }).await;
                
                match stop_result {
                    Ok(Ok(_)) => {
                        tracing::info!("✅ 录制已停止: {}", anchor_name_for_log);
                        
                        // 如果需要转码
                        if should_transcode {
                            if let Some(input_path) = output_path {
                                // 生成 MP4 输出路径
                                let mp4_path = input_path.with_extension("mp4");
                                
                                tracing::info!("🔄 开始转码: {:?} -> {:?}", input_path, mp4_path);
                                
                                // 获取 ffmpeg 路径
                                let ffmpeg_path = app_state.tool_manager.storage.get_tool_path(magekit_shared::ToolType::Ffmpeg);
                                
                                // 检查 ffmpeg 是否存在
                                let ffmpeg = if ffmpeg_path.exists() {
                                    Some(ffmpeg_path)
                                } else {
                                    which::which("ffmpeg").ok()
                                };
                                
                                if let Some(ffmpeg) = ffmpeg {
                                    // 使用 ffmpeg 转码
                                    let transcode_result = std::process::Command::new(&ffmpeg)
                                        .arg("-i")
                                        .arg(&input_path)
                                        .arg("-c")
                                        .arg("copy")
                                        .arg("-y")
                                        .arg(&mp4_path)
                                        .output();
                                    
                                    match transcode_result {
                                        Ok(output) => {
                                            if output.status.success() {
                                                tracing::info!("✅ 转码完成: {:?}", mp4_path);
                                                // 可选：删除原文件
                                                // let _ = std::fs::remove_file(&input_path);
                                            } else {
                                                let stderr = String::from_utf8_lossy(&output.stderr);
                                                tracing::error!("❌ 转码失败: {}", stderr);
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!("❌ 启动 ffmpeg 失败: {}", e);
                                        }
                                    }
                                } else {
                                    tracing::warn!("⚠️ 未找到 ffmpeg，跳过转码");
                                }
                            }
                        }
                    }
                    Ok(Err(e)) => tracing::error!("❌ 停止录制失败: {}", e),
                    Err(e) => tracing::error!("❌ 停止录制任务失败: {}", e),
                }
            }).detach();
        }

        window.push_notification(
            Notification::success(&format!("停止录制: {}", anchor_name)),
            cx,
        );
        cx.notify();
    }

    /// 启动进度监控任务，定期获取录制进度并更新 UI
    fn start_progress_monitor(
        &self,
        room_id: Uuid,
        handle: Arc<TokioMutex<RecordingHandle>>,
        cx: &mut Context<Self>,
    ) {
        let runtime = self.app_state.runtime.clone();
        
        cx.spawn(async move |this, cx| {
            loop {
                // 每秒获取一次进度
                smol::Timer::after(std::time::Duration::from_secs(1)).await;
                
                // 检查是否还在录制
                let still_recording = this.update(cx, |this, _cx| {
                    this.room_states.get(&room_id)
                        .map(|s| s.is_recording)
                        .unwrap_or(false)
                }).ok().unwrap_or(false);
                
                if !still_recording {
                    tracing::info!("📊 进度监控停止: room_id={}", room_id);
                    break;
                }
                
                // 获取进度
                let handle = handle.clone();
                let progress = runtime.spawn(async move {
                    let mut h = handle.lock().await;
                    h.get_progress().await
                }).await;
                
                match progress {
                    Ok(Some(progress)) => {
                        let _ = this.update(cx, |this, cx| {
                            if let Some(state) = this.room_states.get_mut(&room_id) {
                                // 更新录制任务的进度信息
                                if let Some(ref mut task) = state.current_task {
                                    task.duration = progress.duration;
                                    task.recorded_bytes = progress.size;
                                }
                            }
                            cx.notify();
                        });
                    }
                    Ok(None) => {
                        // 进度通道关闭，录制可能已结束
                        tracing::info!("📊 进度通道关闭，录制可能已结束: room_id={}", room_id);
                        let _ = this.update(cx, |this, cx| {
                            if let Some(state) = this.room_states.get_mut(&room_id) {
                                state.is_recording = false;
                                state.recording_handle = None;
                            }
                            cx.notify();
                        });
                        break;
                    }
                    Err(e) => {
                        tracing::warn!("⚠️ 获取进度失败: {}", e);
                    }
                }
            }
        }).detach();
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
        let live_recorder = self.live_recorder.clone();
        let runtime = self.app_state.runtime.clone();
        let url = room.url.clone();
        let anchor_name = room.anchor_name.clone();
        
        // 创建输出目录
        if let Some(parent) = output_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::error!("❌ 无法创建输出目录: {}", e);
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
                Ok(Ok(handle)) => {
                    tracing::info!("✅ 自动录制任务已启动: {}", anchor_name);
                    // 保存 handle 以便后续停止录制和获取进度
                    let handle = Arc::new(TokioMutex::new(handle));
                    let handle_clone = handle.clone();
                    
                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.recording_handle = Some(handle);
                        }
                        cx.notify();
                        
                        // 启动进度监控任务
                        this.start_progress_monitor(room_id, handle_clone, cx);
                    });
                }
                Ok(Err(e)) => {
                    let error_msg = format!("自动录制失败: {}", e);
                    tracing::error!("❌ {}", error_msg);
                    
                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.is_recording = false;
                            state.current_task = None;
                            state.last_error = Some(error_msg.clone());
                            state.recording_handle = None;
                        }
                        cx.notify();
                    });
                }
                Err(e) => {
                    let error_msg = format!("任务执行失败: {}", e);
                    tracing::error!("❌ {}", error_msg);
                    
                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.room_states.get_mut(&room_id) {
                            state.is_recording = false;
                            state.current_task = None;
                            state.recording_handle = None;
                        }
                        cx.notify();
                    });
                }
            }
        }).detach();
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

        tracing::info!("🔄 开始刷新房间: {} ({})", room.anchor_name, room_id);

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
                            state.cover_url = cover_url.clone();
                            state.title = title.clone();
                        }
                        
                        // 更新持久化的房间信息（缓存标题、封面等）
                        let mut need_save = false;
                        if let Some(room) = this.monitored_rooms.iter_mut().find(|r| r.id == room_id) {
                            room.last_checked = Some(Utc::now());
                            
                            // 更新主播名称（如果之前是 Unknown）
                            if room.anchor_name == "Unknown" || room.anchor_name.starts_with("Unknown-") {
                                tracing::info!("📝 更新房间 {} 主播名: {} -> {}", room_id, room.anchor_name, anchor_name);
                                room.anchor_name = anchor_name;
                                need_save = true;
                            }
                            
                            // 同步标题到缓存
                            if title.is_some() && room.cached_title != title {
                                tracing::info!("📝 更新房间 {} 缓存标题: {:?} -> {:?}", room_id, room.cached_title, title);
                                room.cached_title = title;
                                need_save = true;
                            }
                            
                            // 同步封面到缓存
                            if cover_url.is_some() && room.cached_cover_url != cover_url {
                                tracing::info!("🖼️ 更新房间 {} 缓存封面: {:?}", room_id, cover_url);
                                room.cached_cover_url = cover_url;
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
                            this.save_config();
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
                    let error = format!("任务执行失败: {}", e);
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
        }).detach();
    }

    /// 显示设置弹窗（可编辑版本）
    fn show_settings_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = self.record_config.clone();
        let this = cx.entity().clone();

        // 格式选项
        let format_options = ["ts", "mkv", "flv", "mp4"];
        let format_index = format_options.iter().position(|&f| f == config.record_format.as_str()).unwrap_or(0);
        
        // 质量选项
        let quality_options = [
            LiveRecordQuality::Original,
            LiveRecordQuality::Blue,
            LiveRecordQuality::Ultra,
            LiveRecordQuality::High,
            LiveRecordQuality::Standard,
        ];
        let quality_index = quality_options.iter().position(|q| *q == config.quality).unwrap_or(0);
        
        // 使用 Arc<RwLock> 存储选择状态（因为 Dialog 闭包需要 Fn）
        let selected_format = Arc::new(RwLock::new(format_index));
        let selected_quality = Arc::new(RwLock::new(quality_index));
        let auto_transcode = Arc::new(RwLock::new(config.auto_transcode));

        // 在 open_dialog 之前创建所有 InputState
        let interval_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("60")
                .default_value(config.check_interval.to_string())
        });
        let segment_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("留空关闭")
                .default_value(config.segment_duration.map(|d| d.to_string()).unwrap_or_default())
        });
        let retry_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("3")
                .default_value(config.retry_count.to_string())
        });
        let reconnect_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("30")
                .default_value(config.reconnect_delay.to_string())
        });
        let path_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("recordings")
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

        window.open_dialog(cx, move |dialog, _window, cx| {
            let selected_format = selected_format.clone();
            let selected_quality = selected_quality.clone();
            let auto_transcode = auto_transcode.clone();
            let theme = cx.theme();
            let border_color = theme.border;
            let muted_fg = theme.muted_foreground;

            dialog
                .title("录制设置")
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
                                                        .text_base()
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .child("🎬 输出设置")
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
                                                        .child("录制格式")
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
                                                        .child("TS 最稳定，MP4 兼容性最好")
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
                                                        .child("视频质量")
                                                )
                                                .child({
                                                    let selected_quality = selected_quality.clone();
                                                    let current_idx = *selected_quality.read();
                                                    RadioGroup::horizontal("quality-radio")
                                                        .children(["原画", "蓝光", "超清", "高清", "标清"])
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
                                                                .child("录制后自动转码")
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(muted_fg)
                                                                .child("录制完成后自动转为 MP4 格式")
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
                                                        .text_base()
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .child("⏱️ 监控设置")
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
                                                                .child("检测间隔")
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(muted_fg)
                                                                .child("检查直播间状态的时间间隔（最小 10 秒）")
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
                                                                .child("秒")
                                                        )
                                                )
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
                                                        .child("重连设置")
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
                                                                        .child("次")
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
                                                                        .child("间隔")
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
                                                                        .child("秒")
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
                                                        .text_base()
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .child("⚙️ 高级设置")
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
                                                                .child("分段录制")
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(muted_fg)
                                                                .child("按固定时长分割文件（留空关闭）")
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
                                                                .child("秒")
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
                                                        .child("保存路径")
                                                )
                                                .child(Input::new(&path_input).small())
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(muted_fg)
                                                        .child("相对于下载路径的保存目录")
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
                    let this = this.clone();

                    move |_, _, _, _| {
                        let interval_input = interval_input.clone();
                        let segment_input = segment_input.clone();
                        let retry_input = retry_input.clone();
                        let reconnect_input = reconnect_input.clone();
                        let path_input = path_input.clone();
                        let selected_format = selected_format.clone();
                        let selected_quality = selected_quality.clone();
                        let auto_transcode = auto_transcode.clone();
                        let this = this.clone();

                        vec![
                            Button::new("save-settings")
                                .primary()
                                .label("保存")
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
                                        .trim().parse::<u64>().unwrap_or(30).max(5);
                                    let path = path_input.read(cx).text().to_string().trim().to_string();
                                    
                                    // 更新配置
                                    let _ = this.update(cx, |page, cx| {
                                        page.record_config.record_format = format;
                                        page.record_config.quality = quality;
                                        page.record_config.auto_transcode = transcode;
                                        page.record_config.check_interval = interval;
                                        page.record_config.segment_duration = segment;
                                        page.record_config.retry_count = retry;
                                        page.record_config.reconnect_delay = reconnect;
                                        page.record_config.output_base_path = PathBuf::from(
                                            if path.is_empty() { "recordings".to_string() } else { path }
                                        );
                                        page.save_record_config();
                                        cx.notify();
                                    });
                                    
                                    window.push_notification(
                                        Notification::success("设置已保存"),
                                        cx,
                                    );
                                    window.close_dialog(cx);
                                }),
                            Button::new("close-settings")
                                .label("取消")
                                .on_click(|_event, window, cx| {
                                    window.close_dialog(cx);
                                }),
                        ]
                    }
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg_color = theme.background;
        let title_color = theme.foreground;
        let desc_color = theme.muted_foreground;

        // 检查是否有错误需要显示
        if let Some(error) = self.last_add_error.take() {
            window.push_notification(
                Notification::error(&format!("添加失败: {}", error)),
                cx,
            );
        }

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
