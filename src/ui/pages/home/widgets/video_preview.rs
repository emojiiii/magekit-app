//! 视频预览组件
//!
//! 显示解析后的视频信息

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::Disableable;
use gpui_component::button::{Button, ButtonVariants};
use magekit_shared::truncate_string;
use std::sync::Arc;

/// 视频格式信息 (从解析获取)
#[derive(Debug, Clone, PartialEq)]
pub struct VideoFormatInfo {
    pub format_id: String,
    pub label: String,         // 如 "1080p", "720p", "音频"
    pub ext: String,           // 如 "mp4", "webm"
    pub filesize: Option<u64>, // 文件大小
    pub has_video: bool,
    pub has_audio: bool,
}

/// 格式选择状态
#[derive(Debug, Clone, PartialEq)]
pub enum FormatSelection {
    /// 合并格式（音视频一体）
    Combined(String),
    /// 仅视频
    VideoOnly(String),
    /// 仅音频
    AudioOnly(String),
    /// 视频 + 音频（需要合并）
    VideoAndAudio { video_id: String, audio_id: String },
}

impl FormatSelection {
    /// 获取用于 yt-dlp 的格式字符串
    pub fn to_format_string(&self) -> String {
        match self {
            FormatSelection::Combined(id) => id.clone(),
            FormatSelection::VideoOnly(id) => id.clone(),
            FormatSelection::AudioOnly(id) => id.clone(),
            FormatSelection::VideoAndAudio { video_id, audio_id } => {
                format!("{}+{}", video_id, audio_id)
            }
        }
    }

    /// 是否需要合并（下载后用 ffmpeg 合并）
    pub fn needs_merge(&self) -> bool {
        matches!(self, FormatSelection::VideoAndAudio { .. })
    }
}

impl VideoFormatInfo {
    /// 格式化文件大小
    pub fn format_filesize(&self) -> Option<String> {
        self.filesize.map(|bytes| {
            const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
            let mut size = bytes as f64;
            let mut unit_index = 0;
            while size >= 1024.0 && unit_index < UNITS.len() - 1 {
                size /= 1024.0;
                unit_index += 1;
            }
            format!("{:.1} {}", size, UNITS[unit_index])
        })
    }

    /// 判断格式类型
    pub fn format_type(&self) -> FormatType {
        match (self.has_video, self.has_audio) {
            (true, true) => FormatType::Combined,
            (true, false) => FormatType::VideoOnly,
            (false, true) => FormatType::AudioOnly,
            (false, false) => FormatType::Unknown,
        }
    }
}

/// 格式类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FormatType {
    Combined,  // 音视频合并
    VideoOnly, // 仅视频
    AudioOnly, // 仅音频
    Unknown,
}

/// 视频预览信息
#[derive(Debug, Clone, PartialEq)]
pub struct VideoInfo {
    pub title: String,
    pub duration: String,
    pub thumbnail: Option<String>,
    pub uploader: Option<String>,
    pub formats: Vec<VideoFormatInfo>, // 可用格式列表
    pub url: String,                   // 原始 URL
}

/// 视频预览卡片 - 空闲状态
#[derive(IntoElement)]
pub struct VideoPreviewIdle;

impl RenderOnce for VideoPreviewIdle {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_dark = cx.theme().mode.is_dark();
        let bg_color = if is_dark {
            rgb(0x18181b)
        } else {
            rgb(0xffffff)
        };
        let border_color = if is_dark {
            rgb(0x27272a)
        } else {
            rgb(0xf0f0f0)
        };
        let icon_bg = if is_dark {
            rgb(0x27272a)
        } else {
            rgb(0xf4f4f5)
        };
        let text_color = if is_dark {
            rgb(0xa1a1aa)
        } else {
            rgb(0x52525b)
        };
        let sub_text_color = if is_dark {
            rgb(0x71717a)
        } else {
            rgb(0x9ca3af)
        };

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(60.0))
            .px(px(40.0))
            .gap(px(16.0))
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
            .rounded(px(16.0))
            .child(
                div()
                    .w(px(72.0))
                    .h(px(72.0))
                    .rounded(px(16.0))
                    .bg(icon_bg)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_3xl()
                    .child("🎬"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(text_color)
                            .child("输入视频链接开始下载"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(sub_text_color)
                            .child("支持 YouTube、Bilibili、Twitter 等主流平台"),
                    ),
            )
    }
}

