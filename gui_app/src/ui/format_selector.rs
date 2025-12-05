// gui_app/src/ui/format_selector.rs
//! 格式选择器组件 - 视频/音频格式选择、画质和音频质量选择、字幕选项

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use magekit_shared::types::VideoFormat;
use std::sync::Arc;

/// 格式类型
#[derive(Debug, Clone, PartialEq)]
pub enum FormatType {
    /// 视频+音频
    VideoAudio,
    /// 仅视频
    VideoOnly,
    /// 仅音频
    AudioOnly,
}

impl FormatType {
    fn label(&self) -> &'static str {
        match self {
            FormatType::VideoAudio => "视频+音频",
            FormatType::VideoOnly => "仅视频",
            FormatType::AudioOnly => "仅音频",
        }
    }

    fn all() -> Vec<Self> {
        vec![FormatType::VideoAudio, FormatType::VideoOnly, FormatType::AudioOnly]
    }
}

/// 视频分辨率
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoResolution {
    /// 4K
    UHD4K,
    /// 2K
    QHD,
    /// 1080p
    FHD,
    /// 720p
    HD,
    /// 480p
    SD,
    /// 360p
    Low,
    /// 最佳
    Best,
}

impl VideoResolution {
    fn label(&self) -> &'static str {
        match self {
            VideoResolution::UHD4K => "4K (2160p)",
            VideoResolution::QHD => "2K (1440p)",
            VideoResolution::FHD => "1080p",
            VideoResolution::HD => "720p",
            VideoResolution::SD => "480p",
            VideoResolution::Low => "360p",
            VideoResolution::Best => "最佳质量",
        }
    }

    fn format_filter(&self) -> &'static str {
        match self {
            VideoResolution::UHD4K => "bestvideo[height<=2160]",
            VideoResolution::QHD => "bestvideo[height<=1440]",
            VideoResolution::FHD => "bestvideo[height<=1080]",
            VideoResolution::HD => "bestvideo[height<=720]",
            VideoResolution::SD => "bestvideo[height<=480]",
            VideoResolution::Low => "bestvideo[height<=360]",
            VideoResolution::Best => "bestvideo",
        }
    }

    fn all() -> Vec<Self> {
        vec![
            VideoResolution::Best,
            VideoResolution::UHD4K,
            VideoResolution::QHD,
            VideoResolution::FHD,
            VideoResolution::HD,
            VideoResolution::SD,
            VideoResolution::Low,
        ]
    }
}

/// 音频质量
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioQuality {
    /// 最佳
    Best,
    /// 320kbps
    High,
    /// 192kbps
    Medium,
    /// 128kbps
    Low,
}

impl AudioQuality {
    fn label(&self) -> &'static str {
        match self {
            AudioQuality::Best => "最佳",
            AudioQuality::High => "320 kbps",
            AudioQuality::Medium => "192 kbps",
            AudioQuality::Low => "128 kbps",
        }
    }

    fn format_filter(&self) -> &'static str {
        match self {
            AudioQuality::Best => "bestaudio",
            AudioQuality::High => "bestaudio[abr<=320]",
            AudioQuality::Medium => "bestaudio[abr<=192]",
            AudioQuality::Low => "bestaudio[abr<=128]",
        }
    }

    fn all() -> Vec<Self> {
        vec![AudioQuality::Best, AudioQuality::High, AudioQuality::Medium, AudioQuality::Low]
    }
}

/// 音频格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    /// MP3
    Mp3,
    /// AAC
    Aac,
    /// FLAC
    Flac,
    /// WAV
    Wav,
    /// Opus
    Opus,
    /// 原始格式
    Original,
}

impl AudioFormat {
    fn label(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "MP3",
            AudioFormat::Aac => "AAC",
            AudioFormat::Flac => "FLAC",
            AudioFormat::Wav => "WAV",
            AudioFormat::Opus => "Opus",
            AudioFormat::Original => "原始格式",
        }
    }

    fn extension(&self) -> Option<&'static str> {
        match self {
            AudioFormat::Mp3 => Some("mp3"),
            AudioFormat::Aac => Some("aac"),
            AudioFormat::Flac => Some("flac"),
            AudioFormat::Wav => Some("wav"),
            AudioFormat::Opus => Some("opus"),
            AudioFormat::Original => None,
        }
    }

    fn all() -> Vec<Self> {
        vec![
            AudioFormat::Original,
            AudioFormat::Mp3,
            AudioFormat::Aac,
            AudioFormat::Flac,
            AudioFormat::Wav,
            AudioFormat::Opus,
        ]
    }
}

