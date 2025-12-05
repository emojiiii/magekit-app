//! 首页主组件

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::input::InputState;
use gpui_component::notification::Notification;
use gpui_component::ActiveTheme;
use gpui_component::WindowExt;
use gpui_router::use_navigate;
use crate::app::AppState;
use magekit_shared::{TaskStatus, TaskState};
use std::sync::Arc;
use std::time::Duration;

use super::widgets::{
    UrlInputCard, 
    VideoPreviewIdle, VideoPreviewLoading, VideoPreviewReady, 
    VideoPreviewDownloading, VideoPreviewCompleted, VideoPreviewError,
    VideoInfo, VideoFormatInfo, DownloadOptionsCard, DownloadOptionsData,
    QualityOption
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

/// 将 yt-dlp 格式转换为 GUI 友好的格式列表
/// 
/// yt-dlp 返回很多格式，我们需要：
/// 1. 过滤掉重复的分辨率
/// 2. 优先选择常见格式 (mp4, webm)
/// 3. 生成用户友好的标签 (如 "1080p", "720p", "音频")
fn convert_formats(formats: &[magekit_shared::VideoFormat]) -> Vec<VideoFormatInfo> {
    use std::collections::HashMap;
    
    tracing::info!("🔄 开始转换格式列表，原始格式数量: {}", formats.len());
    
    // 打印前几个格式的详细信息用于调试
    for (i, fmt) in formats.iter().take(5).enumerate() {
        tracing::info!("  原始格式 {}: id={}, ext={}, res={:?}, vcodec={:?}, acodec={:?}, quality={:?}", 
            i, fmt.format_id, fmt.ext, fmt.resolution, fmt.vcodec, fmt.acodec, fmt.quality);
    }
    
    let mut result = Vec::new();
    let mut seen_resolutions: HashMap<String, bool> = HashMap::new();
    
    // 先按质量排序：优先高分辨率
    let mut sorted_formats: Vec<_> = formats.iter().collect();
    sorted_formats.sort_by(|a, b| {
        // 解析分辨率高度进行比较
        let height_a = a.resolution.as_ref()
            .and_then(|r| r.split('x').last())
            .and_then(|h| h.parse::<u32>().ok())
            .unwrap_or(0);
        let height_b = b.resolution.as_ref()
            .and_then(|r| r.split('x').last())
            .and_then(|h| h.parse::<u32>().ok())
            .unwrap_or(0);
        height_b.cmp(&height_a) // 降序
    });
    
    // 处理视频格式
    for fmt in &sorted_formats {
        // 判断是否有视频：vcodec 存在且不是 "none"
        let has_video = fmt.vcodec.as_ref()
            .map(|v| !v.is_empty() && v != "none")
            .unwrap_or(false);
        // 判断是否有音频：acodec 存在且不是 "none"  
        let has_audio = fmt.acodec.as_ref()
            .map(|a| !a.is_empty() && a != "none")
            .unwrap_or(false);
        
        // 如果 vcodec 和 acodec 都是 None，但有分辨率，可能是合并格式
        let is_combined = fmt.vcodec.is_none() && fmt.acodec.is_none() && fmt.resolution.is_some();
        
        // 跳过没有视频也没有音频的格式（除非是合并格式）
        if !has_video && !has_audio && !is_combined {
            continue;
        }
        
        // 对于视频格式或合并格式，根据分辨率去重
        if has_video || is_combined {
            let resolution_key = fmt.resolution.clone().unwrap_or_else(|| "unknown".to_string());
            
            // 跳过已处理的分辨率
            if seen_resolutions.contains_key(&resolution_key) {
                continue;
            }
            seen_resolutions.insert(resolution_key.clone(), true);
            
            // 生成用户友好的标签
            let label = if let Some(res) = &fmt.resolution {
                // 尝试提取高度作为标签，如 "1920x1080" -> "1080p"
                if let Some(height) = res.split('x').last() {
                    if let Ok(h) = height.parse::<u32>() {
                        format!("{}p", h)
                    } else {
                        res.clone()
                    }
                } else {
                    res.clone()
                }
            } else {
                fmt.quality.clone().unwrap_or_else(|| "视频".to_string())
            };
            
            tracing::info!("  添加视频格式: {} - {} ({})", fmt.format_id, label, fmt.ext);
            
            result.push(VideoFormatInfo {
                format_id: fmt.format_id.clone(),
                label,
                ext: fmt.ext.clone(),
                filesize: fmt.filesize,
                has_video: true,
                has_audio: has_audio || is_combined,
            });
        }
    }
    
    // 处理纯音频格式 (去重，只保留最高质量的几个)
    let mut audio_seen: HashMap<String, bool> = HashMap::new();
    for fmt in &sorted_formats {
        let has_video = fmt.vcodec.as_ref()
            .map(|v| !v.is_empty() && v != "none")
            .unwrap_or(false);
        let has_audio = fmt.acodec.as_ref()
            .map(|a| !a.is_empty() && a != "none")
            .unwrap_or(false);
        
        if !has_video && has_audio {
            let ext_key = fmt.ext.clone();
            if audio_seen.contains_key(&ext_key) {
                continue;
            }
            audio_seen.insert(ext_key.clone(), true);
            
            // 限制音频格式数量
            if result.iter().filter(|f| !f.has_video).count() >= 3 {
                break;
            }
            
            let label = format!("音频 ({})", fmt.ext.to_uppercase());
            
            tracing::info!("  添加音频格式: {} - {}", fmt.format_id, label);
            
            result.push(VideoFormatInfo {
                format_id: fmt.format_id.clone(),
                label,
                ext: fmt.ext.clone(),
                filesize: fmt.filesize,
                has_video: false,
                has_audio: true,
            });
        }
    }
    
    tracing::info!("🔄 格式转换完成，结果数量: {}", result.len());
    
    // 如果没有任何格式被识别，添加一个默认的 "最佳质量" 选项
    if result.is_empty() {
        tracing::warn!("⚠️ 未能识别任何格式，添加默认选项");
        result.push(VideoFormatInfo {
            format_id: "bestvideo+bestaudio/best".to_string(),
            label: "最佳质量".to_string(),
            ext: "mp4".to_string(),
            filesize: None,
            has_video: true,
            has_audio: true,
        });
    }
    
    result
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
    selected_format_id: Option<String>,  // 用户选中的格式 ID
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
            selected_format_id: None,
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
                        this.current_url = Some(url_for_save.clone());
                        
                        // 详细日志
                        tracing::info!("🎬 视频信息解析成功:");
                        tracing::info!("  标题: {}", info.title);
                        tracing::info!("  作者: {}", info.uploader.as_deref().unwrap_or("未知"));
                        tracing::info!("  时长: {}", format_duration(info.duration));
                        tracing::info!("  格式数量: {}", info.formats.len());
                        if let Some(thumb) = &info.thumbnail {
                            tracing::info!("  封面: {}", thumb);
                        }
                        
                        // 将 yt-dlp 格式转换为 GUI 友好的格式
                        let gui_formats = convert_formats(&info.formats);
                        tracing::info!("  转换后格式数量: {}", gui_formats.len());
                        for fmt in &gui_formats {
                            tracing::info!("    - {} ({}) {} video={} audio={}", 
                                fmt.label, fmt.format_id, fmt.ext, fmt.has_video, fmt.has_audio);
                        }
                        
                        // 默认选中第一个视频格式
                        let default_format_id = gui_formats.iter()
                            .find(|f| f.has_video)
                            .map(|f| f.format_id.clone());
                        this.selected_format_id = default_format_id;
                        
                        // 转换为 GUI 使用的 VideoInfo
                        let gui_info = VideoInfo {
                            title: info.title,
                            duration: format_duration(info.duration),
                            thumbnail: info.thumbnail,
                            uploader: info.uploader,
                            formats: gui_formats,
                            url: url_for_save,
                        };
                        this.download_state = DownloadState::Ready(gui_info);
                    }
                    Err(e) => {
                        tracing::error!("❌ 视频信息解析失败: {}", e);
                        this.current_url = None;
                        this.selected_format_id = None;
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
        
        // 注意：不修改首页状态，保持 Ready 状态
        // 下载进度在任务列表页显示
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
        
        // 进度回调 - 添加调试日志
        let progress_callback: Arc<dyn Fn(f32, u64, u64, u64) + Send + Sync> = Arc::new(move |percent, speed, downloaded, total| {
            tracing::info!("📥 回调收到进度: {:.1}%, 速度: {} B/s, 已下载: {} B, 总大小: {} B", 
                percent * 100.0, speed, downloaded, total);
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
            video_title.clone(),
        );
        
        // 克隆用于更新任务状态
        let tasks_for_update = app_state.tasks.clone();
        let _runtime_for_update = app_state.runtime.clone();
        
        // 使用 cx.spawn 来轮询检查下载结果和更新进度
        cx.spawn(async move |_this, _cx| {
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
            
            // 轮询等待后台线程完成 (start_download)
            tracing::info!("🔄 开始轮询下载进度 (start_download)");
            let mut poll_count = 0u32;
            loop {
                if handle.is_finished() {
                    tracing::info!("🏁 下载线程已完成");
                    break;
                }
                
                // 更新进度 UI
                let percent = progress_percent.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.0;
                let speed = progress_speed.load(std::sync::atomic::Ordering::Relaxed);
                let downloaded = downloaded_bytes.load(std::sync::atomic::Ordering::Relaxed);
                let total = total_bytes.load(std::sync::atomic::Ordering::Relaxed);
                
                // 每 5 次轮询打印一次日志
                poll_count += 1;
                if poll_count % 5 == 0 {
                    tracing::info!("🔄 轮询 #{}: 进度={:.1}%, 速度={} B/s, 已下载={} B, 总大小={} B", 
                        poll_count, percent * 100.0, speed, downloaded, total);
                }
                
                // 更新任务列表中的进度（不更新首页状态）
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
                            task.output_path = Some(path.clone());
                            task.completed_at = Some(std::time::SystemTime::now());
                            
                            // 获取文件大小
                            if let Ok(metadata) = std::fs::metadata(&path) {
                                task.total_bytes = Some(metadata.len());
                                task.downloaded_bytes = metadata.len();
                            }
                        }
                        Err(error) => {
                            task.state = TaskState::Failed(error);
                            task.completed_at = Some(std::time::SystemTime::now());
                        }
                    }
                }
            }).await;
            
            // 日志输出（不更新首页状态）
            match result {
                Ok(path) => {
                    tracing::info!("✅ 下载完成: {}", path.display());
                }
                Err(e) => {
                    tracing::error!("❌ 下载失败: {}", e);
                }
            }
        }).detach();
    }
    
    fn select_quality(&mut self, quality: QualityOption, cx: &mut Context<Self>) {
        self.selected_quality = quality;
        cx.notify();
    }
    
    /// 选择格式
    fn select_format(&mut self, format_id: String, cx: &mut Context<Self>) {
        tracing::info!("📝 选择格式: {}", format_id);
        self.selected_format_id = Some(format_id);
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
    
    /// 使用指定的格式 ID 开始下载
    fn start_download_with_format(&mut self, format_id: String, window: &mut Window, cx: &mut Context<Self>) {
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
        tracing::info!("  格式 ID: {}", format_id);
        tracing::info!("  嵌入元数据: {}", self.options.embed_metadata);
        tracing::info!("  嵌入封面: {}", self.options.embed_thumbnail);
        tracing::info!("  下载字幕: {}", self.options.download_subtitles);
        
        // 判断是否是音频格式（在修改状态前判断）
        let is_audio_only = match &self.download_state {
            DownloadState::Ready(info) => {
                info.formats.iter()
                    .find(|f| f.format_id == format_id)
                    .map(|f| !f.has_video && f.has_audio)
                    .unwrap_or(false)
            }
            _ => false,
        };
        
        // 导航到任务页 - 先调用 navigate 释放 cx 借用
        {
            let mut navigate = use_navigate(cx);
            navigate("/tasks".into());
        }
        window.refresh();
        tracing::info!("📍 已跳转到任务页面");
        
        // 注意：不修改首页状态，保持 Ready 状态
        // 下载进度在任务列表页显示
        cx.notify();
        
        let app_state = self.app_state.clone();
        let output_dir = std::path::PathBuf::from(&self.output_path);
        
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
            audio_only: is_audio_only,
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
            tracing::info!("📥 [with_format] 进度回调: percent={:.3}, speed={}, downloaded={}, total={}", percent, speed, downloaded, total);
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
            video_title.clone(),
        );
        
        // 克隆用于更新任务状态
        let tasks_for_update = app_state.tasks.clone();
        
        // 使用 cx.spawn 来轮询检查下载结果和更新进度
        cx.spawn(async move |_this, _cx| {
            // 首先将任务状态更新为 Downloading
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
            
            // 轮询等待后台线程完成 (start_download_with_format)
            tracing::info!("🔄 开始轮询下载进度 (start_download_with_format)");
            let mut poll_count = 0u32;
            loop {
                if handle.is_finished() {
                    tracing::info!("🏁 下载线程已完成");
                    break;
                }
                
                // 更新进度 UI
                let percent = progress_percent.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.0;
                let speed = progress_speed.load(std::sync::atomic::Ordering::Relaxed);
                let downloaded = downloaded_bytes.load(std::sync::atomic::Ordering::Relaxed);
                let total = total_bytes.load(std::sync::atomic::Ordering::Relaxed);
                
                // 每 5 次轮询打印一次日志
                poll_count += 1;
                if poll_count % 5 == 0 {
                    tracing::info!("🔄 轮询 #{}: 进度={:.1}%, 速度={} B/s, 已下载={} B, 总大小={} B", 
                        poll_count, percent * 100.0, speed, downloaded, total);
                }
                
                // 更新任务列表中的进度（不更新首页状态）
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
                            task.output_path = Some(path.clone());
                            task.completed_at = Some(std::time::SystemTime::now());
                            
                            // 获取文件大小
                            if let Ok(metadata) = std::fs::metadata(&path) {
                                task.total_bytes = Some(metadata.len());
                                task.downloaded_bytes = metadata.len();
                            }
                        }
                        Err(error) => {
                            task.state = TaskState::Failed(error);
                            task.completed_at = Some(std::time::SystemTime::now());
                        }
                    }
                }
            }).await;
            
            // 日志输出（不更新首页状态）
            match result {
                Ok(path) => {
                    tracing::info!("✅ 下载完成: {}", path.display());
                }
                Err(e) => {
                    tracing::error!("❌ 下载失败: {}", e);
                }
            }
        }).detach();
    }
    
    /// 下载封面图
    fn download_thumbnail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let thumbnail_url = match &self.download_state {
            DownloadState::Ready(info) => info.thumbnail.clone(),
            _ => None,
        };
        
        let Some(thumb_url) = thumbnail_url else {
            tracing::warn!("⚠️ 没有封面图可下载");
            window.push_notification(Notification::warning("没有可用的封面图"), cx);
            return;
        };
        
        let video_title = match &self.download_state {
            DownloadState::Ready(info) => info.title.clone(),
            _ => "thumbnail".to_string(),
        };
        
        let output_dir = std::path::PathBuf::from(&self.output_path);
        
        tracing::info!("📥 开始下载封面图:");
        tracing::info!("  URL: {}", thumb_url);
        tracing::info!("  输出目录: {:?}", output_dir);
        
        // 显示开始下载通知
        window.push_notification(Notification::info("正在下载封面..."), cx);
        
        // 使用 spawn_in 以获取 AsyncWindowContext，这样可以访问 window
        cx.spawn_in(window, async move |_this, cx| {
            let result = smol::unblock(move || {
                // 创建文件名（使用视频标题）
                let safe_title: String = video_title.chars()
                    .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
                    .collect();
                let filename = format!("{}_thumbnail.jpg", safe_title);
                let output_path = output_dir.join(&filename);
                
                // 使用 curl 命令下载封面
                let status = std::process::Command::new("curl")
                    .arg("-L")  // 跟随重定向
                    .arg("-o")
                    .arg(&output_path)
                    .arg(&thumb_url)
                    .status()
                    .map_err(|e| anyhow::anyhow!("执行 curl 失败: {}", e))?;
                
                if !status.success() {
                    return Err(anyhow::anyhow!("curl 下载失败，退出码: {:?}", status.code()));
                }
                
                tracing::info!("✅ 封面下载完成: {:?}", output_path);
                Ok::<_, anyhow::Error>(output_path)
            }).await;
            
            // 显示结果通知
            // 使用 cx.update 来获取 window 和 App context
            let _ = cx.update(|window, cx| {
                match result {
                    Ok(path) => {
                        tracing::info!("✅ 封面已保存到: {:?}", path);
                        let filename = path.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "封面".to_string());
                        window.push_notification(
                            Notification::success(format!("封面已保存: {}", filename)),
                            cx
                        );
                    }
                    Err(e) => {
                        tracing::error!("❌ 封面下载失败: {}", e);
                        window.push_notification(
                            Notification::error(format!("封面下载失败: {}", e)),
                            cx
                        );
                    }
                }
            });
        }).detach();
    }
    
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.url_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.download_state = DownloadState::Idle;
        self.selected_format_id = None;
        cx.notify();
    }
}

