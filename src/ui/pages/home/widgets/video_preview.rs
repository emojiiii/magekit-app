//! 视频预览组件
//!
//! 显示解析后的视频信息

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::Disableable;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::empty::{Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle};
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{Icon, IconName, Sizable};
use std::sync::Arc;

/// 视频格式信息 (从解析获取)
#[derive(Debug, Clone, PartialEq)]
pub struct VideoFormatInfo {
    pub format_id: String,
    pub label: String, // 如 "1080p", "720p", "音频"
    /// 仅应用生成的回退标签设置翻译键；来源提供的画质和编码文本保持原样。
    pub label_key: Option<&'static str>,
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
    /// 在渲染时读取当前语言，不改写已解析的格式信息或格式 ID。
    pub fn display_label(&self) -> String {
        self.label_key
            .map(crate::i18n::tr)
            .map(str::to_owned)
            .unwrap_or_else(|| self.label.clone())
    }

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

/// Kit 空状态与加载指示，避免重复维护颜色和交互样式。
#[derive(IntoElement)]
pub struct VideoPreviewIdle;
impl RenderOnce for VideoPreviewIdle {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        GroupBox::new()
            .id("download-preview-empty")
            .outline()
            .content_style(
                StyleRefinement::default()
                    .bg(cx.theme().background)
                    .p_6()
                    .rounded_xl(),
            )
            .child(
                Empty::new()
                    .py_8()
                    .header(
                        EmptyHeader::new()
                            .media(
                                EmptyMedia::new()
                                    .size_12()
                                    .rounded_xl()
                                    .bg(cx.theme().muted)
                                    .child(
                                        Icon::new(IconName::Play)
                                            .size_6()
                                            .text_color(cx.theme().muted_foreground),
                                    ),
                            )
                            .title(EmptyTitle::new().child(crate::i18n::tr("粘贴视频链接开始下载")))
                            .description(
                                EmptyDescription::new()
                                    .child(crate::i18n::tr("解析后可选择视频画质和音频格式")),
                            ),
                    )
                    .child(
                        div().flex().flex_wrap().justify_center().gap_2().children(
                            ["YouTube", "Bilibili", "Twitter / X"]
                                .into_iter()
                                .map(|name| Tag::secondary().small().child(name)),
                        ),
                    ),
            )
    }
}

#[derive(IntoElement)]
pub struct VideoPreviewLoading;
impl RenderOnce for VideoPreviewLoading {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        Empty::new().py_12().header(
            EmptyHeader::new()
                .media(EmptyMedia::new().child(Spinner::new().large()))
                .title(EmptyTitle::new().child(crate::i18n::tr("正在解析视频...")))
                .description(
                    EmptyDescription::new().child(crate::i18n::tr("可以取消解析或输入新的链接")),
                ),
        )
    }
}

