// gui_app/src/ui/batch_panel.rs
//! 批量操作面板组件 - 批量URL导入、下载模板、分类管理

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::*;
use magekit_shared::types::DownloadOptions;
use std::sync::Arc;

/// 批量URL项
#[derive(Debug, Clone)]
pub struct BatchItem {
    /// 唯一ID
    pub id: String,
    /// URL
    pub url: String,
    /// 标题（如果已解析）
    pub title: Option<String>,
    /// 状态
    pub status: BatchItemStatus,
    /// 分类
    pub category: Option<String>,
}

/// 批量项状态
#[derive(Debug, Clone, PartialEq)]
pub enum BatchItemStatus {
    /// 待处理
    Pending,
    /// 正在解析
    Parsing,
    /// 已解析
    Parsed,
    /// 下载中
    Downloading,
    /// 已完成
    Completed,
    /// 失败
    Failed(String),
}

impl BatchItemStatus {
    fn label(&self) -> &'static str {
        match self {
            BatchItemStatus::Pending => "待处理",
            BatchItemStatus::Parsing => "解析中",
            BatchItemStatus::Parsed => "就绪",
            BatchItemStatus::Downloading => "下载中",
            BatchItemStatus::Completed => "已完成",
            BatchItemStatus::Failed(_) => "失败",
        }
    }
}

/// 下载模板
#[derive(Debug, Clone)]
pub struct DownloadTemplate {
    /// 模板名称
    pub name: String,
    /// 描述
    pub description: Option<String>,
    /// 下载选项
    pub options: DownloadOptions,
    /// 输出目录
    pub output_dir: String,
    /// 文件名模板
    pub filename_template: String,
}

impl Default for DownloadTemplate {
    fn default() -> Self {
        Self {
            name: "默认模板".to_string(),
            description: None,
            options: DownloadOptions::default(),
            output_dir: "~/Downloads".to_string(),
            filename_template: "%(title)s.%(ext)s".to_string(),
        }
    }
}

/// 分类
#[derive(Debug, Clone)]
pub struct Category {
    /// 分类名称
    pub name: String,
    /// 图标（emoji或名称）
    pub icon: String,
    /// 输出目录
    pub output_dir: Option<String>,
    /// 项目数量
    pub count: usize,
}

/// 批量面板状态
#[derive(Debug, Clone, PartialEq)]
pub enum BatchPanelState {
    /// 空闲
    Idle,
    /// 导入中
    Importing,
    /// 就绪
    Ready,
}

/// 批量面板事件
#[derive(Debug, Clone)]
pub enum BatchPanelEvent {
    /// 开始全部下载
    StartAll,
    /// 开始选中项下载
    StartSelected(Vec<String>),
    /// 取消
    Cancel,
    /// 清空列表
    Clear,
    /// 删除项
    RemoveItem(String),
    /// 应用模板
    ApplyTemplate(String),
    /// 设置分类
    SetCategory(String, String),
}

/// 事件回调类型
pub type BatchPanelCallback = Arc<dyn Fn(BatchPanelEvent) + Send + Sync>;

/// 批量操作面板
pub struct BatchPanel {
    /// 当前状态
    state: BatchPanelState,
    /// URL列表
    items: Vec<BatchItem>,
    /// 选中的项目ID
    selected_ids: std::collections::HashSet<String>,
    /// 下载模板列表
    templates: Vec<DownloadTemplate>,
    /// 当前选择的模板
    current_template: usize,
    /// 分类列表
    categories: Vec<Category>,
    /// 导入文本（用于批量粘贴）
    import_text: String,
    /// 事件回调
    callback: Option<BatchPanelCallback>,
}

