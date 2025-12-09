//! 频道/作者页面主组件

use crate::app::AppState;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::WindowExt;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputState};
use gpui_component::notification::Notification;
use gpui_component::scroll::{Scrollbar, ScrollbarAxis, ScrollbarState};
use gpui_component::spinner::Spinner;
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, Sizable, VirtualListScrollHandle, v_virtual_list,
};
use gpui_router::use_navigate;
use magekit_shared::{ChannelInfo, ChannelVideoEntry};
use std::rc::Rc;
use std::sync::Arc;

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
        None => "未知".to_string(),
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
const ITEMS_PER_PAGE: usize = 100;

/// 频道/作者页面
pub struct ChannelPage {
    app_state: Arc<AppState>,
    url_input: Entity<InputState>,
    state: ChannelState,
    /// 下载路径
    output_path: String,
    /// 当前选中的 Tab 索引（None 表示"全部"）
    current_tab_index: Option<usize>,
    /// 当前页码（从 0 开始）
    current_page: usize,
    /// VirtualList 滚动句柄
    scroll_handle: VirtualListScrollHandle,
    /// 滚动条状态
    scroll_state: ScrollbarState,
    /// 预计算的 item sizes
    item_sizes: Rc<Vec<Size<Pixels>>>,
}

/// 视频项的固定高度
const VIDEO_ITEM_HEIGHT: f32 = 70.0;

