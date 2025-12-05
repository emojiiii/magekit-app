use thiserror::Error;

/// 工具管理器相关的错误类型
#[derive(Error, Debug)]
pub enum ToolManagerError {
    #[error("Tool not found: {tool}")]
    ToolNotFound { tool: String },

    #[error("Tool installation failed: {tool} - {reason}")]
    InstallationFailed { tool: String, reason: String },

    #[error("Tool update failed: {tool} - {reason}")]
    UpdateFailed { tool: String, reason: String },

    #[error("Tool version check failed: {tool} - {reason}")]
    VersionCheckFailed { tool: String, reason: String },

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization/deserialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),

    #[error("Process execution failed: {command} - {reason}")]
    ProcessFailed { command: String, reason: String },

    #[error("File operation failed: {operation} - {reason}")]
    FileOperationFailed { operation: String, reason: String },

    #[error("Permission denied: {operation}")]
    PermissionDenied { operation: String },

    #[error("Disk space insufficient: required {required}, available {available}")]
    InsufficientDiskSpace { required: u64, available: u64 },

    #[error("Unsupported platform: {platform}")]
    UnsupportedPlatform { platform: String },

    #[error("Timeout occurred: {operation}")]
    Timeout { operation: String },

    #[error("Internal error: {0}")]
    Internal(String),
}

/// 下载相关的错误类型
#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("Invalid URL: {url}")]
    InvalidUrl { url: String },

    #[error("Video not found: {url}")]
    VideoNotFound { url: String },

    #[error("Video extraction failed: {url} - {reason}")]
    ExtractionFailed { url: String, reason: String },

    #[error("Format not available: {format}")]
    FormatNotAvailable { format: String },

    #[error("Download failed: {url} - {reason}")]
    DownloadFailed { url: String, reason: String },

    #[error("Download cancelled: {url}")]
    DownloadCancelled { url: String },

    #[error("Download paused: {url}")]
    DownloadPaused { url: String },

    #[error("File operation failed: {operation} - {reason}")]
    FileOperationFailed { operation: String, reason: String },

    #[error("Insufficient disk space: required {required}, available {available}")]
    InsufficientDiskSpace { required: u64, available: u64 },

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Tool manager error: {0}")]
    ToolManager(#[from] ToolManagerError),

    #[error("Task not found: {task_id}")]
    TaskNotFound { task_id: uuid::Uuid },

    #[error("Task operation failed: {task_id} - {operation} - {reason}")]
    TaskOperationFailed {
        task_id: uuid::Uuid,
        operation: String,
        reason: String,
    },

    #[error("Timeout occurred: {operation}")]
    Timeout { operation: String },

    #[error("Internal error: {0}")]
    Internal(String),
}

impl ToolManagerError {
    /// 创建工具未找到错误
    pub fn tool_not_found<S: Into<String>>(tool: S) -> Self {
        Self::ToolNotFound {
            tool: tool.into(),
        }
    }

    /// 创建安装失败错误
    pub fn installation_failed<S: Into<String>, R: Into<String>>(tool: S, reason: R) -> Self {
        Self::InstallationFailed {
            tool: tool.into(),
            reason: reason.into(),
        }
    }

    /// 创建更新失败错误
    pub fn update_failed<S: Into<String>, R: Into<String>>(tool: S, reason: R) -> Self {
        Self::UpdateFailed {
            tool: tool.into(),
            reason: reason.into(),
        }
    }

    /// 创建版本检查失败错误
    pub fn version_check_failed<S: Into<String>, R: Into<String>>(tool: S, reason: R) -> Self {
        Self::VersionCheckFailed {
            tool: tool.into(),
            reason: reason.into(),
        }
    }

    /// 创建配置错误
    pub fn config<S: Into<String>>(msg: S) -> Self {
        Self::Config(msg.into())
    }

