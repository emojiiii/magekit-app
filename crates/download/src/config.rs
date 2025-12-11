use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use url::Url;

/// 输出路径策略
#[derive(Clone, Debug)]
pub struct DownloadOutput {
    /// 输出目录
    pub directory: PathBuf,
    /// 文件模板，例如 "%(title)s.%(ext)s"
    pub template: Option<String>,
}

/// 断点续传/重试策略
#[derive(Clone, Debug)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 2,
            backoff: Duration::from_secs(2),
        }
    }
}

/// 下载方式
#[derive(Clone, Debug)]
pub enum DownloadStrategy {
    Auto,
    Direct,
    YtDlp,
    Ffmpeg,
    HlsDash,
    /// 自定义下载器名称（需注册）
    Custom(String),
}

/// 额外参数，用于透传给外部工具或下载器实现
#[derive(Clone, Debug, Default)]
pub struct DownloadExtra {
    /// 附加 HTTP headers
    pub headers: HashMap<String, String>,
    /// 附加 Cookie（格式 "k1=v1; k2=v2"）
    pub cookie: Option<String>,
    /// 附加查询参数
    pub query: Vec<(String, String)>,
    /// 外部工具自定义参数（按下载器名称区分）
    pub tool_args: HashMap<String, Vec<String>>,
}

/// 下载请求
#[derive(Clone, Debug)]
pub struct DownloadRequest {
    pub url: Url,
    pub strategy: DownloadStrategy,
    pub output: DownloadOutput,
    pub extra: DownloadExtra,
    pub timeout: Option<Duration>,
    pub bandwidth_limit: Option<u64>, // bytes/sec
    pub retries: RetryPolicy,
    pub resume: bool,
}

impl DownloadRequest {
    pub fn new(url: Url, output: DownloadOutput) -> Self {
        Self {
            url,
            strategy: DownloadStrategy::Auto,
            output,
            extra: DownloadExtra::default(),
            timeout: None,
            bandwidth_limit: None,
            retries: RetryPolicy::default(),
            resume: true,
        }
    }
}