#[derive(IntoElement)]
pub struct VideoPreviewReady {
    info: VideoInfo,
    submitting: bool,
    thumbnail_loading: bool,
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
            submitting: false,
            thumbnail_loading: false,
            selected_video_id: None,
            selected_audio_id: None,
            on_cancel: None,
            on_download: None,
            on_download_thumbnail: None,
            on_select_video: None,
            on_select_audio: None,
        }
    }

    pub fn submitting(mut self, value: bool) -> Self {
        self.submitting = value;
        self
    }
    pub fn thumbnail_loading(mut self, value: bool) -> Self {
        self.thumbnail_loading = value;
        self
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
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let border = cx.theme().border;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let background = cx.theme().background;
        let thumbnail_background = cx.theme().muted;
        let has_selection = self.selected_video_id.is_some() || self.selected_audio_id.is_some();
        let disabled = self.submitting;
        let formats = self.info.formats.clone();
        let format_groups = [
            (FormatType::Combined, "视频和音频"),
            (FormatType::VideoOnly, "视频画质"),
            (FormatType::AudioOnly, "音频格式"),
        ]
        .into_iter()
        .filter_map(|(kind, title)| {
            let entries: Vec<_> = formats
                .iter()
                .filter(|format| format.format_type() == kind)
                .cloned()
                .collect();
            if entries.is_empty() {
                return None;
            }
            let buttons = entries
                .into_iter()
                .map(|format| {
                    let audio = kind == FormatType::AudioOnly;
                    let selected = if audio {
                        self.selected_audio_id.as_ref()
                    } else {
                        self.selected_video_id.as_ref()
                    } == Some(&format.format_id);
                    let handler = if audio {
                        self.on_select_audio.clone()
                    } else {
                        self.on_select_video.clone()
                    };
                    let label = format!(
                        "{} · {}{}",
                        format.display_label(),
                        format.ext.to_uppercase(),
                        format
                            .format_filesize()
                            .map(|size| format!(" · {size}"))
                            .unwrap_or_default()
                    );
                    let format_id = format.format_id.clone();
                    Button::new(SharedString::from(format!(
                        "format-{:?}-{}",
                        kind, format_id
                    )))
                    .small()
                    .map(|button| {
                        if selected {
                            button.primary()
                        } else {
                            button.outline()
                        }
                    })
                    .label(label.clone())
                    .tooltip(label)
                    .disabled(disabled)
                    .when_some(handler, |button, handler| {
                        button.on_click(move |_, window, cx| handler(&format_id, window, cx))
                    })
                })
                .collect::<Vec<_>>();
            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(crate::i18n::tr(title)),
                    )
                    .child(div().flex().flex_wrap().gap_2().children(buttons)),
            )
        })
        .collect::<Vec<_>>();
        let separated = formats
            .iter()
            .any(|format| !format.has_video && format.has_audio)
            && formats.iter().any(|format| {
                self.selected_video_id.as_ref() == Some(&format.format_id) && !format.has_audio
            });
        let selection_hint =
            if separated && self.selected_video_id.is_some() && self.selected_audio_id.is_none() {
                crate::i18n::tr("已自动搭配最佳音频，也可选择其他音轨")
            } else if has_selection {
                crate::i18n::tr("已选择格式，可以开始下载")
            } else {
                crate::i18n::tr("请选择下载格式")
            };

        GroupBox::new()
            .id("download-preview-ready")
            .outline()
            .min_w_0()
            .content_style(
                StyleRefinement::default()
                    .p_6()
                    .gap_6()
                    .bg(background)
                    .rounded_xl(),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_4()
                    .items_start()
                    .when_some(self.info.thumbnail.clone(), |row, thumbnail| {
                        row.child(
                            div()
                                .w(px(224.0))
                                .h(px(126.0))
                                .flex_shrink_0()
                                .rounded_lg()
                                .overflow_hidden()
                                .bg(thumbnail_background)
                                .child(
                                    img(thumbnail)
                                        .size_full()
                                        .object_fit(ObjectFit::Cover)
                                        .with_fallback(|| {
                                            div()
                                                .size_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(Icon::new(IconName::Folder))
                                                .into_any_element()
                                        }),
                                ),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(180.0))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(foreground)
                                    .child(self.info.title),
                            )
                            .when_some(self.info.uploader, |column, uploader| {
                                column.child(div().text_sm().text_color(muted).child(uploader))
                            })
                            .child(div().text_sm().text_color(muted).child(self.info.duration))
                            .when(self.info.thumbnail.is_some(), |column| {
                                column.child(
                                    Button::new("download-thumb-btn")
                                        .small()
                                        .ghost()
                                        .icon(IconName::ArrowDown)
                                        .label(crate::i18n::tr("下载封面"))
                                        .loading(self.thumbnail_loading)
                                        .disabled(self.thumbnail_loading || disabled)
                                        .when_some(
                                            self.on_download_thumbnail,
                                            |button, handler| {
                                                button.on_click(move |event, window, cx| {
                                                    handler(event, window, cx)
                                                })
                                            },
                                        ),
                                )
                            }),
                    ),
            )
            .children(format_groups)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .pt_4()
                    .border_t_1()
                    .border_color(border)
                    .child(div().text_sm().text_color(muted).child(selection_hint))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("cancel-btn")
                                    .ghost()
                                    .label(crate::i18n::tr("取消"))
                                    .disabled(disabled)
                                    .when_some(self.on_cancel, |button, handler| {
                                        button.on_click(move |event, window, cx| {
                                            handler(event, window, cx)
                                        })
                                    }),
                            )
                            .child(
                                Button::new("start-download-btn")
                                    .primary()
                                    .label(if disabled {
                                        crate::i18n::tr("添加任务中...")
                                    } else {
                                        crate::i18n::tr("开始下载")
                                    })
                                    .loading(disabled)
                                    .disabled(disabled || !has_selection)
                                    .when_some(self.on_download, |button, handler| {
                                        button.on_click(move |event, window, cx| {
                                            handler(event, window, cx)
                                        })
                                    }),
                            ),
                    ),
            )
    }
}

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
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                Progress::new("home-download-progress")
                    .value(if self.progress.is_finite() {
                        self.progress * 100.0
                    } else {
                        0.0
                    })
                    .accessibility_label(crate::i18n::tr("下载进度")),
            )
            .child(self.speed)
    }
}

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
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                Alert::success("download-complete", self.path).title(crate::i18n::tr("下载完成")),
            )
            .child(
                Button::new("new-download-btn")
                    .primary()
                    .label(crate::i18n::tr("新建下载"))
                    .when_some(self.on_new_download, |button, handler| {
                        button.on_click(move |event, window, cx| handler(event, window, cx))
                    }),
            )
    }
}

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
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                Alert::error("video-parse-error", self.message).title(crate::i18n::tr("解析失败")),
            )
            .child(
                Button::new("retry-btn")
                    .outline()
                    .label(crate::i18n::tr("重试"))
                    .when_some(self.on_retry, |button, handler| {
                        button.on_click(move |event, window, cx| handler(event, window, cx))
                    }),
            )
    }
}
