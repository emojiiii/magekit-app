//! M3U8 抓取与下载能力
//!
//! - 负责浏览器探测、嗅探任务启动与事件流转
//! - UI 通过 `CaptureEvent` 订阅进度/日志

use crate::app::AppState;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use magekit_shared::utils::{get_app_data_dir, resolve_browser_path};
use rand::{Rng, distributions::Alphanumeric};
use reqwest::header::{ACCEPT_LANGUAGE, HeaderMap, HeaderValue};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::future::pending;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time;
use tokio_tungstenite::connect_async;
use url::Url;

const CAPTURE_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const CAPTURE_ACCEPT_LANGUAGE: &str = "zh-CN,zh;q=0.9,en;q=0.8";
const BROWSER_PROFILE_DIR_NAME: &str = "browser_profile";
const DEVTOOLS_HOST: &str = "127.0.0.1";
// 需要透传的 Header 白名单（小写）
const HEADER_WHITELIST: &[&str] = &[
    "referer",
    "user-agent",
    "cookie",
    "origin",
    "authorization",
    "accept-language",
];

/// 抓取请求
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    /// 目标页面 URL
    pub target_url: String,
    /// 自定义浏览器路径（优先级最高）
    pub custom_browser_path: Option<PathBuf>,
    /// 是否启用无头模式
    pub headless: bool,
    /// 抓取超时
    pub timeout: Duration,
}

/// 抓到的 m3u8 流
#[derive(Debug, Clone, PartialEq)]
pub struct M3u8Stream {
    pub url: String,
    pub mime_type: Option<String>,
    pub referer: Option<String>,
    pub title: Option<String>,
    /// 估算总时长（秒）
    pub duration_seconds: Option<f64>,
    /// 抓到时的请求头（用于透传 UA/Cookie/Referer 等）
    pub headers: Option<Vec<(String, String)>>,
}

/// 抓取事件
#[derive(Debug, Clone)]
pub enum CaptureEvent {
    Log(String),
    Found(M3u8Stream),
    Finished,
    Error(String),
}

/// 抓取会话句柄
pub struct CaptureSession {
    pub rx: mpsc::Receiver<CaptureEvent>,
    cancel_tx: Option<oneshot::Sender<()>>,
}

impl CaptureSession {
    pub fn cancel(&mut self) {
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
    }

    pub fn split(self) -> (mpsc::Receiver<CaptureEvent>, Option<oneshot::Sender<()>>) {
        (self.rx, self.cancel_tx)
    }
}

