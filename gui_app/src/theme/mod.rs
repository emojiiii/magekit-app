//! 主题管理模块
//!
//! 提供应用程序主题管理功能，包括亮色/暗色主题切换和自定义主题支持

use gpui::Hsla;
use magekit_shared::types::Theme as AppTheme;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 主题颜色定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeColors {
    /// 主色调
    pub primary: String,
    /// 主色调前景色
    pub primary_foreground: String,
    /// 次要色
    pub secondary: String,
    /// 次要色前景色
    pub secondary_foreground: String,
    /// 背景色
    pub background: String,
    /// 前景色
    pub foreground: String,
    /// 卡片背景色
    pub card: String,
    /// 卡片前景色
    pub card_foreground: String,
    /// 弹窗背景色
    pub popover: String,
    /// 弹窗前景色
    pub popover_foreground: String,
    /// 边框色
    pub border: String,
    /// 输入框背景色
    pub input: String,
    /// 静音文本色
    pub muted: String,
    /// 静音前景色
    pub muted_foreground: String,
    /// 强调色
    pub accent: String,
    /// 强调前景色
    pub accent_foreground: String,
    /// 危险色
    pub destructive: String,
    /// 危险前景色
    pub destructive_foreground: String,
    /// 成功色
    pub success: String,
    /// 警告色
    pub warning: String,
    /// 错误色
    pub error: String,
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self::light()
    }
}

impl ThemeColors {
    /// 亮色主题
    pub fn light() -> Self {
        Self {
            primary: "#0f172a".to_string(),
            primary_foreground: "#f8fafc".to_string(),
            secondary: "#f1f5f9".to_string(),
            secondary_foreground: "#0f172a".to_string(),
            background: "#ffffff".to_string(),
            foreground: "#0f172a".to_string(),
            card: "#ffffff".to_string(),
            card_foreground: "#0f172a".to_string(),
            popover: "#ffffff".to_string(),
            popover_foreground: "#0f172a".to_string(),
            border: "#e2e8f0".to_string(),
            input: "#e2e8f0".to_string(),
            muted: "#f1f5f9".to_string(),
            muted_foreground: "#64748b".to_string(),
            accent: "#f1f5f9".to_string(),
            accent_foreground: "#0f172a".to_string(),
            destructive: "#ef4444".to_string(),
            destructive_foreground: "#f8fafc".to_string(),
            success: "#22c55e".to_string(),
            warning: "#f59e0b".to_string(),
            error: "#ef4444".to_string(),
        }
    }

    /// 暗色主题
    pub fn dark() -> Self {
        Self {
            primary: "#f8fafc".to_string(),
            primary_foreground: "#0f172a".to_string(),
            secondary: "#1e293b".to_string(),
            secondary_foreground: "#f8fafc".to_string(),
            background: "#0f172a".to_string(),
            foreground: "#f8fafc".to_string(),
            card: "#1e293b".to_string(),
            card_foreground: "#f8fafc".to_string(),
            popover: "#1e293b".to_string(),
            popover_foreground: "#f8fafc".to_string(),
            border: "#334155".to_string(),
            input: "#334155".to_string(),
            muted: "#1e293b".to_string(),
            muted_foreground: "#94a3b8".to_string(),
            accent: "#1e293b".to_string(),
            accent_foreground: "#f8fafc".to_string(),
            destructive: "#dc2626".to_string(),
            destructive_foreground: "#f8fafc".to_string(),
            success: "#16a34a".to_string(),
            warning: "#d97706".to_string(),
            error: "#dc2626".to_string(),
        }
    }

    /// 解析十六进制颜色为 HSLA
    pub fn parse_hex(hex: &str) -> Option<Hsla> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }

        let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;

        // RGB to HSL 转换
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;

        if max == min {
            return Some(Hsla { h: 0.0, s: 0.0, l, a: 1.0 });
        }

        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };

        let h = if max == r {
            ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
        } else if max == g {
            ((b - r) / d + 2.0) / 6.0
        } else {
            ((r - g) / d + 4.0) / 6.0
        };

        Some(Hsla { h, s, l, a: 1.0 })
    }
}

/// 自定义主题
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomTheme {
    /// 主题名称
    pub name: String,
    /// 主题描述
    pub description: Option<String>,
    /// 主题作者
    pub author: Option<String>,
    /// 主题颜色
    pub colors: ThemeColors,
}

impl CustomTheme {
    /// 创建新的自定义主题
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            author: None,
            colors: ThemeColors::default(),
        }
    }

    /// 设置描述
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// 设置作者
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// 设置颜色
    pub fn with_colors(mut self, colors: ThemeColors) -> Self {
        self.colors = colors;
        self
    }
}

/// 主题管理器
pub struct ThemeManager {
    /// 当前主题设置
    current_theme: AppTheme,
    /// 实际使用的主题（解析 System 后）
    resolved_theme: ResolvedTheme,
    /// 自定义主题列表
    custom_themes: HashMap<String, CustomTheme>,
    /// 配置路径
    config_path: PathBuf,
}

