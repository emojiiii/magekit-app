pub mod core;
pub mod error;
pub mod platforms;
pub mod recorder;
pub mod stream;
pub mod types;

pub use core::LiveRecorder;
pub use error::{RecorderError, RecorderResult};
pub use types::*;
