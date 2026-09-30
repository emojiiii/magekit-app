//! 频道/作者页面主组件

use crate::app::AppState;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_kit::component::WindowExt;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::empty::{Empty, EmptyDescription, EmptyHeader, EmptyTitle};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarAxis};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, IconName, Sizable, VirtualListScrollHandle, v_virtual_list,
};
use gpui_router::{use_location, use_navigate};
use magekit_shared::{ChannelInfo, ChannelTab, ChannelTabType, ChannelVideoEntry};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone)]
struct TabPagingState {
    url: String,
    next_cursor: Option<i64>,
    has_more: bool,
    total: Option<usize>,
    initialized: bool,
    loading: bool,
}

#[derive(Clone, Copy)]
struct PageRequest {
    channel: u64,
    view: u64,
    tab: Option<usize>,
}
impl PageRequest {
    fn belongs_to(self, channel: u64) -> bool {
        self.channel == channel
    }
    fn may_navigate(self, channel: u64, view: u64, tab: Option<usize>) -> bool {
        self.belongs_to(channel) && self.view == view && self.tab == tab
    }
}

fn page_count(loaded: usize, total: Option<usize>, has_more: bool) -> usize {
    let loaded_pages = loaded.div_ceil(ITEMS_PER_PAGE);
    if !has_more {
        return loaded_pages;
    }
    total
        .filter(|total| *total > loaded)
        .map(|total| total.div_ceil(ITEMS_PER_PAGE))
        .unwrap_or(loaded_pages + 1)
}

/// 格式化时长
fn format_duration(seconds: Option<u64>) -> String {
    match seconds {
        Some(secs) => {
            let hours = secs / 3600;
            let minutes = (secs % 3600) / 60;
            let seconds = secs % 60;

            if hours > 0 {
                format!("{}:{:02}:{:02}", hours, minutes, seconds)
            } else {
                format!("{}:{:02}", minutes, seconds)
            }
        }
        None => crate::i18n::tr("未知").to_string(),
    }
}

/// 页面状态
#[derive(Debug, Clone, PartialEq)]
pub enum ChannelState {
    /// 空闲状态
    Idle,
    /// 正在解析
    Parsing,
    /// 解析完成
    Ready(ChannelInfo),
    /// 错误状态
    Error(String),
}

impl Default for ChannelState {
    fn default() -> Self {
        Self::Idle
    }
}

/// 每页显示的视频数量
const ITEMS_PER_PAGE: usize = 20;

/// 频道/作者页面
pub struct ChannelPage {
    app_state: Arc<AppState>,
    url_input: Entity<InputState>,
    state: ChannelState,
    /// 当前解析/分页对应的 URL（用于“下一页”增量加载）
    active_url: Option<String>,
    /// 下一页游标（抖音使用 `max_cursor`）
    next_cursor: Option<i64>,
    /// 是否还有更多页（抖音用户主页）
    has_more_remote: bool,
    /// 是否正在加载下一页
    loading_more: bool,
    /// 是否隐藏“全部”Tab（用于 YouTube 等“分类 Tab”场景）
    hide_all_tab: bool,
    /// 当前频道的 Tab 分页状态（与 `ChannelInfo.tabs` 对齐；仅在支持时填充）
    tab_paging: Vec<TabPagingState>,
    /// 当前选中的 Tab 索引（None 表示"全部"）
    current_tab_index: Option<usize>,
    /// 当前页码（从 0 开始）
    current_page: usize,
    /// VirtualList 滚动句柄
    scroll_handle: VirtualListScrollHandle,
    /// 预计算的 item sizes
    item_sizes: Rc<Vec<Size<Pixels>>>,
    request_generation: u64,
    view_generation: u64,
    page_error: Option<String>,
    submitting: bool,
    batch_progress: usize,
    batch_total: usize,
    batch_cancel: Arc<AtomicBool>,
}

/// 视频项的固定高度
const VIDEO_ITEM_HEIGHT: f32 = 70.0;

fn is_probably_youtube_channel_url(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    let host = url.host_str().unwrap_or_default();
    if !(host == "youtube.com" || host.ends_with(".youtube.com"))
        || url.query_pairs().any(|(name, _)| name == "list")
    {
        return false;
    }
    let path = url.path();
    path.starts_with("/@")
        || path.starts_with("/channel/")
        || path.starts_with("/user/")
        || path.starts_with("/c/")
}

fn youtube_channel_base_url(url: &str) -> String {
    // 仅用于生成 tab URL：移除 query/fragment，并去掉 /videos /shorts /streams /playlists /featured /home 后缀。
    let mut base = url
        .split(|c| c == '?' || c == '#')
        .next()
        .unwrap_or(url)
        .trim_end_matches('/')
        .to_string();

    let suffixes = [
        "/videos",
        "/shorts",
        "/streams",
        "/playlists",
        "/featured",
        "/home",
    ];
    loop {
        let lower = base.to_lowercase();
        let mut changed = false;
        for s in suffixes {
            if lower.ends_with(s) {
                base = base[..base.len() - s.len()]
                    .trim_end_matches('/')
                    .to_string();
                changed = true;
                break;
            }
        }
        if !changed {
            break;
        }
    }
    base
}

fn youtube_tab_url(base_url: &str, tab: &str) -> String {
    format!("{}/{}", youtube_channel_base_url(base_url), tab)
}

impl ChannelPage {
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url_input = cx.new(|cx| {
            crate::i18n::input(
                "粘贴 YouTube 频道、播放列表或 Bilibili UP主空间链接...",
                window,
                cx,
            )
            .clean_on_escape()
        });

        cx.subscribe(&url_input, |this, _, event, cx| match event {
            InputEvent::PressEnter { .. } => this.on_parse(cx),
            InputEvent::Change if !this.submitting => this.invalidate_parse(cx),
            _ => {}
        })
        .detach();

