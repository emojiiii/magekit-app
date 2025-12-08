//! 下载面板组件
//!
//! 提供视频下载配置界面，包括URL输入、格式选择、视频预览等功能

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::*;
use magekit_shared::types::{DownloadOptions, VideoFormat, VideoInfo};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// 下载面板状态
#[derive(Debug, Clone)]
pub enum DownloadPanelState {
    /// 空闲状态，等待URL输入
    Idle,
    /// 正在获取视频信息
    Fetching,
    /// 已获取视频信息，可以配置下载
    Ready(VideoInfo),
    /// 获取失败
    Error(String),
}

/// 视频质量选项
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityPreset {
    /// 最佳质量
    Best,
    /// 高清 1080p
    FullHD,
    /// 高清 720p
    HD,
    /// 标清 480p
    SD,
    /// 仅音频
    AudioOnly,
    /// 自定义
    Custom,
}

impl QualityPreset {
    fn label(&self) -> &'static str {
        match self {
            QualityPreset::Best => "最佳质量",
            QualityPreset::FullHD => "1080p 全高清",
            QualityPreset::HD => "720p 高清",
            QualityPreset::SD => "480p 标清",
            QualityPreset::AudioOnly => "仅音频",
            QualityPreset::Custom => "自定义",
        }
    }

    fn format_string(&self) -> &'static str {
        match self {
            QualityPreset::Best => "bestvideo+bestaudio/best",
            QualityPreset::FullHD => "bestvideo[height<=1080]+bestaudio/best[height<=1080]",
            QualityPreset::HD => "bestvideo[height<=720]+bestaudio/best[height<=720]",
            QualityPreset::SD => "bestvideo[height<=480]+bestaudio/best[height<=480]",
            QualityPreset::AudioOnly => "bestaudio",
            QualityPreset::Custom => "best",
        }
    }

    fn all() -> Vec<Self> {
        vec![
            QualityPreset::Best,
            QualityPreset::FullHD,
            QualityPreset::HD,
            QualityPreset::SD,
            QualityPreset::AudioOnly,
        ]
    }
}

/// 下载选项配置
#[derive(Debug, Clone)]
pub struct DownloadConfig {
    /// 视频URL
    pub url: String,
    /// 输出路径
    pub output_path: PathBuf,
    /// 质量预设
    pub quality: QualityPreset,
    /// 嵌入元数据
    pub embed_metadata: bool,
    /// 嵌入缩略图
    pub embed_thumbnail: bool,
    /// 下载字幕
    pub download_subtitles: bool,
    /// 字幕语言
    pub subtitle_languages: Vec<String>,
    /// 嵌入字幕
    pub embed_subtitles: bool,
    /// 自动选择最佳字幕
    pub auto_subtitles: bool,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            output_path: dirs::download_dir().unwrap_or_else(|| PathBuf::from(".")),
            quality: QualityPreset::Best,
            embed_metadata: true,
            embed_thumbnail: true,
            download_subtitles: false,
            subtitle_languages: vec!["zh".to_string(), "en".to_string()],
            embed_subtitles: false,
            auto_subtitles: false,
        }
    }
}

/// 下载面板事件
#[derive(Debug, Clone)]
pub enum DownloadPanelEvent {
    /// URL变更
    UrlChanged(String),
    /// 开始获取视频信息
    FetchInfo,
    /// 质量变更
    QualityChanged(QualityPreset),
    /// 输出路径变更
    OutputPathChanged(PathBuf),
    /// 开始下载
    StartDownload(DownloadConfig),
    /// 选项切换
    ToggleOption(DownloadOption),
}

/// 下载选项类型
#[derive(Debug, Clone, Copy)]
pub enum DownloadOption {
    EmbedMetadata,
    EmbedThumbnail,
    DownloadSubtitles,
    EmbedSubtitles,
    AutoSubtitles,
}

