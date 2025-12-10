//! MageKit 全局常量定义。

use std::time::Duration;

/// 应用名称和版本
pub const APP_NAME: &str = "MageKit";
pub const APP_VERSION: &str = "0.1.0";

/// 默认配置文件名
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// 默认日志文件名
pub const LOG_FILE_NAME: &str = "magekit.log";

/// 默认超时时间
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// 默认重试次数
pub const DEFAULT_RETRY_TIMES: u32 = 3;

/// 默认最大并发下载数
pub const DEFAULT_MAX_CONCURRENT_DOWNLOADS: usize = 3;

/// 支持的视频网站域名
pub const SUPPORTED_DOMAINS: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "bilibili.com",
    "vimeo.com",
    "twitch.tv",
    "douyin.com",
    "tiktok.com",
    "twitter.com",
    "facebook.com",
    "instagram.com",
];

/// 工具下载信息。
pub mod tools {
    //! 工具下载相关常量。
    use std::time::Duration;

    /// yt-dlp 发布 API URL
    pub const YTDLP_API_URL: &str = "https://api.github.com/repos/yt-dlp/yt-dlp/releases";

    /// ffmpeg 官方下载页面
    pub const FFMPEG_DOWNLOAD_URL: &str = "https://ffmpeg.org/download.html";

    /// 默认工具版本检查间隔（小时）
    pub const VERSION_CHECK_INTERVAL_HOURS: u64 = 24;

    /// 工具下载超时时间
    pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300); // 5分钟
}

/// 文件大小单位。
pub mod file_size {
    //! 按二进制计算的文件大小常量。
    pub const KB: u64 = 1024;
    pub const MB: u64 = KB * 1024;
    pub const GB: u64 = MB * 1024;
    pub const TB: u64 = GB * 1024;
}

/// 进度更新间隔
pub const PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_millis(500);

/// 速度计算窗口大小（采样点数量）
pub const SPEED_CALCULATION_WINDOW: usize = 10;

/// UI 相关常量。
pub mod ui {
    //! UI 尺寸与展示的默认值。
    use std::time::Duration;

    /// 默认窗口尺寸
    pub const DEFAULT_WINDOW_WIDTH: u32 = 1200;
    pub const DEFAULT_WINDOW_HEIGHT: u32 = 800;
    pub const MIN_WINDOW_WIDTH: u32 = 800;
    pub const MIN_WINDOW_HEIGHT: u32 = 600;

    /// 任务列表每页显示的项目数
    pub const TASKS_PER_PAGE: usize = 50;

    /// 进度条更新频率
    pub const PROGRESS_BAR_UPDATE_INTERVAL: Duration = Duration::from_millis(100);

    /// 通知显示时长
    pub const NOTIFICATION_DURATION: Duration = Duration::from_secs(5);
}

/// 日志相关常量。
pub mod logging {
    //! 日志滚动相关参数。
    /// 最大日志文件大小 (10MB)
    pub const MAX_LOG_FILE_SIZE: u64 = 10 * super::file_size::MB;

    /// 保留的日志文件数量
    pub const MAX_LOG_FILES: usize = 5;
}

/// URL 验证相关常量。
pub mod url_validation {
    //! URL 格式与协议校验参数。
    /// 最大 URL 长度
    pub const MAX_URL_LENGTH: usize = 2048;

    /// 支持的 URL 协议
    pub const SUPPORTED_PROTOCOLS: &[&str] = &["http", "https"];
}

/// 文件路径相关常量。
pub mod paths {
    //! 应用内部目录命名。
    /// 默认下载目录名
    pub const DEFAULT_DOWNLOAD_DIR: &str = "Downloads";

    /// 工具目录名
    pub const TOOLS_DIR_NAME: &str = "tools";

    /// 临时目录名
    pub const TEMP_DIR_NAME: &str = "temp";

    /// 日志目录名
    pub const LOG_DIR_NAME: &str = "logs";
}
