//! 按平台分流的录制 facade：抖音走原生录制器，其余平台走 Streamlink。
use crate::{
    error::{RecorderError, RecorderResult},
    legacy_recorder,
    streamlink_runtime::{self, RuntimeInfo},
    types::{LiveRoomInfo, RecordConfig, RecordProgress, RecordStatus, StreamInfo, VideoQuality},
};
use magekit_shared::types::PlatformCookie;
use md5::{Digest, Md5};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Output, Stdio},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{oneshot, watch},
};

pub struct LiveRecorder {
    douyin_native: legacy_recorder::LiveRecorder,
    soop_username: Option<String>,
    soop_password: Option<String>,
    proxy: Option<String>,
}

impl Default for LiveRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveRecorder {
    pub fn new() -> Self {
        Self {
            douyin_native: legacy_recorder::LiveRecorder::new(),
            soop_username: None,
            soop_password: None,
            proxy: None,
        }
    }

    /// 配置 Streamlink SOOP 插件登录；留空时使用匿名访问或 Cookie。
    pub fn with_soop_credentials(
        mut self,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        let username = username.into();
        let password = password.into();
        if !username.trim().is_empty() && !password.is_empty() {
            self.soop_username = Some(username);
            self.soop_password = Some(password);
        }
        self
    }

    /// 将应用代理设置传给 Streamlink worker。
    pub fn with_proxy(mut self, proxy: Option<String>) -> Self {
        self.proxy = proxy;
        self
    }

    fn apply_proxy(&self, config: &mut RecordConfig) {
        if self.proxy.is_some() {
            config.proxy = self.proxy.clone();
        }
    }

    async fn cache_douyin_room_cover(&self, url: &str, room: &mut LiveRoomInfo) {
        let Some(image_url) = room.cover_url.as_deref() else {
            return;
        };
        if Path::new(image_url).is_file() {
            return;
        }
        if let Some(path) = cache_douyin_image(image_url, url, self.proxy.as_deref()).await {
            room.cover_url = Some(path.to_string_lossy().into_owned());
        }
    }

    fn soop_credentials(&self) -> Value {
        match (&self.soop_username, &self.soop_password) {
            (Some(username), Some(password)) => json!({"username": username, "password": password}),
            _ => Value::Null,
        }
    }

    pub async fn start_recording(
        &self,
        url: &str,
        config: RecordConfig,
    ) -> RecorderResult<RecordingHandle> {
        self.start_recording_with_cookies(url, config, &[]).await
    }

    pub async fn start_recording_with_cookies(
        &self,
        url: &str,
        config: RecordConfig,
        cookies: &[PlatformCookie],
    ) -> RecorderResult<RecordingHandle> {
        self.start_recording_with_cookies_and_hint(url, config, cookies, None)
            .await
    }

