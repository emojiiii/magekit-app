//! 首页主组件

use crate::app::AppState;
use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::WindowExt;
use gpui_component::input::InputState;
use gpui_component::notification::Notification;
use gpui_router::use_navigate;
use magekit_shared::{TaskState, TaskStatus};
use std::sync::Arc;
use std::time::Duration;

use super::widgets::{
    FormatSelection, QualityOption, UrlInputCard, VideoFormatInfo, VideoInfo,
    VideoPreviewCompleted, VideoPreviewDownloading, VideoPreviewError, VideoPreviewIdle,
    VideoPreviewLoading, VideoPreviewReady,
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
/// 1. 过滤掉重复的分辨率（考虑帧率差异）
/// 2. 优先选择常见格式 (mp4, webm)
/// 3. 生成用户友好的标签 (如 "1080p", "1080p60", "720p", "音频")
fn convert_formats(formats: &[magekit_shared::VideoFormat]) -> Vec<VideoFormatInfo> {
    use std::collections::HashMap;

    tracing::info!("🔄 开始转换格式列表，原始格式数量: {}", formats.len());

    // 打印前几个格式的详细信息用于调试
    for (i, fmt) in formats.iter().take(10).enumerate() {
        tracing::info!(
            "  原始格式 {}: id={}, ext={}, res={:?}, fps={:?}, vcodec={:?}, acodec={:?}, quality={:?}",
            i,
            fmt.format_id,
            fmt.ext,
            fmt.resolution,
            fmt.fps,
            fmt.vcodec,
            fmt.acodec,
            fmt.quality
        );
    }

    let mut result = Vec::new();

    // 用于去重的 key: (分辨率_帧率类别_扩展名)
    // 帧率类别: "normal" (<= 30fps) 或 "high" (> 30fps)
    let mut seen_combined: HashMap<String, bool> = HashMap::new(); // 合并格式去重
    let mut seen_video_only: HashMap<String, bool> = HashMap::new(); // 仅视频去重
    let mut seen_audio: HashMap<String, bool> = HashMap::new(); // 仅音频去重

    // 先按质量排序：优先高分辨率，然后是高帧率
    let mut sorted_formats: Vec<_> = formats.iter().collect();
    sorted_formats.sort_by(|a, b| {
        // 解析分辨率高度进行比较
        let height_a = a
            .resolution
            .as_ref()
            .and_then(|r| r.split('x').last())
            .and_then(|h| h.parse::<u32>().ok())
            .unwrap_or(0);
        let height_b = b
            .resolution
            .as_ref()
            .and_then(|r| r.split('x').last())
            .and_then(|h| h.parse::<u32>().ok())
            .unwrap_or(0);

        // 先比较分辨率
        match height_b.cmp(&height_a) {
            std::cmp::Ordering::Equal => {
                // 分辨率相同时，比较帧率（高帧率优先）
                let fps_a = a.fps.unwrap_or(30.0) as u32;
                let fps_b = b.fps.unwrap_or(30.0) as u32;
                fps_b.cmp(&fps_a)
            }
            other => other,
        }
    });

    // 处理所有格式
    for fmt in &sorted_formats {
        // 判断是否有视频：vcodec 存在且不是 "none"
        let has_video = fmt
            .vcodec
            .as_ref()
            .map(|v| !v.is_empty() && v != "none")
            .unwrap_or(false);
        // 判断是否有音频：acodec 存在且不是 "none"
        let has_audio = fmt
            .acodec
            .as_ref()
            .map(|a| !a.is_empty() && a != "none")
            .unwrap_or(false);

        // 如果 vcodec 和 acodec 都是 None，但有分辨率，可能是合并格式
        let is_combined_default =
            fmt.vcodec.is_none() && fmt.acodec.is_none() && fmt.resolution.is_some();

        // 跳过没有视频也没有音频的格式（除非是合并格式）
        if !has_video && !has_audio && !is_combined_default {
            continue;
        }

        let resolution_key = fmt
            .resolution
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        // 判断帧率类别：高帧率 (> 30fps) 或普通帧率
        let fps = fmt.fps.unwrap_or(30.0);
        let is_high_fps = fps > 30.0;
        let fps_category = if is_high_fps { "high" } else { "normal" };

        // 生成用户友好的标签
        let label = if let Some(res) = &fmt.resolution {
            // 尝试提取高度作为标签，如 "1920x1080" -> "1080p" 或 "1080p60"
            if let Some(height) = res.split('x').last() {
                if let Ok(h) = height.parse::<u32>() {
                    if is_high_fps {
                        format!("{}p{}", h, fps.round() as u32)
                    } else {
                        format!("{}p", h)
                    }
                } else {
                    res.clone()
                }
            } else {
                res.clone()
            }
        } else if !has_video && has_audio {
            // 音频格式显示为 "最佳音质" 或扩展名
            "最佳音质".to_string()
        } else {
            fmt.quality.clone().unwrap_or_else(|| "视频".to_string())
        };

        // 根据格式类型分类处理
        if has_video && has_audio || is_combined_default {
            // 合并格式（视频+音频）
            // 去重 key 包含帧率类别
            let key = format!("{}_{}_{}", resolution_key, fps_category, fmt.ext);
            if !seen_combined.contains_key(&key) {
                seen_combined.insert(key, true);
                tracing::info!(
                    "  添加合并格式: {} - {} ({}) fps={} [video+audio]",
                    fmt.format_id,
                    label,
                    fmt.ext,
                    fps
                );
                result.push(VideoFormatInfo {
                    format_id: fmt.format_id.clone(),
                    label,
                    ext: fmt.ext.clone(),
                    filesize: fmt.filesize,
                    has_video: true,
                    has_audio: true,
                });
            }
        } else if has_video && !has_audio {
            // 仅视频格式
            // 去重 key 包含帧率类别
            let key = format!("{}_{}_{}", resolution_key, fps_category, fmt.ext);
            if !seen_video_only.contains_key(&key) {
                seen_video_only.insert(key, true);
                tracing::info!(
                    "  添加仅视频格式: {} - {} ({}) fps={} [video only]",
                    fmt.format_id,
                    label,
                    fmt.ext,
                    fps
                );
                result.push(VideoFormatInfo {
                    format_id: fmt.format_id.clone(),
                    label,
                    ext: fmt.ext.clone(),
                    filesize: fmt.filesize,
                    has_video: true,
                    has_audio: false,
                });
            }
        } else if !has_video && has_audio {
            // 仅音频格式
            let key = fmt.ext.clone();
            if !seen_audio.contains_key(&key) {
                // 限制音频格式数量
                if seen_audio.len() < 3 {
                    seen_audio.insert(key, true);
                    tracing::info!(
                        "  添加仅音频格式: {} - {} [audio only]",
                        fmt.format_id,
                        label
                    );
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
        }
    }

    tracing::info!(
        "🔄 格式转换完成，结果数量: {} (合并:{}, 仅视频:{}, 仅音频:{})",
        result.len(),
        seen_combined.len(),
        seen_video_only.len(),
        seen_audio.len()
    );

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
    /// 选中的视频格式 ID（合并格式或仅视频）
    selected_video_id: Option<String>,
    /// 选中的音频格式 ID（仅音频）
    selected_audio_id: Option<String>,
    output_path: String,
    /// 当前解析的 URL (用于下载)
    current_url: Option<String>,
}

impl HomePage {
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 从配置读取默认下载路径
        let default_path = app_state.config.blocking_read()
            .download
            .default_output_path
            .to_string_lossy()
            .to_string();

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
            selected_video_id: None,
            selected_audio_id: None,
            output_path: default_path,
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
                handle
                    .join()
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("解析线程崩溃")))
            })
            .await;

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
                            tracing::info!(
                                "    - {} ({}) {} video={} audio={}",
                                fmt.label,
                                fmt.format_id,
                                fmt.ext,
                                fmt.has_video,
                                fmt.has_audio
                            );
                        }

                        // 默认不选中任何格式，让用户自由选择
                        this.selected_video_id = None;
                        this.selected_audio_id = None;

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
                        this.selected_video_id = None;
                        this.selected_audio_id = None;
                        this.download_state = DownloadState::Error(e.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
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
            QualityOption::P1080 => {
                "bestvideo[height<=1080]+bestaudio/best[height<=1080]".to_string()
            }
            QualityOption::P720 => "bestvideo[height<=720]+bestaudio/best[height<=720]".to_string(),
            QualityOption::P480 => "bestvideo[height<=480]+bestaudio/best[height<=480]".to_string(),
            QualityOption::AudioOnly => "bestaudio/best".to_string(),
        };

        // 创建任务 ID
        let task_id = uuid::Uuid::new_v4();

        // 转换下载选项（使用默认值）
        let options = crate::app::DownloadVideoOptions {
            embed_metadata: true,
            embed_thumbnail: false,
            download_subtitles: false,
            audio_only: matches!(self.selected_quality, QualityOption::AudioOnly),
        };

        // 创建下载参数（用于暂停后恢复）
        let download_params = magekit_shared::DownloadParams {
            output_dir: output_dir.clone(),
            format_id: format_id.clone(),
            embed_metadata: options.embed_metadata,
            embed_thumbnail: options.embed_thumbnail,
            download_subtitles: options.download_subtitles,
            audio_only: options.audio_only,
        };

        // 创建任务状态并添加到任务列表
        let mut task_status = TaskStatus::new(task_id, url.clone(), video_title.clone());
        task_status.download_params = Some(download_params);
        {
            let tasks = app_state.tasks.clone();
            let runtime = app_state.runtime.clone();
            let task_status_clone = task_status.clone();
            let app_state_for_save = app_state.clone();
            runtime.spawn(async move {
                let mut tasks = tasks.write().await;
                tasks.insert(task_id, task_status_clone.clone());
                // 立即保存新创建的任务到持久化存储
                app_state_for_save.save_task_to_persistence(&task_status_clone);
            });
        }

        // 克隆 tasks 用于进度回调直接更新
        let tasks_for_callback = app_state.tasks.clone();

        // 进度回调 - 直接更新 tasks，无需轮询
        let progress_callback: Arc<dyn Fn(f32, u64, u64, u64) + Send + Sync> =
            Arc::new(move |percent, speed, downloaded, total| {
                tracing::info!(
                    "📥 [with_format] 进度回调: percent={:.3}, speed={}, downloaded={}, total={}",
                    percent,
                    speed,
                    downloaded,
                    total
                );
                // 直接更新 tasks（使用 blocking_write）
                let mut tasks = tasks_for_callback.blocking_write();
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.progress = percent;
                    task.speed = if speed > 0 { Some(speed) } else { None };
                    task.downloaded_bytes = downloaded;
                    task.total_bytes = if total > 0 { Some(total) } else { None };
                }
            });

        // 在后台线程中运行下载
        let handle = app_state.download_video_in_background(
            task_id,
            url,
            output_dir,
            format_id,
            options,
            progress_callback,
            video_title.clone(),
        );

        // 克隆用于更新任务状态
        let tasks_for_update = app_state.tasks.clone();

        // 克隆 app_state 用于清理取消标志
        let app_state_for_cleanup = app_state.clone();

        // 使用 cx.spawn 来等待下载完成（不再需要轮询进度）
        cx.spawn(async move |_this, _cx| {
            // 首先将任务状态更新为 Downloading
            {
                let tasks = tasks_for_update.clone();
                let app_state_clone = app_state_for_cleanup.clone();
                smol::unblock(move || {
                    // 使用 blocking_write 同步更新
                    let mut tasks = tasks.blocking_write();
                    if let Some(task) = tasks.get_mut(&task_id) {
                        task.state = TaskState::Downloading;
                        task.started_at = Some(std::time::SystemTime::now());
                        // 保存任务状态（开始下载时）
                        app_state_clone.save_task_to_persistence(task);
                    }
                })
                .await;
            }

            // 等待后台线程完成（进度由回调直接更新，无需轮询）
            tracing::info!("⏳ 等待下载完成 (start_download)");
            loop {
                if handle.is_finished() {
                    tracing::info!("🏁 下载线程已完成");
                    break;
                }
                // 只需等待线程完成，不需要更新进度
                Timer::after(std::time::Duration::from_millis(100)).await;
            }

            // 在后台线程获取结果
            let result: anyhow::Result<std::path::PathBuf> = smol::unblock(move || {
                handle
                    .join()
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("下载线程崩溃")))
            })
            .await;

            // 清理取消标志
            app_state_for_cleanup.cleanup_download_task(task_id);

            // 更新任务状态
            let tasks = tasks_for_update.clone();
            let result_for_task = result
                .as_ref()
                .map(|p| p.clone())
                .map_err(|e| e.to_string());
            let app_state_for_final_save = app_state_for_cleanup.clone();
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
                            
                            // 保存完成状态到持久化存储
                            app_state_for_final_save.save_task_to_persistence(task);
                        }
                        Err(error) => {
                            // 如果是暂停导致的错误，保持 Paused 状态
                            if error.contains("下载已暂停") {
                                task.state = TaskState::Paused;
                            } else if error.contains("下载已取消") {
                                task.state = TaskState::Cancelled;
                            } else {
                                task.state = TaskState::Failed(error);
                                task.completed_at = Some(std::time::SystemTime::now());
                            }
                            // 保存失败/取消状态到持久化存储
                            app_state_for_final_save.save_task_to_persistence(task);
                        }
                    }
                }
            })
            .await;

            // 日志输出（不更新首页状态）
            match &result {
                Ok(path) => {
                    tracing::info!("✅ 下载完成: {}", path.display());
                }
                Err(e) => {
                    if e.to_string().contains("下载已暂停") {
                        tracing::info!("⏸️ 下载已暂停");
                    } else {
                        tracing::error!("❌ 下载失败: {}", e);
                    }
                }
            }
        })
        .detach();
    }

    fn select_quality(&mut self, quality: QualityOption, cx: &mut Context<Self>) {
        self.selected_quality = quality;
        cx.notify();
    }

    /// 选择/取消选择视频格式（toggle）
    fn select_video_format(&mut self, format_id: String, cx: &mut Context<Self>) {
        // 如果点击的是已选中的，则取消选择
        if self.selected_video_id.as_ref() == Some(&format_id) {
            tracing::info!("📹 取消选择视频格式: {}", format_id);
            self.selected_video_id = None;
        } else {
            tracing::info!("📹 选择视频格式: {}", format_id);
            self.selected_video_id = Some(format_id.clone());
            // 如果选中的是合并格式（含音频），清除单独选中的音频
            if let DownloadState::Ready(info) = &self.download_state {
                if let Some(fmt) = info.formats.iter().find(|f| f.format_id == format_id) {
                    if fmt.has_audio {
                        self.selected_audio_id = None;
                    }
                }
            }
        }
        cx.notify();
    }

    /// 选择/取消选择音频格式（toggle）
    fn select_audio_format(&mut self, format_id: String, cx: &mut Context<Self>) {
        // 如果点击的是已选中的，则取消选择
        if self.selected_audio_id.as_ref() == Some(&format_id) {
            tracing::info!("🎵 取消选择音频格式: {}", format_id);
            self.selected_audio_id = None;
        } else {
            tracing::info!("🎵 选择音频格式: {}", format_id);
            self.selected_audio_id = Some(format_id);
        }
        cx.notify();
    }

    /// 获取当前选中的格式字符串（用于 yt-dlp）
    fn get_format_selection(&self) -> Option<FormatSelection> {
        // 判断是否是分离格式网站（如B站）
        let is_separated_source = if let DownloadState::Ready(info) = &self.download_state {
            let has_combined = info.formats.iter().any(|f| f.has_video && f.has_audio);
            let has_video_only = info.formats.iter().any(|f| f.has_video && !f.has_audio);
            let has_audio_only = info.formats.iter().any(|f| !f.has_video && f.has_audio);
            !has_combined && has_video_only && has_audio_only
        } else {
            false
        };

        match (&self.selected_video_id, &self.selected_audio_id) {
            (Some(vid), Some(aid)) => {
                // 检查视频格式是否已包含音频
                if let DownloadState::Ready(info) = &self.download_state {
                    let video_has_audio = info
                        .formats
                        .iter()
                        .find(|f| &f.format_id == vid)
                        .map(|f| f.has_audio)
                        .unwrap_or(false);

                    if video_has_audio {
                        return Some(FormatSelection::Combined(vid.clone()));
                    }
                }
                Some(FormatSelection::VideoAndAudio {
                    video_id: vid.clone(),
                    audio_id: aid.clone(),
                })
            }
            (Some(vid), None) => {
                if let DownloadState::Ready(info) = &self.download_state {
                    let fmt = info.formats.iter().find(|f| &f.format_id == vid)?;
                    if fmt.has_audio {
                        return Some(FormatSelection::Combined(vid.clone()));
                    }
                    // 对于分离格式网站，自动加上最佳音频
                    if is_separated_source {
                        return Some(FormatSelection::VideoAndAudio {
                            video_id: vid.clone(),
                            audio_id: "bestaudio".to_string(),
                        });
                    }
                }
                Some(FormatSelection::VideoOnly(vid.clone()))
            }
            (None, Some(aid)) => Some(FormatSelection::AudioOnly(aid.clone())),
            (None, None) => None,
        }
    }

    /// 使用指定的格式 ID 开始下载
    fn start_download_with_format(
        &mut self,
        format_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

        // 判断是否是音频格式（在修改状态前判断）
        let is_audio_only = match &self.download_state {
            DownloadState::Ready(info) => info
                .formats
                .iter()
                .find(|f| f.format_id == format_id)
                .map(|f| !f.has_video && f.has_audio)
                .unwrap_or(false),
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

        // 转换下载选项（使用默认值）
        let options = crate::app::DownloadVideoOptions {
            embed_metadata: true,
            embed_thumbnail: false,
            download_subtitles: false,
            audio_only: is_audio_only,
        };

        // 创建下载参数（用于暂停后恢复）
        let download_params = magekit_shared::DownloadParams {
            output_dir: output_dir.clone(),
            format_id: format_id.clone(),
            embed_metadata: options.embed_metadata,
            embed_thumbnail: options.embed_thumbnail,
            download_subtitles: options.download_subtitles,
            audio_only: options.audio_only,
        };

        // 创建任务状态并添加到任务列表
        let mut task_status = TaskStatus::new(task_id, url.clone(), video_title.clone());
        task_status.download_params = Some(download_params);
        {
            let tasks = app_state.tasks.clone();
            let runtime = app_state.runtime.clone();
            let task_status_clone = task_status.clone();
            let app_state_for_save = app_state.clone();
            runtime.spawn(async move {
                let mut tasks = tasks.write().await;
                tasks.insert(task_id, task_status_clone.clone());
                // 立即保存新创建的任务到持久化存储
                app_state_for_save.save_task_to_persistence(&task_status_clone);
            });
        }

        // 克隆 tasks 用于进度回调直接更新
        let tasks_for_callback = app_state.tasks.clone();

        // 进度回调 - 直接更新 tasks，无需轮询
        let progress_callback: Arc<dyn Fn(f32, u64, u64, u64) + Send + Sync> =
            Arc::new(move |percent, speed, downloaded, total| {
                tracing::info!(
                    "📥 [with_format] 进度回调: percent={:.3}, speed={}, downloaded={}, total={}",
                    percent,
                    speed,
                    downloaded,
                    total
                );
                // 直接更新 tasks（使用 blocking_write）
                let mut tasks = tasks_for_callback.blocking_write();
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.progress = percent;
                    task.speed = if speed > 0 { Some(speed) } else { None };
                    task.downloaded_bytes = downloaded;
                    task.total_bytes = if total > 0 { Some(total) } else { None };
                }
            });

        // 在后台线程中运行下载
        let handle = app_state.download_video_in_background(
            task_id,
            url,
            output_dir,
            format_id,
            options,
            progress_callback,
            video_title.clone(),
        );

        // 克隆用于更新任务状态
        let tasks_for_update = app_state.tasks.clone();

        // 克隆 app_state 用于清理取消标志
        let app_state_for_cleanup = app_state.clone();

        // 使用 cx.spawn 来等待下载完成（不再需要轮询进度）
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
                })
                .await;
            }

            // 等待后台线程完成（进度由回调直接更新，无需轮询）
            tracing::info!("⏳ 等待下载完成 (start_download_with_format)");
            loop {
                if handle.is_finished() {
                    tracing::info!("🏁 下载线程已完成");
                    break;
                }
                // 只需等待线程完成，不需要更新进度
                Timer::after(std::time::Duration::from_millis(100)).await;
            }

            // 在后台线程获取结果
            let result: anyhow::Result<std::path::PathBuf> = smol::unblock(move || {
                handle
                    .join()
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("下载线程崩溃")))
            })
            .await;

            // 清理取消标志
            app_state_for_cleanup.cleanup_download_task(task_id);

            // 更新任务状态
            let tasks = tasks_for_update.clone();
            let result_for_task = result
                .as_ref()
                .map(|p| p.clone())
                .map_err(|e| e.to_string());
            let app_state_for_final_save = app_state_for_cleanup.clone();
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
                            
                            // 保存完成状态到持久化存储
                            app_state_for_final_save.save_task_to_persistence(task);
                        }
                        Err(error) => {
                            // 如果是暂停导致的错误，保持 Paused 状态
                            if error.contains("下载已暂停") {
                                task.state = TaskState::Paused;
                            } else if error.contains("下载已取消") {
                                task.state = TaskState::Cancelled;
                            } else {
                                task.state = TaskState::Failed(error);
                                task.completed_at = Some(std::time::SystemTime::now());
                            }
                            // 保存失败/取消状态到持久化存储
                            app_state_for_final_save.save_task_to_persistence(task);
                        }
                    }
                }
            })
            .await;

            // 日志输出（不更新首页状态）
            match &result {
                Ok(path) => {
                    tracing::info!("✅ 下载完成: {}", path.display());
                }
                Err(e) => {
                    if e.to_string().contains("下载已暂停") {
                        tracing::info!("⏸️ 下载已暂停");
                    } else {
                        tracing::error!("❌ 下载失败: {}", e);
                    }
                }
            }
        })
        .detach();
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
                let safe_title: String = video_title
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                let filename = format!("{}_thumbnail.jpg", safe_title);
                let output_path = output_dir.join(&filename);

                // 使用 curl 命令下载封面（无窗口模式）
                let status = magekit_shared::create_command("curl")
                    .arg("-L") // 跟随重定向
                    .arg("-o")
                    .arg(&output_path)
                    .arg(&thumb_url)
                    .status()
                    .map_err(|e| anyhow::anyhow!("执行 curl 失败: {}", e))?;

                if !status.success() {
                    return Err(anyhow::anyhow!(
                        "curl 下载失败，退出码: {:?}",
                        status.code()
                    ));
                }

                tracing::info!("✅ 封面下载完成: {:?}", output_path);
                Ok::<_, anyhow::Error>(output_path)
            })
            .await;

            // 显示结果通知
            // 使用 cx.update 来获取 window 和 App context
            let _ = cx.update(|window, cx| match result {
                Ok(path) => {
                    tracing::info!("✅ 封面已保存到: {:?}", path);
                    let filename = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "封面".to_string());
                    window.push_notification(
                        Notification::success(format!("封面已保存: {}", filename)),
                        cx,
                    );
                }
                Err(e) => {
                    tracing::error!("❌ 封面下载失败: {}", e);
                    window
                        .push_notification(Notification::error(format!("封面下载失败: {}", e)), cx);
                }
            });
        })
        .detach();
    }

    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.url_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.download_state = DownloadState::Idle;
        self.selected_video_id = None;
        self.selected_audio_id = None;
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
                            })),
                    )
                    // 状态内容（包含格式选择）
                    .child(self.render_state_content(cx)),
            )
    }
}