/// 视频预览卡片 - 加载中状态
#[derive(IntoElement)]
pub struct VideoPreviewLoading;

impl RenderOnce for VideoPreviewLoading {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_dark = cx.theme().mode.is_dark();
        let bg_color = if is_dark {
            rgb(0x18181b)
        } else {
            rgb(0xffffff)
        };
        let border_color = if is_dark {
            rgb(0x27272a)
        } else {
            rgb(0xf0f0f0)
        };
        let icon_bg = if is_dark {
            rgb(0x27272a)
        } else {
            rgb(0xf4f4f5)
        };
        let text_color = if is_dark {
            rgb(0xfafafa)
        } else {
            rgb(0x18181b)
        };
        let sub_text_color = if is_dark {
            rgb(0x71717a)
        } else {
            rgb(0x9ca3af)
        };

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(60.0))
            .px(px(40.0))
            .gap(px(16.0))
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
            .rounded(px(16.0))
            .child(
                div()
                    .w(px(72.0))
                    .h(px(72.0))
                    .rounded(px(16.0))
                    .bg(icon_bg)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_3xl()
                    .child("⏳"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(text_color)
                            .child("正在获取视频信息..."),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(sub_text_color)
                            .child("请稍候，正在解析视频数据"),
                    ),
            )
    }
}

/// 视频预览卡片 - 就绪状态
#[derive(IntoElement)]
pub struct VideoPreviewReady {
    info: VideoInfo,
    /// 选中的视频格式 ID（仅视频 或 合并格式）
    selected_video_id: Option<String>,
    /// 选中的音频格式 ID（仅音频）
    selected_audio_id: Option<String>,
    on_cancel: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_download: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_download_thumbnail: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_select_video: Option<Arc<dyn Fn(&str, &mut Window, &mut App) + 'static>>,
    on_select_audio: Option<Arc<dyn Fn(&str, &mut Window, &mut App) + 'static>>,
}

impl VideoPreviewReady {
    pub fn new(info: VideoInfo) -> Self {
        // 默认不选中任何格式，让用户自由选择
        Self {
            info,
            selected_video_id: None,
            selected_audio_id: None,
            on_cancel: None,
            on_download: None,
            on_download_thumbnail: None,
            on_select_video: None,
            on_select_audio: None,
        }
    }

    pub fn selected_video(mut self, format_id: Option<String>) -> Self {
        self.selected_video_id = format_id;
        self
    }

    pub fn selected_audio(mut self, format_id: Option<String>) -> Self {
        self.selected_audio_id = format_id;
        self
    }

    /// 获取当前的格式选择
    pub fn get_format_selection(&self) -> Option<FormatSelection> {
        match (&self.selected_video_id, &self.selected_audio_id) {
            (Some(vid), Some(aid)) => {
                // 检查视频格式是否已包含音频
                let video_has_audio = self
                    .info
                    .formats
                    .iter()
                    .find(|f| &f.format_id == vid)
                    .map(|f| f.has_audio)
                    .unwrap_or(false);

                if video_has_audio {
                    // 如果视频已有音频，使用合并格式
                    Some(FormatSelection::Combined(vid.clone()))
                } else {
                    // 需要合并视频和音频
                    Some(FormatSelection::VideoAndAudio {
                        video_id: vid.clone(),
                        audio_id: aid.clone(),
                    })
                }
            }
            (Some(vid), None) => {
                let fmt = self.info.formats.iter().find(|f| &f.format_id == vid)?;
                if fmt.has_audio {
                    Some(FormatSelection::Combined(vid.clone()))
                } else {
                    Some(FormatSelection::VideoOnly(vid.clone()))
                }
            }
            (None, Some(aid)) => Some(FormatSelection::AudioOnly(aid.clone())),
            (None, None) => None,
        }
    }

    pub fn on_cancel(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_cancel = Some(Box::new(handler));
        self
    }

    pub fn on_download(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_download = Some(Box::new(handler));
        self
    }

    pub fn on_download_thumbnail(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_download_thumbnail = Some(Box::new(handler));
        self
    }

