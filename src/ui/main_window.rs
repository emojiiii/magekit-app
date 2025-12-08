//! 主窗口组件
//!
//! 应用程序的主窗口，使用路由系统管理页面切换。

use crate::app::AppState;
use crate::ui::layout::{AppLayout, not_found_page};
use crate::ui::pages::{ChannelPage, HomePage, RecordingPage, SettingsPage, TasksPage, ToolsPage};
use gpui::*;
use gpui_component::*;
use gpui_router::{Route, Routes};
use std::sync::Arc;

/// 主窗口组件
pub struct MainWindow {
    /// 应用状态
    #[allow(dead_code)]
    app_state: Arc<AppState>,
    /// 缓存的首页 Entity
    home_page: Entity<HomePage>,
    /// 缓存的任务页 Entity
    tasks_page: Entity<TasksPage>,
    /// 缓存的工具页 Entity
    tools_page: Entity<ToolsPage>,
    /// 缓存的设置页 Entity
    settings_page: Entity<SettingsPage>,
    /// 缓存的频道页 Entity
    channel_page: Entity<ChannelPage>,
    /// 缓存的录制页 Entity
    recording_page: Entity<RecordingPage>,
}

impl MainWindow {
    /// 创建新的主窗口
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 预先创建并缓存所有页面 Entity
        let home_page = cx.new(|cx| HomePage::new(app_state.clone(), window, cx));
        let tasks_page = cx.new(|cx| TasksPage::new(app_state.clone(), window, cx));
        let tools_page = cx.new(|cx| ToolsPage::new(app_state.clone(), window, cx));
        let settings_page = cx.new(|cx| SettingsPage::new(app_state.clone(), window, cx));
        let channel_page = cx.new(|cx| ChannelPage::new(app_state.clone(), window, cx));
        let recording_page = cx.new(|cx| RecordingPage::new(app_state.clone(), window, cx));

        Self {
            app_state,
            home_page,
            tasks_page,
            tools_page,
            settings_page,
            channel_page,
            recording_page,
        }
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        // 克隆 Entity handles 用于路由
        let home_page = self.home_page.clone();
        let tasks_page = self.tasks_page.clone();
        let tools_page = self.tools_page.clone();
        let settings_page = self.settings_page.clone();
        let channel_page = self.channel_page.clone();
        let recording_page = self.recording_page.clone();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .child(
                // 路由系统 - 使用 layout 方式
                Routes::new().child(
                    Route::new()
                        .layout(AppLayout::new())
                        .child(
                            // 首页 - 下载页面 (index route) - 使用缓存的 Entity
                            Route::new().index().element(home_page),
                        )
                        .child(
                            // 任务页面 - 使用缓存的 Entity
                            Route::new().path("tasks").element(tasks_page),
                        )
                        .child(
                            // 频道页面 - 使用缓存的 Entity
                            Route::new().path("channel").element(channel_page),
                        )
                        .child(
                            // 工具管理页面 - 使用缓存的 Entity
                            Route::new().path("tools").element(tools_page),
                        )
                        .child(
                            // 录制页面 - 使用缓存的 Entity
                            Route::new().path("record").element(recording_page),
                        )
                        .child(
                            // 设置页面 - 使用缓存的 Entity
                            Route::new().path("settings").element(settings_page),
                        )
                        .child(
                            // 404 页面
                            Route::new().path("{*not_match}").element(not_found_page()),
                        ),
                ),
            )
            // 渲染对话框和通知层
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}