    /// 录制页可复用最近一次房态检查得到的 SOOP 频道元数据，跳过重复查询。
    pub async fn start_recording_with_cookies_and_hint(
        &self,
        url: &str,
        mut config: RecordConfig,
        cookies: &[PlatformCookie],
        soop_hint: Option<Value>,
    ) -> RecorderResult<RecordingHandle> {
        let normalized = room_url(url)?;
        let url = normalized.as_str();
        if is_douyin_url(url) {
            let handle = self
                .douyin_native
                .start_recording_with_cookies(url, config, cookies)
                .await?;
            return Ok(RecordingHandle::native(handle));
        }

        let runtime = streamlink_runtime::ensure().await?;
        let credentials = self.soop_credentials();
        self.apply_proxy(&mut config);
        if config.segment_duration.is_some() || config.include_danmaku {
            return Err(RecorderError::ConfigError(
                "当前录制后端暂未实现定时分文件或弹幕录制，请关闭相关选项".into(),
            ));
        }
        if !["ts", "mp4", "mkv", "flv"].contains(&config.format.as_str()) {
            return Err(RecorderError::ConfigError(
                "Streamlink 支持 ts/mp4/mkv/flv 输出".into(),
            ));
        }
        validate_output_extension(Path::new(&config.output_path_template), &config.format)?;
        let ffmpeg = magekit_shared::resolve_ffmpeg_path().ok_or(RecorderError::FFmpegNotFound)?;
        // 录制页已经生成了唯一的绝对文件路径。SOOP 全量 probe 会逐个请求
        // 所有清晰度的授权信息，而且 worker 开始拉流时还要再解析一次；GUI
        // 录制直接交给 worker 按所选清晰度解析，避免启动前等待数十秒。
        // 使用模板路径的库调用仍保留原 probe，以便拿到主播名等替换字段。
        let fast_soop_path = if is_soop_url(url) {
            concrete_output_path(&config)
        } else {
            None
        };
        let info = if fast_soop_path.is_some() {
            None
        } else {
            let info = probe(&runtime, url, &config, cookies, &credentials, false, false).await?;
            if info.room.status != crate::types::LiveStatus::Live {
                return Err(RecorderError::StreamNotAvailable("直播间未开播".into()));
            }
            Some(info)
        };
        let path = match (fast_soop_path, info.as_ref()) {
            (Some(path), _) => path,
            (None, Some(info)) => output_path(&info.room, &config)?,
            (None, None) => {
                unreachable!("probe result is present when no concrete SOOP path exists")
            }
        };
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        // Refuse collisions rather than overwriting or returning a different path than the GUI expects.
        let reserved = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await?;
        drop(reserved);
        // SOOP 的 HLS 流还携带插件生成的 aid 请求参数；不能只复用裸 HLS URL，
        // 否则重建流时会丢掉认证参数，导致预检成功但录制拉片段失败。
        let direct_ts = is_soop_url(url) && config.format == "ts";
        if direct_ts {
            tracing::info!("🎥 SOOP 使用 Streamlink 插件流对象直写 TS，保留播放授权参数");
        }
        // 将 PathBuf 显式转换成 Unicode 字符串传给 Python，避免跨语言路径编码差异。
        let output_path = path.to_string_lossy().into_owned();
        let request = json!({"mode":"record", "url":url, "config":config, "cookies":scoped_cookies(url, cookies),
                             "soop_credentials":credentials,
                             "output":output_path, "ffmpeg":ffmpeg, "direct_ts":direct_ts,
                             "soop_hint":soop_hint});
        let start = chrono::Utc::now();
        let initial = RecordProgress {
            status: RecordStatus::Connecting,
            start_time: Some(start),
            duration: 0,
            size: 0,
            speed: 0,
            error: None,
        };
        let (tx, rx) = watch::channel(initial.clone());
        let (stop, stop_rx) = oneshot::channel();
        let cleanup_path = path.clone();
        let diagnostic_room_id = soop_broadcast_id(url);
        if let Some(room_id) = diagnostic_room_id.as_deref() {
            tracing::info!(target: "magekit_record_diagnostics", event = "start", stage = "record", room_id);
        } else {
            tracing::info!(target: "magekit_record_diagnostics", event = "start", stage = "record");
        }
        let task = tokio::spawn(async move {
            let mut latest = initial;
            let result = supervise(runtime, request, stop_rx, &tx, &mut latest).await;
            // 只清理没有媒体数据的常规文件；有数据的部分录制始终保留。
            let output_metadata = tokio::fs::symlink_metadata(&cleanup_path).await.ok();
            let is_reserved_file = output_metadata
                .as_ref()
                .is_some_and(|metadata| metadata.is_file());
            let recorded_bytes = if is_reserved_file {
                output_metadata
                    .as_ref()
                    .map_or(latest.size, |metadata| metadata.len())
            } else {
                latest.size
            };
            if is_reserved_file && recorded_bytes == 0 {
                if let Err(cleanup_error) = tokio::fs::remove_file(&cleanup_path).await {
                    tracing::warn!("⚠️ 无法清理空录制占位文件: {cleanup_error}");
                }
            }
            if let Err(ref error) = result {
                let mut error_message = match error {
                    RecorderError::RecordingError(message) => message.clone(),
                    other => other.to_string(),
                };
                if is_reserved_file && recorded_bytes > 0 {
                    error_message.push_str(&format!(
                        "；已保留部分录制文件（{:.1} MiB）",
                        recorded_bytes as f64 / (1024.0 * 1024.0)
                    ));
                }
                let reason = safe_recording_error_reason(error);
                if let Some(room_id) = diagnostic_room_id.as_deref() {
                    tracing::error!(
                        target: "magekit_record_diagnostics",
                        event = "error", stage = "record", reason, room_id,
                        bytes = recorded_bytes, duration_secs = latest.duration,
                    );
                } else {
                    tracing::error!(
                        target: "magekit_record_diagnostics",
                        event = "error", stage = "record", reason,
                        bytes = recorded_bytes, duration_secs = latest.duration,
                    );
                }
                tracing::error!("❌ Streamlink 录制后台失败: {error_message}");
                latest.status = RecordStatus::Error(error_message.clone());
                latest.error = Some(error_message);
                latest.speed = 0;
                let _ = tx.send(latest);
            }
            result
        });
        Ok(RecordingHandle {
            running: Some(Running::Streamlink {
                stop: Some(stop),
                task,
                progress: rx,
            }),
            output_path: path,
            status: RecordStatus::Connecting,
            completion_error: None,
        })
    }

