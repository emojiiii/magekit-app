//! 首页主组件

use crate::app::{AppState, DownloadVideoOptions};
use gpui::*;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::WindowExt;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_router::{use_location, use_navigate};
use std::sync::Arc;
use std::time::Duration;

use super::widgets::{
    FormatSelection, UrlInputCard, VideoFormatInfo, VideoInfo, VideoPreviewCompleted,
    VideoPreviewDownloading, VideoPreviewError, VideoPreviewIdle, VideoPreviewLoading,
    VideoPreviewReady,
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
        None => crate::i18n::tr("未知").to_string(),
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

        // 判断帧率类别：使用“整数帧率”避免 30.001 这类浮点误差导致重复项
        let fps = fmt.fps.unwrap_or(30.0);
        let fps_int = fps.round() as u32;
        let is_high_fps = fps_int > 30;
        let fps_category = if is_high_fps { "high" } else { "normal" };

        // 生成用户友好的标签
        let label = if let Some(res) = &fmt.resolution {
            // 尝试提取高度作为标签，如 "1920x1080" -> "1080p" 或 "1080p60"
            if let Some(height) = res.split('x').last() {
                if let Ok(h) = height.parse::<u32>() {
                    if is_high_fps {
                        format!("{}p{}", h, fps_int)
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
            crate::i18n::tr("最佳音质").to_string()
        } else {
            fmt.quality
                .clone()
                .unwrap_or_else(|| crate::i18n::tr("视频").to_string())
        };

        let label_key = if fmt.resolution.is_some() {
            None
        } else if !has_video && has_audio {
            Some("最佳音质")
        } else if fmt.quality.is_none() {
            Some("视频")
        } else {
            None
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
                    label_key,
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
                    label_key,
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
                        label_key,
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
            label: crate::i18n::tr("最佳质量").to_string(),
            label_key: Some("最佳质量"),
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

/// 取消、修改链接或重试后，较早请求不得替换当前结果。
fn accept_parse_result(active: u64, result: u64, fetching: bool) -> bool {
    active == result && fetching
}

/// 首页组件
pub struct HomePage {
    app_state: Arc<AppState>,
    url_input: Entity<InputState>,
    download_state: DownloadState,
    /// 选中的视频格式 ID（合并格式或仅视频）
    selected_video_id: Option<String>,
    /// 选中的音频格式 ID（仅音频）
    selected_audio_id: Option<String>,
    /// 当前解析的 URL (用于下载)
    current_url: Option<String>,
    /// 原始视频信息（用于传递给 ToolManager，避免重复获取）
    original_video_info: Option<magekit_shared::VideoInfo>,
    request_generation: u64,
    submitting: bool,
    thumbnail_loading: bool,
}

impl HomePage {
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 创建 URL 输入框状态
        let url_input = cx.new(|cx| {
            crate::i18n::input("粘贴 YouTube、Bilibili 等视频链接...", window, cx).clean_on_escape()
        });

        // 输入变化立即使旧解析结果失效；Enter 与按钮走同一个受保护入口。
        cx.subscribe(&url_input, |this, _, event, cx| match event {
            InputEvent::PressEnter { .. } => this.on_parse(cx),
            InputEvent::Change if !this.submitting => {
                this.invalidate_parse(cx);
            }
            _ => {}
        })
        .detach();

        Self {
            app_state,
            url_input,
            download_state: DownloadState::Idle,
            selected_video_id: None,
            selected_audio_id: None,
            current_url: None,
            original_video_info: None,
            request_generation: 0,
            submitting: false,
            thumbnail_loading: false,
        }
    }

    fn get_url(&self, cx: &Context<Self>) -> String {
        self.url_input.read(cx).value().to_string()
    }

    fn invalidate_parse(&mut self, cx: &mut Context<Self>) {
        self.request_generation = self.request_generation.wrapping_add(1);
        self.download_state = DownloadState::Idle;
        self.current_url = None;
        self.original_video_info = None;
        self.selected_video_id = None;
        self.selected_audio_id = None;
        cx.notify();
    }

    fn on_parse(&mut self, cx: &mut Context<Self>) {
        if self.submitting || matches!(self.download_state, DownloadState::Fetching) {
            return;
        }
        let url = match AppState::validate_media_url(&self.get_url(cx)) {
            Ok(url) => url,
            Err(error) => {
                self.invalidate_parse(cx);
                self.download_state = DownloadState::Error(error.to_string());
                cx.notify();
                return;
            }
        };
        self.invalidate_parse(cx);
        let generation = self.request_generation;
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
                smol::Timer::after(std::time::Duration::from_millis(100)).await;
            }

            // 在后台线程获取结果，避免阻塞主线程
            let result: anyhow::Result<magekit_shared::VideoInfo> = smol::unblock(move || {
                handle.join().unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(crate::i18n::format("解析线程崩溃", &[])))
                })
            })
            .await;

            // 更新 UI
            let _ = this.update(cx, |this, cx| {
                if !accept_parse_result(
                    this.request_generation,
                    generation,
                    matches!(this.download_state, DownloadState::Fetching),
                ) {
                    return;
                }
                match result {
                    Ok(info) => {
                        // 保存当前 URL
                        this.current_url = Some(url_for_save.clone());

                        // 保存原始视频信息（用于传递给 ToolManager）
                        this.original_video_info = Some(info.clone());

                        // 详细日志
                        tracing::info!("🎬 视频信息解析成功:");
                        tracing::info!("  标题: {}", info.title);
                        tracing::info!(
                            "  作者: {}",
                            info.uploader.as_deref().unwrap_or(crate::i18n::tr("未知"))
                        );
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

                        // 按现有质量排序提供可直接下载的默认格式，仍可调整。
                        this.selected_video_id = gui_formats
                            .iter()
                            .find(|format| format.has_video)
                            .map(|format| format.format_id.clone());
                        this.selected_audio_id = if this.selected_video_id.is_none() {
                            gui_formats
                                .iter()
                                .find(|format| format.has_audio)
                                .map(|format| format.format_id.clone())
                        } else {
                            None
                        };

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
                        this.original_video_info = None;
                        this.download_state = DownloadState::Error(e.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
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
            // 单独音轨不应与包含音频的合并格式同时高亮却被静默忽略。
            if let DownloadState::Ready(info) = &self.download_state {
                if info.formats.iter().any(|format| {
                    self.selected_video_id.as_ref() == Some(&format.format_id) && format.has_audio
                }) {
                    self.selected_video_id = None;
                }
            }
        }
        cx.notify();
    }

    /// 获取当前选中的格式字符串（用于 yt-dlp）
    fn get_format_selection(&self) -> Option<FormatSelection> {
        // 判断是否是分离格式网站（如B站）
        let is_separated_source = if let DownloadState::Ready(info) = &self.download_state {
            let has_video_only = info.formats.iter().any(|f| f.has_video && !f.has_audio);
            let has_audio_only = info.formats.iter().any(|f| !f.has_video && f.has_audio);
            has_video_only && has_audio_only
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
        if self.submitting {
            return;
        }
        let (Some(url), DownloadState::Ready(info)) = (&self.current_url, &self.download_state)
        else {
            return;
        };
        let is_audio_only = info
            .formats
            .iter()
            .find(|format| format.format_id == format_id)
            .is_some_and(|format| !format.has_video && format.has_audio);
        let options = DownloadVideoOptions {
            embed_metadata: true,
            embed_thumbnail: false,
            download_subtitles: false,
            audio_only: is_audio_only,
        };
        // 每次开始读取最新保存路径，避免修改设置后仍写入页面缓存的旧路径。
        let output_dir = self.app_state.config().download.default_output_path.clone();
        let handle = self.app_state.start_download_in_background_with_info(
            url.clone(),
            output_dir,
            format_id,
            options,
            self.original_video_info.clone(),
        );
        self.submitting = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = smol::unblock(move || {
                handle
                    .join()
                    .unwrap_or_else(|_| Err(anyhow::anyhow!(crate::i18n::tr("创建下载任务失败"))))
            })
            .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.submitting = false;
                match result {
                    Ok(_) => {
                        this.invalidate_parse(cx);
                        window.push_notification(
                            Notification::success(crate::i18n::tr("下载任务已添加")),
                            cx,
                        );
                        // 离开首页后完成的请求不抢回用户的新导航。
                        if use_location(cx).pathname.as_ref() == "/" {
                            use_navigate(cx)("/tasks".into());
                        }
                    }
                    Err(error) => {
                        window.push_notification(Notification::error(error.to_string()), cx)
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 下载封面图
    fn download_thumbnail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.thumbnail_loading || self.submitting {
            return;
        }
        let thumbnail_url = match &self.download_state {
            DownloadState::Ready(info) => info.thumbnail.clone(),
            _ => None,
        };

        let Some(thumb_url) = thumbnail_url else {
            tracing::warn!("⚠️ 没有封面图可下载");
            window.push_notification(
                Notification::warning(crate::i18n::tr("没有可用的封面图")),
                cx,
            );
            return;
        };

        let video_title = match &self.download_state {
            DownloadState::Ready(info) => info.title.clone(),
            _ => "thumbnail".to_string(),
        };

        let output_dir = self.app_state.config().download.default_output_path.clone();

        tracing::info!("📥 开始下载封面图:");
        tracing::info!("  URL: {}", thumb_url);
        tracing::info!("  输出目录: {:?}", output_dir);

        self.thumbnail_loading = true;
        cx.notify();

        // 显示开始下载通知
        window.push_notification(Notification::info(crate::i18n::tr("正在下载封面...")), cx);

        // 使用 spawn_in 以获取 AsyncWindowContext，这样可以访问 window
        cx.spawn_in(window, async move |this, cx| {
            let result = smol::unblock(move || {
                use std::io::{Read, Write};
                let url = AppState::validate_media_url(&thumb_url)?;
                let mut response = reqwest::blocking::Client::builder()
                    .connect_timeout(Duration::from_secs(10))
                    .timeout(Duration::from_secs(45))
                    .build()?
                    .get(url)
                    .send()?
                    .error_for_status()?;
                let extension = match response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("")
                    .split(';')
                    .next()
                    .unwrap_or("")
                {
                    "image/png" => "png",
                    "image/webp" => "webp",
                    "image/jpeg" => "jpg",
                    _ => anyhow::bail!(crate::i18n::tr("封面响应不是受支持的图片")),
                };
                let mut bytes = Vec::new();
                response
                    .by_ref()
                    .take(20 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)?;
                anyhow::ensure!(
                    !bytes.is_empty() && bytes.len() <= 20 * 1024 * 1024,
                    crate::i18n::tr("封面文件为空或过大")
                );
                std::fs::create_dir_all(&output_dir)?;
                let safe_title: String = video_title
                    .chars()
                    .take(100)
                    .map(|c| {
                        if c.is_alphanumeric() || c == '-' || c == '_' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                let mut temporary = tempfile::NamedTempFile::new_in(&output_dir)?;
                temporary.write_all(&bytes)?;
                // 原子保存且不覆盖已有封面；重复下载保留两个文件。
                for suffix in 0..1000 {
                    let filename = if suffix == 0 {
                        format!("{}_thumbnail.{}", safe_title, extension)
                    } else {
                        format!("{}_thumbnail_{}.{}", safe_title, suffix, extension)
                    };
                    let output_path = output_dir.join(filename);
                    match temporary.persist_noclobber(&output_path) {
                        Ok(_) => return Ok::<_, anyhow::Error>(output_path),
                        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                            temporary = error.file
                        }
                        Err(error) => return Err(error.error.into()),
                    }
                }
                anyhow::bail!(crate::i18n::tr("无法创建新的封面文件"))
            })
            .await;

            // 显示结果通知
            // 使用 cx.update 来获取 window 和 App context
            let _ = this.update_in(cx, |this, window, cx| {
                this.thumbnail_loading = false;
                cx.notify();
                match result {
                    Ok(path) => {
                        tracing::info!("✅ 封面已保存到: {:?}", path);
                        let filename = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "封面".to_string());
                        window.push_notification(
                            Notification::success(crate::i18n::format(
                                "封面已保存: {}",
                                &[format!("{}", filename)],
                            )),
                            cx,
                        );
                    }
                    Err(e) => {
                        tracing::error!("❌ 封面下载失败: {}", e);
                        window.push_notification(
                            Notification::error(crate::i18n::format(
                                "封面下载失败: {}",
                                &[format!("{}", e)],
                            )),
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }

    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.submitting {
            return;
        }
        self.request_generation = self.request_generation.wrapping_add(1);
        self.url_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.download_state = DownloadState::Idle;
        self.selected_video_id = None;
        self.selected_audio_id = None;
        self.current_url = None;
        self.original_video_info = None;
        cx.notify();
    }
}

impl Render for HomePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let url = self.get_url(cx);
        let is_url_empty = url.trim().is_empty();
        let is_loading = matches!(self.download_state, DownloadState::Fetching);

        div().id("home-page").size_full().overflow_y_scroll().child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .p_6()
                .gap_4()
                // 页面标题
                .child(self.render_header(cx))
                // URL 输入区域
                .child(
                    UrlInputCard::new(&self.url_input)
                        .loading(is_loading)
                        .disabled(self.submitting)
                        .on_cancel(cx.listener(|this, _, _, cx| this.invalidate_parse(cx)))
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
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .pb_4()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_size(px(22.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(crate::i18n::tr("视频下载")),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(crate::i18n::tr("从一个链接开始，轻松保存视频和音频")),
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
                    .submitting(self.submitting)
                    .thumbnail_loading(self.thumbnail_loading)
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
                    this.on_parse(cx);
                }))
                .into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::accept_parse_result;

    #[test]
    fn parse_result_requires_same_generation_and_active_fetch() {
        assert!(accept_parse_result(3, 3, true));
        assert!(
            !accept_parse_result(4, 3, true),
            "older URL must not overwrite a newer request"
        );
        assert!(
            !accept_parse_result(4, 3, false),
            "cancelled request must not reopen preview"
        );
        assert!(
            !accept_parse_result(3, 3, false),
            "completed request must not apply twice"
        );
    }
}
