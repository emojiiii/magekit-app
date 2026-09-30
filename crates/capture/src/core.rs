//! 核心捕捉逻辑

use crate::cdp::CdpListener;
use crate::scanner::StaticScanner;
use crate::types::{CaptureEvent, CaptureRequest, CaptureSession};
use anyhow::{Context, Result};
use magekit_shared::utils::resolve_browser_path;
use reqwest::header::{ACCEPT_LANGUAGE, HeaderMap, HeaderValue};
use std::collections::{HashMap, HashSet};
use std::future::pending;
use std::net::TcpListener;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time;

const CAPTURE_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const CAPTURE_ACCEPT_LANGUAGE: &str = "zh-CN,zh;q=0.9,en;q=0.8";
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

    let parsed = url::Url::parse(&request.target_url).context("无效的网页 URL")?;
    anyhow::ensure!(
        matches!(parsed.scheme(), "http" | "https")
            && parsed.host_str().is_some()
            && parsed.username().is_empty()
            && parsed.password().is_none(),
        "只支持 HTTP 或 HTTPS 网页"
    );
    let target_url = request.target_url.clone();
    let timeout = request.timeout;
    let headless = request.headless;
    let filter = request.filter.clone();
    let auto_speedup_ads = request.auto_speedup_ads;
    let speedup_rate = request.speedup_rate;
    let browser_path = resolve_browser_path(request.custom_browser_path.clone());
    // 每个会话拥有独立临时 profile；不能清理用户或其他程序的 Chrome 进程。
    let profile = tempfile::Builder::new()
        .prefix("magekit-capture-")
        .tempdir()
        .context("创建嗅探浏览器临时目录失败")?;
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let sent_urls: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let request_headers: Arc<Mutex<HashMap<String, Vec<(String, String)>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let devtools_port = pick_free_port();
    anyhow::ensure!(devtools_port != 0, "无法分配浏览器调试端口");

    // 在 Tokio runtime 上运行
    tokio::spawn(async move {
        let mut browser_child: Option<Child> = None;
        let mut cdp_task = None;
        // 将网络准备也纳入取消范围，start_capture 可立即返回取消句柄。
        let work = async {
            let scanner = StaticScanner::new(client.clone());
            let page_title = scanner.fetch_page_title(&target_url).await.ok().flatten();
            let page_title_shared = Arc::new(Mutex::new(page_title.clone()));

            let _ = event_tx
                .send(CaptureEvent::Log(format!(
                    "🚀 开始嗅探: {} (headless={})",
                    magekit_shared::redact_url_for_log(&target_url),
                    headless
                )))
                .await;

            if let Some(path) = browser_path.clone() {
                match spawn_browser(&path, profile.path(), devtools_port, &target_url, headless)
                    .await
                {
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
                cdp_task = Some(tokio::spawn(async move {
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
                }));
            }

            // 持续监听，直到用户停止
            pending::<()>().await;
        };

        tokio::select! {
            _ = &mut cancel_rx => {}
            _ = work => {}
        }
        cancel_flag.store(true, Ordering::Relaxed);
        if let Some(task) = cdp_task {
            task.abort();
            let _ = task.await;
        }
        if let Some(mut child) = browser_child {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        let _ = event_tx.try_send(CaptureEvent::Finished);
        // profile 在本会话拥有的浏览器退出后自动删除。
        drop(profile);
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
    cmd.stdout(Stdio::null()).stderr(Stdio::null());

    // 保留 Chromium 沙箱、站点隔离和同站 Cookie 默认安全策略。
    cmd.kill_on_drop(true);
    cmd.args(browser_arguments(profile_dir, devtools_port, headless));

    cmd.arg(target_url);

    cmd.spawn().context("启动浏览器失败")
}

/// 仅配置功能必需参数；不允许通配调试来源或关闭浏览器安全边界。
fn browser_arguments(profile_dir: &Path, devtools_port: u16, headless: bool) -> Vec<String> {
    let mut args = vec![
        format!("--user-data-dir={}", profile_dir.display()),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        format!("--remote-debugging-address={DEVTOOLS_HOST}"),
        format!("--remote-debugging-port={devtools_port}"),
        "--window-size=1280,720".into(),
        "--log-level=3".into(),
    ];
    if headless {
        args.push("--headless=new".into());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_retains_sandbox_and_site_isolation() {
        let args = browser_arguments(Path::new("profile with spaces"), 9222, true);
        assert!(args.contains(&"--headless=new".to_string()));
        assert!(args.contains(&"--remote-debugging-address=127.0.0.1".to_string()));
        assert!(!args.iter().any(|arg| arg.contains("no-sandbox")
            || arg.contains("disable-setuid-sandbox")
            || arg.contains("disable-features")
            || arg.contains("remote-allow-origins")));
        assert!(
            !browser_arguments(Path::new("profile"), 9222, false)
                .iter()
                .any(|arg| arg.starts_with("--headless"))
        );
    }
    #[tokio::test]
    async fn cancellation_interrupts_stalled_startup() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (_socket, _) = listener.accept().await.unwrap();
            pending::<()>().await;
        });
        let request = CaptureRequest {
            target_url: format!("http://{address}/slow-page"),
            ..CaptureRequest::default()
        };
        let mut session = tokio::time::timeout(Duration::from_secs(1), start_capture(request))
            .await
            .expect("session handle must not wait for HTTP headers")
            .unwrap();
        session.cancel();
        let event = tokio::time::timeout(Duration::from_secs(1), session.rx_mut().recv())
            .await
            .expect("stop must interrupt the startup request");
        assert!(matches!(event, Some(CaptureEvent::Finished)));
        server.abort();
    }
}