impl AppState {
    /// 启动 m3u8 嗅探任务
    pub async fn start_m3u8_capture(&self, request: CaptureRequest) -> Result<CaptureSession> {
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
        let browser_path = resolve_browser_path(request.custom_browser_path.clone());
        let profile_dir = get_app_data_dir()
            .context("获取应用数据目录失败")?
            .join(BROWSER_PROFILE_DIR_NAME);
        let _ = fs::create_dir_all(&profile_dir);
        let page_title = fetch_html_title(&client, &target_url).await.ok().flatten();
        let page_title_shared = Arc::new(Mutex::new(page_title.clone()));

        let cancel_flag = Arc::new(AtomicBool::new(false));
        let sent_urls: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
        let request_headers: Arc<Mutex<HashMap<String, Vec<(String, String)>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let devtools_port = pick_free_port();

        // 在 Tokio runtime 上运行
        self.runtime.spawn(async move {
            let mut browser_child: Option<Child> = None;

            let _ = event_tx
                .send(CaptureEvent::Log(format!(
                    "🚀 开始嗅探: {} (headless={})",
                    target_url, headless
                )))
                .await;

            if let Some(path) = browser_path.clone() {
                match spawn_browser(&path, &profile_dir, devtools_port, &target_url, headless).await
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

            let work = async {
                // 1) 快速静态扫描（不依赖浏览器）
                match time::timeout(
                    timeout,
                    quick_scan_for_m3u8(&client, &target_url, page_title.clone()),
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

                // 2) 预留：CDP 抓包逻辑（目前仅占位）
                if browser_path.is_some() {
                    let _ = event_tx
                        .send(CaptureEvent::Log(
                            "ℹ️ 已保持浏览器运行，尝试通过 CDP 监听网络请求".into(),
                        ))
                        .await;

                    let event_tx_clone = event_tx.clone();
                    let client_clone = client.clone();
                    let sent_urls_clone = sent_urls.clone();
                    let cancel_flag_clone = cancel_flag.clone();
                    let request_headers_clone = request_headers.clone();
                    let target_for_cdp = target_url.clone();
                    let page_title_shared = page_title_shared.clone();
                    tokio::spawn(async move {
                        let maybe_err = cdp_listen(
                            client_clone,
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

        Ok(CaptureSession {
            rx: event_rx,
            cancel_tx: Some(cancel_tx),
        })
    }
}

// =========================================================================================
// 辅助函数
// =========================================================================================

async fn quick_scan_for_m3u8(
    client: &reqwest::Client,
    target_url: &str,
    page_title_prefetch: Option<String>,
) -> Result<Vec<M3u8Stream>> {
    let mut results = Vec::new();
    let url = Url::parse(target_url).context("URL 不合法")?;

    // 先抓取 HTML，用于提取 title（优先使用预取）
    let page_title = match page_title_prefetch {
        Some(t) => Some(t),
        None => fetch_html_title(client, target_url).await.unwrap_or(None),
    };

    // 1) HEAD 检查
    if let Ok(resp) = client.head(url.clone()).send().await {
        if let Some(mt) = resp.headers().get(reqwest::header::CONTENT_TYPE) {
            if let Ok(mt) = mt.to_str() {
                if mt.contains("mpegurl") || mt.contains("vnd.apple.mpegurl") {
                    let duration = fetch_m3u8_duration(client, target_url, None).await;
                    results.push(M3u8Stream {
                        url: target_url.to_string(),
                        mime_type: Some(mt.to_string()),
                        referer: None,
                        title: page_title.clone(),
                        duration_seconds: duration,
                        headers: None,
                    });
                    return Ok(results);
                }
            }
        }
    }

    // 2) GET 内容扫描
    let body = client
        .get(url.clone())
        .send()
        .await
        .context("获取页面失败")?
        .text()
        .await
        .context("读取页面内容失败")?;

    let mut seen = HashSet::new();
    for token in
        body.split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '<' || c == '>')
    {
        if token.contains(".m3u8") {
            let candidate = token.trim_matches(['"', '\'', ',', ';']);
            if candidate.len() < 5 {
                continue;
            }

            let absolute = if let Ok(u) = Url::parse(candidate) {
                u
            } else if let Ok(joined) = url.join(candidate) {
                joined
            } else {
                continue;
            };

            let final_url = absolute.to_string();
            if seen.insert(final_url.clone()) {
                let duration = fetch_m3u8_duration(client, &final_url, Some(target_url)).await;
                results.push(M3u8Stream {
                    url: final_url,
                    mime_type: None,
                    referer: Some(target_url.to_string()),
                    title: page_title.clone(),
                    duration_seconds: duration,
                    headers: None,
                });
            }
        }
    }

    Ok(results)
}

async fn fetch_m3u8_duration(
    client: &reqwest::Client,
    m3u8_url: &str,
    referer: Option<&str>,
) -> Option<f64> {
    let mut req = client.get(m3u8_url);
    if let Some(r) = referer {
        req = req.header(reqwest::header::REFERER, r);
    }
    let text = req.send().await.ok()?.text().await.ok()?;
    parse_m3u8_duration(&text)
}

fn parse_m3u8_duration(content: &str) -> Option<f64> {
    let mut total = 0.0;
    let mut found = false;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            if let Some((dur_str, _)) = rest.split_once(',') {
                if let Ok(v) = dur_str.trim().parse::<f64>() {
                    total += v;
                    found = true;
                }
            } else if let Ok(v) = rest.trim().parse::<f64>() {
                total += v;
                found = true;
            }
        }
    }
    if found { Some(total) } else { None }
}

async fn ensure_title(
    client: &reqwest::Client,
    shared_title: &Arc<Mutex<Option<String>>>,
    target_url: &str,
) -> Option<String> {
    // fast path
    if let Some(t) = shared_title.lock().await.clone() {
        return Some(t);
    }
    if let Ok(t) = fetch_html_title(client, target_url).await {
        if let Some(tt) = t {
            let mut guard = shared_title.lock().await;
            *guard = Some(tt.clone());
            return Some(tt);
        }
    }
    None
}

fn extract_headers(v: Option<&Value>) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    if let Some(obj) = v.and_then(|vv| {
        vv.get("headers")
            .and_then(|h| h.as_object())
            .or_else(|| vv.as_object())
    }) {
        for (k, v) in obj {
            if let Some(val) = v.as_str() {
                out.push((k.clone(), val.to_string()));
            }
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

fn canonical_header_name(key_lower: &str) -> String {
    match key_lower {
        "user-agent" => "User-Agent".into(),
        "referer" => "Referer".into(),
        "cookie" => "Cookie".into(),
        "origin" => "Origin".into(),
        "authorization" => "Authorization".into(),
        "accept-language" => "Accept-Language".into(),
        _ => key_lower.to_string(),
    }
}

fn header_value(headers: Option<&Vec<(String, String)>>, key: &str) -> Option<String> {
    headers.and_then(|list| {
        list.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.clone())
    })
}

fn fetch_cookies_string(headers: Option<&Vec<(String, String)>>) -> Option<String> {
    header_value(headers, "cookie")
}

fn build_m3u8_headers(
    request_headers: Option<Vec<(String, String)>>,
    fallback_referer: &str,
) -> Option<Vec<(String, String)>> {
    let headers_ref = request_headers.as_ref();
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for key in HEADER_WHITELIST {
        match *key {
            "user-agent" => {
                let ua = header_value(headers_ref, "user-agent")
                    .unwrap_or_else(|| CAPTURE_USER_AGENT.to_string());
                if seen.insert("user-agent".into()) {
                    out.push((canonical_header_name("user-agent"), ua));
                }
            }
            "referer" => {
                let referer_val = header_value(headers_ref, "referer")
                    .unwrap_or_else(|| fallback_referer.to_string());
                if !referer_val.is_empty() && seen.insert("referer".into()) {
                    out.push((canonical_header_name("referer"), referer_val));
                }
            }
            "cookie" => {
                if let Some(cookie) = fetch_cookies_string(headers_ref) {
                    if seen.insert("cookie".into()) {
                        out.push((canonical_header_name("cookie"), cookie));
                    }
                }
            }
            "origin" => {
                if let Some(origin) = header_value(headers_ref, "origin") {
                    if seen.insert("origin".into()) {
                        out.push((canonical_header_name("origin"), origin));
                    }
                }
            }
            other => {
                if let Some(val) = header_value(headers_ref, other) {
                    if seen.insert(other.to_string()) {
                        out.push((canonical_header_name(other), val));
                    }
                }
            }
        }
    }

    if out.is_empty() { None } else { Some(out) }
}

async fn fetch_html_title(client: &reqwest::Client, target_url: &str) -> Result<Option<String>> {
    let body = client
        .get(target_url)
        .send()
        .await
        .context("获取页面失败")?
        .text()
        .await
        .context("读取页面内容失败")?;

    if let Some(start) = body.to_lowercase().find("<title>") {
        if let Some(end) = body.to_lowercase().find("</title>") {
            if end > start + 7 {
                let title_raw = &body[start + 7..end];
                let title = title_raw.trim().replace('\n', " ").replace('\r', " ");
                return Ok(Some(title));
            }
        }
    }
    Ok(None)
}

/// 为 CDP 会话创建临时用户数据目录
#[allow(dead_code)]
fn make_temp_user_data_dir() -> PathBuf {
    let random_suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();

    std::env::temp_dir().join(format!("magekit-profile-{}", random_suffix))
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
    let mut cmd = Command::new(path);
    cmd.arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-popup-blocking")
        .arg("--remote-allow-origins=*")
        .arg(format!("--remote-debugging-port={}", devtools_port))
        .arg("--lang=zh-CN")
        .arg("--disable-features=PrivacySandboxAdsAPIs,SameSiteByDefaultCookies")
        .arg("--window-size=1280,720")
        .arg(target_url);

    if headless {
        cmd.arg("--headless=new");
    }

    cmd.spawn().context("启动浏览器失败")
}

async fn cdp_listen(
    client: reqwest::Client,
    devtools_port: u16,
    sent_urls: Arc<Mutex<HashSet<String>>>,
    event_tx: mpsc::Sender<CaptureEvent>,
    cancel_flag: Arc<AtomicBool>,
    page_title: Arc<Mutex<Option<String>>>,
    request_headers: Arc<Mutex<HashMap<String, Vec<(String, String)>>>>,
    referer: String,
) -> Result<()> {
    if devtools_port == 0 {
        let _ = event_tx
            .send(CaptureEvent::Log(
                "⚠️ 未找到可用调试端口，跳过 CDP 监听".into(),
            ))
            .await;
        return Ok(());
    }

    let ws_url = wait_for_ws_url(&client, devtools_port).await?;
    let (ws_stream, _) = connect_async(ws_url)
        .await
        .context("连接 DevTools WebSocket 失败")?;
    let (mut write, mut read) = ws_stream.split();

    // 启用 Network
    let enable_msg = serde_json::json!({
        "id": 1,
        "method": "Network.enable",
        "params": {
            "maxResourceBufferSize": 0,
            "maxTotalBufferSize": 0,
        }
    })
    .to_string();
    write
        .send(tokio_tungstenite::tungstenite::Message::Text(enable_msg))
        .await?;

    // 获取页面标题（通过 CDP，而不是额外拉取页面）
    let title_eval_msg = serde_json::json!({
        "id": 2,
        "method": "Runtime.evaluate",
        "params": {
            "expression": "document.title",
            "returnByValue": true
        }
    })
    .to_string();
    write
        .send(tokio_tungstenite::tungstenite::Message::Text(
            title_eval_msg,
        ))
        .await?;

    let mut request_id_url: HashMap<String, String> = HashMap::new();

    while let Some(msg) = read.next().await {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }
        let msg = match msg {
            Ok(tokio_tungstenite::tungstenite::Message::Text(t)) => t,
            _ => continue,
        };
        let v: Value = match serde_json::from_str(&msg) {
            Ok(v) => v,
            Err(_) => continue,
        };
        // 捕获标题的响应
        if v.get("id").and_then(|i| i.as_i64()) == Some(2) {
            if let Some(val) = v
                .get("result")
                .and_then(|r| r.get("result"))
                .and_then(|r| r.get("value"))
                .and_then(|v| v.as_str())
            {
                let mut guard = page_title.lock().await;
                *guard = Some(val.to_string());
            }
            continue;
        }
        if let Some(method) = v.get("method").and_then(|m| m.as_str()) {
            match method {
                "Network.requestWillBeSent" => {
                    if let Some(params) = v.get("params") {
                        if let Some(req_id) = params.get("requestId").and_then(|r| r.as_str()) {
                            if let Some(url) = params
                                .get("request")
                                .and_then(|r| r.get("url"))
                                .and_then(|u| u.as_str())
                            {
                                request_id_url.insert(req_id.to_string(), url.to_string());
                            }
                        }
                    }
                }
                "Network.requestWillBeSentExtraInfo" => {
                    if let Some(params) = v.get("params") {
                        if let Some(req_id) = params.get("requestId").and_then(|r| r.as_str()) {
                            if let Some(headers) = extract_headers(params.get("headers")) {
                                let mut guard = request_headers.lock().await;
                                guard.insert(req_id.to_string(), headers);
                                // 防止过度增长
                                if guard.len() > 2000 {
                                    guard.clear();
                                }
                            }
                        }
                    }
                }
                "Network.responseReceived" => {
                    if let Some(params) = v.get("params") {
                        let request_id = params.get("requestId").and_then(|r| r.as_str());
                        let mut request_headers_for_id = None;
                        if let Some(req_id) = request_id {
                            let mut guard = request_headers.lock().await;
                            request_headers_for_id = guard.remove(req_id);
                        }

                        let url = params
                            .get("response")
                            .and_then(|r| r.get("url"))
                            .and_then(|u| u.as_str())
                            .map(|u| u.to_string())
                            .or_else(|| request_id.and_then(|id| request_id_url.remove(id)));
                        let mime = params
                            .get("response")
                            .and_then(|r| r.get("mimeType"))
                            .and_then(|m| m.as_str());
                        if let Some(url) = url {
                            if is_m3u8_url(&url)
                                || mime.map(|m| m.contains("mpegurl")).unwrap_or(false)
                            {
                                let title = ensure_title(&client, &page_title, &referer).await;
                                let duration =
                                    fetch_m3u8_duration(&client, &url, Some(&referer)).await;
                                // 仅使用请求阶段的白名单头，避免 response 头过多
                                let headers = build_m3u8_headers(request_headers_for_id, &referer);
                                push_found(
                                    url,
                                    mime.map(|s| s.to_string()),
                                    title,
                                    duration,
                                    Some(referer.clone()),
                                    headers,
                                    sent_urls.clone(),
                                    event_tx.clone(),
                                )
                                .await;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

async fn push_found(
    url: String,
    mime: Option<String>,
    title: Option<String>,
    duration: Option<f64>,
    referer: Option<String>,
    headers: Option<Vec<(String, String)>>,
    sent_urls: Arc<Mutex<HashSet<String>>>,
    event_tx: mpsc::Sender<CaptureEvent>,
) {
    let mut guard = sent_urls.lock().await;
    if guard.insert(url.clone()) {
        drop(guard);
        let _ = event_tx
            .send(CaptureEvent::Found(M3u8Stream {
                url,
                mime_type: mime,
                referer,
                title,
                duration_seconds: duration,
                headers,
            }))
            .await;
    }
}

async fn wait_for_ws_url(client: &reqwest::Client, port: u16) -> Result<String> {
    let endpoint_version = format!("http://{}:{}/json/version", DEVTOOLS_HOST, port);
    let endpoint_list = format!("http://{}:{}/json", DEVTOOLS_HOST, port);
    for _ in 0..30 {
        // 优先取 page target
        if let Ok(resp) = client.get(&endpoint_list).send().await {
            if let Ok(text) = resp.text().await {
                if let Ok(arr) = serde_json::from_str::<Vec<Value>>(&text) {
                    if let Some(ws) = arr
                        .iter()
                        .filter(|v| v.get("type").and_then(|t| t.as_str()) == Some("page"))
                        .filter_map(|v| v.get("webSocketDebuggerUrl").and_then(|u| u.as_str()))
                        .next()
                    {
                        return Ok(ws.to_string());
                    }
                    // 退而求其次取列表里的第一个 ws
                    if let Some(ws) = arr
                        .iter()
                        .filter_map(|v| v.get("webSocketDebuggerUrl").and_then(|u| u.as_str()))
                        .next()
                    {
                        return Ok(ws.to_string());
                    }
                }
            }
        }
        // 最后再尝试 browser 级 ws
        if let Ok(resp) = client.get(&endpoint_version).send().await {
            if let Ok(text) = resp.text().await {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    if let Some(ws) = v.get("webSocketDebuggerUrl").and_then(|u| u.as_str()) {
                        return Ok(ws.to_string());
                    }
                }
            }
        }
        time::sleep(Duration::from_millis(200)).await;
    }
    Err(anyhow::anyhow!("获取 DevTools WebSocket 地址失败"))
}

fn is_m3u8_url(url: &str) -> bool {
    url.contains(".m3u8")
}
