//! 首页主组件

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::input::InputState;
use crate::app::AppState;
use std::sync::Arc;

use super::widgets::*;

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
    #[allow(dead_code)]
    app_state: Arc<AppState>,
    url_input: Entity<InputState>,
    download_state: DownloadState,
    selected_quality: QualityOption,
    output_path: String,
    options: DownloadOptionsData,
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
        }
    }
    
    fn get_url(&self, cx: &Context<Self>) -> String {
        self.url_input.read(cx).value().to_string()
    }
    
    fn on_paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.url_input.update(cx, |state, cx| {
            state.set_value("https://www.youtube.com/watch?v=dQw4w9WgXcQ", window, cx);
        });
        cx.notify();
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
        
        // TODO: 实现实际的解析逻辑
        self.download_state = DownloadState::Ready(VideoInfo {
            title: "示例视频标题 - 这是一个测试视频".to_string(),
            duration: "10:30".to_string(),
            thumbnail: None,
            uploader: Some("示例上传者".to_string()),
        });
        cx.notify();
    }
    
    fn start_download(&mut self, cx: &mut Context<Self>) {
        self.download_state = DownloadState::Downloading { 
            progress: 0.0, 
            speed: "准备中...".to_string() 
        };
        cx.notify();
        
        // TODO: 实现实际的下载逻辑
        self.download_state = DownloadState::Completed(format!("{}/video.mp4", self.output_path));
        cx.notify();
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
                            .on_paste(cx.listener(|this, _ev, window, cx| {
                                this.on_paste(window, cx);
                            }))
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
                    .on_download(cx.listener(|this, _ev, _window, cx| {
                        this.start_download(cx);
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
