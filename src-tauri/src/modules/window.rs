use std::sync::atomic::Ordering;

use tauri::{
    Emitter, LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl,
    WebviewWindowBuilder, WindowBuilder, WindowEvent,
};
use tauri_plugin_opener::open_url;

use crate::{
    BACKDROP_BLUR_ENABLED, CALL_WINDOW_COUNTER, CURRENT_THEME, INITIAL_HEIGHT, INITIAL_WIDTH,
    INJECT_SCRIPT, IS_LOGGED_OUT, IS_LOGGING_OUT, MESSENGER_URL, NAV_HANDLER_SCRIPT,
    SIDEBAR_RESIZE_SCRIPT, STANDARD_THEME_ENABLED, TITLEBAR_HEIGHT, WINDOW_NARROW,
    navigation::{
        NavigationAction, is_blank_url, is_call_url, is_internal_url, is_messages_page,
        navigation_action,
    },
    settings::{SettingsStore, SidebarSettings},
};

pub fn create_window(app: &mut tauri::App) -> Result<tauri::Window, Box<dyn std::error::Error>> {
    let window = WindowBuilder::new(app, "main")
        .inner_size(INITIAL_WIDTH, INITIAL_HEIGHT)
        .min_inner_size(316.0, 400.0)
        .title("Mesa")
        .decorations(false)
        .transparent(true)
        .build()?;
    Ok(window)
}

pub fn create_titlebar(
    window: &tauri::Window,
) -> Result<tauri::webview::Webview, Box<dyn std::error::Error>> {
    let titlebar = window.add_child(
        WebviewBuilder::new("top_bar", WebviewUrl::App("index.html".into())).transparent(true),
        LogicalPosition::new(0., 0.),
        LogicalSize::new(INITIAL_WIDTH, TITLEBAR_HEIGHT),
    )?;
    Ok(titlebar)
}