impl Render for HomePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let url = self.get_url(cx);
        let is_url_empty = url.trim().is_empty();
        let is_loading = matches!(self.download_state, DownloadState::Fetching);
        
        // 使用主题颜色
        let bg_color = cx.theme().background;

        div()
            .id("home-page")
            .size_full()
            .overflow_y_scroll()
            .bg(bg_color)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p(px(24.0))
                    .gap(px(24.0))
                    // 页面标题
                    .child(self.render_header(cx))
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
                    // 下载选项（仅在解析成功后显示）
                    .when(matches!(self.download_state, DownloadState::Ready(_)), |this| {
                        this.child(
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
                    })
            )
    }
}

impl HomePage {
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // 使用主题颜色
        let title_color = cx.theme().foreground;
        let desc_color = cx.theme().muted_foreground;
        
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
                            .text_color(title_color)
                            .child("视频下载")
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(desc_color)
                            .child("粘贴视频链接，一键下载")
                    )
            )
    }

    fn render_state_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.download_state {
            DownloadState::Idle => VideoPreviewIdle.into_any_element(),
            DownloadState::Fetching => VideoPreviewLoading.into_any_element(),
            DownloadState::Ready(info) => {
                let selected_format = self.selected_format_id.clone();
                VideoPreviewReady::new(info.clone())
                    .selected_format(selected_format)
                    .on_cancel(cx.listener(|this, _ev, window, cx| {
                        this.reset(window, cx);
                    }))
                    .on_download(cx.listener(|this, _ev, window, cx| {
                        // 使用当前选中的格式，如果没有选中则使用默认
                        let format_id = this.selected_format_id.clone()
                            .unwrap_or_else(|| "bestvideo+bestaudio/best".to_string());
                        this.start_download_with_format(format_id, window, cx);
                    }))
                    .on_download_thumbnail(cx.listener(|this, _ev, window, cx| {
                        this.download_thumbnail(window, cx);
                    }))
                    .on_select_format(cx.listener(|this, format_id: &str, _window, cx| {
                        this.select_format(format_id.to_string(), cx);
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
}
