use std::sync::atomic::Ordering;

use tauri::{
    LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl, WebviewWindowBuilder,
    WindowBuilder, WindowEvent,
};
use tauri_plugin_opener::open_url;

#[allow(unused_imports)]
use window_vibrancy::{NSVisualEffectMaterial, apply_acrylic, apply_vibrancy};

use crate::{
    CALL_WINDOW_COUNTER, CURRENT_THEME, INITIAL_HEIGHT, INITIAL_WIDTH, INJECT_SCRIPT,
    IS_LOGGED_OUT, MESSENGER_URL, TITLEBAR_HEIGHT,
    navigation::{should_allow_navigation, should_inject_css},
};

pub fn create_window(app: &mut tauri::App) -> Result<tauri::Window, Box<dyn std::error::Error>> {
    let window = WindowBuilder::new(app, "main")
        .inner_size(INITIAL_WIDTH, INITIAL_HEIGHT)
        .title("Mesa")
        .decorations(false)
        .transparent(true)
        .build()?;
    Ok(window)
}

pub fn apply_platform_effects(window: &tauri::Window) {
    #[cfg(target_os = "macos")]
    if let Err(e) = apply_vibrancy(window, NSVisualEffectMaterial::HudWindow, None, None) {
        eprintln!("failed to apply vibrancy effect: {e}");
    }

    #[cfg(target_os = "windows")]
    if let Err(e) = apply_acrylic(window, Some((0, 0, 0, 204))) {
        eprintln!("failed to apply acrylic effect: {e}");
    }
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

pub fn create_chat(
    window: &tauri::Window,
) -> Result<tauri::webview::Webview, Box<dyn std::error::Error>> {
    let app_handle = window.app_handle().clone();
    let chat = window.add_child(
        WebviewBuilder::new(
            "chat_window",
            WebviewUrl::External(MESSENGER_URL.parse().expect("valid messenger url")),
        )
        .transparent(true)
        .initialization_script(include_str!("../scripts/theme-override.js"))
        .on_navigation(|url| {
            if should_allow_navigation(url) {
                true
            } else {
                let _ = open_url(url.as_str(), None::<&str>);
                false
            }
        })
        .on_new_window(move |url, _features| {
            // Open non-Messenger links in the external browser instead of a new webview
            if !should_allow_navigation(&url) {
                let _ = open_url(url.as_str(), None::<&str>);
                return tauri::webview::NewWindowResponse::Deny;
            }

            let number = CALL_WINDOW_COUNTER.fetch_add(1, Ordering::Relaxed);
            let label = format!("call-{number}");

            let app_handle_for_nav = app_handle.clone();
            let label_for_nav = label.clone();

            let builder = WebviewWindowBuilder::new(&app_handle, label, WebviewUrl::External(url))
                .title("Messenger Call")
                .inner_size(960.0, 640.0)
                .auto_resize()
                .initialization_script(include_str!("../scripts/call-window-close.js"))
                .on_navigation(move |url| {
                    if url.as_str().contains("mesa-close") {
                        if let Some(window) = app_handle_for_nav.get_webview_window(&label_for_nav)
                        {
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

            match builder.build() {
                Ok(window) => tauri::webview::NewWindowResponse::Create { window },
                Err(e) => {
                    eprintln!("failed to create call window: {e}");
                    tauri::webview::NewWindowResponse::Deny
                }
            }
        })
        .on_page_load(|window, payload| {
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

                // Track whether we're on a login page
                let url_str = payload.url().as_str();
                IS_LOGGED_OUT.store(
                    url_str == "about:blank" || url_str.contains("/login"),
                    Ordering::Relaxed,
                );

                if should_inject_css(payload.url()) {
                    let _ = window.eval(&*INJECT_SCRIPT);
                }
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
