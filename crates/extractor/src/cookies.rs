use magekit_shared::{PlatformCookie, platform_from_url, validate_http_header};

/// 仅为明确的平台选择对应 Cookie；未知平台绝不能匹配第一个已保存的凭据。
pub fn build_cookie_header(
    platform: Option<&str>,
    cookies: Option<&[PlatformCookie]>,
) -> Option<String> {
    let platform = platform?.trim().to_ascii_lowercase();
    if platform.is_empty() {
        return None;
    }
    cookies?
        .iter()
        .find(|cookie| {
            let configured = cookie.platform.trim().to_ascii_lowercase();
            cookie.enabled
                && !cookie.cookie.trim().is_empty()
                && (configured == platform
                    || platform_from_url(&configured) == Some(platform.as_str()))
                && validate_http_header("Cookie", &cookie.cookie).is_ok()
        })
        .map(|cookie| cookie.cookie.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_platform_never_receives_a_saved_cookie() {
        let cookies = [PlatformCookie::new(
            "youtube".into(),
            "session=private".into(),
        )];
        for platform in [None, Some(""), Some("you"), Some("unknown")] {
            assert!(build_cookie_header(platform, Some(&cookies)).is_none());
        }
    }

    #[test]
    fn cookie_selection_accepts_exact_names_and_real_domain_aliases() {
        let cookies = [
            PlatformCookie::new("youtube.com.attacker.invalid".into(), "wrong=1".into()),
            PlatformCookie::new("notyoutube".into(), "wrong=2".into()),
            PlatformCookie::new(" YouTube.com ".into(), "session=correct".into()),
        ];
        assert_eq!(
            build_cookie_header(Some("youtube"), Some(&cookies)).as_deref(),
            Some("session=correct")
        );
    }

    #[test]
    fn disabled_empty_and_injected_cookies_are_not_selected() {
        let mut disabled = PlatformCookie::new("youtube".into(), "private=1".into());
        disabled.enabled = false;
        let cookies = [
            disabled,
            PlatformCookie::new("youtube".into(), " ".into()),
            PlatformCookie::new("youtube".into(), "a=1\r\nX-Injected: yes".into()),
        ];
        assert!(build_cookie_header(Some("youtube"), Some(&cookies)).is_none());
    }
}