    /// 设置视频格式选择回调
    pub fn on_select_video(
        mut self,
        handler: impl Fn(&str, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select_video = Some(Arc::new(handler));
        self
    }

    /// 设置音频格式选择回调
    pub fn on_select_audio(
        mut self,
        handler: impl Fn(&str, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select_audio = Some(Arc::new(handler));
        self
    }
}

impl RenderOnce for VideoPreviewReady {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_dark = cx.theme().mode.is_dark();
        let bg_color = if is_dark {
            rgb(0x18181b)
        } else {
            rgb(0xffffff)
        };
        let border_color = if is_dark {
            rgb(0x27272a)
        } else {
            rgb(0xf0f0f0)
        };
        let thumb_bg = if is_dark {
            rgb(0x27272a)
        } else {
            rgb(0xf4f4f5)
        };
        let title_color = if is_dark {
            rgb(0xfafafa)
        } else {
            rgb(0x18181b)
        };
        let meta_color = if is_dark {
            rgb(0xa1a1aa)
        } else {
            rgb(0x71717a)
        };
        let format_bg = if is_dark {
            rgb(0x27272a)
        } else {
            rgb(0xf8f8f8)
        };
        let format_hover_bg = if is_dark {
            rgb(0x3f3f46)
        } else {
            rgb(0xf0f0f0)
        };
        let format_selected_bg = if is_dark {
            rgb(0x1d4ed8)
        } else {
            rgb(0x2563eb)
        };
        let format_selected_border = if is_dark {
            rgb(0x3b82f6)
        } else {
            rgb(0x3b82f6)
        };
        let format_text = if is_dark {
            rgb(0xe4e4e7)
        } else {
            rgb(0x3f3f46)
        };
        let section_title_color = if is_dark {
            rgb(0x9ca3af)
        } else {
            rgb(0x6b7280)
        };
        let hint_color = if is_dark {
            rgb(0x6b7280)
        } else {
            rgb(0x9ca3af)
        };
        let badge_green_bg = if is_dark {
            rgba(0x22c55e33)
        } else {
            rgba(0x22c55e22)
        };
        let badge_green_text = rgb(0x22c55e);
        let _ = format_hover_bg;

        // 分类格式
        // 1. 合并格式（视频+音频一体）
        let combined_formats: Vec<_> = self
            .info
            .formats
            .iter()
            .filter(|f| f.has_video && f.has_audio)
            .cloned()
            .collect();

        // 2. 仅视频格式
        let video_only_formats: Vec<_> = self
            .info
            .formats
            .iter()
            .filter(|f| f.has_video && !f.has_audio)
            .cloned()
            .collect();

        // 3. 仅音频格式
        let audio_only_formats: Vec<_> = self
            .info
            .formats
            .iter()
            .filter(|f| !f.has_video && f.has_audio)
            .cloned()
            .collect();

        // 判断是否是分离格式网站（如B站）：没有合并格式，但有视频和音频分开的格式
        let is_separated_source = combined_formats.is_empty()
            && !video_only_formats.is_empty()
            && !audio_only_formats.is_empty();

        let selected_video = self.selected_video_id.clone();
        let selected_audio = self.selected_audio_id.clone();
        let on_download = self.on_download;
        let on_download_thumbnail = self.on_download_thumbnail;
        let on_select_video = self.on_select_video;
        let on_select_audio = self.on_select_audio;
        let has_thumbnail = self.info.thumbnail.is_some();

        // 计算总格式数量，决定是否需要滚动
        let total_formats =
            combined_formats.len() + video_only_formats.len() + audio_only_formats.len();

        // 计算选中状态的提示信息
        let selection_hint = {
            let has_video = selected_video.is_some();
            let has_audio = selected_audio.is_some();
            let video_includes_audio = selected_video
                .as_ref()
                .and_then(|vid| self.info.formats.iter().find(|f| &f.format_id == vid))
                .map(|f| f.has_audio)
                .unwrap_or(false);

            if video_includes_audio {
                "✅ 已选择音视频合并格式，可直接下载".to_string()
            } else if is_separated_source && has_video {
                // B站等分离格式网站，选择视频时自动带音频
                "✅ 已选择视频质量，下载时自动包含音频".to_string()
            } else if has_video && has_audio {
                "🔀 已选择视频+音频，下载后将自动合并".to_string()
            } else if has_video {
                "📹 仅下载视频（无声音）".to_string()
            } else if has_audio {
                "🎵 仅下载音频".to_string()
            } else {
                "请选择下载格式".to_string()
            }
        };

        let _ = total_formats;
        let thumbnail_url = self.info.thumbnail.clone();

        div()
            .flex()
            .flex_col()
            .gap(px(20.0))
            .p(px(24.0))
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
            .rounded(px(16.0))
            .shadow_sm()
            // 顶部：缩略图 + 基本信息
            .child(
                div()
                    .flex()
                    .gap(px(20.0))
                    // 缩略图区域
                    .child(
                        div()
                            .flex_shrink_0()
                            .relative()
                            .w(px(220.0))
                            .h(px(124.0))
                            .bg(thumb_bg)
                            .rounded(px(12.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .overflow_hidden()
                            // 显示封面图或占位符
                            .when_some(thumbnail_url.clone(), |el, url| {
                                el.child(
                                    img(url)
                                        .w(px(220.0))
                                        .h(px(124.0))
                                        .object_fit(ObjectFit::Cover),
                                )
                            })
                            .when(thumbnail_url.is_none(), |el| {
                                el.child(div().text_3xl().child("🎬"))
                            })
                            // 时长标签
                            .child(
                                div()
                                    .absolute()
                                    .bottom(px(8.0))
                                    .right(px(8.0))
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .bg(rgba(0x000000cc))
                                    .rounded(px(4.0))
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(0xffffff))
                                    .child(self.info.duration.clone()),
                            )
                            // 封面下载按钮
                            .when(has_thumbnail, |el| {
                                el.child(div().absolute().top(px(8.0)).right(px(8.0)).child({
                                    let mut btn = Button::new("download-thumb-btn")
                                        .ghost()
                                        .compact()
                                        .label("📥");
                                    if let Some(handler) = on_download_thumbnail {
                                        btn = btn.on_click(move |ev, window, cx| {
                                            handler(ev, window, cx)
                                        });
                                    }
                                    btn
                                }))
                            }),
                    )
                    // 信息区域
                    .child(
                        div()
                            .flex_1()
                            .min_w_0() // 防止 flex 子元素撑开容器
                            .flex()
                            .flex_col()
                            .justify_center()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(title_color)
                                    .line_height(px(24.0))
                                    // 手动截断标题，避免 GPUI DirectWrite 在 Windows 上的 UTF-8 边界 bug
                                    .child(truncate_string(&self.info.title, 60)),
                            )
                            .when_some(self.info.uploader.clone(), |el, uploader| {
                                el.child(
                                    div().flex().items_center().gap(px(6.0)).child(
                                        div()
                                            .text_sm()
                                            .text_color(meta_color)
                                            .child(format!("👤 {}", uploader)),
                                    ),
                                )
                            }),
                    ),
            )
            // 📦 合并格式（推荐）
            .when(!combined_formats.is_empty(), |el| {
                let selected = selected_video.clone();
                let on_select = on_select_video.clone();
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(section_title_color)
                                        .child("选择画质"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .px(px(8.0))
                                        .py(px(3.0))
                                        .bg(badge_green_bg)
                                        .text_color(badge_green_text)
                                        .rounded(px(6.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child("含音频"),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(10.0))
                                .pt(px(6.0)) // 给勾选标记留出空间
                                .children(combined_formats.into_iter().map({
                                    let selected = selected.clone();
                                    let on_select = on_select.clone();
                                    move |fmt| {
                                        let is_selected = selected.as_ref() == Some(&fmt.format_id);
                                        let format_id = fmt.format_id.clone();
                                        let on_select = on_select.clone();

                                        div()
                                            .id(SharedString::from(format!(
                                                "combined-{}",
                                                fmt.format_id
                                            )))
                                            .relative()
                                            .px(px(16.0))
                                            .py(px(10.0))
                                            .min_w(px(90.0))
                                            .bg(if is_selected {
                                                format_selected_bg
                                            } else {
                                                format_bg
                                            })
                                            .border_1()
                                            .border_color(if is_selected {
                                                format_selected_border
                                            } else {
                                                border_color
                                            })
                                            .rounded(px(10.0))
                                            .cursor_pointer()
                                            .when(is_selected, |el| el.shadow_sm())
                                            .on_click({
                                                let format_id = format_id.clone();
                                                move |_ev, window, cx| {
                                                    if let Some(ref handler) = on_select {
                                                        handler(&format_id, window, cx);
                                                    }
                                                }
                                            })
                                            // 选中指示器
                                            .when(is_selected, |el| {
                                                el.child(
                                                    div()
                                                        .absolute()
                                                        .top(px(4.0))
                                                        .right(px(4.0))
                                                        .w(px(16.0))
                                                        .h(px(16.0))
                                                        .rounded_full()
                                                        .bg(rgb(0x22c55e))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .text_xs()
                                                        .text_color(rgb(0xffffff))
                                                        .child("✓"),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .items_center()
                                                    .gap(px(4.0))
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(if is_selected {
                                                                rgb(0xffffff)
                                                            } else {
                                                                format_text
                                                            })
                                                            .child(fmt.label.clone()),
                                                    )
                                                    .when_some(
                                                        fmt.format_filesize(),
                                                        |el, size| {
                                                            el.child(
                                                                div()
                                                                    .text_xs()
                                                                    .text_color(if is_selected {
                                                                        rgba(0xffffffaa)
                                                                    } else {
                                                                        meta_color
                                                                    })
                                                                    .child(size),
                                                            )
                                                        },
                                                    ),
                                            )
                                    }
                                })),
                        ),
                )
            })
            // 📹 仅视频格式（对于分离源，显示为"视频质量"）
            .when(!video_only_formats.is_empty(), |el| {
                let selected = selected_video.clone();
                let on_select = on_select_video.clone();
                let section_title = if is_separated_source {
                    "选择画质"
                } else {
                    "仅视频"
                };
                let section_hint = if is_separated_source {
                    Some("下载时自动合并音频")
                } else {
                    Some("无声音，可搭配音频")
                };
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(section_title_color)
                                        .child(section_title),
                                )
                                .when_some(section_hint, |el, hint| {
                                    el.child(div().text_xs().text_color(hint_color).child(hint))
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(10.0))
                                .pt(px(6.0)) // 给勾选标记留出空间
                                .children(video_only_formats.into_iter().map({
                                    let selected = selected.clone();
                                    let on_select = on_select.clone();
                                    move |fmt| {
                                        let is_selected = selected.as_ref() == Some(&fmt.format_id);
                                        let format_id = fmt.format_id.clone();
                                        let on_select = on_select.clone();

                                        div()
                                            .id(SharedString::from(format!(
                                                "video-{}",
                                                fmt.format_id
                                            )))
                                            .relative()
                                            .px(px(16.0))
                                            .py(px(10.0))
                                            .min_w(px(90.0))
                                            .bg(if is_selected {
                                                format_selected_bg
                                            } else {
                                                format_bg
                                            })
                                            .border_1()
                                            .border_color(if is_selected {
                                                format_selected_border
                                            } else {
                                                border_color
                                            })
                                            .rounded(px(10.0))
                                            .cursor_pointer()
                                            .when(is_selected, |el| el.shadow_sm())
                                            .on_click({
                                                let format_id = format_id.clone();
                                                move |_ev, window, cx| {
                                                    if let Some(ref handler) = on_select {
                                                        handler(&format_id, window, cx);
                                                    }
                                                }
                                            })
                                            // 选中指示器
                                            .when(is_selected, |el| {
                                                el.child(
                                                    div()
                                                        .absolute()
                                                        .top(px(4.0))
                                                        .right(px(4.0))
                                                        .w(px(16.0))
                                                        .h(px(16.0))
                                                        .rounded_full()
                                                        .bg(rgb(0x22c55e))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .text_xs()
                                                        .text_color(rgb(0xffffff))
                                                        .child("✓"),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .items_center()
                                                    .gap(px(4.0))
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(if is_selected {
                                                                rgb(0xffffff)
                                                            } else {
                                                                format_text
                                                            })
                                                            .child(fmt.label.clone()),
                                                    )
                                                    .when_some(
                                                        fmt.format_filesize(),
                                                        |el, size| {
                                                            el.child(
                                                                div()
                                                                    .text_xs()
                                                                    .text_color(if is_selected {
                                                                        rgba(0xffffffaa)
                                                                    } else {
                                                                        meta_color
                                                                    })
                                                                    .child(size),
                                                            )
                                                        },
                                                    ),
                                            )
                                    }
                                })),
                        ),
                )
            })
            // 🎵 仅音频格式（始终显示，允许用户单独下载音频）
            .when(!audio_only_formats.is_empty(), |el| {
                let selected = selected_audio.clone();
                let on_select = on_select_audio.clone();
                let hint = if is_separated_source {
                    "可单独下载音频"
                } else {
                    "可单独下载"
                };
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(section_title_color)
                                        .child("仅音频"),
                                )
                                .child(div().text_xs().text_color(hint_color).child(hint)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(10.0))
                                .pt(px(6.0)) // 给勾选标记留出空间
                                .children(audio_only_formats.into_iter().map({
                                    let selected = selected.clone();
                                    let on_select = on_select.clone();
                                    move |fmt| {
                                        let is_selected = selected.as_ref() == Some(&fmt.format_id);
                                        let format_id = fmt.format_id.clone();
                                        let on_select = on_select.clone();

                                        div()
                                            .id(SharedString::from(format!(
                                                "audio-{}",
                                                fmt.format_id
                                            )))
                                            .relative()
                                            .px(px(16.0))
                                            .py(px(10.0))
                                            .min_w(px(90.0))
                                            .bg(if is_selected {
                                                format_selected_bg
                                            } else {
                                                format_bg
                                            })
                                            .border_1()
                                            .border_color(if is_selected {
                                                format_selected_border
                                            } else {
                                                border_color
                                            })
                                            .rounded(px(10.0))
                                            .cursor_pointer()
                                            .when(is_selected, |el| el.shadow_sm())
                                            .on_click({
                                                let format_id = format_id.clone();
                                                move |_ev, window, cx| {
                                                    if let Some(ref handler) = on_select {
                                                        handler(&format_id, window, cx);
                                                    }
                                                }
                                            })
                                            // 选中指示器
                                            .when(is_selected, |el| {
                                                el.child(
                                                    div()
                                                        .absolute()
                                                        .top(px(4.0))
                                                        .right(px(4.0))
                                                        .w(px(16.0))
                                                        .h(px(16.0))
                                                        .rounded_full()
                                                        .bg(rgb(0x22c55e))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .text_xs()
                                                        .text_color(rgb(0xffffff))
                                                        .child("✓"),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .items_center()
                                                    .gap(px(4.0))
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(if is_selected {
                                                                rgb(0xffffff)
                                                            } else {
                                                                format_text
                                                            })
                                                            .child(fmt.label.clone()),
                                                    )
                                                    .when_some(
                                                        fmt.format_filesize(),
                                                        |el, size| {
                                                            el.child(
                                                                div()
                                                                    .text_xs()
                                                                    .text_color(if is_selected {
                                                                        rgba(0xffffffaa)
                                                                    } else {
                                                                        meta_color
                                                                    })
                                                                    .child(size),
                                                            )
                                                        },
                                                    ),
                                            )
                                    }
                                })),
                        ),
                )
            })
            // 底部操作栏
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pt(px(16.0))
                    .mt(px(4.0))
                    .border_t_1()
                    .border_color(border_color)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_sm()
                            .text_color(hint_color)
                            .child(selection_hint),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(12.0))
                            .child({
                                let mut btn = Button::new("cancel-btn").ghost().label("取消");
                                if let Some(handler) = self.on_cancel {
                                    btn =
                                        btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                                }
                                btn
                            })
                            .child({
                                let has_selection =
                                    selected_video.is_some() || selected_audio.is_some();
                                let mut btn = Button::new("start-download-btn")
                                    .primary()
                                    .label("开始下载")
                                    .disabled(!has_selection);
                                if let Some(handler) = on_download {
                                    btn =
                                        btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                                }
                                btn
                            }),
                    ),
            )
    }
}

