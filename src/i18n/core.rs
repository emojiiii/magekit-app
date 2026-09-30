//! 不依赖 GUI 的语言解析、翻译和格式化，便于独立测试。
use std::sync::atomic::{AtomicU8, Ordering};

#[path = "catalog.rs"]
mod catalog;

/// 实际显示语言；不支持的系统语言回退到英文。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
    English,
    Chinese,
}

impl Locale {
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Chinese => "zh-CN",
        }
    }
}

/// 配置保存的是用户选择，而不是 System 当前解析出的语言。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    System,
    English,
    Chinese,
}

impl Language {
    pub fn from_config(value: &str) -> Self {
        let value = value.trim();
        if value.is_empty()
            || value.eq_ignore_ascii_case("system")
            || value.eq_ignore_ascii_case("auto")
        {
            return Self::System;
        }
        match language_tag(value) {
            Some("zh") => Self::Chinese,
            _ => Self::English,
        }
    }

    pub fn config_value(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::English => "en",
            Self::Chinese => "zh",
        }
    }

    pub fn resolve(self, system_locale: Option<&str>) -> Locale {
        match self {
            Self::Chinese => Locale::Chinese,
            Self::English => Locale::English,
            Self::System => resolve_system_locale(system_locale),
        }
    }
}

fn language_tag(value: &str) -> Option<&'static str> {
    let language = value.trim().split(['-', '_', '.', '@']).next()?;
    if language.eq_ignore_ascii_case("zh") {
        Some("zh")
    } else if language.eq_ignore_ascii_case("en") {
        Some("en")
    } else {
        None
    }
}

pub fn resolve_system_locale(value: Option<&str>) -> Locale {
    match value.and_then(language_tag) {
        Some("zh") => Locale::Chinese,
        _ => Locale::English,
    }
}

static CURRENT_LOCALE: AtomicU8 = AtomicU8::new(0);

pub fn set_locale(locale: Locale) {
    CURRENT_LOCALE.store(u8::from(locale == Locale::Chinese), Ordering::Relaxed);
}

pub fn locale() -> Locale {
    if CURRENT_LOCALE.load(Ordering::Relaxed) == 1 {
        Locale::Chinese
    } else {
        Locale::English
    }
}

fn lookup<'a>(catalog: &'a [(&str, &str)], key: &str) -> Option<&'a str> {
    catalog
        .binary_search_by_key(&key, |(key, _)| key)
        .ok()
        .map(|index| catalog[index].1)
}

/// 缺少当前语言的条目时逐条回退到英文；未知键保持原文。
fn translate_from_catalogs<'a>(
    locale: Locale,
    key: &'a str,
    english: &'a [(&str, &str)],
    chinese: &'a [(&str, &str)],
) -> &'a str {
    if locale == Locale::Chinese {
        if let Some(value) = lookup(chinese, key) {
            return value;
        }
    }
    lookup(english, key).unwrap_or(key)
}

pub fn translate_for<'a>(locale: Locale, key: &'a str) -> &'a str {
    translate_from_catalogs(locale, key, catalog::EN, catalog::ZH)
}

pub fn tr(key: &'static str) -> &'static str {
    translate_for(locale(), key)
}

/// 参数先按 Rust 格式化（例如 {:.1} / {:02}），再插入本地化模板。
/// 不把用户提供的值当作模板再次解释，因此 URL/错误中的花括号保持原样。
pub fn format_for(locale: Locale, key: &str, arguments: &[String]) -> String {
    let template = translate_for(locale, key);
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    let mut argument = arguments.iter();
    while let Some(character) = chars.next() {
        match character {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                result.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                result.push('}');
            }
            '{' => {
                let mut placeholder = String::from("{");
                for next in chars.by_ref() {
                    placeholder.push(next);
                    if next == '}' {
                        break;
                    }
                }
                if placeholder.ends_with('}') {
                    if let Some(value) = argument.next() {
                        result.push_str(value);
                    } else {
                        result.push_str(&placeholder);
                    }
                } else {
                    result.push_str(&placeholder);
                }
            }
            other => result.push(other),
        }
    }
    result
}

pub fn format(key: &str, arguments: &[String]) -> String {
    format_for(locale(), key, arguments)
}

/// 缓存的运行时提示保存键和参数，在绘制时解析，不推测外部字符串的含义。
#[derive(Clone, Debug)]
pub struct Message {
    key: &'static str,
    arguments: Vec<String>,
}

impl Message {
    pub fn plain(key: &'static str) -> Self {
        Self {
            key,
            arguments: Vec::new(),
        }
    }
    pub fn new(key: &'static str, arguments: &[String]) -> Self {
        Self {
            key,
            arguments: arguments.to_vec(),
        }
    }
    pub fn render(&self) -> String {
        self.render_for(locale())
    }
    pub fn render_for(&self, locale: Locale) -> String {
        format_for(locale, self.key, &self.arguments)
    }
}