/// 字幕选项
#[derive(Debug, Clone)]
pub struct SubtitleOptions {
    /// 是否下载字幕
    pub enabled: bool,
    /// 选择的语言
    pub languages: Vec<String>,
    /// 是否嵌入字幕
    pub embed: bool,
    /// 是否下载自动生成字幕
    pub auto_generated: bool,
}

impl Default for SubtitleOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            languages: vec!["zh".to_string(), "en".to_string()],
            embed: false,
            auto_generated: false,
        }
    }
}

/// 格式选择器事件
#[derive(Debug, Clone)]
pub enum FormatSelectorEvent {
    /// 格式类型变更
    FormatTypeChanged(FormatType),
    /// 分辨率变更
    ResolutionChanged(VideoResolution),
    /// 音频质量变更
    AudioQualityChanged(AudioQuality),
    /// 音频格式变更
    AudioFormatChanged(AudioFormat),
    /// 字幕选项变更
    SubtitleOptionsChanged(SubtitleOptions),
    /// 选择了具体的格式
    FormatSelected(String),
}

/// 格式选择器回调类型
pub type FormatSelectorCallback = Arc<dyn Fn(FormatSelectorEvent) + Send + Sync>;

/// 格式选择器组件
pub struct FormatSelector {
    /// 可用格式列表
    formats: Vec<VideoFormat>,
    /// 当前选择的格式类型
    format_type: FormatType,
    /// 分辨率
    resolution: VideoResolution,
    /// 音频质量
    audio_quality: AudioQuality,
    /// 音频格式
    audio_format: AudioFormat,
    /// 字幕选项
    subtitle_options: SubtitleOptions,
    /// 选择的格式 ID
    selected_format_id: Option<String>,
    /// 是否展开详细格式列表
    show_all_formats: bool,
    /// 事件回调
    callback: Option<FormatSelectorCallback>,
}

impl FormatSelector {
    /// 创建新的格式选择器
    pub fn new() -> Self {
        Self {
            formats: Vec::new(),
            format_type: FormatType::VideoAudio,
            resolution: VideoResolution::Best,
            audio_quality: AudioQuality::Best,
            audio_format: AudioFormat::Original,
            subtitle_options: SubtitleOptions::default(),
            selected_format_id: None,
            show_all_formats: false,
            callback: None,
        }
    }

    /// 设置可用格式
    pub fn set_formats(&mut self, formats: Vec<VideoFormat>) {
        self.formats = formats;
    }

    /// 设置事件回调
    pub fn on_event(mut self, callback: FormatSelectorCallback) -> Self {
        self.callback = Some(callback);
        self
    }

    /// 设置格式类型
    pub fn set_format_type(&mut self, format_type: FormatType) {
        self.format_type = format_type;
    }

    /// 切换显示所有格式
    pub fn toggle_show_all(&mut self) {
        self.show_all_formats = !self.show_all_formats;
    }

    /// 选择特定格式
    pub fn select_format(&mut self, format_id: String) {
        self.selected_format_id = Some(format_id);
    }

    /// 生成 yt-dlp 格式字符串
    pub fn build_format_string(&self) -> String {
        match self.format_type {
            FormatType::VideoAudio => {
                let video = self.resolution.format_filter();
                let audio = self.audio_quality.format_filter();
                format!("{}+{}/best", video, audio)
            }
            FormatType::VideoOnly => {
                self.resolution.format_filter().to_string()
            }
            FormatType::AudioOnly => {
                self.audio_quality.format_filter().to_string()
            }
        }
    }

    /// 获取字幕语言列表
    pub fn get_subtitle_langs(&self) -> Option<Vec<String>> {
        if self.subtitle_options.enabled && !self.subtitle_options.languages.is_empty() {
            Some(self.subtitle_options.languages.clone())
        } else {
            None
        }
    }
}

