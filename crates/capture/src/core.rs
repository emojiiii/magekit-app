//! 核心捕捉逻辑

use crate::cdp::CdpListener;
use crate::scanner::StaticScanner;
use crate::types::{CaptureEvent, CaptureRequest, CaptureSession};
use anyhow::{Context, Result};
use magekit_shared::utils::{get_app_data_dir, resolve_browser_path};
use reqwest::header::{ACCEPT_LANGUAGE, HeaderMap, HeaderValue};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::future::pending;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time;

const CAPTURE_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const CAPTURE_ACCEPT_LANGUAGE: &str = "zh-CN,zh;q=0.9,en;q=0.8";
const BROWSER_PROFILE_DIR_NAME: &str = "browser_profile";
const DEVTOOLS_HOST: &str = "127.0.0.1";

/// 启动资源捕捉任务
pub async fn start_capture(request: CaptureRequest) -> Result<CaptureSession> {
    let (event_tx, event_rx) = mpsc::channel(200);
    let (cancel_tx, mut cancel_rx) = oneshot::channel();

    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_static(CAPTURE_ACCEPT_LANGUAGE),
    );
    let client = reqwest::Client::builder()
        .user_agent(CAPTURE_USER_AGENT)
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(Duration::from_secs(20))
        .build()
        .context("创建 HTTP 客户端失败")?;

    let target_url = request.target_url.clone();
    let timeout = request.timeout;
    let headless = request.headless;
    let filter = request.filter.clone();
    let auto_speedup_ads = request.auto_speedup_ads;
    let speedup_rate = request.speedup_rate;
    let browser_path = resolve_browser_path(request.custom_browser_path.clone());
    let profile_dir = get_app_data_dir()
        .context("获取应用数据目录失败")?
        .join(BROWSER_PROFILE_DIR_NAME);
    let _ = fs::create_dir_all(&profile_dir);

    // 🔧 在启动新浏览器前，清理可能残留的Chrome进程
    cleanup_stale_chrome_processes(&profile_dir).await;

    let scanner = StaticScanner::new(client.clone());
    let page_title = scanner.fetch_page_title(&target_url).await.ok().flatten();
    let page_title_shared = Arc::new(Mutex::new(page_title.clone()));

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let sent_urls: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let request_headers: Arc<Mutex<HashMap<String, Vec<(String, String)>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let devtools_port = pick_free_port();

    // 在 Tokio runtime 上运行
    tokio::spawn(async move {
        let mut browser_child: Option<Child> = None;

        let _ = event_tx
            .send(CaptureEvent::Log(format!(
                "🚀 开始嗅探: {} (headless={})",
                target_url, headless
            )))
            .await;

        if let Some(path) = browser_path.clone() {
            match spawn_browser(&path, &profile_dir, devtools_port, &target_url, headless).await {
                Ok(child) => {
                    browser_child = Some(child);
                    let _ = event_tx
                        .send(CaptureEvent::Log(format!(
                            "🖥️ 已启动浏览器: {}",
                            path.display()
                        )))
                        .await;
                }
                Err(err) => {
                    let _ = event_tx
                        .send(CaptureEvent::Log(format!(
                            "⚠️ 浏览器启动失败: {}，仅进行静态扫描",
                            err
                        )))
                        .await;
                }
            }
        } else {
            let _ = event_tx
                .send(CaptureEvent::Log(
                    "⚠️ 未找到浏览器，将仅进行静态扫描（可在 UI 指定路径）".into(),
                ))
                .await;
        }

        let work = async {
            // 1) 快速静态扫描（不依赖浏览器）
            match time::timeout(
                timeout,
                scanner.scan_all(&target_url, page_title.clone(), &filter),
            )
            .await
            {
                Ok(Ok(found)) => {
                    for item in found {
                        let mut guard = sent_urls.lock().await;
                        if guard.insert(item.url.clone()) {
                            drop(guard);
                            let _ = event_tx.send(CaptureEvent::Found(item)).await;
                        }
                    }
                    let _ = event_tx
                        .send(CaptureEvent::Log(
                            "ℹ️ 静态扫描完成，继续监听（点击停止结束）".into(),
                        ))
                        .await;
                }
                Ok(Err(err)) => {
                    let _ = event_tx
                        .send(CaptureEvent::Log(format!("⚠️ 静态扫描失败: {}", err)))
                        .await;
                }
                Err(_) => {
                    let _ = event_tx
                        .send(CaptureEvent::Log("⚠️ 静态扫描超时，仍保持监听".into()))
                        .await;
                }
            }

            // 2) CDP 抓包逻辑
            if browser_path.is_some() {
                let _ = event_tx
                    .send(CaptureEvent::Log(
                        "ℹ️ 已保持浏览器运行，尝试通过 CDP 监听网络请求".into(),
                    ))
                    .await;

                let mut cdp_listener = CdpListener::new(client.clone(), filter.clone())
                    .with_auto_speedup(auto_speedup_ads, speedup_rate);
                let event_tx_clone = event_tx.clone();
                let sent_urls_clone = sent_urls.clone();
                let cancel_flag_clone = cancel_flag.clone();
                let request_headers_clone = request_headers.clone();
                let target_for_cdp = target_url.clone();
                let page_title_shared = page_title_shared.clone();
                tokio::spawn(async move {
                    let maybe_err = cdp_listener
                        .listen(
                            devtools_port,
                            sent_urls_clone,
                            event_tx_clone.clone(),
                            cancel_flag_clone,
                            page_title_shared,
                            request_headers_clone,
                            target_for_cdp,
                        )
                        .await;
                    if let Err(err) = maybe_err {
                        let _ = event_tx_clone
                            .send(CaptureEvent::Log(format!("⚠️ CDP 监听失败: {}", err)))
                            .await;
                    }
                });
            }

            // 持续监听，直到用户停止
            pending::<()>().await;
        };

        tokio::select! {
            _ = &mut cancel_rx => {
                cancel_flag.store(true, Ordering::Relaxed);
                let _ = event_tx.send(CaptureEvent::Log("⏹️ 嗅探已取消".into())).await;
                if let Some(mut child) = browser_child {
                    let _ = child.kill();
                }
            }
            _ = work => { }
        };
    });

    Ok(CaptureSession::new(event_rx, cancel_tx))
}