/// 下载面板回调
pub type DownloadPanelCallback = Arc<dyn Fn(DownloadPanelEvent) + Send + Sync + 'static>;

/// 下载面板组件
pub struct DownloadPanel {
    /// 面板状态
    state: DownloadPanelState,
    /// 下载配置
    config: DownloadConfig,
    /// 事件回调
    _on_event: Option<DownloadPanelCallback>,
    /// 是否展开高级选项
    show_advanced: bool,
    /// 当前选中的格式索引
    _selected_format_index: Option<usize>,
}

impl DownloadPanel {
    /// 创建新的下载面板
    pub fn new() -> Self {
        Self {
            state: DownloadPanelState::Idle,
            config: DownloadConfig::default(),
            _on_event: None,
            show_advanced: false,
            _selected_format_index: None,
        }
    }

    /// 设置事件回调
    pub fn on_event(mut self, callback: DownloadPanelCallback) -> Self {
        self._on_event = Some(callback);
        self
    }

    /// 设置URL
    pub fn set_url(&mut self, url: String) {
        self.config.url = url;
    }

    /// 设置视频信息
    pub fn set_video_info(&mut self, info: VideoInfo) {
        self.state = DownloadPanelState::Ready(info);
    }

    /// 设置为获取中状态
    pub fn set_fetching(&mut self) {
        self.state = DownloadPanelState::Fetching;
    }

    /// 设置错误状态
    pub fn set_error(&mut self, error: String) {
        self.state = DownloadPanelState::Error(error);
    }

    /// 重置为空闲状态
    pub fn reset(&mut self) {
        self.state = DownloadPanelState::Idle;
        self.config = DownloadConfig::default();
    }

    /// 切换高级选项显示
    pub fn toggle_advanced(&mut self) {
        self.show_advanced = !self.show_advanced;
    }

    /// 设置质量预设
    pub fn set_quality(&mut self, quality: QualityPreset) {
        self.config.quality = quality;
    }

    /// 切换选项
    pub fn toggle_option(&mut self, option: DownloadOption) {
        match option {
            DownloadOption::EmbedMetadata => {
                self.config.embed_metadata = !self.config.embed_metadata;
            }
            DownloadOption::EmbedThumbnail => {
                self.config.embed_thumbnail = !self.config.embed_thumbnail;
            }
            DownloadOption::DownloadSubtitles => {
                self.config.download_subtitles = !self.config.download_subtitles;
            }
            DownloadOption::EmbedSubtitles => {
                self.config.embed_subtitles = !self.config.embed_subtitles;
            }
            DownloadOption::AutoSubtitles => {
                self.config.auto_subtitles = !self.config.auto_subtitles;
            }
        }
    }

    /// 构建下载选项
    pub fn build_options(&self) -> DownloadOptions {
        DownloadOptions {
            format_id: self.config.quality.format_string().to_string(),
            output_path: self.config.output_path.clone(),
            embed_metadata: self.config.embed_metadata,
            embed_thumbnail: self.config.embed_thumbnail,
            subtitle_langs: if self.config.download_subtitles {
                self.config.subtitle_languages.clone()
            } else {
                vec![]
            },
            embed_subs: self.config.embed_subtitles,
            write_subs: self.config.download_subtitles,
            write_auto_subs: self.config.auto_subtitles,
            ..Default::default()
        }
    }
}

impl Default for DownloadPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for DownloadPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .child(
                // 面板标题
                div()
                    .h(px(48.0))
                    .px(px(16.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .child("📥 新建下载"),
                    ),
            )
            .child(
                // 主内容区域
                div()
                    .flex_1()
                    .overflow_hidden()
                    .p(px(16.0))
                    .child(self.render_content(cx)),
            )
    }
}

