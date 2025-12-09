use magekit_shared::PlatformCookie;

/// 直接拼接 Cookie header，避免写临时文件
pub fn build_cookie_header(
    platform: Option<&str>,
    cookies: Option<&[PlatformCookie]>,
) -> Option<String> {
    let platform_lower = platform.unwrap_or_default().to_lowercase();
    let matching = cookies?
        .iter()
        .find(|c| c.enabled && c.platform.to_lowercase().contains(&platform_lower));
    matching.map(|c| c.cookie.clone())
}