fn download_filename(url: &tauri::Url, path: Option<&std::path::Path>) -> String {
    path.and_then(|p| p.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .or_else(|| {
            url.path_segments()
                .and_then(|mut segments| segments.next_back())
                .filter(|segment| !segment.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "download".to_string())
}

pub fn create_chat(
    window: &tauri::Window,
    sidebar: &SidebarSettings,
    standard_theme: bool,
) -> Result<tauri::webview::Webview, Box<dyn std::error::Error>> {
    let app_handle = window.app_handle().clone();
    let download_app_handle = app_handle.clone();
    let sidebar_app_handle = app_handle.clone();
    let sidebar_state_json = serde_json::to_string(sidebar).unwrap_or_else(|_| "null".to_string());
    let sidebar_state_script = format!("window.__mesaSidebarState = {sidebar_state_json};");
    let standard_theme_script = format!("window.__mesaStandardTheme = {standard_theme};");
    let chat = window.add_child(
        WebviewBuilder::new(
            "chat_window",
            WebviewUrl::External(MESSENGER_URL.parse().expect("valid messenger url")),
        )
        .transparent(true)
        .initialization_script(include_str!("../scripts/theme-override.js"))
        .initialization_script(NAV_HANDLER_SCRIPT)
        .initialization_script(sidebar_state_script)
        .initialization_script(standard_theme_script)
        // Inject the styling at document-start so the page is never painted
        // unstyled. The script itself is a no-op outside Messenger.
        .initialization_script(INJECT_SCRIPT.as_str())
        .initialization_script(SIDEBAR_RESIZE_SCRIPT)
        .on_download(move |_webview, event| {
            match event {
                tauri::webview::DownloadEvent::Requested { url, destination } => {
                    let filename = download_filename(&url, Some(destination.as_path()));
                    eprintln!(
                        "[mesa] DOWNLOAD REQUESTED: {url} -> {}",
                        destination.display()
                    );
                    let _ = download_app_handle.emit(
                        "download-started",
                        serde_json::json!({
                            "url": url.to_string(),
                            "filename": filename,
                            "path": destination.to_string_lossy(),
                        }),
                    );
                }
                tauri::webview::DownloadEvent::Finished { url, path, success } => {
                    let filename = download_filename(&url, path.as_deref());
                    eprintln!("[mesa] DOWNLOAD FINISHED: {url} -> {path:?} (success: {success})");
                    let _ = download_app_handle.emit(
                        "download-finished",
                        serde_json::json!({
                            "url": url.to_string(),
                            "filename": filename,
                            "path": path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                            "success": success,
                        }),
                    );
                }
                _ => {}
            }
            true
        })
        .on_navigation(|url| {
            let url_str = url.as_str();

            match navigation_action(url) {
                NavigationAction::Allow => {
                    eprintln!("[mesa] NAVIGATION ALLOWED: {url_str}");
                    true
                }
                NavigationAction::OpenExternal => {
                    eprintln!("[mesa] NAVIGATION EXTERNAL: {url_str}");
                    let _ = open_url(url_str, None::<&str>);
                    false
                }
                NavigationAction::Block => {
                    eprintln!("[mesa] NAVIGATION BLOCKED: {url_str}");
                    false
                }
            }
        })
        .on_new_window(move |url, features| {
            let url_str = url.as_str();
            eprintln!("[mesa] NEW WINDOW REQUESTED: {url_str}");

            let logged_out = IS_LOGGED_OUT.load(Ordering::Relaxed);
            let internal = is_internal_url(&url, logged_out);

            // Messenger opens calls (and some other flows) as about:blank and
            // then assigns the real URL on the returned window. Those must be
            // real popups. Call URLs opened directly also need their own window.
            let is_popup = is_blank_url(&url) || is_call_url(&url);

            if !is_popup {
                match url.scheme() {
                    "http" | "https" => {
                        if internal {
                            // Ordinary internal links opened in a new window are
                            // loaded in the main chat view instead of spawning
                            // windows all over the place.
                            eprintln!("[mesa] NEW WINDOW -> INTERNAL (navigating in place)");
                            if let Some(main_window) = app_handle.get_window("main")
                                && let Some(chat) = main_window.get_webview("chat_window")
                            {
                                let _ = chat.navigate(url);
                            }
                        } else {
                            eprintln!("[mesa] NEW WINDOW -> EXTERNAL (opening in browser)");
                            let _ = open_url(url_str, None::<&str>);
                        }
                    }
                    "mailto" | "tel" => {
                        eprintln!("[mesa] NEW WINDOW -> EXTERNAL (opening in browser)");
                        let _ = open_url(url_str, None::<&str>);
                    }
                    // Page-local URLs cannot be opened externally and may be
                    // downloads. Let WebView2 create its native popup.
                    "blob" | "data" => {
                        eprintln!("[mesa] NEW WINDOW -> BLOB/DATA (native)");
                        return tauri::webview::NewWindowResponse::Allow;
                    }
                    _ => {}
                }

                return tauri::webview::NewWindowResponse::Deny;
            }

            let number = CALL_WINDOW_COUNTER.fetch_add(1, Ordering::Relaxed);
            let label = format!("call-{number}");

            let app_handle_for_nav = app_handle.clone();
            let label_for_nav = label.clone();

            // Use the size from the window features if available (Messenger sets
            // width,height via window.open(..., 'width=X,height=Y')). Fall back to
            // the main window dimensions.
            let (width, height) = features
                .size()
                .map(|s| (s.width, s.height))
                .unwrap_or((INITIAL_WIDTH, INITIAL_HEIGHT));

            let mut builder = WebviewWindowBuilder::new(
                &app_handle,
                label,
                // WebView2 navigates the attached popup to the requested URL
                // itself; start from about:blank like the Tauri docs example.
                WebviewUrl::External("about:blank".parse().expect("valid about:blank url")),
            )
            .title("Messenger Call")
            .initialization_script(include_str!("../scripts/call-window-close.js"))
            .on_navigation(move |url| {
                if url.as_str().contains("mesa-close") {
                    if let Some(window) = app_handle_for_nav.get_webview_window(&label_for_nav) {
                        let _ = window.destroy();
                    }
                    false
                } else {
                    true
                }
            })
            .on_document_title_changed(|window, title| {
                let _ = window.set_title(&title);
            });

            if features.size().is_none() {
                builder = builder.inner_size(width, height);
            }

            // Shares the opener's WebView2 environment on Windows, which is
            // required for NewWindowResponse::Create. It also applies the
            // requested position and size.
            let builder = builder.window_features(features);

            match builder.build() {
                Ok(window) => {
                    // Force the webview to match the actual window size. Without this
                    // the webview content can render at a wider viewport than the
                    // window, only snapping correct on manual resize.
                    let _ = window.set_size(tauri::LogicalSize::new(width, height));
                    tauri::webview::NewWindowResponse::Create { window }
                }
                Err(e) => {
                    eprintln!("failed to create popup window: {e}");
                    tauri::webview::NewWindowResponse::Deny
                }
            }
        })
        .on_page_load(move |window, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                // Apply the current theme on every page load
                if let Ok(theme) = CURRENT_THEME.lock() {
                    let theme_json = serde_json::value::to_value(&*theme)
                        .map(|v| v.to_string())
                        .unwrap_or_else(|_| "\"dark\"".into());
                    let js = include_str!("../scripts/apply-theme.js")
                        .replace("__THEME_JSON__", &theme_json);
                    let _ = window.eval(&js);
                }

                // Re-apply classes that live on the chat document. Full page
                // loads (login, logout, redirects) discard them. The chat script
                // restores its own sidebar state at document start.
                let url_str = payload.url().as_str();
                let blur = BACKDROP_BLUR_ENABLED.load(Ordering::Relaxed);
                let narrow = WINDOW_NARROW.load(Ordering::Relaxed);
                let standard_theme = STANDARD_THEME_ENABLED.load(Ordering::Relaxed);
                let _ = window.eval(format!(
                    "window.__mesaStandardTheme = {standard_theme};\
                     document.documentElement.classList.toggle('backdrop-blur', {blur});\
                     document.documentElement.classList.toggle('narrow-window', {narrow});\
                     document.documentElement.classList.toggle('mesa-standard-theme', {standard_theme});"
                ));

                // Fallback for the document-start injection hooks. Both scripts
                // are idempotent, so running them again is safe.
                if is_messages_page(payload.url()) {
                    IS_LOGGING_OUT.store(false, Ordering::Relaxed);
                    eprintln!("[mesa] CHAT CSS FALLBACK: {url_str}");
                    let _ = window.eval(INJECT_SCRIPT.as_str());
                    let _ = window.eval(SIDEBAR_RESIZE_SCRIPT);
                }

                // The durable copy lives outside the webview profile, so it
                // survives logout clearing site data. Apply the latest values
                // after the sidebar script is available.
                let sidebar = sidebar_app_handle.state::<SettingsStore>().sidebar();
                if let Ok(json) = serde_json::to_string(&sidebar) {
                    let _ = window.eval(format!(
                        "window.__mesaApplySidebarState && window.__mesaApplySidebarState({json});"
                    ));
                }

                // Track whether we're on a login page
                let is_logged_out = url_str == "about:blank" || url_str.contains("/login");
                IS_LOGGED_OUT.store(is_logged_out, Ordering::Relaxed);
            }
        }),
        LogicalPosition::new(0., TITLEBAR_HEIGHT),
        LogicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT - TITLEBAR_HEIGHT),
    )?;
    Ok(chat)
}