    /// 在后台为手动录制预取 SOOP 所选清晰度的播放授权；仅返回到本次运行内存。
    pub async fn prepare_soop_stream_with_cookies(
        &self,
        url: &str,
        quality: VideoQuality,
        cookies: &[PlatformCookie],
        mut soop_hint: Value,
    ) -> RecorderResult<Value> {
        let normalized = room_url(url)?;
        let url = normalized.as_str();
        if !is_soop_url(url) || !soop_hint.is_object() {
            return Err(RecorderError::ConfigError("SOOP 预热参数无效".into()));
        }
        let runtime = streamlink_runtime::ensure().await?;
        let mut config = RecordConfig {
            quality,
            timeout: 20,
            ..RecordConfig::default()
        };
        self.apply_proxy(&mut config);
        let request = json!({"mode":"prepare", "url":url, "config":config,
                             "cookies":scoped_cookies(url, cookies),
                             "soop_credentials":self.soop_credentials(), "soop_hint":soop_hint});
        let mut command = streamlink_runtime::worker(&runtime)?;
        let mut child = command.spawn()?;
        let pid = child.id();
        let mut guard = ProcessGuard(pid);
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| RecorderError::RecordingError("SOOP 预热 worker 缺少输入管道".into()))?;
        input.write_all(format!("{}\n", request).as_bytes()).await?;
        drop(input);
        let output = tokio::time::timeout(Duration::from_secs(30), child.wait_with_output()).await;
        cleanup_group(pid).await;
        guard.0 = None;
        let output = output.map_err(|_| RecorderError::NetworkTimeout)??;
        if !output.status.success() {
            return Err(RecorderError::RecordingError(
                "SOOP 播放流预热未完成".into(),
            ));
        }
        let event = output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
            .find(|value| value["kind"] == "prepared")
            .ok_or_else(|| RecorderError::RecordingError("SOOP 预热响应无效".into()))?;
        let prepared = event["prepared"].clone();
        if !prepared.is_object() {
            return Err(RecorderError::RecordingError("SOOP 预热响应无效".into()));
        }
        soop_hint["prepared"] = prepared;
        Ok(soop_hint)
    }

    pub async fn check_room_status(&self, url: &str) -> RecorderResult<LiveRoomInfo> {
        self.check_room_status_with_cookies(url, &[]).await
    }
    pub async fn check_room_status_with_cookies(
        &self,
        url: &str,
        cookies: &[PlatformCookie],
    ) -> RecorderResult<LiveRoomInfo> {
        self.check_room_status_with_cookies_and_cover(url, cookies, true)
            .await
    }

    /// 检查直播间状态；`fetch_cover` 为 false 时跳过额外的直播页封面请求。
    pub async fn check_room_status_with_cookies_and_cover(
        &self,
        url: &str,
        cookies: &[PlatformCookie],
        fetch_cover: bool,
    ) -> RecorderResult<LiveRoomInfo> {
        let normalized = room_url(url)?;
        let url = normalized.as_str();
        if is_douyin_url(url) {
            let mut room = self
                .douyin_native
                .check_room_status_with_cookies(url, cookies)
                .await?;
            if fetch_cover {
                self.cache_douyin_room_cover(url, &mut room).await;
            } else {
                // 首次获取后沿用状态缓存中的本地封面，避免每轮轮询都请求图片 CDN。
                room.cover_url = None;
            }
            return Ok(room);
        }

        let runtime = streamlink_runtime::ensure().await?;
        let credentials = self.soop_credentials();
        let mut config = RecordConfig::default();
        // SOOP 房态查询只需频道元数据，不需要逐个解析清晰度；给状态探测更短的网络超时。
        if url
            .parse::<url::Url>()
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_owned))
            .is_some_and(|host| is_soop_host(&host))
        {
            config.timeout = 10;
        }
        self.apply_proxy(&mut config);
        probe(
            &runtime,
            url,
            &config,
            cookies,
            &credentials,
            fetch_cover,
            true,
        )
        .await
        .map(|info| info.room)
    }
    pub async fn get_stream_info(&self, url: &str) -> RecorderResult<StreamInfo> {
        self.get_stream_info_with_cookies(url, &[]).await
    }
    pub async fn get_stream_info_with_cookies(
        &self,
        url: &str,
        cookies: &[PlatformCookie],
    ) -> RecorderResult<StreamInfo> {
        let normalized = room_url(url)?;
        let url = normalized.as_str();
        if is_douyin_url(url) {
            let mut info = self
                .douyin_native
                .get_stream_info_with_cookies(url, cookies)
                .await?;
            self.cache_douyin_room_cover(url, &mut info.room).await;
            return Ok(info);
        }

        let runtime = streamlink_runtime::ensure().await?;
        let credentials = self.soop_credentials();
        let mut config = RecordConfig::default();
        self.apply_proxy(&mut config);
        probe(&runtime, url, &config, cookies, &credentials, true, false).await
    }
}

fn is_douyin_url(value: &str) -> bool {
    let Ok(parsed) = url::Url::parse(value) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    host == "douyin.com"
        || host.ends_with(".douyin.com")
        || host == "iesdouyin.com"
        || host.ends_with(".iesdouyin.com")
}

fn room_url(value: &str) -> RecorderResult<String> {
    let normalized = magekit_shared::utils::normalize_url(value);
    let parsed = url::Url::parse(&normalized)?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(RecorderError::InvalidUrlFormat(
            "Only HTTP(S) room URLs without embedded credentials are accepted".into(),
        ));
    }
    Ok(parsed.to_string())
}