impl ChannelPage {
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let default_path = dirs::download_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "~/Downloads".to_string());

        let url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("粘贴 YouTube 频道、播放列表或 Bilibili UP主空间链接...")
                .clean_on_escape()
        });

        Self {
            app_state,
            url_input,
            state: ChannelState::Idle,
            output_path: default_path,
            current_tab_index: None,
            current_page: 0,
            scroll_handle: VirtualListScrollHandle::new(),
            scroll_state: ScrollbarState::default(),
            item_sizes: Rc::new(Vec::new()),
        }
    }

    /// 获取当前 Tab 下当前页的视频列表
    fn get_current_page_entries(&self) -> Vec<(usize, ChannelVideoEntry)> {
        if let ChannelState::Ready(ref info) = self.state {
            // 获取当前 Tab 的视频
            let all_entries: Vec<(usize, &ChannelVideoEntry)> = match self.current_tab_index {
                None => {
                    // "全部" Tab - 显示所有视频
                    info.entries.iter().enumerate().collect()
                }
                Some(tab_index) => {
                    // 特定 Tab
                    if let Some(tab) = info.tabs.get(tab_index) {
                        // 需要找到这些视频在 entries 中的原始索引
                        tab.entries
                            .iter()
                            .filter_map(|tab_entry| {
                                info.entries
                                    .iter()
                                    .enumerate()
                                    .find(|(_, e)| e.id == tab_entry.id)
                            })
                            .collect()
                    } else {
                        Vec::new()
                    }
                }
            };

            // 分页
            let start = self.current_page * ITEMS_PER_PAGE;
            let end = (start + ITEMS_PER_PAGE).min(all_entries.len());

            all_entries
                .into_iter()
                .skip(start)
                .take(end - start)
                .map(|(idx, entry)| (idx, entry.clone()))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// 获取当前 Tab 的总视频数
    fn get_current_tab_total(&self) -> usize {
        if let ChannelState::Ready(ref info) = self.state {
            match self.current_tab_index {
                None => info.entries.len(),
                Some(tab_index) => info
                    .tabs
                    .get(tab_index)
                    .map(|t| t.entries.len())
                    .unwrap_or(0),
            }
        } else {
            0
        }
    }

    /// 获取总页数
    fn get_total_pages(&self) -> usize {
        let total = self.get_current_tab_total();
        (total + ITEMS_PER_PAGE - 1) / ITEMS_PER_PAGE
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
        self.current_tab_index = tab_index;
        self.current_page = 0;
        self.scroll_handle = VirtualListScrollHandle::new();
        self.update_item_sizes();
        cx.notify();
    }

    /// 切换页码
    fn switch_page(&mut self, page: usize, cx: &mut Context<Self>) {
        let total_pages = self.get_total_pages();
        if page < total_pages {
            self.current_page = page;
            self.scroll_handle = VirtualListScrollHandle::new();
            self.update_item_sizes();
            cx.notify();
        }
    }

    fn get_url(&self, cx: &Context<Self>) -> String {
        self.url_input.read(cx).value().to_string()
    }

    /// 解析频道
    fn on_parse(&mut self, cx: &mut Context<Self>) {
        let url = self.get_url(cx);
        if url.trim().is_empty() {
            self.state = ChannelState::Error("请输入频道链接".to_string());
            cx.notify();
            return;
        }

        self.state = ChannelState::Parsing;
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
                        .get_channel_videos(&url_for_request, cookies_opt)
                        .await
                })
            })
            .await;

            // 更新 UI
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(mut info) => {
                        tracing::info!(
                            "📺 频道解析成功: url={} title={} videos={} tabs={}",
                            url_for_log,
                            info.title,
                            info.video_count,
                            info.tabs.len()
                        );
                        // 默认选中所有视频
                        for entry in &mut info.entries {
                            entry.selected = true;
                        }
                        // 同时更新 tabs 中的 entries 选中状态
                        for tab in &mut info.tabs {
                            for entry in &mut tab.entries {
                                entry.selected = true;
                            }
                        }
                        this.state = ChannelState::Ready(info);
                        // 重置分页和 Tab 状态
                        this.current_tab_index = None;
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
        if let ChannelState::Ready(ref mut info) = self.state {
            if let Some(entry) = info.entries.get_mut(index) {
                entry.selected = !entry.selected;
                cx.notify();
            }
        }
    }

    /// 全选/取消全选
    fn select_all(&mut self, selected: bool, cx: &mut Context<Self>) {
        if let ChannelState::Ready(ref mut info) = self.state {
            for entry in &mut info.entries {
                entry.selected = selected;
            }
            cx.notify();
        }
    }

    /// 开始下载选中的视频
    fn download_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected_entries: Vec<ChannelVideoEntry> =
            if let ChannelState::Ready(ref info) = self.state {
                info.entries
                    .iter()
                    .filter(|e| e.selected)
                    .cloned()
                    .collect()
            } else {
                return;
            };

        if selected_entries.is_empty() {
            window.push_notification(Notification::error("请至少选择一个视频"), cx);
            return;
        }

        let count = selected_entries.len();
        tracing::info!("📥 开始批量下载 {} 个视频", count);

        // 导航到任务页
        {
            let mut navigate = use_navigate(cx);
            navigate("/tasks".into());
        }
        window.refresh();

        let app_state = self.app_state.clone();
        let output_dir = std::path::PathBuf::from(&self.output_path);

        // 为每个视频创建下载任务
        // ToolManager 会自动处理任务创建、状态更新和持久化
        for entry in selected_entries {
            let url = entry.url.clone();

            let options = crate::app::DownloadVideoOptions {
                embed_metadata: true,
                embed_thumbnail: false,
                download_subtitles: false,
                audio_only: false,
            };

            // 在后台启动下载
            let _handle = app_state.start_download_in_background(
                url,
                output_dir.clone(),
                "bestvideo+bestaudio/best".to_string(),
                options,
            );
        }

        window.push_notification(
            Notification::success(format!("已添加 {} 个下载任务", count)),
            cx,
        );
    }

    /// 渲染空闲状态
    fn render_idle(&self, _cx: &Context<Self>) -> impl IntoElement {
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
                    .text_color(gpui::hsla(0.0, 0.0, 0.5, 0.3)),
            )
            .child(
                div()
                    .text_lg()
                    .text_color(gpui::hsla(0.0, 0.0, 0.5, 0.5))
                    .child("输入频道链接开始解析"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(gpui::hsla(0.0, 0.0, 0.5, 0.4))
                    .max_w(px(400.0))
                    .text_center()
                    .child("支持 YouTube 频道 (@username)、播放列表、Bilibili UP主空间等"),
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
            .child(div().text_lg().child("正在解析频道..."))
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("这可能需要一些时间，取决于视频数量"),
            )
    }

    /// 渲染错误状态
    fn render_error(&self, error: &str) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py_12()
            .gap_4()
            .child(
                Icon::new(IconName::CircleX)
                    .size_12()
                    .text_color(gpui::hsla(0.0, 0.7, 0.5, 1.0)),
            )
            .child(
                div()
                    .text_lg()
                    .text_color(gpui::hsla(0.0, 0.7, 0.5, 1.0))
                    .child("解析失败"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(gpui::hsla(0.0, 0.0, 0.5, 0.6))
                    .max_w(px(400.0))
                    .text_center()
                    .child(error.to_string()),
            )
    }

    /// 渲染视频列表（使用 VirtualList + Tab + 分页）
    fn render_video_list(
        &mut self,
        info: &ChannelInfo,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let channel_title = info.title.clone();
        let selected_count = info.entries.iter().filter(|e| e.selected).count();
        let total_count = info.entries.len();
        let all_selected = selected_count == total_count && total_count > 0;
        let theme = cx.theme();

        // 提取主题颜色供后续使用
        let primary = theme.primary;
        let border_color = theme.border;
        let secondary = theme.secondary;
        let _muted = theme.muted;
        let muted_foreground = theme.muted_foreground;
        let primary_foreground = theme.primary_foreground;
        let foreground = theme.foreground;

        // 当前 Tab 和分页信息
        let current_tab_index = self.current_tab_index;
        let current_page = self.current_page;
        let total_pages = self.get_total_pages();
        let current_tab_total = self.get_current_tab_total();

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

        // 渲染 Tab 选择器
        let has_tabs = !info.tabs.is_empty();
        let tabs_clone = info.tabs.clone();
        let tab_buttons = if has_tabs {
            let mut buttons = vec![
                // "全部" Tab
                {
                    let is_selected = current_tab_index.is_none();
                    let btn_primary = primary;
                    let btn_secondary = secondary;
                    let btn_foreground = foreground;
                    let btn_primary_foreground = primary_foreground;

                    div()
                        .id("tab-all")
                        .px_4()
                        .py_2()
                        .rounded_md()
                        .cursor_pointer()
                        .when(is_selected, |d| {
                            d.bg(btn_primary).text_color(btn_primary_foreground)
                        })
                        .when(!is_selected, |d| {
                            d.bg(btn_secondary)
                                .text_color(btn_foreground)
                                .hover(|s| s.bg(btn_secondary.opacity(0.8)))
                        })
                        .child(format!("全部 ({})", total_count))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.switch_tab(None, cx);
                        }))
                },
            ];

            // 各个 Tab 按钮
            for (idx, tab) in tabs_clone.iter().enumerate() {
                let tab_name = tab.tab_type.display_name().to_string();
                let tab_count = tab.entries.len();
                let is_selected = current_tab_index == Some(idx);
                let btn_primary = primary;
                let btn_secondary = secondary;
                let btn_foreground = foreground;
                let btn_primary_foreground = primary_foreground;

                buttons.push(
                    div()
                        .id(SharedString::from(format!("tab-{}", idx)))
                        .px_4()
                        .py_2()
                        .rounded_md()
                        .cursor_pointer()
                        .when(is_selected, |d| {
                            d.bg(btn_primary).text_color(btn_primary_foreground)
                        })
                        .when(!is_selected, |d| {
                            d.bg(btn_secondary)
                                .text_color(btn_foreground)
                                .hover(|s| s.bg(btn_secondary.opacity(0.8)))
                        })
                        .child(format!("{} ({})", tab_name, tab_count))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.switch_tab(Some(idx), cx);
                        })),
                );
            }

            Some(
                div()
                    .flex()
                    .gap_2()
                    .mb_2()
                    .overflow_hidden()
                    .children(buttons),
            )
        } else {
            None
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

        let video_list = v_virtual_list(
            cx.entity().clone(),
            "video-list",
            item_sizes,
            move |_page, visible_range, _window, cx| {
                let theme = cx.theme();
                let primary = theme.primary;
                let border_color = theme.border;
                let secondary = theme.secondary;
                let muted = theme.muted;
                let muted_foreground = theme.muted_foreground;
                let primary_foreground = theme.primary_foreground;
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

                                div()
                                    .id(SharedString::from(format!("video-item-{}", global_idx)))
                                    .w_full()
                                    .h(px(VIDEO_ITEM_HEIGHT))
                                    .px_3()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(border_color.opacity(0.5))
                                    .hover(|style| style.bg(secondary.opacity(0.5)))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        let _ = entity_clone.update(cx, |this, cx| {
                                            this.toggle_video(global_idx, cx);
                                        });
                                    })
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_3()
                                            .h_full()
                                            // 复选框 - 更现代的样式
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .w(px(20.0))
                                                    .h(px(20.0))
                                                    .rounded(px(4.0))
                                                    .border_1()
                                                    .when(is_selected, |d| {
                                                        d.bg(primary).border_color(primary).child(
                                                            div()
                                                                .text_xs()
                                                                .font_weight(FontWeight::BOLD)
                                                                .text_color(primary_foreground)
                                                                .child("✓"),
                                                        )
                                                    })
                                                    .when(!is_selected, |d| {
                                                        d.border_color(
                                                            muted_foreground.opacity(0.5),
                                                        )
                                                        .bg(gpui::transparent_black())
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
                                                                    .object_fit(ObjectFit::Cover),
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
                            .child(div().text_sm().text_color(muted_foreground).child(format!(
                                "共 {} 个视频，已选择 {} | 当前显示 {}",
                                total_count, selected_count, current_tab_total
                            ))),
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
                                        "取消全选"
                                    } else {
                                        "全选"
                                    })
                                    .on_click(select_all_listener),
                            )
                            // 下载按钮
                            .child(
                                Button::new("download-selected")
                                    .primary()
                                    .icon(IconName::ArrowDown)
                                    .label(format!("下载选中 ({})", selected_count))
                                    .disabled(selected_count == 0)
                                    .on_click(download_listener),
                            ),
                    ),
            )
            // Tab 选择器
            .when_some(tab_buttons, |el, tabs| el.child(tabs))
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
                    // 添加垂直滚动条
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .w(px(12.))
                            .child(
                                Scrollbar::both(&self.scroll_state, &self.scroll_handle)
                                    .axis(ScrollbarAxis::Vertical),
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
                                        .label("上一页")
                                        .disabled(current_page == 0)
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
                                        .child(format!(
                                            "第 {} / {} 页",
                                            current_page + 1,
                                            total_pages
                                        )),
                                )
                                // 下一页按钮
                                .child(
                                    Button::new("next-page-bottom")
                                        .outline()
                                        .small()
                                        .label("下一页")
                                        .disabled(current_page >= total_pages - 1)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            let max_page = this.get_total_pages();
                                            if this.current_page < max_page - 1 {
                                                this.switch_page(this.current_page + 1, cx);
                                            }
                                        })),
                                ),
                        )
                        // 右侧：显示统计
                        .child(div().text_sm().text_color(muted_foreground).child(format!(
                                "显示 {} - {} 条，共 {} 条",
                                current_page * ITEMS_PER_PAGE + 1,
                                (current_page * ITEMS_PER_PAGE + page_entry_count)
                                    .min(current_tab_total),
                                current_tab_total
                            ))),
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
                            .child("频道/作者"),
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
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child("支持 YouTube 频道、播放列表、Bilibili UP主空间等"),
                            )
                            // 输入框和按钮
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .child(Input::new(&self.url_input).cleanable(true)),
                                    )
                                    .child(
                                        Button::new("parse-channel")
                                            .primary()
                                            .label(if matches!(self.state, ChannelState::Parsing) {
                                                "解析中..."
                                            } else {
                                                "解析"
                                            })
                                            .icon(if matches!(self.state, ChannelState::Parsing) {
                                                IconName::LoaderCircle
                                            } else {
                                                IconName::Search
                                            })
                                            .disabled(matches!(self.state, ChannelState::Parsing))
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.on_parse(cx);
                                            })),
                                    ),
                            ),
                    ),
            )
            // 内容区域
            .child(div().flex_1().overflow_hidden().child(match &self.state {
                ChannelState::Idle => self.render_idle(cx).into_any_element(),
                ChannelState::Parsing => self.render_parsing(cx).into_any_element(),
                ChannelState::Error(e) => self.render_error(e).into_any_element(),
                ChannelState::Ready(info) => {
                    let info = info.clone();
                    self.render_video_list(&info, cx).into_any_element()
                }
            }))
    }
}