/// 只本地化已知界面文案，不改变外部标题、URL、Cookie 和路径。
pub fn text(value: &str) -> String {
    let key = catalog::EN
        .iter()
        .find(|(key, english)| *key == value || *english == value)
        .map(|(key, _)| *key);
    translate_for(locale(), key.unwrap_or(value)).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_chinese_system_variants() {
        for value in [
            "zh",
            "zh-CN",
            "zh-TW",
            "zh-HK",
            "zh-Hans-CN",
            "zh_Hant_TW.UTF-8",
            "ZH_cn",
        ] {
            assert_eq!(
                resolve_system_locale(Some(value)),
                Locale::Chinese,
                "{value}"
            );
        }
    }

    #[test]
    fn english_is_safe_fallback() {
        for value in [
            None,
            Some(""),
            Some("en"),
            Some("en_US.UTF-8"),
            Some("en-GB"),
            Some("ja-JP"),
            Some("fr-FR"),
            Some("C"),
            Some("zhongwen"),
        ] {
            assert_eq!(resolve_system_locale(value), Locale::English, "{value:?}");
        }
    }

    #[test]
    fn explicit_selection_overrides_system_and_remains_persistable() {
        assert_eq!(Language::from_config("system").config_value(), "system");
        assert_eq!(
            Language::from_config("").resolve(Some("zh_CN")),
            Locale::Chinese
        );
        assert_eq!(
            Language::from_config("en").resolve(Some("zh_CN")),
            Locale::English
        );
        assert_eq!(
            Language::from_config("zh-TW").resolve(Some("en_US")),
            Locale::Chinese
        );
        assert_eq!(
            Language::from_config("unsupported").resolve(Some("zh_CN")),
            Locale::English
        );
    }

    #[test]
    fn translates_both_languages_and_unknown_keys() {
        assert_eq!(translate_for(Locale::English, "设置"), "Settings");
        assert_eq!(translate_for(Locale::Chinese, "设置"), "设置");
        assert_eq!(translate_for(Locale::Chinese, "unknown key"), "unknown key");
        assert_eq!(translate_for(Locale::English, "未知"), "Unknown");
    }

    #[test]
    fn missing_chinese_entry_falls_back_to_english_per_key() {
        let english = [("settings", "Settings")];
        assert_eq!(
            translate_from_catalogs(Locale::Chinese, "settings", &english, &[]),
            "Settings"
        );
        assert_eq!(
            translate_from_catalogs(
                Locale::English,
                "settings",
                &english,
                &[("settings", "设置")]
            ),
            "Settings"
        );
    }

    #[test]
    fn formats_counts_and_preserves_argument_text() {
        assert_eq!(
            format_for(Locale::English, "共 {} 个任务", &["3".into()]),
            "3 tasks total"
        );
        assert_eq!(
            format_for(Locale::Chinese, "共 {} 个任务", &["3".into()]),
            "共 3 个任务"
        );
        assert_eq!(
            format_for(Locale::English, "下载中 {:.1}%", &[format!("{:.1}", 12.34)]),
            "Downloading 12.3%"
        );
        assert_eq!(
            format_for(Locale::English, "解析错误: {}", &["bad {value}}".into()]),
            "Parse error: bad {value}}"
        );
    }

    #[test]
    fn cached_messages_switch_language_without_losing_arguments() {
        let message = Message::new("Deno {} · MageKit 管理", &["2.3.1".into()]);
        assert_eq!(
            message.render_for(Locale::English),
            "Deno 2.3.1 · Managed by MageKit"
        );
        assert_eq!(
            message.render_for(Locale::Chinese),
            "Deno 2.3.1 · MageKit 管理"
        );
        let error = Message::new("解析错误: {}", &["external {{value}}".into()]);
        assert_eq!(
            error.render_for(Locale::English),
            "Parse error: external {{value}}"
        );
        assert_eq!(
            error.render_for(Locale::Chinese),
            "解析错误: external {{value}}"
        );
    }

    #[test]
    fn catalogs_are_complete_and_sorted() {
        assert!(catalog::EN.len() > 500);
        assert_eq!(catalog::EN.len(), catalog::ZH.len());
        for entries in [catalog::EN, catalog::ZH] {
            assert!(entries.windows(2).all(|pair| pair[0].0 < pair[1].0));
            assert!(entries.iter().all(|(_, value)| !value.is_empty()));
        }
        for (key, _) in catalog::EN {
            assert!(lookup(catalog::ZH, key).is_some(), "{key}");
        }
    }
}
