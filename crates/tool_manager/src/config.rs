use crate::error::{ToolManagerError, ToolManagerResult};
use magekit_shared::{AppConfig, ToolsConfig, UpdateChannel, get_app_config_dir};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

/// 工具管理器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolManagerConfig {
    pub tools: ToolsConfig,
    pub download_defaults: DownloadDefaultsConfig,
    pub advanced: AdvancedConfig,
}

impl Default for ToolManagerConfig {
    fn default() -> Self {
        Self {
            tools: ToolsConfig::default(),
            download_defaults: DownloadDefaultsConfig::default(),
            advanced: AdvancedConfig::default(),
        }
    }
}

impl ToolManagerConfig {
    /// 验证配置（纯函数：不依赖磁盘与运行时）
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        // 验证并发下载数
        if self.download_defaults.max_concurrent_downloads == 0 {
            errors.push("Max concurrent downloads must be greater than 0".to_string());
        }

        // 验证超时时间
        if self.download_defaults.download_timeout_secs == 0 {
            errors.push("Download timeout must be greater than 0".to_string());
        }

        // 验证重试次数
        if self.download_defaults.retry_times > 10 {
            errors.push("Retry times should not exceed 10".to_string());
        }

        // 验证默认格式
        if self.download_defaults.default_format.is_empty() {
            errors.push("Default format cannot be empty".to_string());
        }

        errors
    }
}

/// 下载默认配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadDefaultsConfig {
    pub default_format: String,
    pub max_concurrent_downloads: usize,
    pub download_timeout_secs: u64,
    pub retry_times: u32,
}

impl Default for DownloadDefaultsConfig {
    fn default() -> Self {
        Self {
            default_format: "best".to_string(),
            max_concurrent_downloads: 3,
            download_timeout_secs: 300,
            retry_times: 3,
        }
    }
}

/// 高级配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedConfig {
    pub log_level: String,
    pub enable_debug: bool,
    pub cache_size_mb: u64,
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            enable_debug: false,
            cache_size_mb: 1024, // 1GB
        }
    }
}

/// 配置管理器
pub struct ConfigManager {
    config_path: PathBuf,
    config: ToolManagerConfig,
}

impl ConfigManager {
    /// 创建新的配置管理器 (同步版本)
    pub fn new_sync() -> ToolManagerResult<Self> {
        let config_dir =
            get_app_config_dir().map_err(|e| ToolManagerError::internal(e.to_string()))?;

        // 确保目录存在
        std::fs::create_dir_all(&config_dir).map_err(|e| {
            ToolManagerError::file_operation_failed("create config dir", e.to_string())
        })?;

        let config_path = config_dir.join("tool_manager.toml");

        let config = if config_path.exists() {
            Self::load_config_sync(&config_path)?
        } else {
            let default_config = ToolManagerConfig::default();
            Self::save_config_sync(&config_path, &default_config)?;
            default_config
        };

        Ok(Self {
            config_path,
            config,
        })
    }

    /// 创建新的配置管理器 (异步版本，保持兼容)
    pub async fn new() -> ToolManagerResult<Self> {
        Self::new_sync()
    }

    /// 保存配置到文件 (同步版本)
    fn save_config_sync(path: &PathBuf, config: &ToolManagerConfig) -> ToolManagerResult<()> {
        let toml_string = toml::to_string_pretty(config)
            .map_err(|e| ToolManagerError::config(format!("Failed to serialize config: {}", e)))?;

        std::fs::write(path, toml_string).map_err(|e| {
            ToolManagerError::file_operation_failed("write config file", e.to_string())
        })?;

        tracing::info!("Configuration saved to: {:?}", path);
        Ok(())
    }

