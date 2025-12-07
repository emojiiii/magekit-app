//! 工具管理功能
//!
//! 包含工具检测、安装、删除等功能

use anyhow::Result;
use std::sync::Arc;

use super::state::AppState;
use super::types::ToolStatus;

impl AppState {
    /// 检查工具是否已安装
    pub async fn is_tool_installed(&self, tool_type: magekit_shared::ToolType) -> bool {
        self.tool_manager.storage.is_tool_installed(tool_type).await
    }

    /// 获取工具版本
    pub async fn get_tool_version(&self, tool_type: magekit_shared::ToolType) -> Option<String> {
        self.tool_manager.storage.get_tool_version(tool_type).await.ok().flatten()
    }

    /// 检测工具状态 - 同步版本 (用于 UI 初始化)
    pub fn check_tool_status_sync(&self, tool_type: magekit_shared::ToolType) -> ToolStatus {
        // 先检查应用内安装 (使用同步版本)
        if self.tool_manager.storage.is_tool_installed_sync(tool_type) {
            let version = self.tool_manager.storage.get_tool_version_sync(tool_type).ok().flatten();
            return ToolStatus::Installed { version, is_system: false };
        }

        // 再检查系统 PATH
        let tool_name = match tool_type {
            magekit_shared::ToolType::YtDlp => "yt-dlp",
            magekit_shared::ToolType::Ffmpeg => "ffmpeg",
        };

        if let Ok(path) = which::which(tool_name) {
            // 获取系统工具版本
            let version = Self::get_system_tool_version_sync(tool_type, &path);
            return ToolStatus::Installed { version, is_system: true };
        }

        ToolStatus::NotInstalled
    }

    /// 检测工具状态 - 异步版本 (用于后台任务)
    pub async fn check_tool_status(&self, tool_type: magekit_shared::ToolType) -> ToolStatus {
        self.check_tool_status_sync(tool_type)
    }

    /// 获取系统工具版本 (同步版本)
    fn get_system_tool_version_sync(tool_type: magekit_shared::ToolType, path: &std::path::Path) -> Option<String> {
        let output = magekit_shared::create_command(path)
            .arg("--version")
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let version_output = String::from_utf8_lossy(&output.stdout);

        match tool_type {
            magekit_shared::ToolType::YtDlp => {
                // yt-dlp 输出格式: "yt-dlp 2023.07.06" 或 "2023.07.06"
                version_output
                    .lines()
                    .next()
                    .and_then(|line| {
                        line.split_whitespace()
                            .find(|s| s.starts_with("20"))
                    })
                    .map(|v| v.to_string())
            }
            magekit_shared::ToolType::Ffmpeg => {
                // ffmpeg 输出格式: "ffmpeg version 8.0 Copyright..."
                version_output
                    .lines()
                    .find(|line| line.contains("ffmpeg version"))
                    .and_then(|line| {
                        line.split("version").nth(1)
                            .and_then(|rest| rest.split_whitespace().next())
                    })
                    .map(|v| v.to_string())
            }
        }
    }

    /// 安装单个工具 (使用内部 Tokio 运行时，在后台线程中运行)
    pub fn install_tool_in_background(&self, tool_type: magekit_shared::ToolType) -> std::thread::JoinHandle<Result<()>> {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();
        
        std::thread::spawn(move || {
            runtime.block_on(async {
                match tool_type {
                    magekit_shared::ToolType::YtDlp => {
                        let updater = magekit_tool_manager::updater::ToolUpdater::new(tool_manager.storage.clone());
                        updater.ensure_yt_dlp(magekit_shared::UpdateChannel::Stable).await
                            .map_err(|e| anyhow::anyhow!("安装 yt-dlp 失败: {}", e))?;
                    }
                    magekit_shared::ToolType::Ffmpeg => {
                        let updater = magekit_tool_manager::updater::ToolUpdater::new(tool_manager.storage.clone());
                        updater.ensure_ffmpeg().await
                            .map_err(|e| anyhow::anyhow!("安装 ffmpeg 失败: {}", e))?;
                    }
                }
                Ok(())
            })
        })
    }

