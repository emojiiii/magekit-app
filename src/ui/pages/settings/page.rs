//! 设置页面主组件

use super::widgets::{
    AboutSection, AdvancedSettingsCard, CookiePlatformOption, CookieSettingsCard,
    DownloadSettingsCard, LanguageSettingsCard, ProxyMode, ProxySettingsCard, ProxyTestStatus,
    SoopCredentialsCard, ThemeSettingsCard, cookie_platform_identity, cookie_platform_options,
};
use crate::app::{AppEvent, AppState};
use crate::i18n::Language;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::InputState;
use gpui_kit::component::select::{SelectEvent, SelectState};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{ActiveTheme, Theme, ThemeRegistry};
use gpui_kit::component::{WindowExt, notification::Notification};
use magekit_shared::PlatformCookie;
use magekit_shared::types::Theme as AppTheme;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// 设置页面
pub struct SettingsPage {
    app_state: Arc<AppState>,
    active_section: usize,
    // 下载设置
    download_path: String,
    max_concurrent: usize,
    embed_metadata: bool,
    embed_thumbnail: bool,

    // 外观设置 - 存储主题名称
    theme_name: SharedString,
    language: Language,

    // 代理设置
    proxy_mode: ProxyMode,
    proxy_url: String,
    proxy_input: Entity<InputState>,
    proxy_test_status: ProxyTestStatus,

    // Cookie 设置
    cookies: Vec<PlatformCookie>,
    cookie_platform_select: Entity<SelectState<Vec<CookiePlatformOption>>>,
    cookie_custom_platform_input: Entity<InputState>,
    cookie_content_input: Entity<InputState>,
    soop_username_input: Entity<InputState>,
    soop_password_input: Entity<InputState>,

    // 高级设置
    auto_check_updates: bool,
    debug_mode: bool,
    // 是否有未保存的更改
    has_changes: bool,
    save_generation: Arc<AtomicU64>,
    save_error: Option<String>,
    is_saving: bool,
    proxy_test_generation: u64,
    soop_username: String,
    soop_password: String,
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

        // 加载 Cookie 配置
        let cookies = config.advanced.cookies.clone();

        // 平台下拉选择和自定义域名输入
        let cookie_platform_select = cx.new(|cx| {
            SelectState::new(cookie_platform_options(), None, window, cx).searchable(true)
        });
        cx.subscribe_in(
            &cookie_platform_select,
            window,
            Self::on_cookie_platform_selected,
        )
        .detach();
        let cookie_custom_platform_input =
            cx.new(|cx| crate::i18n::input("例如: example.com", window, cx));
        let cookie_content_input =
            cx.new(|cx| crate::i18n::input("粘贴完整的 Cookie 字符串", window, cx).masked(true));

        let soop_username = config.live_record.soop_username.clone();
        let soop_username_input = cx.new(|cx| {
            let mut state = crate::i18n::input("SOOP 用户名", window, cx);
            if !soop_username.is_empty() {
                state.insert(&soop_username, window, cx);
            }
            state
        });
        let soop_password = config.live_record.soop_password.clone();
        let soop_password_input = cx.new(|cx| {
            let mut state = crate::i18n::input("SOOP 密码", window, cx).masked(true);
            if !soop_password.is_empty() {
                state.insert(&soop_password, window, cx);
            }
            state
        });

        // Select 会缓存选中项标题；刷新快照不清空查询、选择或 Cookie 内容。
        cx.observe_global_in::<crate::i18n::LocaleChanged>(window, |this, window, cx| {
            this.cookie_platform_select.update(cx, |select, cx| {
                let selected = select.selected_index(cx);
                select.set_selected_index(selected, window, cx);
            });
            cx.notify();
        })
        .detach();

        Self {
            app_state,
            active_section: 0,
            download_path,
            max_concurrent: config.download.max_concurrent_downloads,
            embed_metadata: config.download.embed_metadata,
            embed_thumbnail: config.download.embed_thumbnail,
            theme_name,
            language: Language::from_config(&config.ui.language),
            proxy_mode,
            proxy_url,
            proxy_input,
            proxy_test_status: ProxyTestStatus::Idle,
            cookies,
            cookie_platform_select,
            cookie_custom_platform_input,
            cookie_content_input,
            soop_username_input,
            soop_password_input,
            auto_check_updates: config.tools.auto_update,
            debug_mode: matches!(
                config.advanced.log_level,
                magekit_shared::LogLevel::Debug | magekit_shared::LogLevel::Trace
            ),
            has_changes: false,
            save_generation: Arc::new(AtomicU64::new(0)),
            save_error: None,
            is_saving: false,
            proxy_test_generation: 0,
            soop_username: config.live_record.soop_username.clone(),
            soop_password: config.live_record.soop_password.clone(),
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
            Theme::update(cx, |theme| theme.apply_config(&theme_config));
            cx.refresh_windows();
        }