/// 解析后的主题
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResolvedTheme {
    Light,
    Dark,
}

impl ThemeManager {
    /// 创建新的主题管理器
    pub fn new(config_path: PathBuf) -> Self {
        let system_is_dark = Self::detect_system_theme();
        Self {
            current_theme: AppTheme::System,
            resolved_theme: if system_is_dark { ResolvedTheme::Dark } else { ResolvedTheme::Light },
            custom_themes: HashMap::new(),
            config_path,
        }
    }

    /// 检测系统主题
    fn detect_system_theme() -> bool {
        // macOS: 检测系统是否为暗色模式
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;
            if let Ok(output) = Command::new("defaults")
                .args(["read", "-g", "AppleInterfaceStyle"])
                .output()
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                return stdout.trim().eq_ignore_ascii_case("dark");
            }
        }

        // Windows: 检测系统主题
        #[cfg(target_os = "windows")]
        {
            // 可以通过注册表检测
            // HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize
            // AppsUseLightTheme = 0 表示暗色模式
        }

        // 默认返回 false（亮色模式）
        false
    }

    /// 获取当前主题设置
    pub fn current_theme(&self) -> &AppTheme {
        &self.current_theme
    }

    /// 获取解析后的主题
    pub fn resolved_theme(&self) -> ResolvedTheme {
        self.resolved_theme
    }

    /// 设置主题
    pub fn set_theme(&mut self, theme: AppTheme) {
        self.current_theme = theme.clone();
        self.resolved_theme = match &theme {
            AppTheme::Light => ResolvedTheme::Light,
            AppTheme::Dark => ResolvedTheme::Dark,
            AppTheme::System => {
                if Self::detect_system_theme() {
                    ResolvedTheme::Dark
                } else {
                    ResolvedTheme::Light
                }
            }
            AppTheme::Custom(config) => {
                match config.mode {
                    magekit_shared::types::ThemeMode::Dark => ResolvedTheme::Dark,
                    magekit_shared::types::ThemeMode::Light => ResolvedTheme::Light,
                }
            }
        };
    }

    /// 切换亮/暗主题
    pub fn toggle_theme(&mut self) {
        let new_theme = match self.resolved_theme {
            ResolvedTheme::Light => AppTheme::Dark,
            ResolvedTheme::Dark => AppTheme::Light,
        };
        self.set_theme(new_theme);
    }

    /// 获取当前主题颜色
    pub fn colors(&self) -> ThemeColors {
        match self.resolved_theme {
            ResolvedTheme::Light => ThemeColors::light(),
            ResolvedTheme::Dark => ThemeColors::dark(),
        }
    }

    /// 添加自定义主题
    pub fn add_custom_theme(&mut self, theme: CustomTheme) {
        self.custom_themes.insert(theme.name.clone(), theme);
    }

    /// 移除自定义主题
    pub fn remove_custom_theme(&mut self, name: &str) -> Option<CustomTheme> {
        self.custom_themes.remove(name)
    }

    /// 获取自定义主题
    pub fn get_custom_theme(&self, name: &str) -> Option<&CustomTheme> {
        self.custom_themes.get(name)
    }

    /// 列出所有自定义主题
    pub fn custom_themes(&self) -> impl Iterator<Item = &CustomTheme> {
        self.custom_themes.values()
    }

    /// 从文件加载主题配置
    pub async fn load(&mut self) -> Result<(), std::io::Error> {
        let themes_path = self.config_path.join("themes.json");
        if themes_path.exists() {
            let content = tokio::fs::read_to_string(&themes_path).await?;
            let themes: HashMap<String, CustomTheme> = serde_json::from_str(&content)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            self.custom_themes = themes;
        }
        Ok(())
    }

    /// 保存主题配置到文件
    pub async fn save(&self) -> Result<(), std::io::Error> {
        tokio::fs::create_dir_all(&self.config_path).await?;
        let themes_path = self.config_path.join("themes.json");
        let content = serde_json::to_string_pretty(&self.custom_themes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        tokio::fs::write(&themes_path, content).await?;
        Ok(())
    }

    /// 导出主题到文件
    pub async fn export_theme(&self, name: &str, path: &PathBuf) -> Result<(), std::io::Error> {
        if let Some(theme) = self.custom_themes.get(name) {
            let content = serde_json::to_string_pretty(theme)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            tokio::fs::write(path, content).await?;
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Theme '{}' not found", name),
            ))
        }
    }

    /// 从文件导入主题
    pub async fn import_theme(&mut self, path: &PathBuf) -> Result<CustomTheme, std::io::Error> {
        let content = tokio::fs::read_to_string(path).await?;
        let theme: CustomTheme = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.custom_themes.insert(theme.name.clone(), theme.clone());
        Ok(theme)
    }
}

