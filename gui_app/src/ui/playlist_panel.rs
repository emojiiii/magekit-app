// gui_app/src/ui/playlist_panel.rs
//! 播放列表下载组件 - 支持播放列表视频选择和批量下载

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use std::sync::Arc;
use std::collections::HashSet;

/// 播放列表项
#[derive(Debug, Clone)]
pub struct PlaylistItem {
    /// 视频ID
    pub id: String,
    /// 视频标题
    pub title: String,
    /// 视频时长（秒）
    pub duration: Option<u64>,
    /// 索引（在播放列表中的位置）
    pub index: usize,
    /// 缩略图URL
    pub thumbnail: Option<String>,
    /// 上传者
    pub uploader: Option<String>,
    /// 视频URL
    pub url: String,
}

/// 播放列表信息
#[derive(Debug, Clone)]
pub struct PlaylistInfo {
    /// 播放列表ID
    pub id: String,
    /// 播放列表标题
    pub title: String,
    /// 描述
    pub description: Option<String>,
    /// 上传者/频道
    pub uploader: Option<String>,
    /// 视频总数
    pub video_count: usize,
    /// 缩略图
    pub thumbnail: Option<String>,
    /// 视频列表
    pub entries: Vec<PlaylistItem>,
}

/// 播放列表面板状态
#[derive(Debug, Clone, PartialEq)]
pub enum PlaylistPanelState {
    /// 空闲
    Idle,
    /// 加载中
    Loading,
    /// 已就绪
    Ready,
    /// 错误
    Error(String),
}

/// 选择模式
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionMode {
    /// 全选
    All,
    /// 自定义选择
    Custom,
    /// 范围选择
    Range { start: usize, end: usize },
}

/// 播放列表面板事件
#[derive(Debug, Clone)]
pub enum PlaylistPanelEvent {
    /// 开始下载选中视频
    StartDownload(Vec<PlaylistItem>),
    /// 取消
    Cancel,
    /// 选择变更
    SelectionChanged(Vec<String>),
}

/// 事件回调类型
pub type PlaylistPanelCallback = Arc<dyn Fn(PlaylistPanelEvent) + Send + Sync>;

/// 播放列表面板
pub struct PlaylistPanel {
    /// 当前状态
    state: PlaylistPanelState,
    /// 播放列表信息
    playlist: Option<PlaylistInfo>,
    /// 选中的视频ID
    selected_ids: HashSet<String>,
    /// 选择模式
    selection_mode: SelectionMode,
    /// 搜索过滤
    search_query: String,
    /// 事件回调
    callback: Option<PlaylistPanelCallback>,
}

impl PlaylistPanel {
    /// 创建新的播放列表面板
    pub fn new() -> Self {
        Self {
            state: PlaylistPanelState::Idle,
            playlist: None,
            selected_ids: HashSet::new(),
            selection_mode: SelectionMode::All,
            search_query: String::new(),
            callback: None,
        }
    }

    /// 设置事件回调
    pub fn on_event(mut self, callback: PlaylistPanelCallback) -> Self {
        self.callback = Some(callback);
        self
    }

    /// 设置播放列表信息
    pub fn set_playlist(&mut self, playlist: PlaylistInfo) {
        // 默认全选
        self.selected_ids = playlist.entries.iter().map(|e| e.id.clone()).collect();
        self.playlist = Some(playlist);
        self.state = PlaylistPanelState::Ready;
    }

    /// 设置加载状态
    pub fn set_loading(&mut self) {
        self.state = PlaylistPanelState::Loading;
    }

    /// 设置错误
    pub fn set_error(&mut self, error: String) {
        self.state = PlaylistPanelState::Error(error);
    }

    /// 切换视频选择
    pub fn toggle_item(&mut self, id: &str) {
        if self.selected_ids.contains(id) {
            self.selected_ids.remove(id);
        } else {
            self.selected_ids.insert(id.to_string());
        }
        self.selection_mode = SelectionMode::Custom;
    }

    /// 全选
    pub fn select_all(&mut self) {
        if let Some(playlist) = &self.playlist {
            self.selected_ids = playlist.entries.iter().map(|e| e.id.clone()).collect();
        }
        self.selection_mode = SelectionMode::All;
    }

    /// 取消全选
    pub fn deselect_all(&mut self) {
        self.selected_ids.clear();
        self.selection_mode = SelectionMode::Custom;
    }