fn scoped_cookies<'a>(url: &str, cookies: &'a [PlatformCookie]) -> Vec<&'a PlatformCookie> {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default();
    cookies
        .iter()
        .filter(|cookie| {
            if !cookie.enabled {
                return false;
            }
            let key = cookie.platform.trim().to_ascii_lowercase();
            let soop_global_cookie = key == "soop_global";
            let soop_korean_cookie = matches!(key.as_str(), "soop" | "sooplive");
            let domain = match key.as_str() {
                "douyin" => "douyin.com".to_owned(),
                "bilibili" => "bilibili.com".to_owned(),
                "huya" => "huya.com".to_owned(),
                "douyu" => "douyu.com".to_owned(),
                "kuaishou" => "kuaishou.com".to_owned(),
                "twitch" => "twitch.tv".to_owned(),
                "soop" | "sooplive" => "sooplive.co.kr".to_owned(),
                "soop_global" => "sooplive.com".to_owned(),
                "afreeca" => "afreecatv.com".to_owned(),
                _ => {
                    let url = if key.contains("://") {
                        key
                    } else {
                        format!("https://{key}")
                    };
                    let Ok(parsed) = url::Url::parse(&url) else {
                        return false;
                    };
                    if !matches!(parsed.path(), "" | "/") {
                        return false;
                    }
                    parsed.host_str().unwrap_or("").to_owned()
                }
            };
            let matches_domain = host == domain || host.ends_with(&format!(".{domain}"));
            // Streamlink 的 SOOP 插件即使处理韩国房间，也会把认证和直播 API
            // 请求发往 sooplive.com。允许用户明确选择的 SOOP Cookie 进入该插件，
            // worker 会将 Cookie 限定在 SOOP 自有域名，不会发往其它平台。
            let soop_plugin_auth =
                (soop_global_cookie || soop_korean_cookie) && is_soop_host(&host);
            domain.contains('.') && (matches_domain || soop_plugin_auth)
        })
        .collect()
}

fn is_soop_host(host: &str) -> bool {
    ["sooplive.com", "sooplive.co.kr", "afreecatv.com"]
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

fn is_soop_url(value: &str) -> bool {
    url::Url::parse(value)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_owned))
        .is_some_and(|host| is_soop_host(&host))
}

/// 仅将 SOOP 地址末段的纯数字播号用于关联诊断事件，不持久化主播名或完整地址。
fn soop_broadcast_id(value: &str) -> Option<String> {
    let parsed = url::Url::parse(value).ok()?;
    if !parsed.host_str().is_some_and(is_soop_host) {
        return None;
    }
    let last = parsed
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .last()?;
    if last.len() <= 18 && last.bytes().all(|byte| byte.is_ascii_digit()) {
        Some(last.to_owned())
    } else {
        None
    }
}

async fn probe(
    runtime: &RuntimeInfo,
    url: &str,
    config: &RecordConfig,
    cookies: &[PlatformCookie],
    soop_credentials: &Value,
    fetch_cover: bool,
    status_only: bool,
) -> RecorderResult<StreamInfo> {
    let cover_cache = if fetch_cover {
        Some(cover_cache_path(url)?)
    } else {
        None
    };
    let request = json!({"mode":"probe", "url":url, "config":config, "fetch_cover":fetch_cover,
                         "status_only":status_only,
                         "cover_cache_path":cover_cache,
                         "cookies":scoped_cookies(url, cookies),
                         "soop_credentials":soop_credentials,
                         "ffmpeg":magekit_shared::resolve_ffmpeg_path()});
    let mut command = streamlink_runtime::worker(runtime)?;
    command.stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let pid = child.id();
    let mut guard = ProcessGuard(pid);
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| RecorderError::RecordingError("Worker stdin missing".into()))?;
    input.write_all(format!("{}\n", request).as_bytes()).await?;
    drop(input);
    let output = tokio::time::timeout(Duration::from_secs(90), child.wait_with_output()).await;
    cleanup_group(pid).await;
    guard.0 = None;
    let output = output.map_err(|_| RecorderError::NetworkTimeout)??;
    let event = output
        .stdout
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .rev()
        .find_map(|line| {
            serde_json::from_slice::<Value>(line)
                .ok()
                .filter(|event| event["kind"].is_string())
        })
        .ok_or_else(|| RecorderError::RecordingError(invalid_probe_response(&output)))?;
    let message = event["message"]
        .as_str()
        .unwrap_or("Streamlink probe failed")
        .to_owned();
    match event["kind"].as_str() {
        Some("unsupported") => Err(RecorderError::UnsupportedPlatform(message)),
        Some("probe") if output.status.success() => Ok(StreamInfo {
            room: serde_json::from_value(event["room"].clone())?,
            streams: serde_json::from_value(event["streams"].clone())?,
        }),
        _ if output.status.code() == Some(12) => {
            Err(RecorderError::AuthenticationRequired(message))
        }
        _ => Err(RecorderError::RecordingError(message)),
    }
}