    /// 从文件加载配置 (同步版本)
    fn load_config_sync(path: &PathBuf) -> ToolManagerResult<ToolManagerConfig> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            ToolManagerError::file_operation_failed("read config file", e.to_string())
        })?;

        let config: ToolManagerConfig = toml::from_str(&content)
            .map_err(|e| ToolManagerError::config(format!("Failed to parse config: {}", e)))?;

        tracing::info!("Configuration loaded from: {:?}", path);
        Ok(config)
    }

    /// 获取配置
    pub fn config(&self) -> &ToolManagerConfig {
        &self.config
    }

    /// 获取可变配置引用
    pub fn config_mut(&mut self) -> &mut ToolManagerConfig {
        &mut self.config
    }

    /// 更新配置
    pub async fn update_config<F>(&mut self, updater: F) -> ToolManagerResult<()>
    where
        F: FnOnce(&mut ToolManagerConfig),
    {
        updater(&mut self.config);
        Self::save_config(&self.config_path, &self.config).await
    }

    /// 更新工具配置
    pub async fn update_tools_config<F>(&mut self, updater: F) -> ToolManagerResult<()>
    where
        F: FnOnce(&mut ToolsConfig),
    {
        updater(&mut self.config.tools);
        Self::save_config(&self.config_path, &self.config).await
    }

    /// 从应用配置合并
    pub async fn merge_from_app_config(&mut self, app_config: &AppConfig) -> ToolManagerResult<()> {
        self.config.tools = app_config.tools.clone();
        self.config.download_defaults.max_concurrent_downloads =
            app_config.download.max_concurrent_downloads;
        self.config.advanced.log_level =
            format!("{:?}", app_config.advanced.log_level).to_lowercase();

        Self::save_config(&self.config_path, &self.config).await?;
        Ok(())
    }

    /// 保存配置到文件
    async fn save_config(path: &PathBuf, config: &ToolManagerConfig) -> ToolManagerResult<()> {
        let toml_string = toml::to_string_pretty(config)
            .map_err(|e| ToolManagerError::config(format!("Failed to serialize config: {}", e)))?;

        fs::write(path, toml_string).await.map_err(|e| {
            ToolManagerError::file_operation_failed("write config file", e.to_string())
        })?;

        tracing::info!("Configuration saved to: {:?}", path);
        Ok(())
    }

    /// 从文件加载配置
    async fn load_config(path: &PathBuf) -> ToolManagerResult<ToolManagerConfig> {
        let content = fs::read_to_string(path).await.map_err(|e| {
            ToolManagerError::file_operation_failed("read config file", e.to_string())
        })?;

        let config: ToolManagerConfig = toml::from_str(&content)
            .map_err(|e| ToolManagerError::config(format!("Failed to parse config: {}", e)))?;

        tracing::info!("Configuration loaded from: {:?}", path);
        Ok(config)
    }

    /// 重置为默认配置
    pub async fn reset_to_default(&mut self) -> ToolManagerResult<()> {
        self.config = ToolManagerConfig::default();
        Self::save_config(&self.config_path, &self.config).await?;
        tracing::info!("Configuration reset to default");
        Ok(())
    }

    /// 获取工具路径
    pub fn get_tool_path(&self, tool_name: &str) -> Option<PathBuf> {
        match tool_name {
            "yt-dlp" => self.config.tools.custom_yt_dlp_path.clone(),
            "ffmpeg" => self.config.tools.custom_ffmpeg_path.clone(),
            _ => None,
        }
    }

    /// 设置工具路径
    pub async fn set_tool_path(&mut self, tool_name: &str, path: PathBuf) -> ToolManagerResult<()> {
        match tool_name {
            "yt-dlp" => {
                self.config.tools.custom_yt_dlp_path = Some(path);
            }
            "ffmpeg" => {
                self.config.tools.custom_ffmpeg_path = Some(path);
            }
            _ => {
                return Err(ToolManagerError::config(format!(
                    "Unknown tool: {}",
                    tool_name
                )));
            }
        }
        Self::save_config(&self.config_path, &self.config).await?;
        Ok(())
    }

    /// 启用/禁用自动更新
    pub async fn set_auto_update(&mut self, enabled: bool) -> ToolManagerResult<()> {
        self.config.tools.auto_update = enabled;
        Self::save_config(&self.config_path, &self.config).await?;
        Ok(())
    }

    /// 设置更新通道
    pub async fn set_update_channel(&mut self, channel: UpdateChannel) -> ToolManagerResult<()> {
        self.config.tools.update_channel = channel;
        Self::save_config(&self.config_path, &self.config).await?;
        Ok(())
    }

    /// 获取最大并发下载数
    pub fn max_concurrent_downloads(&self) -> usize {
        self.config.download_defaults.max_concurrent_downloads
    }

    /// 设置最大并发下载数
    pub async fn set_max_concurrent_downloads(&mut self, max: usize) -> ToolManagerResult<()> {
        if max == 0 {
            return Err(ToolManagerError::config(
                "Max concurrent downloads must be > 0".to_string(),
            ));
        }
        self.config.download_defaults.max_concurrent_downloads = max;
        Self::save_config(&self.config_path, &self.config).await?;
        Ok(())
    }

    /// 验证配置
    pub fn validate(&self) -> Vec<String> {
        self.config.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_config_default() {
        let config = ToolManagerConfig::default();
        assert_eq!(config.download_defaults.max_concurrent_downloads, 3);
        assert_eq!(config.download_defaults.default_format, "best");
        assert!(config.tools.auto_update);
    }

    #[tokio::test]
    async fn test_config_save_load() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");

        let original_config = ToolManagerConfig {
            download_defaults: DownloadDefaultsConfig {
                max_concurrent_downloads: 5,
                ..Default::default()
            },
            ..Default::default()
        };

        // 保存配置
        ConfigManager::save_config(&config_path, &original_config)
            .await
            .unwrap();

        // 加载配置
        let loaded_config = ConfigManager::load_config(&config_path).await.unwrap();

        assert_eq!(loaded_config.download_defaults.max_concurrent_downloads, 5);
        assert_eq!(loaded_config.download_defaults.default_format, "best");
    }

    #[test]
    fn test_config_validation() {
        let config = ToolManagerConfig::default();
        let errors = config.validate();
        assert!(errors.is_empty());

        let invalid_config = ToolManagerConfig {
            download_defaults: DownloadDefaultsConfig {
                max_concurrent_downloads: 0,
                ..Default::default()
            },
            ..Default::default()
        };

        let errors = invalid_config.validate();
        assert!(!errors.is_empty());
    }
}
