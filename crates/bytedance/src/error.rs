//! 错误类型定义

use thiserror::Error;

/// 字节跳动 API 错误类型
#[derive(Debug, Error)]
pub enum BdError {
    /// 网络请求错误
    #[error("网络请求失败: {0}")]
    Network(String),

    /// JSON 解析错误
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    /// URL 解析错误
    #[error("URL 解析失败: {0}")]
    UrlParse(#[from] url::ParseError),

    /// API 返回错误
    #[error("API 返回错误: status_code={status_code}, msg={msg}")]
    ApiError { status_code: i32, msg: String },

    /// 无效的 URL 格式
    #[error("无效的 URL 格式: {0}")]
    InvalidUrl(String),

    /// 缺少必要的数据
    #[error("缺少必要的数据: {0}")]
    MissingData(String),

    /// 签名错误
    #[error("签名生成失败: {0}")]
    SignError(String),

    /// 直播间未开播或不存在
    #[error("直播间未开播或不存在: {0}")]
    LiveNotAvailable(String),

    /// 视频不可用
    #[error("视频不可用: {0}")]
    VideoNotAvailable(String),

    /// 用户不存在
    #[error("用户不存在: {0}")]
    UserNotFound(String),

    /// 其他错误
    #[error("其他错误: {0}")]
    Other(String),
}

impl From<reqwest::Error> for BdError {
    fn from(e: reqwest::Error) -> Self {
        BdError::Network(e.to_string())
    }
}

/// 结果类型别名
pub type BdResult<T> = Result<T, BdError>;