    /// 反选
    pub fn invert_selection(&mut self) {
        if let Some(playlist) = &self.playlist {
            let all_ids: HashSet<String> = playlist.entries.iter().map(|e| e.id.clone()).collect();
            self.selected_ids = all_ids.difference(&self.selected_ids).cloned().collect();
        }
        self.selection_mode = SelectionMode::Custom;
    }

    /// 设置搜索过滤
    pub fn set_search(&mut self, query: String) {
        self.search_query = query;
    }

    /// 获取过滤后的项目
    fn filtered_entries(&self) -> Vec<&PlaylistItem> {
        if let Some(playlist) = &self.playlist {
            if self.search_query.is_empty() {
                playlist.entries.iter().collect()
            } else {
                let query = self.search_query.to_lowercase();
                playlist.entries
                    .iter()
                    .filter(|e| e.title.to_lowercase().contains(&query))
                    .collect()
            }
        } else {
            Vec::new()
        }
    }

    /// 获取选中的视频
    pub fn get_selected_items(&self) -> Vec<PlaylistItem> {
        if let Some(playlist) = &self.playlist {
            playlist.entries
                .iter()
                .filter(|e| self.selected_ids.contains(&e.id))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }
}

impl Render for PlaylistPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg = theme.background;
        let border = theme.border;

        v_flex()
            .w_full()
            .gap_4()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(border)
            .bg(bg)
            .child(self.render_content(cx))
    }
}

impl PlaylistPanel {
    /// 渲染主内容
    fn render_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.state {
            PlaylistPanelState::Idle => self.render_idle(cx).into_any_element(),
            PlaylistPanelState::Loading => self.render_loading(cx).into_any_element(),
            PlaylistPanelState::Ready => self.render_ready(cx).into_any_element(),
            PlaylistPanelState::Error(err) => self.render_error(err.clone(), cx).into_any_element(),
        }
    }

    /// 渲染空闲状态
    fn render_idle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;

        v_flex()
            .items_center()
            .justify_center()
            .py_8()
            .gap_2()
            .child(
                Icon::new(IconName::Folder)
                    .size(px(32.0))
                    .text_color(muted)
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("输入播放列表URL开始下载")
            )
    }

    /// 渲染加载状态
    fn render_loading(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;

        v_flex()
            .items_center()
            .justify_center()
            .py_8()
            .gap_2()
            .child(
                Icon::new(IconName::LoaderCircle)
                    .size(px(32.0))
                    .text_color(muted)
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("正在加载播放列表信息...")
            )
    }

    /// 渲染就绪状态
    fn render_ready(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let fg = theme.foreground;
        let muted = theme.muted_foreground;

        let playlist = self.playlist.as_ref().unwrap();
        let selected_count = self.selected_ids.len();
        let total_count = playlist.entries.len();

        v_flex()
            .gap_4()
            // 播放列表信息头
            .child(self.render_playlist_header(cx))
            // 工具栏
            .child(self.render_toolbar(cx))
            // 视频列表
            .child(self.render_video_list(cx))
            // 底部操作栏
            .child(self.render_footer(cx))
    }

    /// 渲染播放列表头部
    fn render_playlist_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let border = theme.border;
        let bg = theme.muted;

        let playlist = self.playlist.as_ref().unwrap();

        h_flex()
            .gap_4()
            .p_3()
            .rounded_lg()
            .bg(bg)
            // 缩略图
            .when(playlist.thumbnail.is_some(), |this| {
                this.child(
                    div()
                        .w(px(120.0))
                        .h(px(68.0))
                        .rounded_md()
                        .bg(border)
                        .overflow_hidden()
                )
            })
            // 信息
            .child(
                v_flex()
                    .flex_1()
                    .gap_1()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .child(playlist.title.clone())
                    )
                    .when(playlist.uploader.is_some(), |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .child(playlist.uploader.clone().unwrap())
                        )
                    })
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(format!("{} 个视频", playlist.video_count))
                    )
            )
    }

    /// 渲染工具栏
    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let border = theme.border;

        let selected_count = self.selected_ids.len();
        let total_count = self.playlist.as_ref().map(|p| p.entries.len()).unwrap_or(0);
        let is_all_selected = selected_count == total_count;

        h_flex()
            .items_center()
            .justify_between()
            .gap_4()
            // 左侧：选择操作
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("select-all")
                            .ghost()
                            .small()
                            .label(SharedString::from(if is_all_selected { "取消全选" } else { "全选" }))
                    )
                    .child(
                        Button::new("invert-selection")
                            .ghost()
                            .small()
                            .label("反选")
                    )
            )
            // 中间：搜索框（简化版）
            .child(
                h_flex()
                    .flex_1()
                    .max_w(px(300.0))
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(border)
                    .items_center()
                    .gap_2()
                    .child(
                        Icon::new(IconName::Search)
                            .size(px(16.0))
                            .text_color(muted)
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("搜索视频...")
                    )
            )
            // 右侧：选中计数
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(format!("已选 {}/{}", selected_count, total_count))
            )
    }

    /// 渲染视频列表
    fn render_video_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let border = theme.border;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let primary = theme.primary;
        let accent = theme.accent;
        
        let entries = self.filtered_entries();
        let selected_ids = self.selected_ids.clone();

        v_flex()
            .max_h(px(400.0))
            .overflow_hidden()
            .border_1()
            .border_color(border)
            .rounded_md()
            .children(entries.into_iter().enumerate().map(|(idx, item)| {
                let is_selected = selected_ids.contains(&item.id);
                render_playlist_item_inline(item, is_selected, idx == 0, fg, muted, border, primary, accent)
            }))
    }

    /// 渲染底部操作栏
    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;

        let selected_count = self.selected_ids.len();
        let has_selection = selected_count > 0;

        // 计算预估时长
        let total_duration: u64 = if let Some(playlist) = &self.playlist {
            playlist.entries
                .iter()
                .filter(|e| self.selected_ids.contains(&e.id))
                .filter_map(|e| e.duration)
                .sum()
        } else {
            0
        };

        h_flex()
            .items_center()
            .justify_between()
            .pt_4()
            .border_t_1()
            .border_color(theme.border)
            // 左侧：统计信息
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(format!("总时长: {}", format_duration_seconds(total_duration)))
                    )
            )
            // 右侧：操作按钮
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("cancel-playlist")
                            .ghost()
                            .label("取消")
                    )
                    .child(
                        Button::new("download-playlist")
                            .primary()
                            .label(SharedString::from(format!("下载 {} 个视频", selected_count)))
                            .disabled(!has_selection)
                    )
            )
    }

    /// 渲染错误状态
    fn render_error(&self, error: String, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let danger = theme.danger;
        let muted = theme.muted_foreground;

        v_flex()
            .items_center()
            .justify_center()
            .py_8()
            .gap_3()
            .child(
                Icon::new(IconName::TriangleAlert)
                    .size(px(32.0))
                    .text_color(danger)
            )
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(danger)
                    .child("加载失败")
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .max_w(px(300.0))
                    .text_center()
                    .child(error)
            )
            .child(
                Button::new("retry-playlist")
                    .ghost()
                    .small()
                    .label("重试")
            )
    }
}

