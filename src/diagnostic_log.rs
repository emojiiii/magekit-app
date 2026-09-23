//! 只将明确允许的录制诊断字段写入磁盘，避免把 Cookie 或签名流地址持久化。

use std::{
    fs::{File, OpenOptions},
    io::Write,
    sync::Mutex,
};

use anyhow::Result;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, layer::Context};

pub const TARGET: &str = "magekit_record_diagnostics";

const EVENTS: &[&str] = &[
    "app_start",
    "start",
    "retry",
    "error",
    "finished",
    "stopped",
    "probe_error",
    "worker_error",
];
const STAGES: &[&str] = &[
    "probe", "resolve", "select", "open", "read", "write", "record", "monitor", "finish",
];
const REASONS: &[&str] = &[
    "auth",
    "timeout",
    "offline",
    "network",
    "plugin",
    "io",
    "process",
    "stalled",
    "no_media",
    "source_failed",
    "unknown",
];

pub struct DiagnosticLogLayer {
    file: Mutex<File>,
}

impl DiagnosticLogLayer {
    pub fn new() -> Result<Self> {
        let path =
            magekit_shared::utils::get_log_dir()?.join(magekit_shared::constants::LOG_FILE_NAME);
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }
}

impl<S: Subscriber> Layer<S> for DiagnosticLogLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = SafeFields::default();
        event.record(&mut fields);
        let Some(kind) = fields.event else { return };

        let mut line = format!(
            "{} {} event={kind}",
            chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false),
            event.metadata().level()
        );
        if let Some(stage) = fields.stage {
            line.push_str(&format!(" stage={stage}"));
        }
        if let Some(reason) = fields.reason {
            line.push_str(&format!(" reason={reason}"));
        }
        if let Some(room_id) = fields.room_id {
            line.push_str(&format!(" room_id={room_id}"));
        }
        if let Some(attempt) = fields.attempt {
            line.push_str(&format!(" attempt={attempt}"));
        }
        if let Some(duration_secs) = fields.duration_secs {
            line.push_str(&format!(" duration_secs={duration_secs}"));
        }
        if let Some(bytes) = fields.bytes {
            line.push_str(&format!(" bytes={bytes}"));
        }
        if let Some(exit_code) = fields.exit_code {
            line.push_str(&format!(" exit_code={exit_code}"));
        }
        if let Some(http_status) = fields.http_status {
            line.push_str(&format!(" http_status={http_status}"));
        }
        line.push('\n');

        if let Ok(mut file) = self.file.lock() {
            // 诊断事件很少，但便携版可能长期开启；只保留最近的日志。
            if file
                .metadata()
                .is_ok_and(|metadata| metadata.len() > 8 * 1024 * 1024)
            {
                let _ = file.set_len(0);
            }
            let _ = file.write_all(line.as_bytes());
        }
    }
}

#[derive(Default)]
struct SafeFields {
    event: Option<&'static str>,
    stage: Option<&'static str>,
    reason: Option<&'static str>,
    room_id: Option<String>,
    attempt: Option<u64>,
    duration_secs: Option<u64>,
    bytes: Option<u64>,
    exit_code: Option<i64>,
    http_status: Option<u64>,
}

impl Visit for SafeFields {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "event" => self.event = EVENTS.iter().copied().find(|allowed| *allowed == value),
            "stage" => self.stage = STAGES.iter().copied().find(|allowed| *allowed == value),
            "reason" => self.reason = REASONS.iter().copied().find(|allowed| *allowed == value),
            "room_id"
                if !value.is_empty()
                    && value.len() <= 18
                    && value.bytes().all(|c| c.is_ascii_digit()) =>
            {
                self.room_id = Some(value.to_owned());
            }
            _ => {}
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        match field.name() {
            "attempt" => self.attempt = Some(value),
            "duration_secs" => self.duration_secs = Some(value),
            "bytes" => self.bytes = Some(value),
            "http_status" if (100..=599).contains(&value) => self.http_status = Some(value),
            _ => {}
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == "exit_code" {
            self.exit_code = Some(value);
        } else if value >= 0 {
            self.record_u64(field, value as u64);
        }
    }

    // 不接收 Debug 或其他任意文本字段；它们可能包含完整 URL、Cookie 或授权信息。
    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
}