fn invalid_probe_response(output: &Output) -> String {
    let exit_code = output.status.code();
    let exception = String::from_utf8_lossy(&output.stderr)
        .lines()
        .rev()
        .find_map(|line| {
            let prefix = line.trim().split_once(':')?.0;
            let kind = prefix.rsplit('.').next()?.trim();
            (!kind.is_empty()
                && kind
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_'))
            .then(|| kind.to_owned())
        });
    let exception = exception
        .map(|kind| format!(", Python exception: {kind}"))
        .unwrap_or_default();
    format!(
        "Invalid Streamlink probe response (exit code: {exit_code:?}, stdout: {} bytes{exception})",
        output.stdout.len()
    )
}

fn cover_cache_path(url: &str) -> RecorderResult<PathBuf> {
    let app_data = magekit_shared::utils::get_app_data_dir()
        .map_err(|e| RecorderError::ConfigError(e.to_string()))?;
    let directory = app_data.join("live_covers");
    std::fs::create_dir_all(&directory)?;
    let key = hex::encode(Md5::digest(url.as_bytes()));
    Ok(directory.join(format!("{key}.img")))
}

fn is_douyin_image_url(value: &str) -> bool {
    let Ok(parsed) = url::Url::parse(value) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    matches!(parsed.scheme(), "http" | "https")
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && ["douyinpic.com", "byteimg.com"]
            .iter()
            .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

async fn cache_douyin_image(
    image_url: &str,
    room_url: &str,
    proxy: Option<&str>,
) -> Option<PathBuf> {
    if !is_douyin_image_url(image_url) {
        return None;
    }
    let path = cover_cache_path(image_url).ok()?;
    if tokio::fs::metadata(&path)
        .await
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
    {
        return Some(path);
    }

    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/131.0 Safari/537.36");
    if let Some(proxy_url) = proxy.and_then(|value| crate::proxy::resolve_proxy_url(Some(value))) {
        let proxy = reqwest::Proxy::all(proxy_url).ok()?;
        builder = builder.no_proxy().proxy(proxy);
    }
    let client = builder.build().ok()?;
    let response = client
        .get(image_url)
        .header(reqwest::header::REFERER, room_url)
        .send()
        .await
        .ok()?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length > 8 * 1024 * 1024)
    {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    let valid_image = bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"\xff\xd8\xff")
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"));
    if bytes.is_empty() || bytes.len() > 8 * 1024 * 1024 || !valid_image {
        return None;
    }

    let staged = path.with_file_name(format!(
        "{}.{}.tmp",
        path.file_name()?.to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    tokio::fs::write(&staged, &bytes).await.ok()?;
    if tokio::fs::rename(&staged, &path).await.is_err() {
        let _ = tokio::fs::remove_file(&staged).await;
        if tokio::fs::metadata(&path)
            .await
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
        {
            return Some(path);
        }
        return None;
    }
    Some(path)
}

fn safe_component(value: &str) -> String {
    let value: String = value
        .chars()
        .take(120)
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let value = value.trim().trim_matches('.');
    let base = value.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if value.is_empty() {
        "unknown".into()
    } else if reserved.contains(&base.as_str()) {
        format!("_{value}")
    } else {
        value.to_owned()
    }
}

fn output_path(room: &LiveRoomInfo, config: &RecordConfig) -> RecorderResult<PathBuf> {
    let platform = room
        .extra
        .get("streamlink_plugin")
        .and_then(Value::as_str)
        .unwrap_or("streamlink");
    let path = config
        .output_path_template
        .replace("{platform}", &safe_component(platform))
        .replace("{anchor_name}", &safe_component(&room.anchor_name))
        .replace("{room_id}", &safe_component(&room.room_id))
        .replace("{title}", &safe_component(&room.title))
        .replace("{quality}", &format!("{:?}", config.quality))
        .replace(
            "{timestamp}",
            &chrono::Utc::now().format("%Y%m%d_%H%M%S_%3f").to_string(),
        );
    let path = PathBuf::from(path);
    validate_output_extension(&path, &config.format)?;
    Ok(if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    })
}

fn concrete_output_path(config: &RecordConfig) -> Option<PathBuf> {
    let raw = &config.output_path_template;
    if raw.contains('{') || raw.contains('}') {
        return None;
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() || validate_output_extension(&path, &config.format).is_err() {
        return None;
    }
    Some(path)
}

fn validate_output_extension(path: &Path, format: &str) -> RecorderResult<()> {
    if path.extension().and_then(|v| v.to_str()) != Some(format) {
        return Err(RecorderError::ConfigError(
            "输出文件扩展名必须与录制格式一致".into(),
        ));
    }
    Ok(())
}

// Fallback for cancellation of the Rust future itself (runtime shutdown/task abort).
struct ProcessGuard(Option<u32>);
impl Drop for ProcessGuard {
    fn drop(&mut self) {
        let Some(pid) = self.0 else { return };
        #[cfg(unix)]
        {
            let _ = magekit_shared::create_command("/bin/kill")
                .args(["-KILL", "--", &format!("-{pid}")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
        #[cfg(windows)]
        {
            let _ = magekit_shared::create_command("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
    }
}

/// The worker is in its own Unix process group. This also reaps orphaned media subprocesses.
async fn cleanup_group(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    #[cfg(unix)]
    {
        let mut cmd = magekit_shared::create_tokio_command("/bin/kill");
        let _ = cmd
            .args(["-KILL", "--", &format!("-{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    #[cfg(windows)]
    {
        let mut cmd = magekit_shared::create_tokio_command("taskkill");
        let _ = cmd
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
}

/// 诊断文件只接收有限枚举和数值，绝不把 worker 的任意文本写入磁盘。
fn log_worker_diagnostic(value: &Value, room_id: Option<&str>) {
    let event = match value["event"].as_str() {
        Some("start") => "start",
        Some("retry") => "retry",
        Some("error") => "error",
        Some("finished") => "finished",
        Some("stopped") => "stopped",
        Some("worker_error") => "worker_error",
        _ => "worker_error",
    };
    let stage = match value["stage"].as_str() {
        Some("probe") => "probe",
        Some("resolve") => "resolve",
        Some("select") => "select",
        Some("open") => "open",
        Some("read") => "read",
        Some("write") => "write",
        Some("record") => "record",
        Some("monitor") => "monitor",
        Some("finish") => "finish",
        _ => "record",
    };
    let reason = match value["reason"].as_str() {
        Some("auth") => "auth",
        Some("timeout") => "timeout",
        Some("offline") => "offline",
        Some("network") => "network",
        Some("plugin") => "plugin",
        Some("io") => "io",
        Some("process") => "process",
        Some("stalled") => "stalled",
        Some("no_media") => "no_media",
        Some("source_failed") => "source_failed",
        _ => "unknown",
    };
    let attempt = value["attempt"]
        .as_u64()
        .filter(|v| *v <= 1_000)
        .unwrap_or(0);
    let bytes = value["bytes"]
        .as_u64()
        .filter(|v| *v <= 1_000_000_000_000_000)
        .unwrap_or(0);
    let duration_secs = value["duration_secs"]
        .as_u64()
        .filter(|v| *v <= 366 * 24 * 60 * 60)
        .unwrap_or(0);
    let exit_code = value["exit_code"]
        .as_i64()
        .filter(|v| i32::try_from(*v).is_ok());
    let http_status = value["http_status"]
        .as_u64()
        .filter(|v| (100..=599).contains(v));

    if let Some(room_id) = room_id {
        match (exit_code, http_status) {
            (Some(exit_code), Some(http_status)) => tracing::info!(
                target: "magekit_record_diagnostics",
                event, stage, reason, room_id, attempt, bytes, duration_secs, exit_code, http_status,
            ),
            (Some(exit_code), None) => tracing::info!(
                target: "magekit_record_diagnostics",
                event, stage, reason, room_id, attempt, bytes, duration_secs, exit_code,
            ),
            (None, Some(http_status)) => tracing::info!(
                target: "magekit_record_diagnostics",
                event, stage, reason, room_id, attempt, bytes, duration_secs, http_status,
            ),
            (None, None) => tracing::info!(
                target: "magekit_record_diagnostics",
                event, stage, reason, room_id, attempt, bytes, duration_secs,
            ),
        }
    } else {
        match (exit_code, http_status) {
            (Some(exit_code), Some(http_status)) => tracing::info!(
                target: "magekit_record_diagnostics",
                event, stage, reason, attempt, bytes, duration_secs, exit_code, http_status,
            ),
            (Some(exit_code), None) => tracing::info!(
                target: "magekit_record_diagnostics",
                event, stage, reason, attempt, bytes, duration_secs, exit_code,
            ),
            (None, Some(http_status)) => tracing::info!(
                target: "magekit_record_diagnostics",
                event, stage, reason, attempt, bytes, duration_secs, http_status,
            ),
            (None, None) => tracing::info!(
                target: "magekit_record_diagnostics",
                event, stage, reason, attempt, bytes, duration_secs,
            ),
        }
    }
}

fn safe_recording_error_reason(error: &RecorderError) -> &'static str {
    match error {
        RecorderError::AuthenticationFailed(_) | RecorderError::AuthenticationRequired(_) => "auth",
        RecorderError::NetworkTimeout | RecorderError::RecordingStartupTimeout { .. } => "timeout",
        RecorderError::StreamNotAvailable(_) => "offline",
        RecorderError::HttpError(_) | RecorderError::ProxyError(_) => "network",
        RecorderError::IoError(_) => "io",
        RecorderError::RecordingStalled { .. } => "stalled",
        RecorderError::FFmpegNotFound => "process",
        RecorderError::RecordingError(_) => "source_failed",
        _ => "unknown",
    }
}

async fn supervise(
    runtime: RuntimeInfo,
    request: Value,
    mut stop: oneshot::Receiver<()>,
    progress: &watch::Sender<RecordProgress>,
    latest: &mut RecordProgress,
) -> RecorderResult<()> {
    let diagnostic_room_id = request["url"].as_str().and_then(soop_broadcast_id);
    let mut child = streamlink_runtime::worker(&runtime)?.spawn()?;
    let pid = child.id();
    let mut guard = ProcessGuard(pid);
    let result = async {
        let mut input = child.stdin.take().ok_or_else(|| RecorderError::RecordingError("Worker stdin missing".into()))?;
        input.write_all(format!("{}\n", request).as_bytes()).await?;
        let stdout = child.stdout.take().ok_or_else(|| RecorderError::RecordingError("Worker stdout missing".into()))?;
        let mut lines = BufReader::new(stdout).lines();
        let mut stopping: Option<Instant> = None;
        let mut heartbeat = Instant::now();
        let mut finished = false;
        let mut output_verified = false;
        let mut selected_quality: Option<String> = None;
        loop {
            tokio::select! {
                _ = &mut stop, if stopping.is_none() => {
                    stopping = Some(Instant::now());
                    let _ = input.write_all(b"stop\n").await;
                    let _ = input.shutdown().await;
                }
                line = lines.next_line() => {
                    let Some(line) = line? else { break };
                    heartbeat = Instant::now();
                    let event: Value = serde_json::from_str(&line)
                        .map_err(|_| RecorderError::RecordingError("Invalid Streamlink progress response".into()))?;
                    if event["kind"] == "diagnostic" {
                        log_worker_diagnostic(&event, diagnostic_room_id.as_deref());
                        continue;
                    }
                    let message = event["message"].as_str().map(str::to_owned);
                    if let Some(quality) = event["quality"].as_str()
                        && quality.len() <= 160
                        && selected_quality.as_deref() != Some(quality)
                    {
                        tracing::info!("🎞️ Streamlink 录制实际选档: {quality}");
                        selected_quality = Some(quality.to_owned());
                    }
                    latest.status = match event["status"].as_str() {
                        Some("recording") => RecordStatus::Recording,
                        Some("connecting") => RecordStatus::Connecting,
                        Some("stopped") => RecordStatus::Stopped,
                        Some("completed") => RecordStatus::Completed,
                        Some("error") => RecordStatus::Error(message.clone().unwrap_or_else(|| "Streamlink recording failed".into())),
                        _ => return Err(RecorderError::RecordingError(message.unwrap_or_else(|| "Unexpected worker response".into()))),
                    };
                    latest.duration = event["duration"].as_u64().unwrap_or(latest.duration);
                    let reported_size = event["size"].as_u64().unwrap_or(latest.size);
                    if !output_verified && reported_size > 0 {
                        let expected_path = request["output"].as_str().ok_or_else(|| {
                            RecorderError::RecordingError("录制输出路径无效".into())
                        })?;
                        let actual_size = tokio::fs::metadata(expected_path)
                            .await
                            .map(|metadata| metadata.len())
                            .unwrap_or(0);
                        if actual_size < reported_size {
                            return Err(RecorderError::RecordingError(
                                "worker 报告已收到媒体，但目标文件未写入相应数据；请检查输出路径编码或权限".into(),
                            ));
                        }
                        output_verified = true;
                    }
                    latest.size = reported_size;
                    latest.speed = event["speed"].as_u64().unwrap_or(0);
                    latest.error = if matches!(latest.status, RecordStatus::Error(_)) { message } else { None };
                    if event["kind"] == "finished" {
                        finished = true;
                        // worker 的控制线程仍在读取 stdin。收到终态后主动发送 EOF，
                        // 避免 Python 在退出时带着阻塞的 daemon 线程结束进程。
                        let _ = input.shutdown().await;
                    }
                    let _ = progress.send(latest.clone());
                }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    if stopping.is_some_and(|time| time.elapsed() > Duration::from_secs(25)) {
                        return Err(RecorderError::RecordingError("停止超时：强制清理录制进程，保留现有文件".into()));
                    }
                    if heartbeat.elapsed() > Duration::from_secs(100) {
                        return Err(RecorderError::RecordingError("Streamlink worker stopped responding".into()));
                    }
                }
            }
        }
        let exit = tokio::time::timeout(Duration::from_secs(5), child.wait()).await
            .map_err(|_| RecorderError::RecordingError("Worker did not exit after finishing".into()))??;
        if !exit.success() || !finished {
            return Err(RecorderError::RecordingError(latest.error.clone().unwrap_or_else(|| "Streamlink worker exited unexpectedly".into())));
        }
        if let RecordStatus::Error(ref message) = latest.status {
            return Err(RecorderError::RecordingError(message.clone()));
        }
        Ok(())
    }.await;
    cleanup_group(pid).await;
    guard.0 = None;
    if result.is_err() {
        let _ = child.kill().await;
    }
    let _ = child.wait().await;
    result
}

enum Running {
    Native(legacy_recorder::RecordingHandle),
    Streamlink {
        stop: Option<oneshot::Sender<()>>,
        task: tokio::task::JoinHandle<RecorderResult<()>>,
        progress: watch::Receiver<RecordProgress>,
    },
}

pub struct RecordingHandle {
    running: Option<Running>,
    output_path: PathBuf,
    status: RecordStatus,
    completion_error: Option<String>,
}

impl RecordingHandle {
    fn native(handle: legacy_recorder::RecordingHandle) -> Self {
        Self {
            output_path: handle.output_path().to_owned(),
            status: handle.status().clone(),
            running: Some(Running::Native(handle)),
            completion_error: None,
        }
    }

    /// Returns ONLY after media subprocesses have stopped and the output has been finalized.
    pub async fn stop(&mut self) -> RecorderResult<()> {
        match self.running.as_mut() {
            Some(Running::Native(handle)) => handle.stop().await?,
            Some(Running::Streamlink { stop, .. }) => {
                if let Some(stop) = stop.take() {
                    let _ = stop.send(());
                }
            }
            None => {}
        }
        self.finish().await?;
        self.status = RecordStatus::Stopped;
        Ok(())
    }
    pub async fn get_progress(&mut self) -> Option<RecordProgress> {
        let result = match self.running.as_mut()? {
            Running::Native(handle) => handle.get_progress().await,
            Running::Streamlink { progress, .. } => {
                progress.changed().await.ok()?;
                Some(progress.borrow_and_update().clone())
            }
        };
        if let Some(ref update) = result {
            self.status = update.status.clone();
        }
        result
    }
    /// 等待后台 worker 完成，并将 worker 的最终错误同步到句柄状态。
    pub async fn wait_for_completion(&mut self) -> RecorderResult<()> {
        self.finish().await
    }
    async fn finish(&mut self) -> RecorderResult<()> {
        let mut completion_result = None;
        if let Some(running) = self.running.take() {
            let result = match running {
                Running::Native(handle) => handle.wait().await,
                Running::Streamlink {
                    task,
                    progress,
                    stop: _stop,
                } => {
                    // Keep the stop sender alive while awaiting natural completion.
                    let result = task
                        .await
                        .map_err(|_| RecorderError::TaskCancelled)
                        .and_then(|r| r);
                    self.status = progress.borrow().status.clone();
                    result
                }
            };
            if let Err(error) = result {
                let message = match &error {
                    RecorderError::RecordingError(message) => message.clone(),
                    other => other.to_string(),
                };
                self.completion_error = Some(message.clone());
                self.status = RecordStatus::Error(message);
                completion_result = Some(error);
            } else if !matches!(self.status, RecordStatus::Stopped) {
                self.status = RecordStatus::Completed;
            }
        }
        if let Some(error) = completion_result {
            return Err(error);
        }
        match &self.completion_error {
            Some(message) => Err(RecorderError::RecordingError(message.clone())),
            None => Ok(()),
        }
    }
    pub async fn wait(mut self) -> RecorderResult<()> {
        self.finish().await
    }
    pub fn output_path(&self) -> &Path {
        &self.output_path
    }
    pub fn status(&self) -> &RecordStatus {
        &self.status
    }
}

impl Drop for RecordingHandle {
    fn drop(&mut self) {
        match self.running.as_mut() {
            Some(Running::Native(_)) => {
                // Dropping the native handle closes its stop sender, ending its receive loop.
            }
            Some(Running::Streamlink { stop, .. }) => {
                if let Some(stop) = stop.take() {
                    let _ = stop.send(());
                }
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filename_components_cannot_escape_directory() {
        assert_eq!(safe_component("../a\\b:c\n"), "_a_b_c_");
        assert_eq!(safe_component("CON.txt"), "_CON.txt");
        assert_eq!(safe_component(".."), "unknown");
    }
    #[test]
    fn output_uses_plugin_not_hardcoded_douyin() {
        let room = LiveRoomInfo {
            room_id: "123".into(),
            anchor_name: "test".into(),
            title: "title".into(),
            status: crate::types::LiveStatus::Live,
            start_time: None,
            viewer_count: None,
            cover_url: None,
            extra: std::collections::HashMap::from([("streamlink_plugin".into(), json!("soop"))]),
        };
        let config = RecordConfig {
            output_path_template: "record/{platform}/{room_id}.ts".into(),
            format: "ts".into(),
            ..Default::default()
        };
        assert!(
            output_path(&room, &config)
                .unwrap()
                .ends_with("record/soop/123.ts")
        );
    }
}
