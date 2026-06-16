use std::sync::atomic::Ordering;

use crate::{INJECT_URLS, IS_LOGGED_OUT, MESSENGER_URL};

pub fn should_inject_css(url: &tauri::Url) -> bool {
    let url_str = url.as_str();
    INJECT_URLS.iter().any(|pattern| url_str.contains(pattern))
}

pub fn is_facebook_domain(url: &tauri::Url) -> bool {
    url.host_str().is_some_and(|host| {
        host == "facebook.com"
            || host == "www.facebook.com"
            || host == "messenger.com"
            || host == "www.messenger.com"
    })
}

pub fn should_allow_navigation(url: &tauri::Url) -> bool {
    let url_str = url.as_str();

    // Always allow the core Messenger page
    if url_str.starts_with(MESSENGER_URL) {
        return true;
    }

    // When logged out, allow any Facebook URL so login flows work
    if IS_LOGGED_OUT.load(Ordering::Relaxed) && is_facebook_domain(url) {
        return true;
    }

    false
}
