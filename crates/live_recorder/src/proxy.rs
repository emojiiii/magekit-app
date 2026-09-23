//! 直播请求使用的系统代理解析。

use url::Url;

/// 返回操作系统/进程当前配置的 HTTP 代理地址。
///
/// 优先读取常见代理环境变量；Windows 桌面代理通常配置在当前用户注册表中，
/// 因此在环境变量缺失时再读取 WinINet 的 ProxyServer。
pub fn system_proxy_url() -> Option<String> {
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        if let Ok(value) = std::env::var(key) {
            if let Some(proxy) = normalize_proxy(&value) {
                return Some(proxy);
            }
        }
    }

    #[cfg(windows)]
    if let Some(proxy) = windows_proxy_url() {
        return Some(proxy);
    }

    None
}

/// 将设置页保存的自定义代理或“system”标记转换成可直接交给 HTTP 客户端的 URL。
pub fn resolve_proxy_url(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.eq_ignore_ascii_case("system") {
        system_proxy_url()
    } else {
        normalize_proxy(value)
    }
}

/// 规范化 Windows 注册表中的 ProxyServer 格式。
fn normalize_proxy(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let entries = value
        .split(';')
        .filter_map(|entry| entry.trim().split_once('='))
        .map(|(scheme, address)| (scheme.trim().to_ascii_lowercase(), address.trim()))
        .collect::<Vec<_>>();
    let address = if entries.is_empty() {
        value
    } else {
        entries
            .iter()
            .find(|(scheme, _)| scheme == "https")
            .or_else(|| entries.iter().find(|(scheme, _)| scheme == "http"))
            .or_else(|| entries.iter().find(|(scheme, _)| scheme == "socks"))
            .map(|(_, address)| *address)?
    };

    let candidate = if address.contains("://") {
        address.to_owned()
    } else {
        format!("http://{address}")
    };
    let parsed = Url::parse(&candidate).ok()?;
    matches!(parsed.scheme(), "http" | "https" | "socks4" | "socks5").then(|| parsed.to_string())
}

#[cfg(windows)]
fn windows_proxy_url() -> Option<String> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};

    let settings = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;
    let enabled: u32 = settings.get_value("ProxyEnable").ok()?;
    if enabled == 0 {
        return None;
    }
    let server: String = settings.get_value("ProxyServer").ok()?;
    normalize_proxy(&server)
}