impl Render for FormatSelector {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 获取主题颜色
        let theme = cx.theme();
        let bg = theme.background;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let border = theme.border;
        let primary = theme.primary;
        let primary_fg = theme.primary_foreground;

        // 渲染主内容
        v_flex()
            .gap_4()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(border)
            .bg(bg)
            // 格式类型选择
            .child(self.render_format_type_selector(cx))
            // 根据类型显示不同选项
            .when(self.format_type != FormatType::AudioOnly, |this| {
                this.child(self.render_resolution_selector(cx))
            })
            .when(self.format_type != FormatType::VideoOnly, |this| {
                this.child(self.render_audio_quality_selector(cx))
            })
            .when(self.format_type == FormatType::AudioOnly, |this| {
                this.child(self.render_audio_format_selector(cx))
            })
            // 字幕选项
            .child(self.render_subtitle_options(cx))
            // 高级格式列表
            .when(!self.formats.is_empty(), |this| {
                this.child(self.render_format_list(cx))
            })
    }
}

impl FormatSelector {
    /// 渲染格式类型选择器
    fn render_format_type_selector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let current_type = self.format_type.clone();

        v_flex()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("下载类型")
            )
            .child(
                h_flex()
                    .gap_2()
                    .children(FormatType::all().into_iter().map(|ft| {
                        let is_selected = ft == current_type;
                        let label = ft.label();
                        Button::new(SharedString::from(format!("format-type-{}", label)))
                            .label(SharedString::from(label))
                            .small()
                            .when(is_selected, |btn| btn.primary())
                            .when(!is_selected, |btn| btn.ghost())
                            .into_any_element()
                    }))
            )
    }

    /// 渲染分辨率选择器
    fn render_resolution_selector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let border = theme.border;
        let current_res = self.resolution;

        v_flex()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("视频质量")
            )
            .child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .children(VideoResolution::all().into_iter().map(|res| {
                        let is_selected = res == current_res;
                        let label = res.label();
                        Button::new(SharedString::from(format!("res-{}", label)))
                            .label(SharedString::from(label))
                            .small()
                            .when(is_selected, |btn| btn.primary())
                            .when(!is_selected, |btn| btn.outline())
                            .into_any_element()
                    }))
            )
    }

    /// 渲染音频质量选择器
    fn render_audio_quality_selector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let current_quality = self.audio_quality;

        v_flex()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("音频质量")
            )
            .child(
                h_flex()
                    .gap_2()
                    .children(AudioQuality::all().into_iter().map(|quality| {
                        let is_selected = quality == current_quality;
                        let label = quality.label();
                        Button::new(SharedString::from(format!("audio-quality-{}", label)))
                            .label(SharedString::from(label))
                            .small()
                            .when(is_selected, |btn| btn.primary())
                            .when(!is_selected, |btn| btn.outline())
                            .into_any_element()
                    }))
            )
    }

    /// 渲染音频格式选择器
    fn render_audio_format_selector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let current_format = self.audio_format;

        v_flex()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("音频格式")
            )
            .child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .children(AudioFormat::all().into_iter().map(|format| {
                        let is_selected = format == current_format;
                        let label = format.label();
                        Button::new(SharedString::from(format!("audio-format-{}", label)))
                            .label(SharedString::from(label))
                            .small()
                            .when(is_selected, |btn| btn.primary())
                            .when(!is_selected, |btn| btn.outline())
                            .into_any_element()
                    }))
            )
    }

    /// 渲染字幕选项
    fn render_subtitle_options(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let border = theme.border;
        let bg = theme.background;
        let enabled = self.subtitle_options.enabled;
        let embed = self.subtitle_options.embed;
        let auto_gen = self.subtitle_options.auto_generated;

        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Checkbox::new("subtitle-enabled")
                            .checked(enabled)
                    )
                    .child(
                        div()
                            .text_sm()
                            .child("下载字幕")
                    )
            )
            .when(enabled, |this| {
                this.child(
                    v_flex()
                        .pl_6()
                        .gap_2()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(muted)
                                        .child("语言：")
                                )
                                .child(
                                    h_flex()
                                        .gap_1()
                                        .children(vec![("zh", "中文"), ("en", "English"), ("ja", "日本語")].into_iter().map(|(code, label)| {
                                            let is_selected = self.subtitle_options.languages.contains(&code.to_string());
                                            Button::new(SharedString::from(format!("sub-lang-{}", code)))
                                                .label(SharedString::from(label))
                                                .xsmall()
                                                .when(is_selected, |btn| btn.primary())
                                                .when(!is_selected, |btn| btn.ghost())
                                                .into_any_element()
                                        }))
                                )
                        )
                        .child(
                            h_flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    Checkbox::new("subtitle-embed")
                                        .checked(embed)
                                )
                                .child(
                                    div().text_sm().child("嵌入字幕到视频")
                                )
                        )
                        .child(
                            h_flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    Checkbox::new("subtitle-auto")
                                        .checked(auto_gen)
                                )
                                .child(
                                    div().text_sm().child("包含自动生成字幕")
                                )
                        )
                )
            })
    }

    /// 渲染格式列表
    fn render_format_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let border = theme.border;
        let bg = theme.muted;
        let primary = theme.primary;
        let show_all = self.show_all_formats;

        let formats = if show_all {
            self.formats.clone()
        } else {
            self.formats.iter().take(5).cloned().collect()
        };

        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("可用格式")
                    )
                    .child(
                        Button::new("toggle-formats")
                            .ghost()
                            .xsmall()
                            .label(SharedString::from(if show_all { "收起" } else { "显示全部" }))
                    )
            )
            .child(
                render_format_list_static(&formats, self.selected_format_id.as_ref(), muted, border, primary, bg)
            )
    }
}

