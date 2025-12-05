use crate::error::{ToolManagerError, ToolManagerResult};
use magekit_shared::{get_tools_dir, ToolType};
use std::path::PathBuf;

/// 工具存储管理器
#[derive(Clone)]
pub struct ToolStorage {
    tools_dir: PathBuf,
}

impl ToolStorage {
    /// 创建新的工具存储管理器 (同步版本)
    pub fn new_sync() -> ToolManagerResult<Self> {
        let tools_dir = get_tools_dir().map_err(|e| ToolManagerError::internal(e.to_string()))?;
        // 确保目录存在
        std::fs::create_dir_all(&tools_dir).map_err(|e| ToolManagerError::internal(e.to_string()))?;
        Ok(Self { tools_dir })
    }

    /// 创建新的工具存储管理器 (异步版本，保持兼容)
    pub async fn new() -> ToolManagerResult<Self> {
        Self::new_sync()
    }

    /// 获取工具的安装路径
    pub fn get_tool_path(&self, tool_type: ToolType) -> PathBuf {
        let filename = match tool_type {
            ToolType::YtDlp => {
                if cfg!(windows) {
                    "yt-dlp.exe"
                } else {
                    "yt-dlp"
                }
            }
            ToolType::Ffmpeg => {
                if cfg!(windows) {
                    "ffmpeg.exe"
                } else {
                    "ffmpeg"
                }
            }
        };

        self.tools_dir.join(filename)
    }

    /// 检查工具是否已安装 (同步版本)
    pub fn is_tool_installed_sync(&self, tool_type: ToolType) -> bool {
        let tool_path = self.get_tool_path(tool_type);
        tool_path.exists() && tool_path.is_file()
    }

    /// 检查工具是否已安装 (异步版本 - 用于 Tokio 运行时)
    pub async fn is_tool_installed(&self, tool_type: ToolType) -> bool {
        self.is_tool_installed_sync(tool_type)
    }

    /// 获取工具版本 (同步版本)
    pub fn get_tool_version_sync(&self, tool_type: ToolType) -> ToolManagerResult<Option<String>> {
        if !self.is_tool_installed_sync(tool_type) {
            return Ok(None);
        }

        let tool_path = self.get_tool_path(tool_type);

        // 使用 std::process 执行工具获取版本 (同步)
        let output = std::process::Command::new(&tool_path)
            .arg("--version")
            .output()
            .map_err(|e| ToolManagerError::process_failed(tool_path.display().to_string(), e.to_string()))?;

        if !output.status.success() {
            return Ok(None);
        }

        let version_output = String::from_utf8_lossy(&output.stdout);

        // 解析版本信息
        let version = match tool_type {
            ToolType::YtDlp => {
                // yt-dlp 输出格式: "yt-dlp 2023.07.06" 或 "2023.07.06"
                version_output
                    .lines()
                    .next()
                    .and_then(|line| {
                        // 查找以 20 开头的版本号（如 2023.07.06）
                        line.split_whitespace()
                            .find(|s| s.starts_with("20"))
                    })
                    .map(|v| v.to_string())
            }
            ToolType::Ffmpeg => {
                // ffmpeg 输出格式: "ffmpeg version 8.0 Copyright..." 或 "ffmpeg version 5.1.2"
                version_output
                    .lines()
                    .find(|line| line.contains("ffmpeg version"))
                    .and_then(|line| {
                        // 获取 "version" 后面的部分
                        line.split("version").nth(1)
                            .and_then(|rest| rest.split_whitespace().next())
                    })
                    .map(|v| v.to_string())
            }
        };

        Ok(version)
    }

    /// 获取工具版本 (异步版本 - 用于 Tokio 运行时)
    pub async fn get_tool_version(&self, tool_type: ToolType) -> ToolManagerResult<Option<String>> {
        self.get_tool_version_sync(tool_type)
    }

    /// 删除已安装的工具 (同步版本)
    pub fn delete_tool_sync(&self, tool_type: ToolType) -> ToolManagerResult<()> {
        let tool_path = self.get_tool_path(tool_type);
        if tool_path.exists() {
            std::fs::remove_file(&tool_path)
                .map_err(|e| ToolManagerError::file_operation_failed(
                    "delete tool",
                    format!("Failed to delete {}: {}", tool_path.display(), e),
                ))?;
            tracing::info!("Tool deleted: {:?}", tool_path);
        }
        Ok(())
    }

    /// 删除已安装的工具 (异步版本)
    pub async fn delete_tool(&self, tool_type: ToolType) -> ToolManagerResult<()> {
        let tool_path = self.get_tool_path(tool_type);
        if tool_path.exists() {
            tokio::fs::remove_file(&tool_path)
                .await
                .map_err(|e| ToolManagerError::file_operation_failed(
                    "delete tool",
                    format!("Failed to delete {}: {}", tool_path.display(), e),
                ))?;
            tracing::info!("Tool deleted: {:?}", tool_path);
        }
        Ok(())
    }

    /// 获取工具目录
    pub fn tools_dir(&self) -> &PathBuf {
        &self.tools_dir
    }
}

impl Default for ToolStorage {
    fn default() -> Self {
        Self {
            tools_dir: get_tools_dir().unwrap_or_else(|_| {
                std::env::current_dir()
                    .unwrap()
                    .join("tools")
            }),
        }
    }
}