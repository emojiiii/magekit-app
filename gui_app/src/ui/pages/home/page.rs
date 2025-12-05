//! 首页主组件

use gpui::*;
use gpui_component::input::InputState;
use gpui_router::use_navigate;
use crate::app::AppState;
use magekit_shared::{TaskStatus, TaskState};
use std::sync::Arc;
use std::time::Duration;

use super::widgets::{
    UrlInputCard, 
    VideoPreviewIdle, VideoPreviewLoading, VideoPreviewReady, 
    VideoPreviewDownloading, VideoPreviewCompleted, VideoPreviewError,
    VideoInfo, DownloadOptionsCard, DownloadOptionsData, OutputPathCard,
    QualitySelector, QualityOption
};

/// 格式化时长
fn format_duration(duration: Option<Duration>) -> String {
    match duration {
        Some(d) => {
            let total_secs = d.as_secs();
            let hours = total_secs / 3600;
            let minutes = (total_secs % 3600) / 60;
            let seconds = total_secs % 60;
            
            if hours > 0 {
                format!("{}:{:02}:{:02}", hours, minutes, seconds)
            } else {
                format!("{}:{:02}", minutes, seconds)
            }
        }
        None => "未知".to_string(),
    }
}

/// 格式化下载速度
fn format_speed(bytes_per_sec: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    
    if bytes_per_sec == 0 {
        return "准备中...".to_string();
    } else if bytes_per_sec >= GB {
        format!("{:.2} GB/s", bytes_per_sec as f64 / GB as f64)
    } else if bytes_per_sec >= MB {
        format!("{:.2} MB/s", bytes_per_sec as f64 / MB as f64)
    } else if bytes_per_sec >= KB {
        format!("{:.2} KB/s", bytes_per_sec as f64 / KB as f64)
    } else {
        format!("{} B/s", bytes_per_sec)
    }
}

/// 下载页面状态
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadState {
    Idle,
    Fetching,
    Ready(VideoInfo),
    Downloading { progress: f32, speed: String },
    Completed(String),
    Error(String),
}

impl Default for DownloadState {
    fn default() -> Self {
        Self::Idle
    }
}

/// 首页组件
pub struct HomePage {
    app_state: Arc<AppState>,
    url_input: Entity<InputState>,
    download_state: DownloadState,
    selected_quality: QualityOption,
    output_path: String,
    options: DownloadOptionsData,
    /// 当前解析的 URL (用于下载)
    current_url: Option<String>,
}

