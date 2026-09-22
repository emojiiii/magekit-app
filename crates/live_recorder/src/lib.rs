pub mod core;
pub mod error;
pub mod platforms;
pub mod recorder;
pub mod stream;
pub mod streamlink_runtime;
pub mod types;
// Preserve the previous backend for unsupported plugins and explicit rollback.
mod legacy_recorder;

pub use core::LiveRecorder;
pub use error::{RecorderError, RecorderResult};
pub use recorder::RecordingBackend;
pub use types::*;
