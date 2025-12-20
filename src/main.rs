// 在 Windows Release 模式下隐藏控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

use anyhow::Result;
use gpui::*;
use gpui_component::Root;
use gpui_router::init as router_init;
use std::{path::PathBuf, sync::Arc};

mod app;
mod theme;
mod ui;

use app::{AppState, GlobalAppState};
use ui::MainWindow;

fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .init();

    tracing::debug!("🚀 MageKit 视频下载器启动");

    // 注册图标资源
    let app = Application::new().with_assets(gpui_component_assets::Assets);

    app.run(move |cx| {
        // 必须在GPUI组件使用前调用
        gpui_component::init(cx);

        // 设置 HTTP 客户端，用于加载远程图片
        let http_client =
            std::sync::Arc::new(reqwest_client::ReqwestClient::user_agent("MageKit/1.0").unwrap());
        cx.set_http_client(http_client);

        // 加载主题文件
        let themes_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("themes");
        if let Err(err) = gpui_component::ThemeRegistry::watch_dir(themes_dir, cx, |cx| {
            // 主题加载完成后，尝试应用上次保存的主题
            tracing::debug!(
                "🎨 主题加载完成，共 {} 个主题可用",
                gpui_component::ThemeRegistry::global(cx)
                    .sorted_themes()
                    .len()
            );
        }) {
            tracing::warn!("⚠️ 无法加载主题目录: {}", err);
        }

        // 初始化路由系统
        router_init(cx);

        // 创建应用状态（同步初始化Tool Manager）
        let app_state = match AppState::new_sync() {
            Ok(state) => Arc::new(state),
            Err(e) => {
                tracing::error!("❌ 应用状态初始化失败: {}", e);
                return;
            }
        };

        // 设置 Global AppState，供路由页面访问
        cx.set_global(GlobalAppState(app_state.clone()));

        // 打开主窗口（无边框 + 自定义 TitleBar）
        if let Err(e) = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(100.0), px(100.0)),
                    size: size(px(1200.0), px(800.0)),
                })),
                titlebar: None, // 禁用系统标题栏
                window_background: WindowBackgroundAppearance::Transparent,
                focus: true,
                show: true,
                kind: WindowKind::Normal,
                is_movable: true,
                display_id: None,
                window_decorations: None, // 无边框
                app_id: None,
                is_minimizable: true,
                is_resizable: true,
                window_min_size: Some(size(px(800.0), px(600.0))),
                tabbing_identifier: None,
            },
            |window, cx| {
                // 创建主窗口组件
                let main_window = cx.new(|cx| MainWindow::new(app_state.clone(), window, cx));

                // 窗口的第一级必须是Root组件
                cx.new(|cx| Root::new(main_window, window, cx))
            },
        ) {
            tracing::error!("❌ 打开窗口失败: {}", e);
            return;
        }

        tracing::info!("✅ GUI 已启动");
    });

    Ok(())
}