fn pick_free_port() -> u16 {
    TcpListener::bind((DEVTOOLS_HOST, 0))
        .ok()
        .and_then(|s| s.local_addr().ok().map(|a| a.port()))
        .unwrap_or(0)
}

async fn spawn_browser(
    path: &Path,
    profile_dir: &Path,
    devtools_port: u16,
    target_url: &str,
    headless: bool,
) -> Result<Child> {
    use std::process::Stdio;

    let mut cmd = Command::new(path);

    // 禁用浏览器日志输出（隐藏 GPU/TensorFlow 等错误）
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null());

    // 基础配置
    cmd.arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-popup-blocking")
        .arg("--remote-allow-origins=*")
        .arg(format!("--remote-debugging-port={}", devtools_port))
        .arg("--lang=zh-CN")
        .arg("--window-size=1280,720")
        .arg("--start-maximized")
        // 禁用日志输出
        .arg("--log-level=3")
        .arg("--silent")
        .arg("--disable-logging");

    // 绕过 Cloudflare 检测的关键参数
    // 禁用自动化控制特征（隐藏 webdriver 标志）
    cmd.arg("--disable-blink-features=AutomationControlled")
        // 排除自动化开关
        .arg("--exclude-switches=enable-automation")
        // 禁用自动化相关的特征
        .arg("--disable-features=IsolateOrigins,site-per-process,AutomationControlled")
        // 禁用沙箱（在某些环境下需要，但可能降低安全性）
        .arg("--no-sandbox")
        .arg("--disable-setuid-sandbox")
        // 禁用 GPU 和硬件加速（headless 模式下推荐）
        .arg("--disable-gpu")
        .arg("--disable-software-rasterizer")
        // 禁用共享内存（避免 /dev/shm 问题）
        .arg("--disable-dev-shm-usage")
        // 禁用扩展和插件（减少指纹特征）
        .arg("--disable-extensions")
        .arg("--disable-plugins")
        .arg("--disable-plugins-discovery")
        // 设置正常的用户代理（与 CAPTURE_USER_AGENT 保持一致）
        .arg(format!(
            "--user-agent={}",
            CAPTURE_USER_AGENT
        ))
        // 禁用一些可能暴露自动化的特征
        .arg("--disable-background-timer-throttling")
        .arg("--disable-backgrounding-occluded-windows")
        .arg("--disable-renderer-backgrounding")
        // 启用正常的浏览器行为
        .arg("--enable-features=NetworkService,NetworkServiceInProcess")
        // 禁用隐私沙箱（某些网站可能需要）
        .arg("--disable-features=PrivacySandboxAdsAPIs,SameSiteByDefaultCookies");

    if headless {
        cmd.arg("--headless=new");
    }

    cmd.arg(target_url);

    cmd.spawn().context("启动浏览器失败")
}

/// 清理残留的Chrome进程
///
/// 这个函数会清理：
/// 1. 使用相同user-data-dir的Chrome进程
/// 2. 所有带--headless参数的Chrome进程
///
/// 这样可以避免之前未正常关闭的浏览器进程阻止新实例启动
async fn cleanup_stale_chrome_processes(profile_dir: &Path) {
    use tokio::process::Command as TokioCommand;

    #[cfg(target_os = "windows")]
    {
        let profile_str = profile_dir.to_string_lossy().to_string();

        tracing::info!("🧹 清理残留的Chrome进程...");

        // 方法1：清理使用相同user-data-dir的进程
        let result = TokioCommand::new("wmic")
            .args([
                "process",
                "where",
                &format!("name='chrome.exe' and commandline like '%{}%'", profile_str),
                "delete",
            ])
            .output()
            .await;

        if let Ok(output) = result {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.contains("deleted") {
                    tracing::info!("✅ 已清理使用相同profile的Chrome进程");
                }
            }
        }

        // 方法2：清理所有无头Chrome进程（更彻底）
        let result = TokioCommand::new("wmic")
            .args([
                "process",
                "where",
                "name='chrome.exe' and commandline like '%--headless%'",
                "delete",
            ])
            .output()
            .await;

        if let Ok(output) = result {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.contains("deleted") {
                    tracing::info!("✅ 已清理无头Chrome进程");
                }
            }
        }

        // 给系统一点时间完成清理
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Linux/macOS: 使用pkill
        let profile_str = profile_dir.to_string_lossy().to_string();

        tracing::info!("🧹 清理残留的Chrome进程...");

        // 清理使用相同user-data-dir的进程
        let _ = TokioCommand::new("pkill")
            .args(["-f", &format!("chrome.*{}", profile_str)])
            .output()
            .await;

        // 清理所有无头Chrome进程
        let _ = TokioCommand::new("pkill")
            .args(["-f", "chrome.*--headless"])
            .output()
            .await;

        tokio::time::sleep(Duration::from_millis(500)).await;
        tracing::info!("✅ Chrome进程清理完成");
    }
}