/// 视频预览卡片 - 下载中状态
#[derive(IntoElement)]
pub struct VideoPreviewDownloading {
    progress: f32,
    speed: String,
}

impl VideoPreviewDownloading {
    pub fn new(progress: f32, speed: impl Into<String>) -> Self {
        Self {
            progress,
            speed: speed.into(),
        }
    }
}

impl RenderOnce for VideoPreviewDownloading {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let progress_percent = (self.progress * 100.0).min(100.0);
        let is_dark = cx.theme().mode.is_dark();
        let bg_color = if is_dark {
            rgb(0x18181b)
        } else {
            rgb(0xffffff)
        };
        let border_color = if is_dark {
            rgb(0x27272a)
        } else {
            rgb(0xf0f0f0)
        };
        let title_color = if is_dark {
            rgb(0xfafafa)
        } else {
            rgb(0x18181b)
        };
        let meta_color = if is_dark {
            rgb(0xa1a1aa)
        } else {
            rgb(0x71717a)
        };
        let progress_bg = if is_dark {
            rgb(0x27272a)
        } else {
            rgb(0xf0f0f0)
        };
        let progress_bar_color = rgb(0x3b82f6);

        div()
            .flex()
            .flex_col()
            .gap(px(20.0))
            .p(px(24.0))
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
            .rounded(px(16.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .child(div().text_xl().child("⬇️"))
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(title_color)
                                    .child("正在下载..."),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(meta_color)
                            .child(self.speed),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .h(px(10.0))
                            .w_full()
                            .bg(progress_bg)
                            .rounded(px(5.0))
                            .overflow_hidden()
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(self.progress))
                                    .bg(progress_bar_color)
                                    .rounded(px(5.0)),
                            ),
                    )
                    .child(
                        div().flex().justify_between().child(
                            div()
                                .text_sm()
                                .text_color(meta_color)
                                .child(format!("{:.1}%", progress_percent)),
                        ),
                    ),
            )
    }
}

