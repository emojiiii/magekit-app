use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("不支持的解析平台: {0}")]
    UnsupportedPlatform(String),
    #[error("缺少依赖工具: {0}")]
    MissingTool(String),
    #[error("命令执行失败: {0}")]
    CommandFailed(String),
    #[error("网络请求失败: {0}")]
    Network(String),
    #[error("解析响应失败: {0}")]
    Parse(String),
    #[error("无效的 URL: {0}")]
    InvalidUrl(String),
    #[error("未知错误: {0}")]
    Other(String),
}

pub type ExtractResult<T> = Result<T, ExtractError>;
