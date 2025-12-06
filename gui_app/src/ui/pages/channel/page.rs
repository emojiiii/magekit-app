//! 频道/作者页面主组件

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::input::{Input, InputState};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::notification::Notification;
use gpui_component::spinner::Spinner;
use gpui_component::{ActiveTheme, Disableable, Icon, IconName, Sizable};
use gpui_component::WindowExt;
use gpui_router::use_navigate;
use crate::app::AppState;
use magekit_shared::{ChannelInfo, ChannelVideoEntry, TaskStatus, TaskState};
use std::sync::Arc;
use std::time::Duration;

/// 格式化时长
fn format_duration(seconds: Option<u64>) -> String {
    match seconds {
        Some(secs) => {
            let hours = secs / 3600;
            let minutes = (secs % 3600) / 60;
            let seconds = secs % 60;
            
            if hours > 0 {
                format!("{}:{:02}:{:02}", hours, minutes, seconds)
            } else {
                format!("{}:{:02}", minutes, seconds)
            }
        }
        None => "未知".to_string(),
    }
}

/// 页面状态
#[derive(Debug, Clone, PartialEq)]
pub enum ChannelState {
    /// 空闲状态
    Idle,
    /// 正在解析
    Parsing,
    /// 解析完成
    Ready(ChannelInfo),
    /// 错误状态
    Error(String),
}

impl Default for ChannelState {
    fn default() -> Self {
        Self::Idle
    }
}

/// 频道/作者页面
pub struct ChannelPage {
    app_state: Arc<AppState>,
    url_input: Entity<InputState>,
    state: ChannelState,
    /// 下载路径
    output_path: String,
}

/// 视频项的固定高度
const VIDEO_ITEM_HEIGHT: f32 = 70.0;

