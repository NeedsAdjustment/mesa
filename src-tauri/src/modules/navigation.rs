use std::sync::atomic::Ordering;

use crate::{INJECT_URLS, IS_LOGGED_OUT, MESSENGER_URL};

/// Allowed path prefixes for Facebook/Messenger URLs.
/// Keep this in sync with the JS side in nav-handler.js.
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

pub fn should_inject_css(url: &tauri::Url) -> bool {
    let url_str = url.as_str();
    INJECT_URLS.iter().any(|pattern| url_str.contains(pattern))
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

pub fn is_internal_url(url: &tauri::Url) -> bool {
    let host = url.host_str();
    let path = url.path();

    if !is_internal_host(host) {
        return false;
    }

    if is_allowed_path(path) {
        return true;
    }

    // When logged out, allow any Facebook URL so login flows work
    if IS_LOGGED_OUT.load(Ordering::Relaxed) {
        return true;
    }

    false
}

pub fn should_allow_navigation(url: &tauri::Url) -> bool {
    let url_str = url.as_str();

    // Always allow the core Messenger page
    if url_str.starts_with(MESSENGER_URL) {
        return true;
    }

    if is_internal_url(url) {
        return true;
    }

    eprintln!("[mesa] NAVIGATION DENIED: {url_str}");
    false
}
