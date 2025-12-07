pub mod core;
pub mod platforms;
pub mod recorder;
pub mod stream;
pub mod error;
pub mod types;

pub use core::LiveRecorder;
pub use error::{RecorderError, RecorderResult};
pub use types::*;