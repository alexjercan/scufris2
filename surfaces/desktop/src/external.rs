//! Opening untrusted conversation links through the desktop browser.
//!
//! The webview permits the same narrow URL set before it offers a link. This
//! native boundary validates it again before any process receives it.

use std::process::Command;

use reqwest::Url;

const MAX_URL_BYTES: usize = 8 * 1024;

/// Returns one canonical browser URL when it is safe to hand to `xdg-open`.
pub fn safe_url(raw: &str) -> Option<Url> {
    if raw.is_empty()
        || raw.len() > MAX_URL_BYTES
        || raw.trim() != raw
        || raw.chars().any(char::is_control)
    {
        return None;
    }
    let (scheme, authority) = raw.split_once("://")?;
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
        || authority.starts_with('/')
    {
        return None;
    }
    let parsed = Url::parse(raw).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none_or(str::is_empty)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    Some(parsed)
}

/// Opens one already displayed safe link with the system browser.
pub fn open(raw: &str) -> Result<(), String> {
    let url = safe_url(raw).ok_or_else(failure)?;
    Command::new("xdg-open")
        .arg(url.as_str())
        .spawn()
        .map_err(|_| failure())?;
    Ok(())
}

fn failure() -> String {
    "The link could not be opened.".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_credential_free_web_urls_with_a_host_are_safe() {
        for safe in [
            "https://example.com/report?q=one#result",
            "http://127.0.0.1:8080/path",
        ] {
            assert!(safe_url(safe).is_some(), "{safe}");
        }
        for unsafe_url in [
            "javascript:alert(1)",
            "data:text/html,hello",
            "file:///etc/passwd",
            "mailto:person@example.com",
            "https:///missing-host",
            "https://user:secret@example.com/",
            " https://example.com/",
            "https://example.com/\nnext",
        ] {
            assert!(safe_url(unsafe_url).is_none(), "{unsafe_url}");
        }
    }

    #[test]
    fn safe_urls_are_canonicalized_before_the_browser_receives_them() {
        assert_eq!(
            safe_url("HTTPS://EXAMPLE.COM/a/../b")
                .expect("safe URL")
                .as_str(),
            "https://example.com/b"
        );
    }
}
