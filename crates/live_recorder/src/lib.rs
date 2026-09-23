pub mod core;
pub mod error;
mod legacy_recorder;
pub mod platforms;
pub mod proxy;
pub mod recorder;
pub mod stream;
pub mod streamlink_runtime;
pub mod types;

pub use core::LiveRecorder;
pub use error::{RecorderError, RecorderResult};
pub use types::*;