impl ChannelPage {
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let default_path = dirs::download_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "~/Downloads".to_string());
        
        let url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("粘贴 YouTube 频道、播放列表或 Bilibili UP主空间链接...")
                .clean_on_escape()
        });
        
        Self {
            app_state,
            url_input,
            state: ChannelState::Idle,
            output_path: default_path,
        }
    }
    
    fn get_url(&self, cx: &Context<Self>) -> String {
        self.url_input.read(cx).value().to_string()
    }
    
    /// 解析频道
    fn on_parse(&mut self, cx: &mut Context<Self>) {
        let url = self.get_url(cx);
        if url.trim().is_empty() {
            self.state = ChannelState::Error("请输入频道链接".to_string());
            cx.notify();
            return;
        }
        
        self.state = ChannelState::Parsing;
        cx.notify();
        
        let app_state = self.app_state.clone();
        let url_clone = url.clone();
        
        // 在后台线程中获取频道信息
        cx.spawn(async move |this, cx| {
            // 使用 smol::unblock 执行阻塞的 tokio 操作
            let result = smol::unblock(move || {
                // 获取 tokio runtime
                let runtime = app_state.runtime.clone();
                let config = app_state.config();
                let cookies = config.advanced.cookies.clone();
                
                // 在 tokio runtime 中执行异步操作
                runtime.block_on(async {
                    let cookies_opt = if cookies.is_empty() { None } else { Some(cookies.as_slice()) };
                    app_state.tool_manager.get_channel_videos(&url_clone, cookies_opt).await
                })
            }).await;
            
            // 更新 UI
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(mut info) => {
                        tracing::info!("📺 频道解析成功: {} - {} 个视频", info.title, info.video_count);
                        // 默认选中所有视频
                        for entry in &mut info.entries {
                            entry.selected = true;
                        }
                        this.state = ChannelState::Ready(info);
                    }
                    Err(e) => {
                        tracing::error!("❌ 频道解析失败: {}", e);
                        this.state = ChannelState::Error(e.to_string());
                    }
                }
                cx.notify();
            });
        }).detach();
    }
    
    /// 切换视频选中状态
    fn toggle_video(&mut self, index: usize, cx: &mut Context<Self>) {
        if let ChannelState::Ready(ref mut info) = self.state {
            if let Some(entry) = info.entries.get_mut(index) {
                entry.selected = !entry.selected;
                cx.notify();
            }
        }
    }
    
    /// 全选/取消全选
    fn select_all(&mut self, selected: bool, cx: &mut Context<Self>) {
        if let ChannelState::Ready(ref mut info) = self.state {
            for entry in &mut info.entries {
                entry.selected = selected;
            }
            cx.notify();
        }
    }
    
    /// 开始下载选中的视频
    fn download_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected_entries: Vec<ChannelVideoEntry> = if let ChannelState::Ready(ref info) = self.state {
            info.entries.iter()
                .filter(|e| e.selected)
                .cloned()
                .collect()
        } else {
            return;
        };
        
        if selected_entries.is_empty() {
            window.push_notification(
                Notification::error("请至少选择一个视频"),
                cx
            );
            return;
        }
        
        let count = selected_entries.len();
        tracing::info!("📥 开始批量下载 {} 个视频", count);
        
        // 导航到任务页
        {
            let mut navigate = use_navigate(cx);
            navigate("/tasks".into());
        }
        window.refresh();
        
        let app_state = self.app_state.clone();
        let output_dir = std::path::PathBuf::from(&self.output_path);
        
        // 为每个视频创建下载任务
        for entry in selected_entries {
            let task_id = uuid::Uuid::new_v4();
            let url = entry.url.clone();
            let title = Some(entry.title.clone());
            
            // 创建任务状态
            let download_params = magekit_shared::DownloadParams {
                output_dir: output_dir.clone(),
                format_id: "bestvideo+bestaudio/best".to_string(),
                embed_metadata: true,
                embed_thumbnail: false,
                download_subtitles: false,
                audio_only: false,
            };
            
            let mut task_status = TaskStatus::new(task_id, url.clone(), title.clone());
            task_status.download_params = Some(download_params);
            
            // 添加任务到列表
            let tasks = app_state.tasks.clone();
            let runtime = app_state.runtime.clone();
            let task_status_clone = task_status.clone();
            runtime.spawn(async move {
                let mut tasks = tasks.write().await;
                tasks.insert(task_id, task_status_clone);
            });
            
            // 克隆 tasks 用于进度回调
            let tasks_for_callback = app_state.tasks.clone();
            
            let progress_callback: Arc<dyn Fn(f32, u64, u64, u64) + Send + Sync> = Arc::new(move |percent, speed, downloaded, total| {
                let mut tasks = tasks_for_callback.blocking_write();
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.progress = percent;
                    task.speed = if speed > 0 { Some(speed) } else { None };
                    task.downloaded_bytes = downloaded;
                    task.total_bytes = if total > 0 { Some(total) } else { None };
                }
            });
            
            let options = crate::app::DownloadVideoOptions {
                embed_metadata: true,
                embed_thumbnail: false,
                download_subtitles: false,
                audio_only: false,
            };
            
            // 在后台启动下载
            let handle = app_state.download_video_in_background(
                task_id,
                url,
                output_dir.clone(),
                "bestvideo+bestaudio/best".to_string(),
                options,
                progress_callback,
                title,
            );
            
            // 启动监控任务
            let tasks_for_update = app_state.tasks.clone();
            let app_state_for_cleanup = app_state.clone();
            
            cx.spawn(async move |_this, _cx| {
                // 更新任务状态为下载中
                {
                    let tasks = tasks_for_update.clone();
                    smol::unblock(move || {
                        let mut tasks = tasks.blocking_write();
                        if let Some(task) = tasks.get_mut(&task_id) {
                            task.state = TaskState::Downloading;
                            task.started_at = Some(std::time::SystemTime::now());
                        }
                    }).await;
                }
                
                // 等待下载完成
                loop {
                    if handle.is_finished() {
                        break;
                    }
                    Timer::after(Duration::from_millis(100)).await;
                }
                
                // 获取结果
                let result: anyhow::Result<std::path::PathBuf> = smol::unblock(move || {
                    handle.join().unwrap_or_else(|_| Err(anyhow::anyhow!("下载线程崩溃")))
                }).await;
                
                // 清理
                app_state_for_cleanup.cleanup_download_task(task_id);
                
                // 更新最终状态
                let tasks = tasks_for_update.clone();
                let result_for_task = result.as_ref().map(|p| p.clone()).map_err(|e| e.to_string());
                smol::unblock(move || {
                    let mut tasks = tasks.blocking_write();
                    if let Some(task) = tasks.get_mut(&task_id) {
                        match result_for_task {
                            Ok(path) => {
                                task.state = TaskState::Completed;
                                task.progress = 1.0;
                                task.output_path = Some(path.clone());
                                task.completed_at = Some(std::time::SystemTime::now());
                                if let Ok(metadata) = std::fs::metadata(&path) {
                                    task.total_bytes = Some(metadata.len());
                                    task.downloaded_bytes = metadata.len();
                                }
                            }
                            Err(error) => {
                                if error.contains("下载已暂停") {
                                    task.state = TaskState::Paused;
                                } else if error.contains("下载已取消") {
                                    task.state = TaskState::Cancelled;
                                } else {
                                    task.state = TaskState::Failed(error);
                                    task.completed_at = Some(std::time::SystemTime::now());
                                }
                            }
                        }
                    }
                }).await;
            }).detach();
        }
        
        window.push_notification(
            Notification::success(format!("已添加 {} 个下载任务", count)),
            cx
        );
    }
    
    /// 渲染空闲状态
    fn render_idle(&self, _cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py_12()
            .gap_4()
            .child(
                Icon::new(IconName::Folder)
                    .size_16()
                    .text_color(gpui::hsla(0.0, 0.0, 0.5, 0.3))
            )
            .child(
                div()
                    .text_lg()
                    .text_color(gpui::hsla(0.0, 0.0, 0.5, 0.5))
                    .child("输入频道链接开始解析")
            )
            .child(
                div()
                    .text_sm()
                    .text_color(gpui::hsla(0.0, 0.0, 0.5, 0.4))
                    .max_w(px(400.0))
                    .text_center()
                    .child("支持 YouTube 频道 (@username)、播放列表、Bilibili UP主空间等")
            )
    }
    
    /// 渲染解析中状态
    fn render_parsing(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py_12()
            .gap_4()
            .child(
                Spinner::new()
                    .large()
                    .color(theme.primary)
            )
            .child(
                div()
                    .text_lg()
                    .child("正在解析频道...")
            )
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("这可能需要一些时间，取决于视频数量")
            )
    }
    
    /// 渲染错误状态
    fn render_error(&self, error: &str) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py_12()
            .gap_4()
            .child(
                Icon::new(IconName::CircleX)
                    .size_12()
                    .text_color(gpui::hsla(0.0, 0.7, 0.5, 1.0))
            )
            .child(
                div()
                    .text_lg()
                    .text_color(gpui::hsla(0.0, 0.7, 0.5, 1.0))
                    .child("解析失败")
            )
            .child(
                div()
                    .text_sm()
                    .text_color(gpui::hsla(0.0, 0.0, 0.5, 0.6))
                    .max_w(px(400.0))
                    .text_center()
                    .child(error.to_string())
            )
    }
    
    /// 渲染视频列表（使用滚动列表）
    fn render_video_list(&self, info: &ChannelInfo, cx: &mut Context<Self>) -> impl IntoElement {
        let channel_title = info.title.clone();
        let selected_count = info.entries.iter().filter(|e| e.selected).count();
        let total_count = info.entries.len();
        let all_selected = selected_count == total_count && total_count > 0;
        let theme = cx.theme();
        let entity = cx.entity().clone();
        
        // 提取主题颜色供后续使用
        let primary = theme.primary;
        let border_color = theme.border;
        let secondary = theme.secondary;
        let muted = theme.muted;
        let muted_foreground = theme.muted_foreground;
        let primary_foreground = theme.primary_foreground;
        let foreground = theme.foreground;
        
        // 先创建控制栏按钮的监听器
        let select_all_listener = cx.listener(move |this, checked: &bool, _window, cx| {
            this.select_all(*checked, cx);
        });
        
        let download_listener = cx.listener(|this, _, window, cx| {
            this.download_selected(window, cx);
        });
        
        // 预先收集视频项数据
        let video_items: Vec<_> = info.entries.iter().enumerate().map(|(index, entry)| {
            let is_selected = entry.selected;
            let entry_title = entry.title.clone();
            let entry_duration = entry.duration;
            let entry_thumbnail = entry.thumbnail.clone();
            let entity_clone = entity.clone();
            
            div()
                .id(SharedString::from(format!("video-item-{}", index)))
                .w_full()
                .h(px(VIDEO_ITEM_HEIGHT))
                .p_3()
                .rounded_md()
                .border_1()
                .when(is_selected, |div| {
                    div.border_color(primary.opacity(0.5))
                        .bg(primary.opacity(0.1))
                })
                .when(!is_selected, |div| {
                    div.border_color(border_color)
                        .bg(secondary.opacity(0.3))
                })
                .hover(|style| style.bg(secondary))
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    let _ = entity_clone.update(cx, |this, cx| {
                        this.toggle_video(index, cx);
                    });
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .h_full()
                        // 复选框
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(24.0))
                                .h(px(24.0))
                                .rounded(px(4.0))
                                .bg(if is_selected { primary } else { muted })
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(if is_selected { primary_foreground } else { muted_foreground })
                                        .child(if is_selected { "✓" } else { "" })
                                )
                        )
                        // 序号
                        .child(
                            div()
                                .w(px(32.0))
                                .text_sm()
                                .text_color(muted_foreground)
                                .child(format!("#{}", index + 1))
                        )
                        // 缩略图
                        .child(
                            div()
                                .w(px(80.0))
                                .h(px(45.0))
                                .rounded(px(4.0))
                                .bg(muted)
                                .flex()
                                .items_center()
                                .justify_center()
                                .overflow_hidden()
                                .when_some(entry_thumbnail.clone(), |el, thumb_url| {
                                    el.child(
                                        img(thumb_url)
                                            .size_full()
                                            .object_fit(ObjectFit::Cover)
                                    )
                                })
                                .when(entry_thumbnail.is_none(), |el| {
                                    el.child(
                                        Icon::new(IconName::Folder)
                                            .size_4()
                                            .text_color(muted_foreground)
                                    )
                                })
                        )
                        // 视频信息
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .overflow_hidden()
                                // 标题
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(foreground)
                                        .truncate()
                                        .child(entry_title)
                                )
                                // 时长
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted_foreground)
                                        .child(format_duration(entry_duration))
                                )
                        )
                )
        }).collect();
        
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap_4()
            // 标题栏
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
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(channel_title)
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted_foreground)
                                    .child(format!("共 {} 个视频，已选择 {}", total_count, selected_count))
                            )
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            // 全选按钮
                            .child(
                                Checkbox::new("select-all")
                                    .checked(all_selected)
                                    .label(if all_selected { "取消全选" } else { "全选" })
                                    .on_click(select_all_listener)
                            )
                            // 下载按钮
                            .child(
                                Button::new("download-selected")
                                    .primary()
                                    .icon(IconName::ArrowDown)
                                    .label(format!("下载选中 ({})", selected_count))
                                    .disabled(selected_count == 0)
                                    .on_click(download_listener)
                            )
                    )
            )
            // 视频列表 - 使用滚动容器
            .child(
                div()
                    .id("video-list-container")
                    .flex_1()
                    .w_full()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .pb_4()
                            .children(video_items)
                    )
            )
    }
}