impl DownloadPanel {
    /// 渲染主内容
    fn render_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let _theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .gap(px(20.0))
            // URL 输入区域
            .child(self.render_url_input(cx))
            // 根据状态渲染不同内容
            .child(match &self.state {
                DownloadPanelState::Idle => self.render_idle_state(cx).into_any_element(),
                DownloadPanelState::Fetching => self.render_fetching_state(cx).into_any_element(),
                DownloadPanelState::Ready(info) => {
                    self.render_ready_state(info.clone(), cx).into_any_element()
                }
                DownloadPanelState::Error(err) => {
                    self.render_error_state(err, cx).into_any_element()
                }
            })
    }

    /// 渲染URL输入区域
    fn render_url_input(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let url = self.config.url.clone();

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.foreground)
                            .child("视频链接"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("支持 YouTube, Bilibili, Twitter 等 1000+ 网站"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .px(px(12.0))
                            .py(px(10.0))
                            .border_1()
                            .border_color(theme.border)
                            .rounded(px(8.0))
                            .bg(theme.input)
                            .text_color(if url.is_empty() {
                                theme.muted_foreground
                            } else {
                                theme.foreground
                            })
                            .child(if url.is_empty() {
                                "粘贴视频URL开始下载...".to_string()
                            } else {
                                url
                            }),
                    )
                    .child(Button::new("paste-url").icon(IconName::Plus).label("粘贴")),
            )
    }

    /// 渲染空闲状态
    fn render_idle_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(48.0))
            .gap(px(16.0))
            .child(div().text_3xl().child("🎬"))
            .child(
                div()
                    .text_center()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.foreground)
                            .child("准备下载"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .mt(px(4.0))
                            .child("粘贴视频链接，自动获取视频信息"),
                    ),
            )
    }

    /// 渲染获取中状态
    fn render_fetching_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(48.0))
            .gap(px(16.0))
            .child(
                Icon::new(IconName::LoaderCircle)
                    .size(px(32.0))
                    .text_color(theme.primary),
            )
            .child(
                div()
                    .text_center()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.foreground)
                            .child("正在获取视频信息..."),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .mt(px(4.0))
                            .child("请稍候，正在解析视频页面"),
                    ),
            )
    }

    /// 渲染就绪状态（已获取视频信息）
    fn render_ready_state(&self, info: VideoInfo, cx: &mut Context<Self>) -> impl IntoElement {
        let _theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .gap(px(20.0))
            // 视频预览卡片
            .child(self.render_video_preview(&info, cx))
            // 质量选择
            .child(self.render_quality_selector(cx))
            // 输出路径
            .child(self.render_output_path(cx))
            // 下载选项
            .child(self.render_download_options(cx))
            // 高级选项（可折叠）
            .when(self.show_advanced, |this| {
                this.child(self.render_advanced_options(&info, cx))
            })
            // 高级选项切换按钮
            .child(
                div().flex().justify_center().child(
                    Button::new("toggle-advanced")
                        .ghost()
                        .small()
                        .icon(if self.show_advanced {
                            IconName::ChevronUp
                        } else {
                            IconName::ChevronDown
                        })
                        .label(if self.show_advanced {
                            "收起高级选项"
                        } else {
                            "展开高级选项"
                        }),
                ),
            )
            // 下载按钮
            .child(
                div()
                    .mt(px(8.0))
                    .child(Button::new("start-download").primary().label("🚀 开始下载")),
            )
    }

    /// 渲染视频预览卡片
    fn render_video_preview(&self, info: &VideoInfo, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .p(px(12.0))
            .border_1()
            .border_color(theme.border)
            .rounded(px(12.0))
            .bg(theme.secondary)
            .flex()
            .gap(px(12.0))
            // 缩略图
            .child(
                div()
                    .w(px(160.0))
                    .h(px(90.0))
                    .rounded(px(8.0))
                    .bg(theme.muted)
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .child(div().text_3xl().child("🎬")),
            )
            // 视频信息
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .overflow_hidden()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .overflow_hidden()
                            .child(info.title.clone()),
                    )
                    .child(
                        div().text_xs().text_color(theme.muted_foreground).child(
                            info.uploader
                                .clone()
                                .unwrap_or_else(|| "未知上传者".to_string()),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .mt(px(4.0))
                            .when_some(info.duration.as_ref(), |this, dur| {
                                this.child(self.render_info_badge(&format_duration(dur), cx))
                            })
                            .child(
                                self.render_info_badge(
                                    &format!("{} 种格式", info.formats.len()),
                                    cx,
                                ),
                            ),
                    ),
            )
    }

    /// 渲染信息标签
    fn render_info_badge(&self, text: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(4.0))
            .bg(theme.muted)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(text.to_string())
    }

    /// 渲染质量选择器
    fn render_quality_selector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let current_quality = self.config.quality;

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.foreground)
                    .child("视频质量"),
            )
            .child(div().flex().flex_wrap().gap(px(8.0)).children(
                QualityPreset::all().into_iter().map(|quality| {
                    let is_selected = quality == current_quality;
                    let primary = theme.primary;
                    let border = theme.border;
                    let accent = theme.accent;
                    let background = theme.background;
                    let foreground = theme.foreground;
                    let muted_foreground = theme.muted_foreground;

                    div()
                        .id(ElementId::Name(format!("quality-{:?}", quality).into()))
                        .px(px(12.0))
                        .py(px(8.0))
                        .border_1()
                        .border_color(if is_selected { primary } else { border })
                        .rounded(px(8.0))
                        .bg(if is_selected { accent } else { background })
                        .cursor_pointer()
                        .child(
                            div()
                                .text_sm()
                                .text_color(if is_selected {
                                    foreground
                                } else {
                                    muted_foreground
                                })
                                .child(quality.label()),
                        )
                }),
            ))
    }

    /// 渲染输出路径
    fn render_output_path(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let path = self.config.output_path.to_string_lossy().to_string();

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.foreground)
                    .child("保存位置"),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .px(px(12.0))
                            .py(px(10.0))
                            .border_1()
                            .border_color(theme.border)
                            .rounded(px(8.0))
                            .bg(theme.input)
                            .text_color(theme.foreground)
                            .text_sm()
                            .overflow_hidden()
                            .child(path),
                    )
                    .child(
                        Button::new("browse-path")
                            .icon(IconName::Folder)
                            .label("浏览"),
                    ),
            )
    }

    /// 渲染下载选项
    fn render_download_options(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.foreground)
                    .child("下载选项"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(12.0))
                    .child(self.render_option_checkbox(
                        "嵌入元数据",
                        self.config.embed_metadata,
                        cx,
                    ))
                    .child(self.render_option_checkbox(
                        "嵌入缩略图",
                        self.config.embed_thumbnail,
                        cx,
                    ))
                    .child(self.render_option_checkbox(
                        "下载字幕",
                        self.config.download_subtitles,
                        cx,
                    )),
            )
    }

    /// 渲染选项复选框
    fn render_option_checkbox(
        &self,
        label: &str,
        checked: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let primary = theme.primary;
        let border = theme.border;
        let background = theme.background;
        let primary_foreground = theme.primary_foreground;
        let foreground = theme.foreground;

        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .child(
                div()
                    .w(px(18.0))
                    .h(px(18.0))
                    .border_1()
                    .border_color(if checked { primary } else { border })
                    .rounded(px(4.0))
                    .bg(if checked { primary } else { background })
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(checked, |this| {
                        this.child(
                            Icon::new(IconName::Check)
                                .size(px(12.0))
                                .text_color(primary_foreground),
                        )
                    }),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(foreground)
                    .child(label.to_string()),
            )
    }

    /// 渲染高级选项
    fn render_advanced_options(
        &self,
        info: &VideoInfo,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let border = theme.border;
        let secondary = theme.secondary;
        let foreground = theme.foreground;
        let background = theme.background;
        let muted = theme.muted;
        let muted_foreground = theme.muted_foreground;
        let primary = theme.primary;
        let primary_foreground = theme.primary_foreground;

        let embed_subtitles = self.config.embed_subtitles;
        let auto_subtitles = self.config.auto_subtitles;
        let formats_count = info.formats.len();
        let formats = info.formats.clone();

        div()
            .p(px(12.0))
            .border_1()
            .border_color(border)
            .rounded(px(8.0))
            .bg(secondary)
            .flex()
            .flex_col()
            .gap(px(12.0))
            // 字幕选项
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(foreground)
                            .child("字幕设置"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(12.0))
                            .child(render_checkbox_static(
                                "嵌入字幕到视频",
                                embed_subtitles,
                                primary,
                                border,
                                background,
                                primary_foreground,
                                foreground,
                            ))
                            .child(render_checkbox_static(
                                "自动生成字幕",
                                auto_subtitles,
                                primary,
                                border,
                                background,
                                primary_foreground,
                                foreground,
                            )),
                    ),
            )
            // 可用格式列表
            .when(!formats.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(foreground)
                                .child(format!("可用格式 ({})", formats_count)),
                        )
                        .child(render_formats_static(
                            &formats,
                            border,
                            background,
                            foreground,
                            muted_foreground,
                            muted,
                        )),
                )
            })
    }

    /// 渲染错误状态
    fn render_error_state(&self, error: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .p(px(16.0))
            .border_1()
            .border_color(theme.danger)
            .rounded(px(8.0))
            .bg(theme.danger.opacity(0.1))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Icon::new(IconName::TriangleAlert)
                            .size(px(20.0))
                            .text_color(theme.danger),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.danger)
                            .child("获取视频信息失败"),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(theme.foreground)
                    .child(error.to_string()),
            )
            .child(Button::new("retry-fetch").ghost().small().label("重试"))
    }
}