    /// 创建进程执行失败错误
    pub fn process_failed<S: Into<String>, R: Into<String>>(command: S, reason: R) -> Self {
        Self::ProcessFailed {
            command: command.into(),
            reason: reason.into(),
        }
    }

    /// 创建文件操作失败错误
    pub fn file_operation_failed<S: Into<String>, R: Into<String>>(operation: S, reason: R) -> Self {
        Self::FileOperationFailed {
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    /// 创建权限拒绝错误
    pub fn permission_denied<S: Into<String>>(operation: S) -> Self {
        Self::PermissionDenied {
            operation: operation.into(),
        }
    }

    /// 创建磁盘空间不足错误
    pub fn insufficient_disk_space(required: u64, available: u64) -> Self {
        Self::InsufficientDiskSpace { required, available }
    }

    /// 创建不支持平台错误
    pub fn unsupported_platform<S: Into<String>>(platform: S) -> Self {
        Self::UnsupportedPlatform {
            platform: platform.into(),
        }
    }

    /// 创建超时错误
    pub fn timeout<S: Into<String>>(operation: S) -> Self {
        Self::Timeout {
            operation: operation.into(),
        }
    }

    /// 创建内部错误
    pub fn internal<S: Into<String>>(msg: S) -> Self {
        Self::Internal(msg.into())
    }
}

impl DownloadError {
    /// 创建无效URL错误
    pub fn invalid_url<S: Into<String>>(url: S) -> Self {
        Self::InvalidUrl { url: url.into() }
    }

    /// 创建视频未找到错误
    pub fn video_not_found<S: Into<String>>(url: S) -> Self {
        Self::VideoNotFound { url: url.into() }
    }

    /// 创建视频提取失败错误
    pub fn extraction_failed<S: Into<String>, R: Into<String>>(url: S, reason: R) -> Self {
        Self::ExtractionFailed {
            url: url.into(),
            reason: reason.into(),
        }
    }

    /// 创建格式不可用错误
    pub fn format_not_available<S: Into<String>>(format: S) -> Self {
        Self::FormatNotAvailable {
            format: format.into(),
        }
    }

    /// 创建下载失败错误
    pub fn download_failed<S: Into<String>, R: Into<String>>(url: S, reason: R) -> Self {
        Self::DownloadFailed {
            url: url.into(),
            reason: reason.into(),
        }
    }

    /// 创建下载取消错误
    pub fn download_cancelled<S: Into<String>>(url: S) -> Self {
        Self::DownloadCancelled { url: url.into() }
    }

    /// 创建下载暂停错误
    pub fn download_paused<S: Into<String>>(url: S) -> Self {
        Self::DownloadPaused { url: url.into() }
    }

    /// 创建文件操作失败错误
    pub fn file_operation_failed<S: Into<String>, R: Into<String>>(operation: S, reason: R) -> Self {
        Self::FileOperationFailed {
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    /// 创建磁盘空间不足错误
    pub fn insufficient_disk_space(required: u64, available: u64) -> Self {
        Self::InsufficientDiskSpace { required, available }
    }

    /// 创建任务未找到错误
    pub fn task_not_found(task_id: uuid::Uuid) -> Self {
        Self::TaskNotFound { task_id }
    }

    /// 创建任务操作失败错误
    pub fn task_operation_failed<S: Into<String>, R: Into<String>>(
        task_id: uuid::Uuid,
        operation: S,
        reason: R,
    ) -> Self {
        Self::TaskOperationFailed {
            task_id,
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    /// 创建超时错误
    pub fn timeout<S: Into<String>>(operation: S) -> Self {
        Self::Timeout {
            operation: operation.into(),
        }
    }

    /// 创建内部错误
    pub fn internal<S: Into<String>>(msg: S) -> Self {
        Self::Internal(msg.into())
    }
}

/// 结果类型别名
pub type ToolManagerResult<T> = Result<T, ToolManagerError>;
pub type DownloadResult<T> = Result<T, DownloadError>;