/// 渲染单个播放列表项（内联版本，不需要 cx）
fn render_playlist_item_inline(
    item: &PlaylistItem,
    is_selected: bool,
    is_first: bool,
    fg: Hsla,
    muted: Hsla,
    border: Hsla,
    primary: Hsla,
    accent: Hsla,
) -> impl IntoElement {
    let bg = if is_selected { accent.opacity(0.1) } else { Hsla::transparent_black() };

    let id = item.id.clone();
    let title = item.title.clone();
    let duration = item.duration.map(format_duration_seconds).unwrap_or_else(|| "未知".to_string());
    let index = item.index;
    let uploader = item.uploader.clone();

    h_flex()
        .w_full()
        .px_3()
        .py_2()
        .gap_3()
        .items_center()
        .bg(bg)
        .cursor_pointer()
        .when(!is_first, |this| {
            this.border_t_1().border_color(border)
        })
        // 选择框
        .child(
            Checkbox::new(SharedString::from(format!("playlist-item-{}", id)))
                .checked(is_selected)
        )
        // 索引
        .child(
            div()
                .w(px(30.0))
                .text_sm()
                .text_color(muted)
                .text_center()
                .child(format!("{}", index + 1))
        )
        // 标题
        .child(
            v_flex()
                .flex_1()
                .gap_px()
                .overflow_hidden()
                .child(
                    div()
                        .text_sm()
                        .text_color(fg)
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(title)
                )
                .when(uploader.is_some(), |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(uploader.unwrap())
                    )
                })
        )
        // 时长
        .child(
            div()
                .w(px(60.0))
                .text_sm()
                .text_color(muted)
                .text_right()
                .child(duration)
        )
}

/// 格式化时长（秒）
fn format_duration_seconds(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{}:{:02}", minutes, secs)
    }
}