/// 渲染静态复选框
fn render_checkbox_static(
    label: &str,
    checked: bool,
    primary: Hsla,
    border: Hsla,
    background: Hsla,
    primary_foreground: Hsla,
    foreground: Hsla,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .cursor_pointer()
        .child(
            div()
                .w(px(18.0))
                .h(px(18.0))
                .border_1()
                .border_color(if checked { primary } else { border })
                .rounded(px(4.0))
                .bg(if checked { primary } else { background })
                .flex()
                .items_center()
                .justify_center()
                .when(checked, |this| {
                    this.child(
                        Icon::new(IconName::Check)
                            .size(px(12.0))
                            .text_color(primary_foreground),
                    )
                }),
        )
        .child(
            div()
                .text_sm()
                .text_color(foreground)
                .child(label.to_string()),
        )
}

/// 渲染静态格式列表
fn render_formats_static(
    formats: &[VideoFormat],
    border: Hsla,
    background: Hsla,
    foreground: Hsla,
    muted_foreground: Hsla,
    muted: Hsla,
) -> impl IntoElement {
    div()
        .max_h(px(200.0))
        .overflow_hidden()
        .border_1()
        .border_color(border)
        .rounded(px(6.0))
        .bg(background)
        .children(formats.iter().take(10).map(move |format| {
            div()
                .px(px(12.0))
                .py(px(8.0))
                .border_b_1()
                .border_color(border)
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_sm()
                                .text_color(foreground)
                                .child(format.format_id.clone()),
                        )
                        .when_some(format.resolution.clone(), |this, res| {
                            this.child(
                                div()
                                    .px(px(6.0))
                                    .py(px(1.0))
                                    .rounded(px(4.0))
                                    .bg(muted)
                                    .text_xs()
                                    .text_color(muted_foreground)
                                    .child(res),
                            )
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted_foreground)
                        .child(format.ext.clone()),
                )
        }))
}

/// 格式化 Duration 为时长字符串
fn format_duration(duration: &Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}