impl HomePage {
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let default_path = dirs::download_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "~/Downloads".to_string());
        
        // 创建 URL 输入框状态
        let url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("粘贴 YouTube、Bilibili 等视频链接...")
                .clean_on_escape()
        });
        
        Self {
            app_state,
            url_input,
            download_state: DownloadState::Idle,
            selected_quality: QualityOption::Best,
            output_path: default_path,
            options: DownloadOptionsData::default(),
            current_url: None,
        }
    }
    
    fn get_url(&self, cx: &Context<Self>) -> String {
        self.url_input.read(cx).value().to_string()
    }
    
    fn on_parse(&mut self, cx: &mut Context<Self>) {
        let url = self.get_url(cx);
        if url.trim().is_empty() {
            self.download_state = DownloadState::Error("请输入视频链接".to_string());
            cx.notify();
            return;
        }
        
        self.download_state = DownloadState::Fetching;
        cx.notify();
        
        let app_state = self.app_state.clone();
        let url_clone = url.clone();
        let url_for_save = url.clone();
        
        // 在后台线程中获取视频信息
        let handle = app_state.get_video_info_in_background(url_clone);
        
        // 使用 cx.spawn 轮询检查结果
        cx.spawn(async move |this, cx| {
            // 轮询等待后台线程完成
            loop {
                if handle.is_finished() {
                    break;
                }
                Timer::after(std::time::Duration::from_millis(100)).await;
            }
            
            // 在后台线程获取结果，避免阻塞主线程
            let result: anyhow::Result<magekit_shared::VideoInfo> = smol::unblock(move || {
                handle.join().unwrap_or_else(|_| Err(anyhow::anyhow!("解析线程崩溃")))
            }).await;
            
            // 更新 UI
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(info) => {
                        // 保存当前 URL
                        this.current_url = Some(url_for_save);
                        
                        // 详细日志
                        tracing::info!("🎬 视频信息解析成功:");
                        tracing::info!("  标题: {}", info.title);
                        tracing::info!("  作者: {}", info.uploader.as_deref().unwrap_or("未知"));
                        tracing::info!("  时长: {}", format_duration(info.duration));
                        tracing::info!("  格式数量: {}", info.formats.len());
                        if let Some(thumb) = &info.thumbnail {
                            tracing::info!("  封面: {}", thumb);
                        }
                        
                        // 转换为 GUI 使用的 VideoInfo
                        let gui_info = VideoInfo {
                            title: info.title,
                            duration: format_duration(info.duration),
                            thumbnail: info.thumbnail,
                            uploader: info.uploader,
                        };
                        this.download_state = DownloadState::Ready(gui_info);
                    }
                    Err(e) => {
                        tracing::error!("❌ 视频信息解析失败: {}", e);
                        this.current_url = None;
                        this.download_state = DownloadState::Error(e.to_string());
                    }
                }
                cx.notify();
            });
        }).detach();
    }
    
    fn start_download(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // 获取当前 URL
        let url = match &self.current_url {
            Some(url) => url.clone(),
            None => {
                self.download_state = DownloadState::Error("请先解析视频链接".to_string());
                cx.notify();
                return;
            }
        };
        
        // 获取视频标题（从当前状态中获取）
        let video_title = match &self.download_state {
            DownloadState::Ready(info) => Some(info.title.clone()),
            _ => None,
        };
        
        // 详细日志
        tracing::info!("📥 开始下载任务:");
        tracing::info!("  URL: {}", url);
        tracing::info!("  标题: {}", video_title.as_deref().unwrap_or("未知"));
        tracing::info!("  输出目录: {}", self.output_path);
        tracing::info!("  质量: {:?}", self.selected_quality);
        tracing::info!("  嵌入元数据: {}", self.options.embed_metadata);
        tracing::info!("  嵌入封面: {}", self.options.embed_thumbnail);
        tracing::info!("  下载字幕: {}", self.options.download_subtitles);
        
        // 导航到任务页 - 先调用 navigate 释放 cx 借用
        {
            let mut navigate = use_navigate(cx);
            navigate("/tasks".into());
        }
        window.refresh();
        tracing::info!("📍 已跳转到任务页面");
        
        self.download_state = DownloadState::Downloading { 
            progress: 0.0, 
            speed: "准备中...".to_string() 
        };
        cx.notify();
        
        let app_state = self.app_state.clone();
        let output_dir = std::path::PathBuf::from(&self.output_path);
        
        // 根据选择的质量转换为 format_id
        let format_id = match self.selected_quality {
            QualityOption::Best => "bestvideo+bestaudio/best".to_string(),
            QualityOption::P1080 => "bestvideo[height<=1080]+bestaudio/best[height<=1080]".to_string(),
            QualityOption::P720 => "bestvideo[height<=720]+bestaudio/best[height<=720]".to_string(),
            QualityOption::P480 => "bestvideo[height<=480]+bestaudio/best[height<=480]".to_string(),
            QualityOption::AudioOnly => "bestaudio/best".to_string(),
        };
        
        // 创建任务 ID
        let task_id = uuid::Uuid::new_v4();
        
        // 创建任务状态并添加到任务列表
        let task_status = TaskStatus::new(task_id, url.clone(), video_title.clone());
        {
            let tasks = app_state.tasks.clone();
            let runtime = app_state.runtime.clone();
            let task_status_clone = task_status.clone();
            runtime.spawn(async move {
                let mut tasks = tasks.write().await;
                tasks.insert(task_id, task_status_clone);
            });
        }
        // 转换下载选项
        let options = crate::app::DownloadVideoOptions {
            embed_metadata: self.options.embed_metadata,
            embed_thumbnail: self.options.embed_thumbnail,
            download_subtitles: self.options.download_subtitles,
            audio_only: matches!(self.selected_quality, QualityOption::AudioOnly),
        };
        
        // 创建共享的进度变量
        let progress_percent = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let progress_speed = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let downloaded_bytes = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let total_bytes = Arc::new(std::sync::atomic::AtomicU64::new(0));
        
        // 克隆用于回调
        let pp = progress_percent.clone();
        let ps = progress_speed.clone();
        let db = downloaded_bytes.clone();
        let tb = total_bytes.clone();
        
        // 进度回调
        let progress_callback: Arc<dyn Fn(f32, u64, u64, u64) + Send + Sync> = Arc::new(move |percent, speed, downloaded, total| {
            pp.store((percent * 100.0) as u32, std::sync::atomic::Ordering::Relaxed);
            ps.store(speed, std::sync::atomic::Ordering::Relaxed);
            db.store(downloaded, std::sync::atomic::Ordering::Relaxed);
            tb.store(total, std::sync::atomic::Ordering::Relaxed);
        });
        
        // 在后台线程中运行下载
        let handle = app_state.download_video_in_background(
            url,
            output_dir,
            format_id,
            options,
            progress_callback,
        );
        
        // 克隆用于更新任务状态
        let tasks_for_update = app_state.tasks.clone();
        let _runtime_for_update = app_state.runtime.clone();
        
        // 使用 cx.spawn 来轮询检查下载结果和更新进度
        cx.spawn(async move |this, cx| {
            // 首先将任务状态更新为 Downloading
            {
                let tasks = tasks_for_update.clone();
                smol::unblock(move || {
                    // 使用 blocking_write 同步更新
                    let mut tasks = tasks.blocking_write();
                    if let Some(task) = tasks.get_mut(&task_id) {
                        task.state = TaskState::Downloading;
                        task.started_at = Some(std::time::SystemTime::now());
                    }
                }).await;
            }
            
            // 轮询等待后台线程完成
            loop {
                if handle.is_finished() {
                    break;
                }
                
                // 更新进度 UI
                let percent = progress_percent.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.0;
                let speed = progress_speed.load(std::sync::atomic::Ordering::Relaxed);
                let downloaded = downloaded_bytes.load(std::sync::atomic::Ordering::Relaxed);
                let total = total_bytes.load(std::sync::atomic::Ordering::Relaxed);
                
                let speed_str = format_speed(speed);
                
                // 更新任务列表中的进度
                {
                    let tasks = tasks_for_update.clone();
                    smol::unblock(move || {
                        let mut tasks = tasks.blocking_write();
                        if let Some(task) = tasks.get_mut(&task_id) {
                            task.progress = percent;
                            task.speed = if speed > 0 { Some(speed) } else { None };
                            task.downloaded_bytes = downloaded;
                            task.total_bytes = if total > 0 { Some(total) } else { None };
                        }
                    }).await;
                }
                
                let _ = this.update(cx, |this, cx| {
                    this.download_state = DownloadState::Downloading {
                        progress: percent,
                        speed: speed_str,
                    };
                    cx.notify();
                });
                
                Timer::after(std::time::Duration::from_millis(200)).await;
            }
            
            // 在后台线程获取结果
            let result: anyhow::Result<std::path::PathBuf> = smol::unblock(move || {
                handle.join().unwrap_or_else(|_| Err(anyhow::anyhow!("下载线程崩溃")))
            }).await;
            
            // 更新任务状态
            let tasks = tasks_for_update.clone();
            let result_for_task = result.as_ref().map(|p| p.clone()).map_err(|e| e.to_string());
            smol::unblock(move || {
                let mut tasks = tasks.blocking_write();
                if let Some(task) = tasks.get_mut(&task_id) {
                    match result_for_task {
                        Ok(path) => {
                            task.state = TaskState::Completed;
                            task.progress = 1.0;
                            task.output_path = Some(path);
                            task.completed_at = Some(std::time::SystemTime::now());
                        }
                        Err(error) => {
                            task.state = TaskState::Failed(error);
                            task.completed_at = Some(std::time::SystemTime::now());
                        }
                    }
                }
            }).await;
            
            // 更新 UI
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(path) => {
                        this.download_state = DownloadState::Completed(
                            path.to_string_lossy().to_string()
                        );
                        tracing::info!("✅ 下载完成: {}", path.display());
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        this.download_state = DownloadState::Error(error_msg.clone());
                        tracing::error!("❌ 下载失败: {}", error_msg);
                    }
                }
                cx.notify();
            });
        }).detach();
    }
    
    fn select_quality(&mut self, quality: QualityOption, cx: &mut Context<Self>) {
        self.selected_quality = quality;
        cx.notify();
    }
    
    fn toggle_option(&mut self, option: &str, cx: &mut Context<Self>) {
        match option {
            "metadata" => self.options.embed_metadata = !self.options.embed_metadata,
            "thumbnail" => self.options.embed_thumbnail = !self.options.embed_thumbnail,
            "subtitles" => self.options.download_subtitles = !self.options.download_subtitles,
            "audio" => self.options.audio_only = !self.options.audio_only,
            _ => {}
        }
        cx.notify();
    }
    
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.url_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.download_state = DownloadState::Idle;
        cx.notify();
    }
}