impl BatchPanel {
    /// 创建新的批量面板
    pub fn new() -> Self {
        Self {
            state: BatchPanelState::Idle,
            items: Vec::new(),
            selected_ids: std::collections::HashSet::new(),
            templates: vec![DownloadTemplate::default()],
            current_template: 0,
            categories: vec![
                Category {
                    name: "音乐".to_string(),
                    icon: "🎵".to_string(),
                    output_dir: Some("~/Downloads/Music".to_string()),
                    count: 0,
                },
                Category {
                    name: "视频".to_string(),
                    icon: "🎬".to_string(),
                    output_dir: Some("~/Downloads/Videos".to_string()),
                    count: 0,
                },
                Category {
                    name: "教程".to_string(),
                    icon: "📚".to_string(),
                    output_dir: Some("~/Downloads/Tutorials".to_string()),
                    count: 0,
                },
                Category {
                    name: "其他".to_string(),
                    icon: "📁".to_string(),
                    output_dir: None,
                    count: 0,
                },
            ],
            import_text: String::new(),
            callback: None,
        }
    }

    /// 设置事件回调
    pub fn on_event(mut self, callback: BatchPanelCallback) -> Self {
        self.callback = Some(callback);
        self
    }

    /// 添加URL
    pub fn add_url(&mut self, url: String) {
        let id = uuid::Uuid::new_v4().to_string();
        self.items.push(BatchItem {
            id,
            url,
            title: None,
            status: BatchItemStatus::Pending,
            category: None,
        });
        self.state = BatchPanelState::Ready;
    }

    /// 批量添加URL
    pub fn add_urls(&mut self, urls: Vec<String>) {
        for url in urls {
            if !url.trim().is_empty() {
                self.add_url(url.trim().to_string());
            }
        }
    }

    /// 从文本解析URL
    pub fn parse_urls_from_text(&mut self, text: &str) {
        let urls: Vec<String> = text
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty()
                    && (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
            })
            .map(|s| s.trim().to_string())
            .collect();

        self.add_urls(urls);
    }

    /// 选择/取消选择项目
    pub fn toggle_item(&mut self, id: &str) {
        if self.selected_ids.contains(id) {
            self.selected_ids.remove(id);
        } else {
            self.selected_ids.insert(id.to_string());
        }
    }

    /// 全选
    pub fn select_all(&mut self) {
        self.selected_ids = self.items.iter().map(|i| i.id.clone()).collect();
    }

    /// 取消全选
    pub fn deselect_all(&mut self) {
        self.selected_ids.clear();
    }

    /// 删除项目
    pub fn remove_item(&mut self, id: &str) {
        self.items.retain(|i| i.id != id);
        self.selected_ids.remove(id);
    }

    /// 清空列表
    pub fn clear(&mut self) {
        self.items.clear();
        self.selected_ids.clear();
        self.state = BatchPanelState::Idle;
    }

    /// 更新项目状态
    pub fn update_item_status(&mut self, id: &str, status: BatchItemStatus) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.status = status;
        }
    }

    /// 设置项目标题
    pub fn set_item_title(&mut self, id: &str, title: String) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.title = Some(title);
        }
    }

    /// 获取待处理项目数
    pub fn pending_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.status == BatchItemStatus::Pending || i.status == BatchItemStatus::Parsed)
            .count()
    }

    /// 获取选中项目
    pub fn get_selected_items(&self) -> Vec<&BatchItem> {
        self.items
            .iter()
            .filter(|i| self.selected_ids.contains(&i.id))
            .collect()
    }
}

impl Render for BatchPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg = theme.background;
        let _fg = theme.foreground;
        let border = theme.border;

        v_flex()
            .w_full()
            .h_full()
            .bg(bg)
            // 头部
            .child(self.render_header(cx))
            // 主内容
            .child(
                h_flex()
                    .flex_1()
                    .overflow_hidden()
                    // 左侧：URL列表
                    .child(
                        v_flex()
                            .flex_1()
                            .border_r_1()
                            .border_color(border)
                            .child(self.render_import_area(cx))
                            .child(self.render_item_list(cx)),
                    )
                    // 右侧：设置面板
                    .child(
                        v_flex()
                            .w(px(300.0))
                            .p_4()
                            .child(self.render_settings_panel(cx)),
                    ),
            )
            // 底部操作栏
            .child(self.render_footer(cx))
    }
}

impl BatchPanel {
    /// 渲染头部
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let border = theme.border;

