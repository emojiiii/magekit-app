use std::io;

use thiserror::Error;

/// 统一的下载错误类型
#[derive(Debug, Error, Clone)]
pub enum DownloadError {
    #[error("网络错误: {0}")]
    Network(String),
    #[error("I/O 错误: {0}")]
    Io(String),
    #[error("外部进程退出异常 code={code:?}, stderr={stderr}")]
    ProcessExit {
        code: Option<i32>,
        stderr: String,
    },
    #[error("请求超时")]
    Timeout,
    #[error("已取消")]
    Canceled,
    #[error("不支持的操作: {0}")]
    Unsupported(String),
    #[error("非法请求: {0}")]
    InvalidRequest(String),
    #[error("进度解析失败: {0}")]
    ProgressParse(String),
    #[error("内部错误: {0}")]
    Internal(String),
}

pub type DownloadResult<T> = Result<T, DownloadError>;

impl From<io::Error> for DownloadError {
    fn from(e: io::Error) -> Self {
        DownloadError::Io(e.to_string())
    }
}

impl From<reqwest::Error> for DownloadError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            DownloadError::Timeout
        } else {
            DownloadError::Network(e.to_string())
        }
    }
}