impl Render for HomePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let url = self.get_url(cx);
        let is_url_empty = url.trim().is_empty();
        let is_loading = matches!(self.download_state, DownloadState::Fetching);

        div()
            .id("home-page")
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(0x09090b))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p(px(24.0))
                    .gap(px(24.0))
                    // 页面标题
                    .child(self.render_header())
                    // URL 输入区域
                    .child(
                        UrlInputCard::new(&self.url_input)
                            .loading(is_loading)
                            .empty(is_url_empty)
                            .on_parse(cx.listener(|this, _ev, _window, cx| {
                                this.on_parse(cx);
                            }))
                    )
                    // 状态内容
                    .child(self.render_state_content(cx))
                    // 快捷设置
                    .child(self.render_quick_settings(cx))
                    // 下载选项
                    .child(
                        DownloadOptionsCard::new(self.options.clone())
                            .on_toggle_metadata(cx.listener(|this, _ev, _window, cx| {
                                this.toggle_option("metadata", cx);
                            }))
                            .on_toggle_thumbnail(cx.listener(|this, _ev, _window, cx| {
                                this.toggle_option("thumbnail", cx);
                            }))
                            .on_toggle_subtitles(cx.listener(|this, _ev, _window, cx| {
                                this.toggle_option("subtitles", cx);
                            }))
                            .on_toggle_audio(cx.listener(|this, _ev, _window, cx| {
                                this.toggle_option("audio", cx);
                            }))
                    )
            )
    }
}