/// 视频预览卡片 - 完成状态
#[derive(IntoElement)]
pub struct VideoPreviewCompleted {
    path: String,
    on_new_download: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl VideoPreviewCompleted {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            on_new_download: None,
        }
    }

    pub fn on_new_download(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_new_download = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for VideoPreviewCompleted {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_dark = cx.theme().mode.is_dark();
        let bg_color = if is_dark {
            rgba(0x22c55e15)
        } else {
            rgba(0x22c55e10)
        };
        let border_color = if is_dark {
            rgba(0x22c55e40)
        } else {
            rgba(0x22c55e30)
        };
        let icon_bg = if is_dark {
            rgba(0x22c55e25)
        } else {
            rgba(0x22c55e20)
        };
        let title_color = rgb(0x22c55e);
        let path_color = if is_dark {
            rgb(0xa1a1aa)
        } else {
            rgb(0x6b7280)
        };

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(48.0))
            .px(px(40.0))
            .gap(px(16.0))
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
            .rounded(px(16.0))
            .child(
                div()
                    .w(px(64.0))
                    .h(px(64.0))
                    .rounded_full()
                    .bg(icon_bg)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_2xl()
                    .child("✅"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(title_color)
                            .child("下载完成!"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(path_color)
                            .max_w(px(400.0))
                            .text_center()
                            .truncate()
                            .child(self.path),
                    ),
            )
            .child({
                let mut btn = Button::new("new-download-btn").primary().label("新下载");
                if let Some(handler) = self.on_new_download {
                    btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                }
                btn
            })
    }
}

