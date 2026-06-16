use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex};

use tauri::{
    LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl, WebviewWindowBuilder,
    WindowBuilder, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_opener::open_url;

#[allow(unused_imports)]
use window_vibrancy::{NSVisualEffectMaterial, apply_acrylic, apply_vibrancy};

const MESSENGER_URL: &str = "https://facebook.com/messages";
const MESSENGER_STYLE_ID: &str = "mesa-custom-messenger-css";
const MESSENGER_CSS: &str = include_str!("messenger.css");
const INJECT_JS: &str = include_str!("inject.js");

const TITLEBAR_HEIGHT: f64 = 32.;
const INITIAL_WIDTH: f64 = 800.;
const INITIAL_HEIGHT: f64 = 600.;

#[cfg(target_os = "windows")]
const ACRYLIC_COLOR: (u8, u8, u8, u8) = (18, 18, 18, 125);

const INJECT_URLS: &[&str] = &["facebook.com/messages"];

const ZOOM_MIN: f64 = 0.5;
const ZOOM_MAX: f64 = 2.0;
const ZOOM_STEP: f64 = 0.1;

static CURRENT_ZOOM: Mutex<f64> = Mutex::new(1.0);
static CALL_WINDOW_COUNTER: AtomicU32 = AtomicU32::new(0);
static IS_LOGGED_OUT: AtomicBool = AtomicBool::new(true);

static INJECT_SCRIPT: LazyLock<String> = LazyLock::new(|| {
    let css = serde_json::to_string(MESSENGER_CSS).expect("failed to serialize custom CSS");
    let style_id = serde_json::to_string(MESSENGER_STYLE_ID).expect("failed to serialize style id");

    INJECT_JS
        .replace("__STYLE_ID__", &style_id)
        .replace("__CSS__", &css)
});

fn apply_layout(
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

fn should_inject_css(url: &tauri::Url) -> bool {
    let url_str = url.as_str();
    INJECT_URLS.iter().any(|pattern| url_str.contains(pattern))
}

fn is_facebook_domain(url: &tauri::Url) -> bool {
    url.host_str().is_some_and(|host| {
        host == "fb.com"
            || host == "www.fb.com"
            || host == "facebook.com"
            || host == "messenger.com"
            || host == "www.messenger.com"
            || host.ends_with(".facebook.com")
            || host.ends_with(".messenger.com")
    })
}

fn should_allow_navigation(url: &tauri::Url) -> bool {
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

fn create_window(app: &mut tauri::App) -> Result<tauri::Window, Box<dyn std::error::Error>> {
    let window = WindowBuilder::new(app, "main")
        .inner_size(INITIAL_WIDTH, INITIAL_HEIGHT)
        .title("Mesa")
        .decorations(false)
        .transparent(true)
        .build()?;
    Ok(window)
}

fn apply_platform_effects(window: &tauri::Window) {
    #[cfg(target_os = "macos")]
    if let Err(e) = apply_vibrancy(window, NSVisualEffectMaterial::HudWindow, None, None) {
        eprintln!("failed to apply vibrancy effect: {e}");
    }

    #[cfg(target_os = "windows")]
    if let Err(e) = apply_acrylic(window, Some(ACRYLIC_COLOR)) {
        eprintln!("failed to apply acrylic effect: {e}");
    }
}

fn create_titlebar(
    window: &tauri::Window,
) -> Result<tauri::webview::Webview, Box<dyn std::error::Error>> {
    let titlebar = window.add_child(
        WebviewBuilder::new("top_bar", WebviewUrl::App("index.html".into())).transparent(true),
        LogicalPosition::new(0., 0.),
        LogicalSize::new(INITIAL_WIDTH, TITLEBAR_HEIGHT),
    )?;
    Ok(titlebar)
}

fn create_chat(
    window: &tauri::Window,
) -> Result<tauri::webview::Webview, Box<dyn std::error::Error>> {
    let app_handle = window.app_handle().clone();
    let chat = window.add_child(
        WebviewBuilder::new(
            "chat_window",
            WebviewUrl::External(MESSENGER_URL.parse().expect("valid messenger url")),
        )
        .transparent(true)
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
                .initialization_script(
                    r#"
                    window.close = function() {
                        window.location.href = 'https://mesa-close.localhost/';
                    };
                    "#,
                )
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

fn setup_resize_handler(
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

fn apply_zoom_via_app(app: &tauri::AppHandle, zoom: f64) {
    if let Some(window) = app.get_window("main") {
        if let Some(chat) = window.get_webview("chat_window") {
            let _ = chat.set_zoom(zoom);
        }
    }
    if let Ok(mut current) = CURRENT_ZOOM.lock() {
        *current = zoom;
    }
}

fn setup_zoom_shortcuts(app: &mut tauri::App, window: &tauri::Window) {
    let shortcuts = [
        Shortcut::new(Some(Modifiers::CONTROL), Code::Equal),
        Shortcut::new(Some(Modifiers::CONTROL), Code::Minus),
        Shortcut::new(Some(Modifiers::CONTROL), Code::Digit0),
        Shortcut::new(Some(Modifiers::CONTROL), Code::NumpadAdd),
        Shortcut::new(Some(Modifiers::CONTROL), Code::NumpadSubtract),
    ];

    let app_handle = app.handle().clone();

    // Register initially (window starts focused)
    for s in &shortcuts {
        let _ = app_handle.global_shortcut().register(*s);
    }

    // Re-register/unregister on focus change so shortcuts don't leak to other apps
    let focus_handle = app_handle.clone();
    let window_for_focus = window.clone();
    window_for_focus.on_window_event(move |event| {
        if let WindowEvent::Focused(focused) = event {
            if *focused {
                for s in &shortcuts {
                    let _ = focus_handle.global_shortcut().register(*s);
                }
            } else {
                let _ = focus_handle.global_shortcut().unregister_all();
            }
        }
    });
}

#[tauri::command]
fn resize_titlebar(window: tauri::Window, height: f64) {
    if let Some(titlebar) = window.get_webview("top_bar") {
        let width = window
            .inner_size()
            .map(|s| {
                let scale = window.scale_factor().unwrap_or(1.0);
                s.to_logical::<f64>(scale).width
            })
            .unwrap_or(INITIAL_WIDTH);
        let _ = titlebar.set_size(LogicalSize::new(width, height));
    }
}

#[tauri::command]
fn set_chat_zoom(window: tauri::Window, zoom: f64) {
    if let Some(chat) = window.get_webview("chat_window") {
        let _ = chat.set_zoom(zoom);
        if let Ok(mut current) = CURRENT_ZOOM.lock() {
            *current = zoom;
        }
    }
}

#[tauri::command]
fn get_chat_zoom() -> f64 {
    CURRENT_ZOOM.lock().map(|z| *z).unwrap_or(1.0)
}

#[tauri::command]
fn simulate_shortcut(window: tauri::Window, action: String) {
    if let Some(chat) = window.get_webview("chat_window") {
        let js = match action.as_str() {
            "undo" => r"document.execCommand('undo')",
            "redo" => r"document.execCommand('redo')",
            "cut" => r"document.execCommand('cut')",
            "copy" => r"document.execCommand('copy')",
            "paste" => r"document.execCommand('paste')",
            "delete" => r"document.execCommand('delete')",
            "selectAll" => r"document.execCommand('selectAll')",
            _ => return,
        };
        let _ = chat.eval(js);
    }
}

#[tauri::command]
fn focus_chat(window: tauri::Window) {
    if let Some(chat) = window.get_webview("chat_window") {
        let _ = chat.set_focus();
    }
}

#[tauri::command]
fn show_devtools(window: tauri::Window) {
    if let Some(chat) = window.get_webview("chat_window") {
        let _ = chat.open_devtools();
    }
}

#[tauri::command]
fn logout(window: tauri::Window) {
    if let Some(chat) = window.get_webview("chat_window") {
        let _ = chat.clear_all_browsing_data();
        let _ = chat.eval(r#"window.location.href = 'https://www.facebook.com/messages/'"#);
    }
}

#[tauri::command]
fn check_logged_in(window: tauri::Window) -> bool {
    if let Some(chat) = window.get_webview("chat_window") {
        if let Ok(url) = chat.url() {
            let s = url.as_str();
            return !(s == "about:blank" || s.contains("/login"));
        }
    }
    false
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }

                    let current = CURRENT_ZOOM.lock().map(|z| *z).unwrap_or(1.0);
                    let next = match shortcut.key {
                        Code::Equal | Code::NumpadAdd => (current + ZOOM_STEP).min(ZOOM_MAX),
                        Code::Minus | Code::NumpadSubtract => (current - ZOOM_STEP).max(ZOOM_MIN),
                        Code::Digit0 | Code::Numpad0 => 1.0,
                        _ => return,
                    };

                    apply_zoom_via_app(app, next);
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            resize_titlebar,
            set_chat_zoom,
            get_chat_zoom,
            simulate_shortcut,
            focus_chat,
            show_devtools,
            logout,
            check_logged_in
        ])
        .setup(|app| {
            let window = create_window(app)?;
            apply_platform_effects(&window);

            let chat = create_chat(&window)?;
            let titlebar = create_titlebar(&window)?;

            apply_layout(&window, &titlebar, &chat);
            setup_resize_handler(&window, &titlebar, &chat);
            setup_zoom_shortcuts(app, &window);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