pub fn apply_layout(
    window: &tauri::Window,
    titlebar: &tauri::webview::Webview,
    chat: &tauri::webview::Webview,
) {
    let Ok(size) = window.inner_size() else {
        eprintln!("failed to get window inner size");
        return;
    };
    let Ok(scale_factor) = window.scale_factor() else {
        eprintln!("failed to get window scale factor");
        return;
    };

    let logical = size.to_logical::<f64>(scale_factor);
    let width = logical.width;
    let height = logical.height;
    let chat_height = (height - TITLEBAR_HEIGHT).max(0.0);

    if let Err(e) = titlebar.set_position(LogicalPosition::new(0.0, 0.0)) {
        eprintln!("failed to set titlebar position: {e}");
    }
    if let Err(e) = titlebar.set_size(LogicalSize::new(width, TITLEBAR_HEIGHT)) {
        eprintln!("failed to set titlebar size: {e}");
    }
    if let Err(e) = chat.set_position(LogicalPosition::new(0.0, TITLEBAR_HEIGHT)) {
        eprintln!("failed to set chat position: {e}");
    }
    if let Err(e) = chat.set_size(LogicalSize::new(width, chat_height)) {
        eprintln!("failed to set chat size: {e}");
    }
}

pub fn setup_resize_handler(
    window: &tauri::Window,
    titlebar: &tauri::webview::Webview,
    chat: &tauri::webview::Webview,
) {
    let window_for_event = window.clone();
    let window_for_layout = window.clone();
    let titlebar = titlebar.clone();
    let chat = chat.clone();

    window_for_event.on_window_event(move |event| {
        if let WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } = event {
            apply_layout(&window_for_layout, &titlebar, &chat);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn download_filename_prefers_destination_path() {
        let url = tauri::Url::parse("https://example.com/files/original.pdf").expect("valid url");
        let path = PathBuf::from("downloads").join("renamed.pdf");
        assert_eq!(download_filename(&url, Some(path.as_path())), "renamed.pdf");
    }

    #[test]
    fn download_filename_falls_back_to_url() {
        let url =
            tauri::Url::parse("https://example.com/files/report.pdf?token=123").expect("valid url");
        assert_eq!(download_filename(&url, None), "report.pdf");
    }

    #[test]
    fn download_filename_handles_blob_urls() {
        let url = tauri::Url::parse("blob:https://www.facebook.com/abc-123").expect("valid url");
        assert_eq!(download_filename(&url, None), "download");
    }
}