/// 视频预览卡片 - 错误状态
#[derive(IntoElement)]
pub struct VideoPreviewError {
    message: String,
    on_retry: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl VideoPreviewError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            on_retry: None,
        }
    }

    pub fn on_retry(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_retry = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for VideoPreviewError {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_dark = cx.theme().mode.is_dark();
        let bg_color = if is_dark {
            rgba(0xef444415)
        } else {
            rgba(0xef444410)
        };
        let border_color = if is_dark {
            rgba(0xef444440)
        } else {
            rgba(0xef444430)
        };
        let icon_bg = if is_dark {
            rgba(0xef444425)
        } else {
            rgba(0xef444420)
        };
        let title_color = rgb(0xef4444);
        let msg_color = if is_dark {
            rgb(0xa1a1aa)
        } else {
            rgb(0x6b7280)
        };

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(48.0))
            .px(px(40.0))
            .gap(px(16.0))
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
            .rounded(px(16.0))
            .child(
                div()
                    .w(px(64.0))
                    .h(px(64.0))
                    .rounded_full()
                    .bg(icon_bg)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_2xl()
                    .child("❌"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(title_color)
                            .child("出错了"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(msg_color)
                            .max_w(px(400.0))
                            .text_center()
                            .child(self.message),
                    ),
            )
            .child({
                let mut btn = Button::new("retry-btn").primary().label("重试");
                if let Some(handler) = self.on_retry {
                    btn = btn.on_click(move |ev, window, cx| handler(ev, window, cx));
                }
                btn
            })
    }
}
