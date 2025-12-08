use thiserror::Error;

/// 录制器错误类型
#[derive(Error, Debug)]
pub enum RecorderError {
    #[error("URL parsing error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("HTTP request error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("JSON parse error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),

    #[error("Room not found: {0}")]
    RoomNotFound(String),

    #[error("Stream not available: {0}")]
    StreamNotAvailable(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Invalid URL format: {0}")]
    InvalidUrlFormat(String),

    #[error("Network timeout")]
    NetworkTimeout,

    #[error("Invalid response format: {0}")]
    InvalidResponseFormat(String),

    #[error("Platform specific error: {platform} - {message}")]
    PlatformError { platform: String, message: String },

    #[error("Recording error: {0}")]
    RecordingError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Task cancelled")]
    TaskCancelled,

    #[error("Proxy error: {0}")]
    ProxyError(String),

    #[error("FFmpeg not found or not executable")]
    FFmpegNotFound,

    #[error("X-Bogus signature generation failed: {0}")]
    XBogusError(String),

    #[error("Authentication required: {0}")]
    AuthenticationRequired(String),

    #[error("JavaScript execution error: {0}")]
    JavaScriptError(String),
}

pub type RecorderResult<T> = Result<T, RecorderError>;