        Self {
            app_state,
            url_input,
            state: ChannelState::Idle,
            active_url: None,
            next_cursor: None,
            has_more_remote: false,
            loading_more: false,
            hide_all_tab: false,
            tab_paging: Vec::new(),
            current_tab_index: None,
            current_page: 0,
            scroll_handle: VirtualListScrollHandle::new(),
            item_sizes: Rc::new(Vec::new()),
            request_generation: 0,
            view_generation: 0,
            page_error: None,
            submitting: false,
            batch_progress: 0,
            batch_total: 0,
            batch_cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 获取当前 Tab 下当前页的视频列表
    fn get_current_page_entries(&self) -> Vec<(usize, ChannelVideoEntry)> {
        if let ChannelState::Ready(ref info) = self.state {
            let all_entries: &[ChannelVideoEntry] = match self.current_tab_index {
                None => info.entries.as_slice(),
                Some(tab_index) => info
                    .tabs
                    .get(tab_index)
                    .map(|t| t.entries.as_slice())
                    .unwrap_or(&[]),
            };

            // 分页
            let start = self.current_page * ITEMS_PER_PAGE;
            if start >= all_entries.len() {
                return Vec::new();
            }
            let end = (start + ITEMS_PER_PAGE).min(all_entries.len());

            all_entries[start..end]
                .iter()
                .cloned()
                .enumerate()
                .map(|(idx, entry)| (start + idx, entry))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// 获取当前 Tab 的总视频数
    fn get_current_tab_total(&self) -> usize {
        if let ChannelState::Ready(ref info) = self.state {
            match self.current_tab_index {
                None => info.video_count.max(info.entries.len()),
                Some(tab_index) => {
                    let loaded = info
                        .tabs
                        .get(tab_index)
                        .map(|t| t.entries.len())
                        .unwrap_or(0);
                    let total = self
                        .tab_paging
                        .get(tab_index)
                        .and_then(|p| p.total)
                        .unwrap_or(loaded);
                    total.max(loaded)
                }
            }
        } else {
            0
        }
    }

    /// 获取总页数
    fn get_total_pages(&self) -> usize {
        if let ChannelState::Ready(ref info) = self.state {
            let loaded = match self.current_tab_index {
                None => info.entries.len(),
                Some(tab_index) => info
                    .tabs
                    .get(tab_index)
                    .map(|t| t.entries.len())
                    .unwrap_or(0),
            };

            let total_opt = match self.current_tab_index {
                None => {
                    let total = info.video_count.max(loaded);
                    if total > loaded { Some(total) } else { None }
                }
                Some(tab_index) => {
                    let total = self.tab_paging.get(tab_index).and_then(|p| p.total);
                    total.filter(|t| *t > loaded)
                }
            };

            let has_more = match self.current_tab_index {
                None => self.has_more_remote,
                Some(tab_index) => self
                    .tab_paging
                    .get(tab_index)
                    .map(|p| p.initialized && p.has_more)
                    .unwrap_or(false),
            };

            page_count(loaded, total_opt, has_more)
        } else {
            0
        }
    }

    /// 更新 item sizes（用于 VirtualList）
    fn update_item_sizes(&mut self) {
        let count = self.get_current_page_entries().len();
        let sizes: Vec<Size<Pixels>> = (0..count)
            .map(|_| size(px(800.0), px(VIDEO_ITEM_HEIGHT)))
            .collect();
        self.item_sizes = Rc::new(sizes);
    }

    /// 切换 Tab
    fn switch_tab(&mut self, tab_index: Option<usize>, cx: &mut Context<Self>) {
        if self.hide_all_tab && tab_index.is_none() {
            return;
        }
        self.view_generation = self.view_generation.wrapping_add(1);
        self.page_error = None;
        self.current_tab_index = tab_index;
        self.current_page = 0;
        self.scroll_handle = VirtualListScrollHandle::new();
        self.update_item_sizes();
        cx.notify();

        if let Some(tab_index) = tab_index {
            if let Some(paging) = self.tab_paging.get(tab_index) {
                if !paging.initialized && !paging.loading {
                    self.load_more_tab_then_switch(tab_index, 0, cx);
                }
            }
        }
    }

    /// 切换页码
    fn switch_page(&mut self, page: usize, cx: &mut Context<Self>) {
        let total_pages = self.get_total_pages();
        if page < total_pages {
            self.view_generation = self.view_generation.wrapping_add(1);
            self.page_error = None;
            self.current_page = page;
            self.scroll_handle = VirtualListScrollHandle::new();
            self.update_item_sizes();
            cx.notify();
        }
    }

    /// 下一页：若需要且支持远端分页，则先加载下一页数据再切页。
    fn go_next_page(&mut self, cx: &mut Context<Self>) {
        let is_loading = match self.current_tab_index {
            Some(tab_index) => self
                .tab_paging
                .get(tab_index)
                .map(|p| p.loading)
                .unwrap_or(false),
            None => self.loading_more,
        };
        if is_loading {
            return;
        }

        let total_pages = self.get_total_pages();
        if total_pages == 0 || self.current_page + 1 >= total_pages {
            return;
        }

        let target_page = self.current_page + 1;
        let need_remote = match self.state {
            ChannelState::Ready(ref info) => match self.current_tab_index {
                None => {
                    self.has_more_remote && info.entries.len() < (target_page + 1) * ITEMS_PER_PAGE
                }
                Some(tab_index) => self
                    .tab_paging
                    .get(tab_index)
                    .map(|p| {
                        p.has_more
                            && info
                                .tabs
                                .get(tab_index)
                                .map(|t| t.entries.len())
                                .unwrap_or(0)
                                < (target_page + 1) * ITEMS_PER_PAGE
                    })
                    .unwrap_or(false),
            },
            _ => false,
        };

        if need_remote {
            if let Some(tab_index) = self.current_tab_index {
                self.load_more_tab_then_switch(tab_index, target_page, cx);
            } else {
                self.load_more_then_switch(target_page, cx);
            }
        } else {
            self.switch_page(target_page, cx);
        }
    }

    fn load_more_tab_then_switch(
        &mut self,
        tab_index: usize,
        target_page: usize,
        cx: &mut Context<Self>,
    ) {
        let paging = match self.tab_paging.get_mut(tab_index) {
            Some(p) => p,
            None => return,
        };
        if paging.loading {
            return;
        }
        let url = paging.url.clone();
        let cursor = paging.next_cursor;
        paging.loading = true;
        self.page_error = None;
        let request = PageRequest {
            channel: self.request_generation,
            view: self.view_generation,
            tab: self.current_tab_index,
        };
        cx.notify();

        let app_state = self.app_state.clone();
        cx.spawn(async move |this, cx| {
            let url_for_log = url.clone();
            let result = smol::unblock(move || {
                let runtime = app_state.runtime.clone();
                let config = app_state.config();
                let cookies = config.advanced.cookies.clone();
                runtime.block_on(async {
                    let cookies_opt = if cookies.is_empty() {
                        None
                    } else {
                        Some(cookies.as_slice())
                    };
                    app_state
                        .tool_manager
                        .get_channel_videos_page(&url, cursor, ITEMS_PER_PAGE, cookies_opt)
                        .await
                })
            })
            .await;

            let _ = this.update(cx, |this, cx| {
                if !request.belongs_to(this.request_generation) {
                    return;
                }
                if let Some(p) = this.tab_paging.get_mut(tab_index) {
                    p.loading = false;
                }

                match result {
                    Ok(page) => {
                        if let ChannelState::Ready(ref mut info) = this.state {
                            if let Some(tab) = info.tabs.get_mut(tab_index) {
                                let mut existing: std::collections::HashSet<String> =
                                    tab.entries.iter().map(|e| e.id.clone()).collect();
                                for mut entry in page.info.entries {
                                    entry.selected = false;
                                    if existing.insert(entry.id.clone()) {
                                        tab.entries.push(entry);
                                    }
                                }
                            }
                        }

                        if let Some(p) = this.tab_paging.get_mut(tab_index) {
                            p.initialized = true;
                            p.next_cursor = page.next_cursor;
                            p.has_more = page.has_more;
                            p.total = Some(page.info.video_count);
                        }

                        if request.may_navigate(
                            this.request_generation,
                            this.view_generation,
                            this.current_tab_index,
                        ) {
                            this.current_page =
                                target_page.min(this.get_total_pages().saturating_sub(1));
                            this.scroll_handle = VirtualListScrollHandle::new();
                        }
                        this.update_item_sizes();
                    }
                    Err(e) => {
                        if request.may_navigate(
                            this.request_generation,
                            this.view_generation,
                            this.current_tab_index,
                        ) {
                            this.page_error = Some(e.to_string());
                        }
                        tracing::error!(
                            "❌ 加载 Tab 下一页失败: url={} err={}",
                            url_for_log,
                            e.to_string()
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_more_then_switch(&mut self, target_page: usize, cx: &mut Context<Self>) {
        if self.loading_more {
            return;
        }
        let url = match self.active_url.clone() {
            Some(u) => u,
            None => return,
        };

        let cursor = match self.next_cursor {
            Some(c) => Some(c),
            None => {
                // 没有游标，无法继续加载
                self.has_more_remote = false;
                return;
            }
        };

        self.loading_more = true;
        self.page_error = None;
        let request = PageRequest {
            channel: self.request_generation,
            view: self.view_generation,
            tab: self.current_tab_index,
        };
        cx.notify();

        let app_state = self.app_state.clone();
        cx.spawn(async move |this, cx| {
            let url_for_log = url.clone();
            let result = smol::unblock(move || {
                let runtime = app_state.runtime.clone();
                let config = app_state.config();
                let cookies = config.advanced.cookies.clone();
                runtime.block_on(async {
                    let cookies_opt = if cookies.is_empty() {
                        None
                    } else {
                        Some(cookies.as_slice())
                    };
                    app_state
                        .tool_manager
                        .get_channel_videos_page(&url, cursor, ITEMS_PER_PAGE, cookies_opt)
                        .await
                })
            })
            .await;

            let _ = this.update(cx, |this, cx| {
                if !request.belongs_to(this.request_generation) {
                    return;
                }
                this.loading_more = false;
                match result {
                    Ok(page) => {
                        if let ChannelState::Ready(ref mut info) = this.state {
                            let mut seen: std::collections::HashSet<String> =
                                info.entries.iter().map(|e| e.id.clone()).collect();
                            let mut next_index = info.entries.len() as u32;

                            for mut entry in page.info.entries {
                                if !seen.insert(entry.id.clone()) {
                                    continue;
                                }
                                next_index += 1;
                                entry.playlist_index = Some(next_index);
                                entry.selected = false;
                                info.entries.push(entry);
                            }
                            info.video_count = info.video_count.max(page.info.video_count);
                        }

                        this.next_cursor = page.next_cursor;
                        this.has_more_remote = page.has_more;

                        // 切换到目标页（确保在追加后执行）
                        if request.may_navigate(
                            this.request_generation,
                            this.view_generation,
                            this.current_tab_index,
                        ) {
                            this.current_page =
                                target_page.min(this.get_total_pages().saturating_sub(1));
                            this.scroll_handle = VirtualListScrollHandle::new();
                        }
                        this.update_item_sizes();
                    }
                    Err(e) => {
                        if request.may_navigate(
                            this.request_generation,
                            this.view_generation,
                            this.current_tab_index,
                        ) {
                            this.page_error = Some(e.to_string());
                        }
                        tracing::error!(
                            "❌ 加载下一页失败: url={} err={}",
                            url_for_log,
                            e.to_string()
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn get_url(&self, cx: &Context<Self>) -> String {
        self.url_input.read(cx).value().to_string()
    }

    fn invalidate_parse(&mut self, cx: &mut Context<Self>) {
        self.request_generation = self.request_generation.wrapping_add(1);
        self.view_generation = self.view_generation.wrapping_add(1);
        self.state = ChannelState::Idle;
        self.active_url = None;
        self.current_tab_index = None;
        self.current_page = 0;
        self.next_cursor = None;
        self.has_more_remote = false;
        self.loading_more = false;
        self.hide_all_tab = false;
        self.tab_paging.clear();
        self.page_error = None;
        self.update_item_sizes();
        cx.notify();
    }

    /// 解析频道：输入变化或取消会废弃旧请求结果。
    fn on_parse(&mut self, cx: &mut Context<Self>) {
        if self.submitting || matches!(self.state, ChannelState::Parsing) {
            return;
        }
        let url = match AppState::validate_media_url(&self.get_url(cx)) {
            Ok(url) => url,
            Err(error) => {
                self.invalidate_parse(cx);
                self.state = ChannelState::Error(error.to_string());
                cx.notify();
                return;
            }
        };
        self.invalidate_parse(cx);
        let generation = self.request_generation;
        self.state = ChannelState::Parsing;
        self.active_url = Some(url.clone());
        self.next_cursor = None;
        self.has_more_remote = false;
        self.loading_more = false;
        self.hide_all_tab = false;
        self.tab_paging.clear();
        cx.notify();

        let app_state = self.app_state.clone();
        let url_clone = url.clone();

        tracing::info!("📡 频道解析请求: url={}", url_clone);

        // 在后台线程中获取频道信息
        cx.spawn(async move |this, cx| {
            // 使用 smol::unblock 执行阻塞的 tokio 操作
            let url_for_log = url_clone.clone();
            let result = smol::unblock(move || {
                let url_for_request = url_clone.clone();
                // 获取 tokio runtime
                let runtime = app_state.runtime.clone();
                let config = app_state.config();
                let cookies = config.advanced.cookies.clone();

                // 在 tokio runtime 中执行异步操作
                runtime.block_on(async {
                    let cookies_opt = if cookies.is_empty() {
                        None
                    } else {
                        Some(cookies.as_slice())
                    };
                    app_state
                        .tool_manager
                        .get_channel_videos_page(
                            &url_for_request,
                            None,
                            ITEMS_PER_PAGE,
                            cookies_opt,
                        )
                        .await
                })
            })
            .await;

            // 更新 UI
            let _ = this.update(cx, |this, cx| {
                if this.request_generation != generation
                    || !matches!(this.state, ChannelState::Parsing)
                {
                    return;
                }
                match result {
                    Ok(page) => {
                        let mut info = page.info;
                        // 使用解析层规整后的 URL 作为后续分页请求的基准（例如清理 Bilibili 的 spm 参数）
                        this.active_url = Some(info.url.clone());
                        tracing::info!(
                            "📺 频道解析成功: url={} title={} videos={} tabs={}",
                            url_for_log,
                            info.title,
                            info.video_count,
                            info.tabs.len()
                        );
                        // 默认不全选（避免误操作）
                        for entry in &mut info.entries {
                            entry.selected = false;
                        }
                        this.hide_all_tab = false;
                        this.tab_paging.clear();

                        if is_probably_youtube_channel_url(&url_for_log) {
                            let mut videos_entries = std::mem::take(&mut info.entries);
                            for entry in &mut videos_entries {
                                entry.selected = false;
                            }

                            info.tabs = vec![
                                ChannelTab {
                                    tab_type: ChannelTabType::Videos,
                                    title: ChannelTabType::Videos.display_name().to_string(),
                                    entries: videos_entries,
                                },
                                ChannelTab {
                                    tab_type: ChannelTabType::Shorts,
                                    title: ChannelTabType::Shorts.display_name().to_string(),
                                    entries: Vec::new(),
                                },
                                ChannelTab {
                                    tab_type: ChannelTabType::Live,
                                    title: ChannelTabType::Live.display_name().to_string(),
                                    entries: Vec::new(),
                                },
                                ChannelTab {
                                    tab_type: ChannelTabType::Playlists,
                                    title: ChannelTabType::Playlists.display_name().to_string(),
                                    entries: Vec::new(),
                                },
                            ];

                            this.tab_paging = vec![
                                TabPagingState {
                                    url: youtube_tab_url(&url_for_log, "videos"),
                                    next_cursor: page.next_cursor,
                                    has_more: page.has_more,
                                    total: Some(info.video_count),
                                    initialized: true,
                                    loading: false,
                                },
                                TabPagingState {
                                    url: youtube_tab_url(&url_for_log, "shorts"),
                                    next_cursor: None,
                                    has_more: true,
                                    total: None,
                                    initialized: false,
                                    loading: false,
                                },
                                TabPagingState {
                                    url: youtube_tab_url(&url_for_log, "streams"),
                                    next_cursor: None,
                                    has_more: true,
                                    total: None,
                                    initialized: false,
                                    loading: false,
                                },
                                TabPagingState {
                                    url: youtube_tab_url(&url_for_log, "playlists"),
                                    next_cursor: None,
                                    has_more: true,
                                    total: None,
                                    initialized: false,
                                    loading: false,
                                },
                            ];

                            this.hide_all_tab = true;
                            this.next_cursor = None;
                            this.has_more_remote = false;
                            this.current_tab_index = Some(0);
                        } else {
                            this.next_cursor = page.next_cursor;
                            this.has_more_remote = page.has_more;
                        }

                        this.state = ChannelState::Ready(info);
                        this.loading_more = false;
                        // 重置分页和 Tab 状态
                        this.current_page = 0;
                        this.scroll_handle = VirtualListScrollHandle::new();
                        this.update_item_sizes();
                    }
                    Err(e) => {
                        tracing::error!("❌ 频道解析失败: url={}, err={}", url_for_log, e);
                        this.state = ChannelState::Error(e.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 切换视频选中状态
    fn toggle_video(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.submitting {
            return;
        }
        if let ChannelState::Ready(ref mut info) = self.state {
            match self.current_tab_index {
                None => {
                    if let Some(entry) = info.entries.get_mut(index) {
                        entry.selected = !entry.selected;
                        cx.notify();
                    }
                }
                Some(tab_index) => {
                    if let Some(tab) = info.tabs.get_mut(tab_index) {
                        if let Some(entry) = tab.entries.get_mut(index) {
                            entry.selected = !entry.selected;
                            cx.notify();
                        }
                    }
                }
            }
        }
    }

    /// 全选/取消全选
    fn select_all(&mut self, selected: bool, cx: &mut Context<Self>) {
        if self.submitting {
            return;
        }
        if let ChannelState::Ready(ref mut info) = self.state {
            match self.current_tab_index {
                None => {
                    for entry in &mut info.entries {
                        entry.selected = selected;
                    }
                }
                Some(tab_index) => {
                    if let Some(tab) = info.tabs.get_mut(tab_index) {
                        for entry in &mut tab.entries {
                            entry.selected = selected;
                        }
                    }
                }
            }
            cx.notify();
        }
    }

    /// 开始下载选中的视频
    fn download_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.submitting {
            return;
        }
        let selected_entries: Vec<ChannelVideoEntry> =
            if let ChannelState::Ready(ref info) = self.state {
                match self.current_tab_index {
                    None => info
                        .entries
                        .iter()
                        .filter(|e| e.selected)
                        .cloned()
                        .collect(),
                    Some(tab_index) => info
                        .tabs
                        .get(tab_index)
                        .map(|t| t.entries.iter().filter(|e| e.selected).cloned().collect())
                        .unwrap_or_default(),
                }
            } else {
                return;
            };

        if selected_entries.is_empty() {
            window.push_notification(
                Notification::error(crate::i18n::tr("请至少选择一个视频")),
                cx,
            );
            return;
        }

        self.submitting = true;
        self.batch_progress = 0;
        self.batch_total = selected_entries.len();
        self.batch_cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.batch_cancel.clone();
        let tab_index = self.current_tab_index;
        let app_state = self.app_state.clone();
        let output_dir = app_state.config().download.default_output_path.clone();
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let mut added = 0usize;
            let mut errors = Vec::new();
            // 逐个创建，避免为大型频道一次生成数百个线程；成功项立即取消选中。
            for entry in selected_entries {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let handle = app_state.start_download_in_background(
                    entry.url.clone(),
                    output_dir.clone(),
                    "bestvideo+bestaudio/best".into(),
                    crate::app::DownloadVideoOptions {
                        embed_metadata: true,
                        embed_thumbnail: false,
                        download_subtitles: false,
                        audio_only: false,
                    },
                );
                let result = smol::unblock(move || {
                    handle.join().unwrap_or_else(|_| {
                        Err(anyhow::anyhow!(crate::i18n::tr("创建下载任务失败")))
                    })
                })
                .await;
                let success = result.is_ok();
                match result {
                    Ok(_) => added += 1,
                    Err(error) => errors.push(format!("{}: {}", entry.title, error)),
                }
                if this
                    .update_in(cx, |this, _, cx| {
                        this.batch_progress += 1;
                        if success {
                            if let ChannelState::Ready(info) = &mut this.state {
                                let entries = match tab_index {
                                    None => Some(&mut info.entries),
                                    Some(index) => {
                                        info.tabs.get_mut(index).map(|tab| &mut tab.entries)
                                    }
                                };
                                if let Some(entries) = entries {
                                    for item in
                                        entries.iter_mut().filter(|item| item.id == entry.id)
                                    {
                                        item.selected = false;
                                    }
                                }
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
            let _ = this.update_in(cx, |this, window, cx| {
                this.submitting = false;
                if errors.is_empty() {
                    window.push_notification(
                        Notification::success(crate::i18n::format(
                            "已添加 {} 个下载任务",
                            &[added.to_string()],
                        )),
                        cx,
                    );
                    if added > 0
                        && !cancel.load(Ordering::Relaxed)
                        && use_location(cx).pathname.as_ref() == "/channel"
                    {
                        use_navigate(cx)("/tasks".into());
                    }
                } else {
                    window.push_notification(
                        Notification::error(crate::i18n::format(
                            "已添加 {} 个任务，{} 个失败。{}",
                            &[
                                added.to_string(),
                                errors.len().to_string(),
                                errors[0].clone(),
                            ],
                        )),
                        cx,
                    );
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 渲染空闲状态
    fn render_idle(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py_12()
            .gap_4()
            .child(
                Icon::new(IconName::Folder)
                    .size_16()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .text_lg()
                    .text_color(cx.theme().muted_foreground)
                    .child(crate::i18n::tr("输入频道链接开始解析")),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .max_w(px(400.0))
                    .text_center()
                    .child(crate::i18n::tr(
                        "支持 YouTube 频道 (@username)、播放列表、Bilibili UP主空间等",
                    )),
            )
    }

    /// 渲染解析中状态
    fn render_parsing(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py_12()
            .gap_4()
            .child(Spinner::new().large().color(theme.primary))
            .child(div().text_lg().child(crate::i18n::tr("正在解析频道...")))
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(crate::i18n::tr("这可能需要一些时间，取决于视频数量")),
            )
    }

    /// 渲染错误状态
    fn render_error(&self, error: &str, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                Alert::error("channel-parse-error", error.to_string())
                    .title(crate::i18n::tr("解析失败")),
            )
            .child(
                Button::new("retry-channel")
                    .outline()
                    .label(crate::i18n::tr("重试"))
                    .on_click(cx.listener(|this, _, _, cx| this.on_parse(cx))),
            )
    }

    /// 渲染视频列表（使用 VirtualList + Tab + 分页）
    fn render_video_list(
        &mut self,
        info: &ChannelInfo,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let channel_title = info.title.clone();

        let current_tab_index = self.current_tab_index;
        let current_entries: &[ChannelVideoEntry] = match current_tab_index {
            None => info.entries.as_slice(),
            Some(tab_index) => info
                .tabs
                .get(tab_index)
                .map(|t| t.entries.as_slice())
                .unwrap_or(&[]),
        };

        let loaded_total_in_tab = current_entries.len();
        let selected_count = current_entries.iter().filter(|e| e.selected).count();
        let all_selected = selected_count == loaded_total_in_tab && loaded_total_in_tab > 0;

        let theme = cx.theme();

        // 提取主题颜色供后续使用
        let primary = theme.primary;
        let border_color = theme.border;
        let muted_foreground = theme.muted_foreground;

        // 当前 Tab 和分页信息
        let current_tab_index = self.current_tab_index;
        let current_page = self.current_page;
        let total_pages = self.get_total_pages();
        let current_tab_total = self.get_current_tab_total();
        let loading_more = match current_tab_index {
            None => self.loading_more,
            Some(tab_index) => self
                .tab_paging
                .get(tab_index)
                .map(|p| p.loading)
                .unwrap_or(false),
        };

        // 获取当前页的视频列表
        let page_entries = self.get_current_page_entries();
        let page_entry_count = page_entries.len();

        // 创建控制栏按钮的监听器
        let select_all_listener = cx.listener(move |this, checked: &bool, _window, cx| {
            this.select_all(*checked, cx);
        });

        let download_listener = cx.listener(|this, _, window, cx| {
            this.download_selected(window, cx);
        });

        // Kit 按钮提供键盘激活和主题状态，窄窗口可换行。
        let mut tab_buttons = Vec::new();
        if !info.tabs.is_empty() {
            if !self.hide_all_tab {
                tab_buttons.push(
                    Button::new("tab-all")
                        .small()
                        .map(|button| {
                            if current_tab_index.is_none() {
                                button.primary()
                            } else {
                                button.outline()
                            }
                        })
                        .label(crate::i18n::tr("全部"))
                        .on_click(cx.listener(|this, _, _, cx| this.switch_tab(None, cx))),
                );
            }
            for (index, tab) in info.tabs.iter().enumerate() {
                let label = match &tab.tab_type {
                    ChannelTabType::Other(name) => name.clone(),
                    kind => crate::i18n::text(kind.display_name()).to_string(),
                };
                tab_buttons.push(
                    Button::new(SharedString::from(format!("tab-{index}")))
                        .small()
                        .map(|button| {
                            if current_tab_index == Some(index) {
                                button.primary()
                            } else {
                                button.outline()
                            }
                        })
                        .label(label)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.switch_tab(Some(index), cx)),
                        ),
                );
            }
        }
        let tab_buttons = if tab_buttons.is_empty() {
            None
        } else {
            Some(div().flex().flex_wrap().gap_2().children(tab_buttons))
        };

        // 使用 VirtualList 渲染视频列表
        let item_sizes = self.item_sizes.clone();
        let scroll_handle = self.scroll_handle.clone();

        // 预先收集当前页的条目信息用于 VirtualList
        let entries_data: Vec<_> = page_entries
            .iter()
            .map(|(global_idx, entry)| {
                (
                    *global_idx,
                    entry.id.clone(),
                    entry.title.clone(),
                    entry.duration,
                    entry.thumbnail.clone(),
                    entry.selected,
                )
            })
            .collect();

        let submitting = self.submitting;
        let video_list = v_virtual_list(
            cx.entity().clone(),
            "video-list",
            item_sizes,
            move |_page, visible_range, _window, cx| {
                let theme = cx.theme();
                let border_color = theme.border;
                let secondary = theme.secondary;
                let muted = theme.muted;
                let muted_foreground = theme.muted_foreground;
                let foreground = theme.foreground;
                let entity = cx.entity().clone();

                visible_range
                    .filter_map(|ix| {
                        entries_data.get(ix).map(
                            |(global_idx, _id, title, duration, thumbnail, is_selected)| {
                                let global_idx = *global_idx;
                                let entry_title = title.clone();
                                let entry_duration = *duration;
                                let entry_thumbnail = thumbnail.clone();
                                let is_selected = *is_selected;
                                let entity_clone = entity.clone();
                                let tab_key = match current_tab_index {
                                    None => "all".to_string(),
                                    Some(tab_index) => format!("tab-{}", tab_index),
                                };

                                div()
                                    .id(SharedString::from(format!(
                                        "video-item-{}-{}",
                                        tab_key, global_idx
                                    )))
                                    .w_full()
                                    .h(px(VIDEO_ITEM_HEIGHT))
                                    .px_3()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(border_color.opacity(0.5))
                                    .hover(|style| style.bg(secondary.opacity(0.5)))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_3()
                                            .h_full()
                                            .child(
                                                Checkbox::new(SharedString::from(format!(
                                                    "video-check-{}-{}",
                                                    tab_key, global_idx
                                                )))
                                                .checked(is_selected)
                                                .accessibility_label(entry_title.clone())
                                                .disabled(submitting)
                                                .on_click(move |_, _, cx| {
                                                    let _ = entity_clone.update(cx, |this, cx| {
                                                        this.toggle_video(global_idx, cx)
                                                    });
                                                }),
                                            )
                                            // 序号 - 更紧凑
                                            .child(
                                                div()
                                                    .min_w(px(40.0))
                                                    .text_xs()
                                                    .text_color(muted_foreground)
                                                    .child(format!("#{}", global_idx + 1)),
                                            )
                                            // 缩略图 - 圆角更大
                                            .child(
                                                div()
                                                    .w(px(96.0))
                                                    .h(px(54.0))
                                                    .rounded(px(6.0))
                                                    .bg(muted)
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .overflow_hidden()
                                                    .flex_shrink_0()
                                                    .when_some(
                                                        entry_thumbnail.clone(),
                                                        |el, thumb_url| {
                                                            el.child(
                                                                img(thumb_url)
                                                                    .size_full()
                                                                    .object_fit(ObjectFit::Cover)
                                                                    .with_fallback(|| {
                                                                        div()
                                                                            .size_full()
                                                                            .into_any_element()
                                                                    }),
                                                            )
                                                        },
                                                    )
                                                    .when(entry_thumbnail.is_none(), |el| {
                                                        el.child(
                                                            Icon::new(IconName::Folder)
                                                                .size_5()
                                                                .text_color(muted_foreground),
                                                        )
                                                    }),
                                            )
                                            // 视频信息
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .flex()
                                                    .flex_col()
                                                    .justify_center()
                                                    .gap(px(4.0))
                                                    .overflow_hidden()
                                                    .min_w_0()
                                                    // 标题
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(foreground)
                                                            .truncate()
                                                            .child(entry_title),
                                                    )
                                                    // 时长
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(muted_foreground)
                                                            .child(format_duration(entry_duration)),
                                                    ),
                                            ),
                                    )
                            },
                        )
                    })
                    .collect()
            },
        )
        .track_scroll(&scroll_handle);

        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap_4()
            // 标题栏
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(channel_title),
                            )
                            .child(div().text_sm().text_color(muted_foreground).child(
                                crate::i18n::format(
                                    "已选择 {} | 已加载 {}",
                                    &[
                                        format!("{}", selected_count),
                                        format!("{}", loaded_total_in_tab),
                                    ],
                                ),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            // 全选按钮
                            .child(
                                Checkbox::new("select-all")
                                    .checked(all_selected)
                                    .label(if all_selected {
                                        crate::i18n::tr("取消全选")
                                    } else {
                                        crate::i18n::tr("全选已加载")
                                    })
                                    .disabled(self.submitting || loaded_total_in_tab == 0)
                                    .on_click(select_all_listener),
                            )
                            // 下载按钮
                            .child(
                                Button::new("download-selected")
                                    .primary()
                                    .icon(IconName::ArrowDown)
                                    .label(crate::i18n::format(
                                        "下载选中 ({})",
                                        &[format!("{}", selected_count)],
                                    ))
                                    .loading(self.submitting)
                                    .disabled(self.submitting || selected_count == 0)
                                    .on_click(download_listener),
                            ),
                    ),
            )
            // Tab 选择器
            .when_some(tab_buttons, |el, tabs| el.child(tabs))
            .when(self.submitting, |column| {
                column.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_3()
                        .child(div().text_sm().child(crate::i18n::format(
                            "正在添加任务: {} / {}",
                            &[
                                self.batch_progress.to_string(),
                                self.batch_total.to_string(),
                            ],
                        )))
                        .child(
                            Button::new("stop-batch")
                                .outline()
                                .label(crate::i18n::tr("停止添加"))
                                .disabled(self.batch_cancel.load(Ordering::Relaxed))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.batch_cancel.store(true, Ordering::Relaxed);
                                    cx.notify();
                                })),
                        ),
                )
            })
            .when_some(self.page_error.clone(), |column, error| {
                column.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(Alert::error("channel-page-error", error))
                        .child(
                            Button::new("retry-channel-page")
                                .outline()
                                .label(crate::i18n::tr("重试"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(index) = this.current_tab_index {
                                        if this
                                            .tab_paging
                                            .get(index)
                                            .is_some_and(|page| !page.initialized)
                                        {
                                            this.load_more_tab_then_switch(index, 0, cx);
                                            return;
                                        }
                                    }
                                    this.go_next_page(cx);
                                })),
                        ),
                )
            })
            // 视频列表 - 使用 VirtualList 和 Scrollbar
            .child(
                div()
                    .id("video-list-container")
                    .flex_1()
                    .w_full()
                    .overflow_hidden()
                    .relative()
                    .border_1()
                    .border_color(border_color)
                    .rounded_md()
                    .child(video_list)
                    .when(page_entry_count == 0 && !loading_more, |container| {
                        container.child(
                            Empty::new().header(
                                EmptyHeader::new()
                                    .title(
                                        EmptyTitle::new().child(crate::i18n::tr("此分类暂无视频")),
                                    )
                                    .description(
                                        EmptyDescription::new()
                                            .child(crate::i18n::tr("可以切换分类或解析其他频道")),
                                    ),
                            ),
                        )
                    })
                    // 分页加载中的居中 Spinner（避免在 Tab 上显示“加载中/数量”）
                    .when(loading_more, |el| {
                        el.child(
                            div()
                                .absolute()
                                .top_0()
                                .right_0()
                                .bottom_0()
                                .left_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(Spinner::new().large().color(primary)),
                        )
                    })
                    // 添加垂直滚动条
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .w(px(12.))
                            .child(
                                Scrollbar::new(&self.scroll_handle).axis(ScrollbarAxis::Vertical),
                            ),
                    ),
            )
            // 分页控件（底部）
            .when(total_pages > 1, |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .flex_wrap()
                        .gap_3()
                        .gap_4()
                        .py_2()
                        // 左侧：分页按钮
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                // 上一页按钮
                                .child(
                                    Button::new("prev-page-bottom")
                                        .outline()
                                        .small()
                                        .label(crate::i18n::tr("上一页"))
                                        .disabled(loading_more || current_page == 0)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if this.current_page > 0 {
                                                this.switch_page(this.current_page - 1, cx);
                                            }
                                        })),
                                )
                                // 页码显示
                                .child(
                                    div()
                                        .px_3()
                                        .py_1()
                                        .text_sm()
                                        .text_color(muted_foreground)
                                        .child(crate::i18n::format(
                                            "第 {} / {} 页",
                                            &[
                                                format!("{}", current_page + 1),
                                                format!("{}", total_pages),
                                            ],
                                        )),
                                )
                                // 下一页按钮
                                .child(
                                    Button::new("next-page-bottom")
                                        .outline()
                                        .small()
                                        .label(crate::i18n::tr("下一页"))
                                        .disabled(loading_more || current_page >= total_pages - 1)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.go_next_page(cx);
                                        })),
                                ),
                        )
                        // 右侧：显示统计
                        .child(div().text_sm().text_color(muted_foreground).child(
                            crate::i18n::format(
                                "显示 {} - {} 条，共 {} 条",
                                &[
                                    format!(
                                        "{}",
                                        if page_entry_count == 0 {
                                            0
                                        } else {
                                            current_page * ITEMS_PER_PAGE + 1
                                        }
                                    ),
                                    format!(
                                            "{}",
                                            (current_page * ITEMS_PER_PAGE + page_entry_count)
                                                .min(current_tab_total)
                                        ),
                                    format!("{}", current_tab_total),
                                ],
                            ),
                        )),
                )
            })
    }
}

impl Render for ChannelPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .id("channel-page")
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .p_6()
            .gap_6()
            .bg(theme.background)
            // 页面标题
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Icon::new(IconName::Folder)
                            .size_6()
                            .text_color(theme.foreground),
                    )
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.foreground)
                            .child(crate::i18n::tr("频道/作者")),
                    ),
            )
            // URL 输入区域
            .child(
                div()
                    .w_full()
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.secondary)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            // 说明文字
                            .child(div().text_sm().text_color(theme.muted_foreground).child(
                                crate::i18n::tr("支持 YouTube 频道、播放列表、Bilibili UP主空间等"),
                            ))
                            // 输入框和按钮
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        div().flex_1().min_w_0().child(
                                            Input::new(&self.url_input)
                                                .id("channel-url-input")
                                                .cleanable(true)
                                                .disabled(self.submitting)
                                                .aria_label(crate::i18n::tr("频道链接")),
                                        ),
                                    )
                                    .child(
                                        Button::new("parse-channel")
                                            .primary()
                                            .label(if matches!(self.state, ChannelState::Parsing) {
                                                crate::i18n::tr("解析中...")
                                            } else {
                                                crate::i18n::tr("解析")
                                            })
                                            .icon(if matches!(self.state, ChannelState::Parsing) {
                                                IconName::LoaderCircle
                                            } else {
                                                IconName::Search
                                            })
                                            .loading(matches!(self.state, ChannelState::Parsing))
                                            .disabled(
                                                self.submitting
                                                    || matches!(self.state, ChannelState::Parsing)
                                                    || self.get_url(cx).trim().is_empty(),
                                            )
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.on_parse(cx);
                                            })),
                                    )
                                    .when(matches!(self.state, ChannelState::Parsing), |row| {
                                        row.child(
                                            Button::new("cancel-channel-parse")
                                                .outline()
                                                .label(crate::i18n::tr("取消"))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.invalidate_parse(cx)
                                                })),
                                        )
                                    }),
                            ),
                    ),
            )
            // 内容区域
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(match &self.state {
                        ChannelState::Idle => self.render_idle(cx).into_any_element(),
                        ChannelState::Parsing => self.render_parsing(cx).into_any_element(),
                        ChannelState::Error(e) => self.render_error(e, cx).into_any_element(),
                        ChannelState::Ready(info) => {
                            let info = info.clone();
                            self.render_video_list(&info, cx).into_any_element()
                        }
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{PageRequest, is_probably_youtube_channel_url, page_count, youtube_tab_url};

    #[test]
    fn pagination_ignores_new_channel_and_preserves_newer_navigation() {
        let request = PageRequest {
            channel: 1,
            view: 4,
            tab: Some(0),
        };
        assert!(!request.belongs_to(2));
        assert!(request.belongs_to(1));
        assert!(request.may_navigate(1, 4, Some(0)));
        assert!(!request.may_navigate(1, 5, Some(1)));
        assert!(
            !request.may_navigate(1, 6, Some(0)),
            "switching away and back must preserve the user's page"
        );
    }

    #[test]
    fn exhausted_remote_results_do_not_create_empty_pages() {
        assert_eq!(page_count(20, Some(200), false), 1);
        assert_eq!(page_count(0, Some(200), false), 0);
        assert_eq!(page_count(21, None, false), 2);
        assert_eq!(page_count(20, None, true), 2);
        assert_eq!(page_count(20, Some(200), true), 10);
    }

    #[test]
    fn youtube_detection_checks_the_host_not_untrusted_path_text() {
        assert!(is_probably_youtube_channel_url(
            "https://www.youtube.com/@creator/videos"
        ));
        assert!(!is_probably_youtube_channel_url(
            "https://youtube.com.evil.example/@creator"
        ));
        assert!(!is_probably_youtube_channel_url(
            "https://example.com/youtube.com/@creator"
        ));
        assert!(!is_probably_youtube_channel_url(
            "https://youtube.com/playlist?list=one"
        ));
        assert_eq!(
            youtube_tab_url(
                "https://www.youtube.com/@creator/videos?source=one",
                "shorts"
            ),
            "https://www.youtube.com/@creator/shorts"
        );
    }
}