        self.save_settings(cx);
    }

    fn set_language(&mut self, language: Language, window: &mut Window, cx: &mut Context<Self>) {
        // 与其他设置写入共用锁，避免过时的整份配置覆盖刚选择的语言。
        let result = {
            let mut config = self.app_state.config.blocking_write();
            let previous = config.ui.language.clone();
            config.ui.language = language.config_value().to_owned();
            let result = magekit_shared::save_app_config(&config);
            if result.is_err() {
                config.ui.language = previous;
            }
            result
        };
        if let Err(error) = result {
            window.push_notification(
                Notification::error(crate::i18n::format(
                    "无法保存语言设置: {}",
                    &[error.to_string()],
                )),
                cx,
            );
            return;
        }
        self.language = language;
        crate::i18n::apply_language(language, cx);
        cx.notify();
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

    fn on_cookie_platform_selected(
        &mut self,
        _: &Entity<SelectState<Vec<CookiePlatformOption>>>,
        event: &SelectEvent<Vec<CookiePlatformOption>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, SelectEvent::Confirm(Some(_))) {
            // 切换平台后清空 Cookie 输入，避免把刚粘贴的内容误存到另一个平台。
            self.cookie_content_input.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
            cx.notify();
        }
    }

    /// 添加 Cookie；同一平台已有配置时直接替换内容。
    fn add_cookie(&mut self, cookie: PlatformCookie, window: &mut Window, cx: &mut Context<Self>) {
        let identity = cookie_platform_identity(&cookie.platform);
        if let Some(existing) = self
            .cookies
            .iter_mut()
            .find(|c| cookie_platform_identity(&c.platform) == identity)
        {
            // 统一使用下拉选项里的平台名，保留原有启用/禁用状态。
            existing.platform = cookie.platform;
            existing.cookie = cookie.cookie;
        } else {
            self.cookies.push(cookie);
        }

        // 平台选择保持不变，方便继续替换同一平台 Cookie。
        self.cookie_content_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });

        self.has_changes = true;
        self.save_settings(cx);
        cx.notify();
    }

    /// 删除 Cookie
    fn delete_cookie(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.cookies.len() {
            self.cookies.remove(index);
            self.has_changes = true;
            self.save_settings(cx);
            cx.notify();
        }
    }

    /// 切换 Cookie 启用状态
    fn toggle_cookie(&mut self, index: usize, enabled: bool, cx: &mut Context<Self>) {
        if let Some(cookie) = self.cookies.get_mut(index) {
            cookie.enabled = enabled;
            self.has_changes = true;
            self.save_settings(cx);
            cx.notify();
        }
    }

    fn set_proxy_mode(&mut self, mode: ProxyMode, cx: &mut Context<Self>) {
        self.proxy_mode = mode;
        self.proxy_test_generation = self.proxy_test_generation.wrapping_add(1);
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
            self.proxy_test_generation = self.proxy_test_generation.wrapping_add(1);
            self.has_changes = true;
            self.proxy_test_status = ProxyTestStatus::Idle;
            self.save_settings(cx);
        }
    }

    fn test_proxy(&mut self, cx: &mut Context<Self>) {
        if self.proxy_mode != ProxyMode::Custom
            || self.proxy_test_status == ProxyTestStatus::Testing
        {
            return;
        }
        let proxy_url = self.proxy_input.read(cx).value().trim().to_string();
        let Some((host, port)) = proxy_endpoint(&proxy_url) else {
            self.proxy_test_status = ProxyTestStatus::Failed;
            cx.notify();
            return;
        };
        self.proxy_test_generation = self.proxy_test_generation.wrapping_add(1);
        let generation = self.proxy_test_generation;
        self.proxy_test_status = ProxyTestStatus::Testing;
        let runtime = self.app_state.runtime.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            // DNS 和 TCP 连接都受同一个超时限制；这里只检测端口可达性。
            let reachable = smol::unblock(move || {
                runtime.block_on(async {
                    tokio::time::timeout(
                        std::time::Duration::from_secs(5),
                        tokio::net::TcpStream::connect((host.as_str(), port)),
                    )
                    .await
                    .is_ok_and(|result| result.is_ok())
                })
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                if this.proxy_test_generation != generation || this.proxy_mode != ProxyMode::Custom
                {
                    return;
                }
                if this.proxy_input.read(cx).value().trim() != proxy_url {
                    this.proxy_test_status = ProxyTestStatus::Idle;
                } else {
                    this.proxy_test_status = if reachable {
                        ProxyTestStatus::Success
                    } else {
                        ProxyTestStatus::Failed
                    };
                }
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
                    .set_title(crate::i18n::tr("选择下载目录"))
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
        let theme_name = cx.theme().theme_name().to_string();
        let proxy_mode = self.proxy_mode;
        let proxy_url = self.proxy_url.clone();
        let cookies = self.cookies.clone();
        // 账号字段只有点击“保存账号”才提交，其他开关不能意外保存输入中的凭据。
        let soop_username = self.soop_username.clone();
        let soop_password = self.soop_password.clone();
        // 每次尝试都使旧回调失效，包括验证失败的输入。
        let generation = self.save_generation.fetch_add(1, Ordering::AcqRel) + 1;
        if proxy_mode == ProxyMode::Custom && proxy_endpoint(&proxy_url).is_none() {
            self.save_error =
                Some(crate::i18n::tr("请输入有效的 HTTP、HTTPS 或 SOCKS5 代理地址").into());
            self.has_changes = true;
            self.is_saving = false;
            cx.notify();
            return;
        }
        let save_generation = self.save_generation.clone();
        self.is_saving = true;
        self.has_changes = true;
        self.save_error = None;

        cx.spawn(async move |this, cx| {
            // 获取当前配置并更新
            let result = smol::unblock(move || -> anyhow::Result<()> {
                let mut config = app_state.config.blocking_write();
                if save_generation.load(Ordering::Acquire) != generation {
                    return Ok(());
                }
                let previous = config.clone();
                config.download.default_output_path = download_path;
                config.download.max_concurrent_downloads = max_concurrent;
                config.download.embed_metadata = embed_metadata;
                config.download.embed_thumbnail = embed_thumbnail;
                config.tools.auto_update = auto_check_updates;
                config.live_record.soop_username = soop_username.trim().to_string();
                config.live_record.soop_password = soop_password;
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
                // 保存 Cookie 设置
                config.advanced.cookies = cookies;

                // 锁内修改最新配置；不覆盖并发更新的语言或其他页面的配置。
                if let Err(error) = magekit_shared::save_app_config(&config) {
                    *config = previous;
                    return Err(error);
                }
                let saved = config.clone();
                drop(config);
                // 这是可选 UI 通知；无人消费的有界事件队列不能阻塞保存完成。
                let _ = app_state.event_tx.try_send(AppEvent::ConfigChanged(saved));
                Ok(())
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                if this.save_generation.load(Ordering::Acquire) != generation {
                    return;
                }
                this.is_saving = false;
                match result {
                    Ok(()) => {
                        this.has_changes = false;
                        this.save_error = None;
                    }
                    Err(error) => {
                        this.has_changes = true;
                        this.save_error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();

        cx.notify();
    }
}

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 顶栏也可切换主题，设置页缓存需以全局 Kit 主题为准。
        self.theme_name = cx.theme().theme_name().clone();
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
                            .w_full()
                            .p_6()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_3()
                                    .pb_4()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .child(
                                        div()
                                            .text_size(px(22.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(title_color)
                                            .child(crate::i18n::tr("设置")),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(desc_color)
                                            .child(crate::i18n::tr("自定义应用程序行为和偏好")),
                                    ),
                            )
                            .when(self.is_saving, |el| {
                                el.child(Alert::info(
                                    "settings-saving",
                                    crate::i18n::tr("正在保存设置..."),
                                ))
                            })
                            .when_some(self.save_error.clone(), |el, error| {
                                el.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_2()
                                        .child(Alert::error(
                                            "settings-save-error",
                                            crate::i18n::format("设置尚未保存: {}", &[error]),
                                        ))
                                        .child(
                                            Button::new("retry-save-settings")
                                                .outline()
                                                .label(crate::i18n::tr("重试保存"))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.save_settings(cx)
                                                })),
                                        ),
                                )
                            })
                            .child(
                                TabBar::new("settings-sections")
                                    .underline()
                                    .selected_index(self.active_section)
                                    .child(Tab::new().label(crate::i18n::tr("常规")))
                                    .child(Tab::new().label(crate::i18n::tr("网络与账号")))
                                    .child(Tab::new().label(crate::i18n::tr("高级")))
                                    .on_click(cx.listener(|this, index: &usize, _, cx| {
                                        this.active_section = *index;
                                        cx.notify();
                                    })),
                            )
                            .when(self.active_section == 0, |el| {
                                el
                                    // 外观设置（主题切换）
                                    .child(
                                        ThemeSettingsCard::new(self.theme_name.clone())
                                            .on_theme_change(cx.listener(
                                                |this, theme_name: &SharedString, window, cx| {
                                                    this.set_theme(theme_name, window, cx);
                                                },
                                            )),
                                    )
                                    .child({
                                        let entity = cx.entity();
                                        LanguageSettingsCard::new(
                                            self.language,
                                            move |language, window, cx| {
                                                entity.update(cx, |this, cx| {
                                                    this.set_language(language, window, cx)
                                                });
                                            },
                                        )
                                    })
                                    // 下载设置
                                    .child(
                                        DownloadSettingsCard::new(
                                            &self.download_path,
                                            self.max_concurrent,
                                        )
                                        .on_browse(cx.listener(|this, _ev, window, cx| {
                                            this.browse_download_path(window, cx);
                                        }))
                                        .on_increment(cx.listener(|this, _ev, _window, cx| {
                                            this.increment_concurrent(cx);
                                        }))
                                        .on_decrement(
                                            cx.listener(|this, _ev, _window, cx| {
                                                this.decrement_concurrent(cx);
                                            }),
                                        ),
                                    )
                            })
                            .when(self.active_section == 1, |el| {
                                el
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
                                    // Cookie 设置
                                    .child({
                                        let cookies = self.cookies.clone();
                                        let platform_select = self.cookie_platform_select.clone();
                                        let custom_platform_input =
                                            self.cookie_custom_platform_input.clone();
                                        let cookie_input = self.cookie_content_input.clone();
                                        let entity = cx.entity().clone();
                                        let entity2 = cx.entity().clone();
                                        let entity3 = cx.entity().clone();
                                        CookieSettingsCard::new(
                                            cookies,
                                            platform_select,
                                            custom_platform_input,
                                            cookie_input,
                                        )
                                        .on_add(move |cookie, window, cx| {
                                            let _ = entity.update(cx, |this, cx| {
                                                this.add_cookie(cookie, window, cx);
                                            });
                                        })
                                        .on_delete(move |index, _window, cx| {
                                            let _ = entity2.update(cx, |this, cx| {
                                                this.delete_cookie(index, cx);
                                            });
                                        })
                                        .on_toggle(
                                            move |index, enabled, _window, cx| {
                                                let _ = entity3.update(cx, |this, cx| {
                                                    this.toggle_cookie(index, enabled, cx);
                                                });
                                            },
                                        )
                                    })
                                    // SOOP 登录信息
                                    .child({
                                        let entity = cx.entity().clone();
                                        SoopCredentialsCard::new(
                                            self.soop_username_input.clone(),
                                            self.soop_password_input.clone(),
                                        )
                                        .on_save(
                                            move |_window, cx| {
                                                let _ = entity.update(cx, |this, cx| {
                                                    this.soop_username = this
                                                        .soop_username_input
                                                        .read(cx)
                                                        .value()
                                                        .to_string();
                                                    this.soop_password = this
                                                        .soop_password_input
                                                        .read(cx)
                                                        .value()
                                                        .to_string();
                                                    this.save_settings(cx);
                                                });
                                            },
                                        )
                                    })
                            })
                            .when(self.active_section == 2, |el| {
                                el
                                    // 高级设置
                                    .child(
                                        AdvancedSettingsCard::new(
                                            self.auto_check_updates,
                                            self.debug_mode,
                                        )
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
                                    .child(AboutSection)
                            }),
                    ),
            )
    }
}

/// 使用 URL 解析器处理认证、IPv6 和协议默认端口，拒绝静默改连 7890。
fn proxy_endpoint(value: &str) -> Option<(String, u16)> {
    let url = url::Url::parse(value.trim()).ok()?;
    let default_port = match url.scheme() {
        "http" => 80,
        "https" => 443,
        "socks5" | "socks5h" => 1080,
        _ => return None,
    };
    if url.query().is_some() || url.fragment().is_some() || !matches!(url.path(), "" | "/") {
        return None;
    }
    let host = match url.host()? {
        url::Host::Domain(host) => host.to_string(),
        url::Host::Ipv4(host) => host.to_string(),
        url::Host::Ipv6(host) => host.to_string(),
    };
    Some((host, url.port().unwrap_or(default_port)))
}

#[cfg(test)]
mod tests {
    use super::proxy_endpoint;
    #[test]
    fn proxy_endpoint_supports_credentials_ipv6_and_defaults() {
        assert_eq!(
            proxy_endpoint("http://user:pass@example.com:8080"),
            Some(("example.com".into(), 8080))
        );
        assert_eq!(
            proxy_endpoint("socks5://[::1]:1081"),
            Some(("::1".into(), 1081))
        );
        assert_eq!(
            proxy_endpoint("https://example.com"),
            Some(("example.com".into(), 443))
        );
        for value in [
            "",
            "localhost",
            "file:///tmp/a",
            "http://host:bad",
            "http://host/path",
        ] {
            assert_eq!(proxy_endpoint(value), None);
        }
    }
}