    /// 安装单个工具 (带进度回调，在后台线程中运行)
    pub fn install_tool_with_progress(
        &self,
        tool_type: magekit_shared::ToolType,
        progress_callback: Arc<dyn Fn(u64, u64, u64) + Send + Sync>,
    ) -> std::thread::JoinHandle<Result<()>> {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();
        
        std::thread::spawn(move || {
            runtime.block_on(async {
                match tool_type {
                    magekit_shared::ToolType::YtDlp => {
                        let updater = magekit_tool_manager::updater::ToolUpdater::new(tool_manager.storage.clone());
                        updater.ensure_yt_dlp_with_progress(
                            magekit_shared::UpdateChannel::Stable,
                            Some(progress_callback),
                        ).await
                            .map_err(|e| anyhow::anyhow!("安装 yt-dlp 失败: {}", e))?;
                    }
                    magekit_shared::ToolType::Ffmpeg => {
                        let updater = magekit_tool_manager::updater::ToolUpdater::new(tool_manager.storage.clone());
                        updater.ensure_ffmpeg().await
                            .map_err(|e| anyhow::anyhow!("安装 ffmpeg 失败: {}", e))?;
                    }
                }
                Ok(())
            })
        })
    }

    /// 删除已安装的工具
    pub fn delete_tool_sync(&self, tool_type: magekit_shared::ToolType) -> Result<()> {
        self.tool_manager.storage.delete_tool_sync(tool_type)
            .map_err(|e| anyhow::anyhow!("删除工具失败: {}", e))
    }

    /// 安装工具 (阻塞版本)
    #[allow(dead_code)]
    pub fn install_tool_blocking(&self, tool_type: magekit_shared::ToolType) -> Result<()> {
        use magekit_shared::UpdateChannel;
        use magekit_tool_manager::updater::ToolUpdater;
        
        self.runtime.block_on(async {
            let updater = ToolUpdater::new(self.tool_manager.storage.clone());
            match tool_type {
                magekit_shared::ToolType::YtDlp => {
                    updater.ensure_yt_dlp(UpdateChannel::Stable).await
                        .map_err(|e| anyhow::anyhow!("安装 yt-dlp 失败: {}", e))?;
                }
                magekit_shared::ToolType::Ffmpeg => {
                    updater.ensure_ffmpeg().await
                        .map_err(|e| anyhow::anyhow!("安装 ffmpeg 失败: {}", e))?;
                }
            }
            Ok(())
        })
    }

    /// 安装工具 (异步版本)
    pub async fn install_tool(&self, tool_type: magekit_shared::ToolType) -> Result<()> {
        use magekit_shared::UpdateChannel;
        
        match tool_type {
            magekit_shared::ToolType::YtDlp => {
                self.tool_manager.ensure_tools(UpdateChannel::Stable).await
                    .map_err(|e| anyhow::anyhow!("安装 yt-dlp 失败: {}", e))?;
            }
            magekit_shared::ToolType::Ffmpeg => {
                self.tool_manager.ensure_tools(UpdateChannel::Stable).await
                    .map_err(|e| anyhow::anyhow!("安装 ffmpeg 失败: {}", e))?;
            }
        }
        Ok(())
    }

    /// 安装所有工具 (阻塞版本)
    pub fn install_all_tools_blocking(&self) -> Result<()> {
        use magekit_shared::UpdateChannel;
        self.runtime.block_on(async {
            self.tool_manager.ensure_tools(UpdateChannel::Stable).await
                .map_err(|e| anyhow::anyhow!("安装工具失败: {}", e))
        })
    }

    /// 安装所有工具 (后台线程)
    pub fn install_all_tools_in_background(&self) -> std::thread::JoinHandle<Result<()>> {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();
        
        std::thread::spawn(move || {
            runtime.block_on(async {
                tool_manager.ensure_tools(magekit_shared::UpdateChannel::Stable).await
                    .map_err(|e| anyhow::anyhow!("安装工具失败: {}", e))
            })
        })
    }

    /// 安装所有工具 (异步版本)
    pub async fn install_all_tools(&self) -> Result<()> {
        use magekit_shared::UpdateChannel;
        self.tool_manager.ensure_tools(UpdateChannel::Stable).await
            .map_err(|e| anyhow::anyhow!("安装工具失败: {}", e))?;
        Ok(())
    }
}
