//! 设置页面主组件

use super::widgets::{
    AboutSection, AdvancedSettingsCard, DownloadSettingsCard, ProxyMode, ProxySettingsCard,
    ProxyTestStatus, ThemeSettingsCard,
};
use crate::app::AppState;
use gpui::*;
use gpui_component::input::InputState;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, Theme, ThemeRegistry};
use magekit_shared::types::Theme as AppTheme;
use std::path::PathBuf;
use std::sync::Arc;

/// 设置页面
pub struct SettingsPage {
    app_state: Arc<AppState>,
    // 下载设置
    download_path: String,
    max_concurrent: usize,
    embed_metadata: bool,
    embed_thumbnail: bool,

    // 外观设置 - 存储主题名称
    theme_name: SharedString,

    // 代理设置
    proxy_mode: ProxyMode,
    proxy_url: String,
    proxy_input: Entity<InputState>,
    proxy_test_status: ProxyTestStatus,

    // 高级设置
    auto_check_updates: bool,
    debug_mode: bool,

    // 是否有未保存的更改
    has_changes: bool,
}

impl SettingsPage {
    pub fn new(app_state: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 从 AppState 加载配置
        let config = app_state.config();
        let download_path = config
            .download
            .default_output_path
            .to_string_lossy()
            .to_string();

        // 获取当前主题名称
        let theme_name: SharedString = cx.theme().theme_name().clone();

        // 解析代理配置
        let (proxy_mode, proxy_url) = if let Some(proxy) = &config.advanced.proxy {
            if proxy.url == "system" {
                (ProxyMode::System, String::new())
            } else {
                (ProxyMode::Custom, proxy.url.clone())
            }
        } else {
            (ProxyMode::None, String::new())
        };

        // 创建代理输入框状态
        let proxy_url_clone = proxy_url.clone();
        let proxy_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("http://127.0.0.1:7890");
            if !proxy_url_clone.is_empty() {
                state.insert(&proxy_url_clone, window, cx);
            }
            state
        });

        Self {
            app_state,
            download_path,
            max_concurrent: config.download.max_concurrent_downloads,
            embed_metadata: config.download.embed_metadata,
            embed_thumbnail: config.download.embed_thumbnail,
            theme_name,
            proxy_mode,
            proxy_url,
            proxy_input,
            proxy_test_status: ProxyTestStatus::Idle,
            auto_check_updates: config.tools.auto_update,
            debug_mode: matches!(
                config.advanced.log_level,
                magekit_shared::LogLevel::Debug | magekit_shared::LogLevel::Trace
            ),
            has_changes: false,
        }
    }

    fn increment_concurrent(&mut self, cx: &mut Context<Self>) {
        if self.max_concurrent < 10 {
            self.max_concurrent += 1;
            self.has_changes = true;
            self.save_settings(cx);
        }
    }

    fn decrement_concurrent(&mut self, cx: &mut Context<Self>) {
        if self.max_concurrent > 1 {
            self.max_concurrent -= 1;
            self.has_changes = true;
            self.save_settings(cx);
        }
    }

    fn set_theme(
        &mut self,
        theme_name: &SharedString,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.theme_name = theme_name.clone();
        self.has_changes = true;

        // 从 ThemeRegistry 获取主题配置并应用
        if let Some(theme_config) = ThemeRegistry::global(cx).themes().get(theme_name).cloned() {
            Theme::global_mut(cx).apply_config(&theme_config);
            cx.refresh_windows();
        }

        self.save_settings(cx);
    }

    fn toggle_auto_check_updates(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.auto_check_updates = enabled;
        self.has_changes = true;
        self.save_settings(cx);
    }

    fn toggle_debug_mode(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.debug_mode = enabled;
        self.has_changes = true;
        self.save_settings(cx);
    }

    fn set_proxy_mode(&mut self, mode: ProxyMode, cx: &mut Context<Self>) {
        self.proxy_mode = mode;
        self.proxy_test_status = ProxyTestStatus::Idle;
        self.has_changes = true;
        // 同步输入框内容到 proxy_url
        if mode == ProxyMode::Custom {
            let input_text = self.proxy_input.read(cx).text().to_string();
            self.proxy_url = input_text;
        }
        self.save_settings(cx);
    }

    fn sync_proxy_url_from_input(&mut self, cx: &mut Context<Self>) {
        let input_text = self.proxy_input.read(cx).text().to_string();
        if input_text != self.proxy_url {
            self.proxy_url = input_text;
            self.has_changes = true;
            self.proxy_test_status = ProxyTestStatus::Idle;
            self.save_settings(cx);
        }
    }

    fn test_proxy(&mut self, cx: &mut Context<Self>) {
        // 获取代理 URL
        let proxy_url = if self.proxy_mode == ProxyMode::Custom {
            self.proxy_input.read(cx).text().to_string()
        } else {
            return;
        };

        if proxy_url.is_empty() {
            self.proxy_test_status = ProxyTestStatus::Failed;
            cx.notify();
            return;
        }

        self.proxy_test_status = ProxyTestStatus::Testing;
        cx.notify();

        cx.spawn(async move |this, cx| {
            // 测试代理连接 - 使用简单的 TCP 连接测试
            let result = smol::unblock(move || {
                // 简单解析代理 URL (http://host:port 或 socks5://host:port)
                let url = proxy_url.trim();
                let url = url.strip_prefix("http://").unwrap_or(
                    url.strip_prefix("https://")
                        .unwrap_or(url.strip_prefix("socks5://").unwrap_or(url)),
                );

                // 分离 host 和 port
                let parts: Vec<&str> = url.split(':').collect();
                let host = parts.first().unwrap_or(&"127.0.0.1");
                let port: u16 = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(7890);

                // 尝试 TCP 连接
                use std::net::TcpStream;
                let addr = format!("{}:{}", host, port);
                match addr.parse::<std::net::SocketAddr>() {
                    Ok(socket_addr) => {
                        TcpStream::connect_timeout(&socket_addr, std::time::Duration::from_secs(5))
                            .is_ok()
                    }
                    Err(_) => {
                        // 如果解析失败，尝试 DNS 解析
                        use std::net::ToSocketAddrs;
                        if let Ok(mut addrs) = addr.to_socket_addrs() {
                            if let Some(socket_addr) = addrs.next() {
                                return TcpStream::connect_timeout(
                                    &socket_addr,
                                    std::time::Duration::from_secs(5),
                                )
                                .is_ok();
                            }
                        }
                        false
                    }
                }
            })
            .await;

            let _ = this.update(cx, |this, cx| {
                this.proxy_test_status = if result {
                    ProxyTestStatus::Success
                } else {
                    ProxyTestStatus::Failed
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn browse_download_path(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // 使用 rfd 打开文件夹选择对话框
        let current_path = self.download_path.clone();

        cx.spawn(async move |this, cx| {
            // 在后台线程中打开文件对话框
            let selected_path: Option<PathBuf> = smol::unblock(move || {
                let dialog = rfd::FileDialog::new()
                    .set_title("选择下载目录")
                    .set_directory(&current_path);
                dialog.pick_folder()
            })
            .await;

            if let Some(path) = selected_path {
                let path_str = path.to_string_lossy().to_string();

                // 更新 SettingsPage
                let _ = this.update(cx, |this, cx| {
                    this.download_path = path_str;
                    this.has_changes = true;
                    this.save_settings(cx);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// 保存设置到 AppState
    fn save_settings(&mut self, cx: &mut Context<Self>) {
        let app_state = self.app_state.clone();
        let download_path = PathBuf::from(&self.download_path);
        let max_concurrent = self.max_concurrent;
        let embed_metadata = self.embed_metadata;
        let embed_thumbnail = self.embed_thumbnail;
        let auto_check_updates = self.auto_check_updates;
        let debug_mode = self.debug_mode;
        let theme_name = self.theme_name.to_string();
        let proxy_mode = self.proxy_mode;
        let proxy_url = self.proxy_url.clone();

        cx.spawn(async move |_this, _cx| {
            // 获取当前配置并更新
            smol::unblock(move || {
                let mut config = app_state.config();
                config.download.default_output_path = download_path;
                config.download.max_concurrent_downloads = max_concurrent;
                config.download.embed_metadata = embed_metadata;
                config.download.embed_thumbnail = embed_thumbnail;
                config.tools.auto_update = auto_check_updates;
                // 保存主题名称到配置
                config.ui.theme = AppTheme::Custom(magekit_shared::types::ThemeConfig {
                    name: theme_name.clone(),
                    mode: if theme_name.to_lowercase().contains("dark")
                        || theme_name.to_lowercase().contains("night")
                        || theme_name.to_lowercase().contains("noir")
                    {
                        magekit_shared::types::ThemeMode::Dark
                    } else {
                        magekit_shared::types::ThemeMode::Light
                    },
                });
                // 保存代理设置
                config.advanced.proxy = match proxy_mode {
                    ProxyMode::None => None,
                    ProxyMode::System => Some(magekit_shared::types::ProxyConfig {
                        url: "system".to_string(),
                        username: None,
                        password: None,
                    }),
                    ProxyMode::Custom => Some(magekit_shared::types::ProxyConfig {
                        url: proxy_url,
                        username: None,
                        password: None,
                    }),
                };
                config.advanced.log_level = if debug_mode {
                    magekit_shared::LogLevel::Debug
                } else {
                    magekit_shared::LogLevel::Info
                };

                // 使用 runtime 保存配置
                app_state.runtime.block_on(async {
                    let _ = app_state.update_config(config).await;
                });
            })
            .await;
        })
        .detach();

        self.has_changes = false;
        cx.notify();
    }
}

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 使用主题颜色
        let theme = cx.theme();
        let title_color = theme.foreground;
        let desc_color = theme.muted_foreground;

        div()
            .id("settings-page")
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                // 可滚动内容区域
                div()
                    .id("settings-scroll-container")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .p(px(24.0))
                            .gap(px(20.0))
                            // 页面标题
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.0))
                                    .child(
                                        Icon::new(IconName::Settings)
                                            .large()
                                            .text_color(title_color),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .text_xl()
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(title_color)
                                                    .child("设置"),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(desc_color)
                                                    .child("自定义应用程序行为和偏好"),
                                            ),
                                    ),
                            )
                            // 外观设置（主题切换）
                            .child(
                                ThemeSettingsCard::new(self.theme_name.clone()).on_theme_change(
                                    cx.listener(|this, theme_name: &SharedString, window, cx| {
                                        this.set_theme(theme_name, window, cx);
                                    }),
                                ),
                            )
                            // 下载设置
                            .child(
                                DownloadSettingsCard::new(&self.download_path, self.max_concurrent)
                                    .on_browse(cx.listener(|this, _ev, window, cx| {
                                        this.browse_download_path(window, cx);
                                    }))
                                    .on_increment(cx.listener(|this, _ev, _window, cx| {
                                        this.increment_concurrent(cx);
                                    }))
                                    .on_decrement(cx.listener(|this, _ev, _window, cx| {
                                        this.decrement_concurrent(cx);
                                    })),
                            )
                            // 代理设置
                            .child({
                                let proxy_mode = self.proxy_mode;
                                let proxy_input = self.proxy_input.clone();
                                let proxy_test_status = self.proxy_test_status;
                                ProxySettingsCard::new(proxy_mode)
                                    .proxy_input(proxy_input)
                                    .test_status(proxy_test_status)
                                    .on_mode_change({
                                        let entity = cx.entity().clone();
                                        move |mode, _window, cx| {
                                            let _ = entity.update(cx, |this, cx| {
                                                this.set_proxy_mode(mode, cx);
                                            });
                                        }
                                    })
                                    .on_test({
                                        let entity = cx.entity().clone();
                                        move |_ev, _window, cx| {
                                            let _ = entity.update(cx, |this, cx| {
                                                this.sync_proxy_url_from_input(cx);
                                                this.test_proxy(cx);
                                            });
                                        }
                                    })
                            })
                            // 高级设置
                            .child(
                                AdvancedSettingsCard::new(self.auto_check_updates, self.debug_mode)
                                    .on_auto_check_change(cx.listener(
                                        |this, enabled: &bool, _window, cx| {
                                            this.toggle_auto_check_updates(*enabled, cx);
                                        },
                                    ))
                                    .on_debug_mode_change(cx.listener(
                                        |this, enabled: &bool, _window, cx| {
                                            this.toggle_debug_mode(*enabled, cx);
                                        },
                                    )),
                            )
                            // 关于
                            .child(AboutSection),
                    ),
            )
    }
}