        let total = self.items.len();
        let selected = self.selected_ids.len();
        let pending = self.pending_count();

        h_flex()
            .h(px(56.0))
            .px_4()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(border)
            // 标题
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .child("📦 批量下载"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(format!("{} 个项目", total)),
                    ),
            )
            // 统计信息
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(format!("待下载: {}", pending)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(format!("已选: {}", selected)),
                    ),
            )
    }

    /// 渲染导入区域
    fn render_import_area(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let border = theme.border;
        let _bg = theme.muted;

        v_flex()
            .p_4()
            .gap_2()
            .border_b_1()
            .border_color(border)
            // 导入按钮
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("paste-urls")
                            .outline()
                            .small()
                            .label("📋 从剪贴板粘贴"),
                    )
                    .child(
                        Button::new("import-file")
                            .outline()
                            .small()
                            .label("📁 从文件导入"),
                    ),
            )
            // 提示文本
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("支持粘贴多个URL，每行一个"),
            )
    }

    /// 渲染项目列表
    fn render_item_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let border = theme.border;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let primary = theme.primary;
        let danger = theme.danger;
        let success = theme.success;
        let accent = theme.accent;

        if self.items.is_empty() {
            // 空状态
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_2()
                .child(div().text_2xl().child("📥"))
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("粘贴或导入URL开始批量下载"),
                )
        } else {
            // 项目列表
            v_flex()
                .flex_1()
                .overflow_hidden()
                // 工具栏
                .child(
                    h_flex()
                        .px_4()
                        .py_2()
                        .gap_2()
                        .items_center()
                        .border_b_1()
                        .border_color(border)
                        .child(
                            Checkbox::new("select-all-batch")
                                .checked(self.selected_ids.len() == self.items.len()),
                        )
                        .child(div().text_sm().text_color(muted).child("全选"))
                        .child(div().flex_1())
                        .child(
                            Button::new("clear-completed")
                                .ghost()
                                .xsmall()
                                .label("清除已完成"),
                        ),
                )
                // 列表
                .child(
                    v_flex()
                        .flex_1()
                        .overflow_hidden()
                        .children(self.items.iter().map(|item| {
                            let is_selected = self.selected_ids.contains(&item.id);
                            render_batch_item_inline(
                                item,
                                is_selected,
                                fg,
                                muted,
                                border,
                                primary,
                                danger,
                                success,
                                accent,
                            )
                        })),
                )
        }
    }

    /// 渲染设置面板
    fn render_settings_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let border = theme.border;
        let bg = theme.muted;

        v_flex()
            .gap_4()
            // 模板选择
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(fg)
                            .child("下载模板"),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .children(self.templates.iter().enumerate().map(|(idx, template)| {
                                let is_current = idx == self.current_template;
                                render_template_item_inline(
                                    &template, is_current, fg, muted, border, bg,
                                )
                            })),
                    )
                    .child(
                        Button::new("new-template")
                            .ghost()
                            .small()
                            .label("+ 新建模板"),
                    ),
            )
            // 分类
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(fg)
                            .child("分类"),
                    )
                    .child(
                        h_flex().gap_1().flex_wrap().children(
                            self.categories
                                .iter()
                                .map(|cat| render_category_badge(&cat, muted, border, bg)),
                        ),
                    ),
            )
            // 输出设置
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(fg)
                            .child("输出设置"),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .p_3()
                            .rounded_md()
                            .bg(bg)
                            .child(
                                h_flex()
                                    .justify_between()
                                    .child(div().text_xs().text_color(muted).child("保存位置"))
                                    .child(div().text_xs().child("~/Downloads")),
                            )
                            .child(
                                h_flex()
                                    .justify_between()
                                    .child(div().text_xs().text_color(muted).child("文件名格式"))
                                    .child(div().text_xs().child("%(title)s.%(ext)s")),
                            ),
                    ),
            )
    }

    /// 渲染底部
    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let _muted = theme.muted_foreground;
        let border = theme.border;

        let has_items = !self.items.is_empty();
        let has_selected = !self.selected_ids.is_empty();
        let pending = self.pending_count();

        h_flex()
            .h(px(60.0))
            .px_4()
            .items_center()
            .justify_between()
            .border_t_1()
            .border_color(border)
            // 左侧：清空按钮
            .child(
                h_flex().gap_2().child(
                    Button::new("clear-all")
                        .ghost()
                        .label("清空列表")
                        .disabled(!has_items),
                ),
            )
            // 右侧：下载按钮
            .child(
                h_flex()
                    .gap_2()
                    .when(has_selected, |this| {
                        this.child(Button::new("download-selected").outline().label(
                            SharedString::from(format!("下载选中 ({})", self.selected_ids.len())),
                        ))
                    })
                    .child(
                        Button::new("download-all")
                            .primary()
                            .label(SharedString::from(format!("全部下载 ({})", pending)))
                            .disabled(pending == 0),
                    ),
            )
    }
}