/// 预设主题
pub mod presets {
    use super::*;

    /// 海洋蓝主题
    pub fn ocean_blue() -> CustomTheme {
        CustomTheme::new("Ocean Blue")
            .with_description("清新的海洋蓝主题")
            .with_colors(ThemeColors {
                primary: "#0284c7".to_string(),
                primary_foreground: "#f0f9ff".to_string(),
                secondary: "#e0f2fe".to_string(),
                secondary_foreground: "#0c4a6e".to_string(),
                background: "#f0f9ff".to_string(),
                foreground: "#0c4a6e".to_string(),
                card: "#ffffff".to_string(),
                card_foreground: "#0c4a6e".to_string(),
                popover: "#ffffff".to_string(),
                popover_foreground: "#0c4a6e".to_string(),
                border: "#bae6fd".to_string(),
                input: "#bae6fd".to_string(),
                muted: "#e0f2fe".to_string(),
                muted_foreground: "#0369a1".to_string(),
                accent: "#e0f2fe".to_string(),
                accent_foreground: "#0c4a6e".to_string(),
                destructive: "#dc2626".to_string(),
                destructive_foreground: "#ffffff".to_string(),
                success: "#16a34a".to_string(),
                warning: "#d97706".to_string(),
                error: "#dc2626".to_string(),
            })
    }

    /// 森林绿主题
    pub fn forest_green() -> CustomTheme {
        CustomTheme::new("Forest Green")
            .with_description("自然的森林绿主题")
            .with_colors(ThemeColors {
                primary: "#15803d".to_string(),
                primary_foreground: "#f0fdf4".to_string(),
                secondary: "#dcfce7".to_string(),
                secondary_foreground: "#14532d".to_string(),
                background: "#f0fdf4".to_string(),
                foreground: "#14532d".to_string(),
                card: "#ffffff".to_string(),
                card_foreground: "#14532d".to_string(),
                popover: "#ffffff".to_string(),
                popover_foreground: "#14532d".to_string(),
                border: "#bbf7d0".to_string(),
                input: "#bbf7d0".to_string(),
                muted: "#dcfce7".to_string(),
                muted_foreground: "#166534".to_string(),
                accent: "#dcfce7".to_string(),
                accent_foreground: "#14532d".to_string(),
                destructive: "#dc2626".to_string(),
                destructive_foreground: "#ffffff".to_string(),
                success: "#16a34a".to_string(),
                warning: "#d97706".to_string(),
                error: "#dc2626".to_string(),
            })
    }

    /// 紫罗兰主题
    pub fn violet() -> CustomTheme {
        CustomTheme::new("Violet")
            .with_description("优雅的紫罗兰主题")
            .with_colors(ThemeColors {
                primary: "#7c3aed".to_string(),
                primary_foreground: "#faf5ff".to_string(),
                secondary: "#ede9fe".to_string(),
                secondary_foreground: "#4c1d95".to_string(),
                background: "#faf5ff".to_string(),
                foreground: "#4c1d95".to_string(),
                card: "#ffffff".to_string(),
                card_foreground: "#4c1d95".to_string(),
                popover: "#ffffff".to_string(),
                popover_foreground: "#4c1d95".to_string(),
                border: "#ddd6fe".to_string(),
                input: "#ddd6fe".to_string(),
                muted: "#ede9fe".to_string(),
                muted_foreground: "#6d28d9".to_string(),
                accent: "#ede9fe".to_string(),
                accent_foreground: "#4c1d95".to_string(),
                destructive: "#dc2626".to_string(),
                destructive_foreground: "#ffffff".to_string(),
                success: "#16a34a".to_string(),
                warning: "#d97706".to_string(),
                error: "#dc2626".to_string(),
            })
    }

    /// 夜间模式主题
    pub fn midnight() -> CustomTheme {
        CustomTheme::new("Midnight")
            .with_description("深邃的午夜主题")
            .with_colors(ThemeColors {
                primary: "#60a5fa".to_string(),
                primary_foreground: "#0f172a".to_string(),
                secondary: "#1e293b".to_string(),
                secondary_foreground: "#e2e8f0".to_string(),
                background: "#0f172a".to_string(),
                foreground: "#e2e8f0".to_string(),
                card: "#1e293b".to_string(),
                card_foreground: "#e2e8f0".to_string(),
                popover: "#1e293b".to_string(),
                popover_foreground: "#e2e8f0".to_string(),
                border: "#334155".to_string(),
                input: "#334155".to_string(),
                muted: "#1e293b".to_string(),
                muted_foreground: "#94a3b8".to_string(),
                accent: "#1e293b".to_string(),
                accent_foreground: "#e2e8f0".to_string(),
                destructive: "#ef4444".to_string(),
                destructive_foreground: "#f8fafc".to_string(),
                success: "#22c55e".to_string(),
                warning: "#f59e0b".to_string(),
                error: "#ef4444".to_string(),
            })
    }
}