/// 渲染格式列表
fn render_format_list_static(
    formats: &[VideoFormat],
    selected_id: Option<&String>,
    muted: Hsla,
    border: Hsla,
    primary: Hsla,
    bg: Hsla,
) -> impl IntoElement {
    v_flex()
        .gap_1()
        .max_h(px(200.0))
        .overflow_hidden()
        .children(formats.iter().map(|format| {
            render_format_item_inline(format, selected_id, muted, border, primary, bg)
        }))
}

/// 渲染单个格式项（内联版本，不需要 cx）
fn render_format_item_inline(
    format: &VideoFormat, 
    selected_id: Option<&String>,
    muted: Hsla,
    border: Hsla,
    primary: Hsla,
    bg: Hsla,
) -> impl IntoElement {
    let is_selected = selected_id.map(|id| id == &format.format_id).unwrap_or(false);

    let format_id = format.format_id.clone();
    let ext = format.ext.clone();
    let resolution = format.resolution.clone().unwrap_or_else(|| "N/A".to_string());
    let filesize = format.filesize.map(format_filesize).unwrap_or_else(|| "未知".to_string());
    let vcodec = format.vcodec.clone().unwrap_or_default();
    let acodec = format.acodec.clone().unwrap_or_default();

    // 确定类型徽章
    let type_badge = if vcodec.is_empty() || vcodec == "none" {
        "音频"
    } else if acodec.is_empty() || acodec == "none" {
        "视频"
    } else {
        "视频+音频"
    };

    h_flex()
        .px_2()
        .py_1()
        .rounded_md()
        .gap_2()
        .items_center()
        .border_1()
        .border_color(if is_selected { primary } else { border })
        .bg(bg)
        .cursor_pointer()
        // 格式ID
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .w(px(60.0))
                .child(format_id)
        )
        // 类型徽章
        .child(
            div()
                .text_xs()
                .px_1()
                .py_px()
                .rounded(px(2.0))
                .bg(primary.opacity(0.1))
                .text_color(primary)
                .child(type_badge)
        )
        // 扩展名
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .w(px(40.0))
                .child(ext)
        )
        // 分辨率
        .child(
            div()
                .text_xs()
                .w(px(80.0))
                .child(resolution)
        )
        // 文件大小
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .w(px(60.0))
                .child(filesize)
        )
        // 编码信息
        .child(
            div()
                .flex_1()
                .text_xs()
                .text_color(muted)
                .overflow_hidden()
                .child(format!("{} / {}", 
                    if vcodec.is_empty() || vcodec == "none" { "-" } else { &vcodec },
                    if acodec.is_empty() || acodec == "none" { "-" } else { &acodec }
                ))
        )
}

/// 格式化文件大小
fn format_filesize(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