/// 渲染批量项目（内联版本）
fn render_batch_item_inline(
    item: &BatchItem,
    is_selected: bool,
    fg: Hsla,
    muted: Hsla,
    border: Hsla,
    primary: Hsla,
    danger: Hsla,
    success: Hsla,
    accent: Hsla,
) -> impl IntoElement {
    let bg = if is_selected {
        accent.opacity(0.1)
    } else {
        Hsla::transparent_black()
    };

    let status_color = match &item.status {
        BatchItemStatus::Pending => muted,
        BatchItemStatus::Parsing => primary,
        BatchItemStatus::Parsed => success,
        BatchItemStatus::Downloading => primary,
        BatchItemStatus::Completed => success,
        BatchItemStatus::Failed(_) => danger,
    };

    let id = item.id.clone();
    let title = item.title.clone().unwrap_or_else(|| item.url.clone());
    let status_label = item.status.label();

    h_flex()
        .w_full()
        .px_4()
        .py_2()
        .gap_3()
        .items_center()
        .bg(bg)
        .border_b_1()
        .border_color(border)
        .cursor_pointer()
        // 选择框
        .child(Checkbox::new(SharedString::from(format!("batch-item-{}", id))).checked(is_selected))
        // 标题/URL
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
                        .child(title),
                )
                .when(item.title.is_some(), |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(item.url.clone()),
                    )
                }),
        )
        // 状态
        .child(
            div()
                .text_xs()
                .px_2()
                .py_px()
                .rounded(px(4.0))
                .bg(status_color.opacity(0.1))
                .text_color(status_color)
                .child(status_label),
        )
        // 删除按钮
        .child(
            Button::new(SharedString::from(format!("delete-batch-{}", id)))
                .ghost()
                .xsmall()
                .icon(IconName::Close),
        )
}

/// 渲染模板项目（内联版本）
fn render_template_item_inline(
    template: &DownloadTemplate,
    is_current: bool,
    fg: Hsla,
    muted: Hsla,
    border: Hsla,
    bg: Hsla,
) -> impl IntoElement {
    h_flex()
        .px_3()
        .py_2()
        .gap_2()
        .items_center()
        .rounded_md()
        .border_1()
        .border_color(if is_current { fg } else { border })
        .bg(if is_current {
            bg
        } else {
            Hsla::transparent_black()
        })
        .cursor_pointer()
        .child(div().text_sm().text_color(fg).child(template.name.clone()))
        .when(template.description.is_some(), |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(template.description.clone().unwrap()),
            )
        })
}

/// 渲染分类徽章
fn render_category_badge(
    category: &Category,
    muted: Hsla,
    border: Hsla,
    bg: Hsla,
) -> impl IntoElement {
    h_flex()
        .px_2()
        .py_1()
        .gap_1()
        .items_center()
        .rounded_md()
        .border_1()
        .border_color(border)
        .bg(bg)
        .cursor_pointer()
        .child(div().text_sm().child(category.icon.clone()))
        .child(div().text_xs().child(category.name.clone()))
        .when(category.count > 0, |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("({})", category.count)),
            )
        })
}