impl HomePage {
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // 使用主题颜色
        let title_color = cx.theme().foreground;
        let desc_color = cx.theme().muted_foreground;

        div().flex().items_center().justify_between().child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_2xl()
                        .font_weight(FontWeight::BOLD)
                        .text_color(title_color)
                        .child("视频下载"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(desc_color)
                        .child("粘贴视频链接，一键下载"),
                ),
        )
    }

    fn render_state_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.download_state {
            DownloadState::Idle => VideoPreviewIdle.into_any_element(),
            DownloadState::Fetching => VideoPreviewLoading.into_any_element(),
            DownloadState::Ready(info) => {
                let selected_video = self.selected_video_id.clone();
                let selected_audio = self.selected_audio_id.clone();
                VideoPreviewReady::new(info.clone())
                    .selected_video(selected_video)
                    .selected_audio(selected_audio)
                    .on_cancel(cx.listener(|this, _ev, window, cx| {
                        this.reset(window, cx);
                    }))
                    .on_download(cx.listener(|this, _ev, window, cx| {
                        // 使用当前选中的格式
                        if let Some(selection) = this.get_format_selection() {
                            let format_str = selection.to_format_string();
                            tracing::info!("📥 开始下载，格式: {}", format_str);
                            this.start_download_with_format(format_str, window, cx);
                        }
                    }))
                    .on_download_thumbnail(cx.listener(|this, _ev, window, cx| {
                        this.download_thumbnail(window, cx);
                    }))
                    .on_select_video(cx.listener(|this, format_id: &str, _window, cx| {
                        this.select_video_format(format_id.to_string(), cx);
                    }))
                    .on_select_audio(cx.listener(|this, format_id: &str, _window, cx| {
                        this.select_audio_format(format_id.to_string(), cx);
                    }))
                    .into_any_element()
            }
            DownloadState::Downloading { progress, speed } => {
                VideoPreviewDownloading::new(*progress, speed.clone()).into_any_element()
            }
            DownloadState::Completed(path) => VideoPreviewCompleted::new(path.clone())
                .on_new_download(cx.listener(|this, _ev, window, cx| {
                    this.reset(window, cx);
                }))
                .into_any_element(),
            DownloadState::Error(msg) => VideoPreviewError::new(msg.clone())
                .on_retry(cx.listener(|this, _ev, _window, cx| {
                    this.download_state = DownloadState::Idle;
                    cx.notify();
                }))
                .into_any_element(),
        }
    }
}