impl HomePage {
    fn render_header(&self) -> impl IntoElement {
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
                            .text_color(rgb(0xfafafa))
                            .child("视频下载")
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xa1a1aa))
                            .child("粘贴视频链接，一键下载")
                    )
            )
    }

    fn render_state_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.download_state {
            DownloadState::Idle => VideoPreviewIdle.into_any_element(),
            DownloadState::Fetching => VideoPreviewLoading.into_any_element(),
            DownloadState::Ready(info) => {
                VideoPreviewReady::new(info.clone())
                    .on_cancel(cx.listener(|this, _ev, window, cx| {
                        this.reset(window, cx);
                    }))
                    .on_download(cx.listener(|this, _ev, window, cx| {
                        this.start_download(window, cx);
                    }))
                    .into_any_element()
            }
            DownloadState::Downloading { progress, speed } => {
                VideoPreviewDownloading::new(*progress, speed.clone()).into_any_element()
            }
            DownloadState::Completed(path) => {
                VideoPreviewCompleted::new(path.clone())
                    .on_new_download(cx.listener(|this, _ev, window, cx| {
                        this.reset(window, cx);
                    }))
                    .into_any_element()
            }
            DownloadState::Error(msg) => {
                VideoPreviewError::new(msg.clone())
                    .on_retry(cx.listener(|this, _ev, _window, cx| {
                        this.download_state = DownloadState::Idle;
                        cx.notify();
                    }))
                    .into_any_element()
            }
        }
    }

    fn render_quick_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .gap(px(16.0))
            .child(
                QualitySelector::new(self.selected_quality)
                    .on_select(cx.listener(|this, quality: &QualityOption, _window, cx| {
                        this.select_quality(*quality, cx);
                    }))
            )
            .child(
                OutputPathCard::new(self.output_path.clone())
            )
    }
}
