//! Streamlink-first facade preserving the existing GUI/CLI recording API.
use crate::{
    error::{RecorderError, RecorderResult},
    legacy_recorder,
    platforms::PlatformFactory,
    streamlink_runtime::{self, RuntimeInfo},
    types::{LiveRoomInfo, RecordConfig, RecordProgress, RecordStatus, StreamInfo},
};
use magekit_shared::types::PlatformCookie;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{oneshot, watch},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecordingBackend {
    #[default]
    Auto,
    Streamlink,
    Native,
}

pub struct LiveRecorder {
    native: legacy_recorder::LiveRecorder,
    backend: RecordingBackend,
}

impl Default for LiveRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveRecorder {
    pub fn new() -> Self {
        let backend = match std::env::var("MAGEKIT_RECORDER_BACKEND").as_deref() {
            Ok("native" | "legacy") => RecordingBackend::Native,
            Ok("streamlink") => RecordingBackend::Streamlink,
            _ => RecordingBackend::Auto,
        };
        Self::with_backend(backend)
    }

    pub fn with_backend(backend: RecordingBackend) -> Self {
        Self {
            native: legacy_recorder::LiveRecorder::new(),
            backend,
        }
    }

    /// Custom factories retain their original semantics (including test/mock handlers).
    pub fn with_factory(factory: PlatformFactory) -> Self {
        Self {
            native: legacy_recorder::LiveRecorder::with_factory(factory),
            backend: RecordingBackend::Native,
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
        if self.backend == RecordingBackend::Native {
            return self
                .native
                .start_recording_with_cookies(url, config, cookies)
                .await
                .map(RecordingHandle::native);
        }
        let normalized = room_url(url)?;
        let url = normalized.as_str();
        let runtime = streamlink_runtime::ensure().await?;
        let info = match probe(&runtime, url, &config, cookies).await {
            Ok(info) => info,
            Err(RecorderError::UnsupportedPlatform(_))
                if self.backend == RecordingBackend::Auto =>
            {
                return self
                    .native
                    .start_recording_with_cookies(url, config, cookies)
                    .await
                    .map(RecordingHandle::native);
            }
            Err(e) => return Err(e), // Authentication, offline and network errors are NOT fallback signals.
        };
        if info.room.status != crate::types::LiveStatus::Live {
            return Err(RecorderError::StreamNotAvailable("直播间未开播".into()));
        }
        if config.segment_duration.is_some() || config.include_danmaku {
            return Err(RecorderError::ConfigError(
                "Streamlink 暂不支持定时分文件或弹幕；请关闭这些选项，或显式使用 native 后端"
                    .into(),
            ));
        }
        if !["ts", "mp4", "mkv", "flv"].contains(&config.format.as_str()) {
            return Err(RecorderError::ConfigError(
                "Streamlink 支持 ts/mp4/mkv/flv 输出".into(),
            ));
        }
        let ffmpeg = magekit_shared::resolve_ffmpeg_path().ok_or(RecorderError::FFmpegNotFound)?;
        let path = output_path(&info.room, &config)?;
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
        let request = json!({"mode":"record", "url":url, "config":config, "cookies":scoped_cookies(url, cookies),
                             "output":path, "ffmpeg":ffmpeg});
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
        let task = tokio::spawn(async move {
            let mut latest = initial;
            let result = supervise(runtime, request, stop_rx, &tx, &mut latest).await;
            if let Err(ref error) = result {
                latest.status = RecordStatus::Error(error.to_string());
                latest.error = Some(error.to_string());
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

    pub async fn check_room_status(&self, url: &str) -> RecorderResult<LiveRoomInfo> {
        self.check_room_status_with_cookies(url, &[]).await
    }
    pub async fn check_room_status_with_cookies(
        &self,
        url: &str,
        cookies: &[PlatformCookie],
    ) -> RecorderResult<LiveRoomInfo> {
        if self.backend == RecordingBackend::Native {
            return self
                .native
                .check_room_status_with_cookies(url, cookies)
                .await;
        }
        let normalized = room_url(url)?;
        let url = normalized.as_str();
        let runtime = streamlink_runtime::ensure().await?;
        match probe(&runtime, url, &RecordConfig::default(), cookies).await {
            Ok(info) => Ok(info.room),
            Err(RecorderError::UnsupportedPlatform(_))
                if self.backend == RecordingBackend::Auto =>
            {
                self.native
                    .check_room_status_with_cookies(url, cookies)
                    .await
            }
            Err(error) => Err(error),
        }
    }
    pub async fn get_stream_info(&self, url: &str) -> RecorderResult<StreamInfo> {
        self.get_stream_info_with_cookies(url, &[]).await
    }
    pub async fn get_stream_info_with_cookies(
        &self,
        url: &str,
        cookies: &[PlatformCookie],
    ) -> RecorderResult<StreamInfo> {
        if self.backend == RecordingBackend::Native {
            return self.native.get_stream_info_with_cookies(url, cookies).await;
        }
        let normalized = room_url(url)?;
        let url = normalized.as_str();
        let runtime = streamlink_runtime::ensure().await?;
        match probe(&runtime, url, &RecordConfig::default(), cookies).await {
            Err(RecorderError::UnsupportedPlatform(_))
                if self.backend == RecordingBackend::Auto =>
            {
                self.native.get_stream_info_with_cookies(url, cookies).await
            }
            result => result,
        }
    }
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
            domain.contains('.') && (host == domain || host.ends_with(&format!(".{domain}")))
        })
        .collect()
}

async fn probe(
    runtime: &RuntimeInfo,
    url: &str,
    config: &RecordConfig,
    cookies: &[PlatformCookie],
) -> RecorderResult<StreamInfo> {
    let request = json!({"mode":"probe", "url":url, "config":config, "cookies":scoped_cookies(url, cookies),
                         "ffmpeg":magekit_shared::resolve_ffmpeg_path()});
    let mut child = streamlink_runtime::worker(runtime).spawn()?;
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
    let event: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| RecorderError::RecordingError("Invalid Streamlink probe response".into()))?;
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
    if path.extension().and_then(|v| v.to_str()) != Some(config.format.as_str()) {
        return Err(RecorderError::ConfigError(
            "输出文件扩展名必须与录制格式一致".into(),
        ));
    }
    Ok(if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    })
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

async fn supervise(
    runtime: RuntimeInfo,
    request: Value,
    mut stop: oneshot::Receiver<()>,
    progress: &watch::Sender<RecordProgress>,
    latest: &mut RecordProgress,
) -> RecorderResult<()> {
    let mut child = streamlink_runtime::worker(&runtime).spawn()?;
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
                    let message = event["message"].as_str().map(str::to_owned);
                    latest.status = match event["status"].as_str() {
                        Some("recording") => RecordStatus::Recording,
                        Some("connecting") => RecordStatus::Connecting,
                        Some("stopped") => RecordStatus::Stopped,
                        Some("completed") => RecordStatus::Completed,
                        Some("error") => RecordStatus::Error(message.clone().unwrap_or_else(|| "Streamlink recording failed".into())),
                        _ => return Err(RecorderError::RecordingError(message.unwrap_or_else(|| "Unexpected worker response".into()))),
                    };
                    latest.duration = event["duration"].as_u64().unwrap_or(latest.duration);
                    latest.size = event["size"].as_u64().unwrap_or(latest.size);
                    latest.speed = event["speed"].as_u64().unwrap_or(0);
                    latest.error = if matches!(latest.status, RecordStatus::Error(_)) { message } else { None };
                    finished |= event["kind"] == "finished";
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
    async fn finish(&mut self) -> RecorderResult<()> {
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
                self.completion_error = Some(error.to_string());
                self.status = RecordStatus::Error(error.to_string());
            } else if !matches!(self.status, RecordStatus::Stopped) {
                self.status = RecordStatus::Completed;
            }
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
        if let Some(Running::Streamlink { stop, .. }) = self.running.as_mut() {
            if let Some(stop) = stop.take() {
                let _ = stop.send(());
            }
        }
        // Dropping a native handle closes its original stop sender as before.
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