impl Render for ChannelPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        
        div()
            .id("channel-page")
            .flex()
            .flex_col()
            .size_full()
            .p_6()
            .gap_6()
            .bg(theme.background)
            // 页面标题
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Icon::new(IconName::Folder)
                            .size_6()
                            .text_color(theme.foreground)
                    )
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.foreground)
                            .child("频道/作者")
                    )
            )
            // URL 输入区域
            .child(
                div()
                    .w_full()
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.secondary)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            // 说明文字
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child("支持 YouTube 频道、播放列表、Bilibili UP主空间等")
                            )
                            // 输入框和按钮
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .child(
                                                Input::new(&self.url_input)
                                                    .cleanable(true)
                                            )
                                    )
                                    .child(
                                        Button::new("parse-channel")
                                            .primary()
                                            .label(if matches!(self.state, ChannelState::Parsing) { "解析中..." } else { "解析" })
                                            .icon(if matches!(self.state, ChannelState::Parsing) { IconName::LoaderCircle } else { IconName::Search })
                                            .disabled(matches!(self.state, ChannelState::Parsing))
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.on_parse(cx);
                                            }))
                                    )
                            )
                    )
            )
            // 内容区域
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        match &self.state {
                            ChannelState::Idle => self.render_idle(cx).into_any_element(),
                            ChannelState::Parsing => self.render_parsing(cx).into_any_element(),
                            ChannelState::Error(e) => self.render_error(e).into_any_element(),
                            ChannelState::Ready(info) => {
                                let info = info.clone();
                                self.render_video_list(&info, cx).into_any_element()
                            }
                        }
                    )
            )
    }
}
