use std::sync::atomic::Ordering;

use tauri::Url;

use crate::IS_LOGGED_OUT;

/// Allowed path prefixes for Facebook/Messenger URLs.
pub const ALLOWED_PATHS: &[&str] = &[
    "/messages",
    "/messenger_media",
    "/login",
    "/login.php",
    "/logout",
    "/checkpoint",
    "/two_step_verification",
    "/two_factor",
    "/",
];

/// Allowed hostnames for internal navigation.
pub const ALLOWED_HOSTS: &[&str] = &[
    "facebook.com",
    "www.facebook.com",
    "web.facebook.com",
    "messenger.com",
    "www.messenger.com",
];

const CALL_PATH_SEGMENTS: &[&str] = &["call", "videochat", "groupcall"];

/// What should happen to a top-level navigation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationAction {
    /// Let the webview navigate.
    Allow,
    /// Open the URL in the system browser and cancel the in-app navigation.
    OpenExternal,
    /// Cancel the navigation without doing anything else.
    Block,
}

pub fn is_internal_host(host: Option<&str>) -> bool {
    host.is_some_and(|host| {
        ALLOWED_HOSTS
            .iter()
            .any(|h| host == *h || host.ends_with(&format!(".{h}")))
    })
}

pub fn is_allowed_path(path: &str) -> bool {
    ALLOWED_PATHS
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
}

/// Whether the URL belongs to Facebook/Messenger.
///
/// When logged out we allow any URL on a Facebook/Messenger host so that login,
/// checkpoint and two-factor flows can navigate freely.
///
/// The check is host- and path-based on purpose: string prefix matching cannot
/// tell `https://facebook.com.evil.com/messages` apart from the real host.
pub fn is_internal_url(url: &Url, logged_out: bool) -> bool {
    if !is_internal_host(url.host_str()) {
        return false;
    }

    if is_allowed_path(url.path()) {
        return true;
    }

    logged_out
}

/// Whether this is a Messenger conversation page (the pages Mesa styles).
pub fn is_messages_page(url: &Url) -> bool {
    let path = url.path();
    is_internal_host(url.host_str()) && (path == "/messages" || path.starts_with("/messages/"))
}

/// `about:blank` and `about:blank#blocked` are used by Messenger for popups.
pub fn is_blank_url(url: &Url) -> bool {
    url.scheme() == "about" && url.path() == "blank"
}

/// Whether the URL looks like one of Messenger's call pages.
pub fn is_call_url(url: &Url) -> bool {
    if !is_internal_host(url.host_str()) {
        return false;
    }

    let path = url.path().to_ascii_lowercase();
    path.split('/')
        .any(|segment| CALL_PATH_SEGMENTS.contains(&segment))
}

/// Classify a top-level navigation.
pub fn navigation_action(url: &Url) -> NavigationAction {
    match url.scheme() {
        "http" | "https" => {
            if is_internal_url(url, IS_LOGGED_OUT.load(Ordering::Relaxed)) {
                NavigationAction::Allow
            } else {
                NavigationAction::OpenExternal
            }
        }
        // The OS opener can handle these.
        "mailto" | "tel" => NavigationAction::OpenExternal,
        // Blank documents are used internally by Messenger popups.
        "about" => {
            if is_blank_url(url) {
                NavigationAction::Allow
            } else {
                NavigationAction::Block
            }
        }
        // Blob and data URLs are page-local (downloads, attachments). Let the
        // webview handle them; they cannot be opened by the OS opener.
        "blob" | "data" => NavigationAction::Allow,
        _ => NavigationAction::Block,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).expect("valid test url")
    }

    #[test]
    fn internal_messages_url_is_allowed() {
        assert!(is_internal_url(
            &url("https://www.facebook.com/messages/e2ee/t/123"),
            false
        ));
    }

    #[test]
    fn lookalike_hosts_are_not_internal() {
        for candidate in [
            "https://facebook.com.evil.com/messages",
            "https://evil.com/facebook.com/messages",
            "https://notfacebook.com/messages",
        ] {
            let url = url(candidate);
            assert!(!is_internal_url(&url, false), "{candidate}");
            assert!(!is_internal_url(&url, true), "{candidate}");
        }
    }

    #[test]
    fn at_sign_after_messages_is_a_path_not_a_host() {
        // Kept as documentation: this is still facebook.com, not a spoof.
        let parsed = url("https://facebook.com/messages@evil.com");
        assert_eq!(parsed.host_str(), Some("facebook.com"));
        assert_eq!(parsed.path(), "/messages@evil.com");
    }

    #[test]
    fn external_http_urls_open_externally() {
        assert_eq!(
            navigation_action(&url("https://example.com/articles/1")),
            NavigationAction::OpenExternal
        );
    }

    #[test]
    fn blob_urls_are_not_sent_to_the_os_opener() {
        let blob = url("blob:https://www.facebook.com/4f1e0f3c-2a3b-4c5d-6e7f-8a9b0c1d2e3f");
        assert_eq!(navigation_action(&blob), NavigationAction::Allow);
    }

    #[test]
    fn data_urls_are_allowed() {
        assert_eq!(
            navigation_action(&url("data:text/plain;base64,aGVsbG8=")),
            NavigationAction::Allow
        );
    }

    #[test]
    fn blank_popups_are_allowed() {
        assert!(is_blank_url(&url("about:blank")));
        assert!(is_blank_url(&url("about:blank#blocked")));
        assert_eq!(
            navigation_action(&url("about:blank#blocked")),
            NavigationAction::Allow
        );
    }

    #[test]
    fn unknown_schemes_are_blocked() {
        assert_eq!(
            navigation_action(&url("javascript:alert(1)")),
            NavigationAction::Block
        );
    }

    #[test]
    fn mailto_opens_externally() {
        assert_eq!(
            navigation_action(&url("mailto:someone@example.com")),
            NavigationAction::OpenExternal
        );
    }

    #[test]
    fn messages_page_detection() {
        assert!(is_messages_page(&url("https://www.facebook.com/messages")));
        assert!(is_messages_page(&url(
            "https://www.facebook.com/messages/e2ee/t/123"
        )));
        assert!(!is_messages_page(&url("https://www.facebook.com/login/")));
        assert!(!is_messages_page(&url("https://www.facebook.com/")));
        assert!(!is_messages_page(&url("https://example.com/messages")));
    }

    #[test]
    fn call_urls_are_detected() {
        assert!(is_call_url(&url(
            "https://www.facebook.com/videochat/123456789"
        )));
        assert!(is_call_url(&url(
            "https://www.facebook.com/messages/e2ee/call/987654321"
        )));
        assert!(is_call_url(&url(
            "https://www.messenger.com/groupcall/123456789"
        )));
        assert!(!is_call_url(&url(
            "https://www.facebook.com/messages/e2ee/t/123"
        )));
        assert!(!is_call_url(&url(
            "https://www.facebook.com/messages/callback"
        )));
    }
}
