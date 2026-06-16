use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{LazyLock, Mutex};

use tauri::Manager;
use tauri_plugin_global_shortcut::ShortcutState;

mod modules;
pub use modules::commands;
pub use modules::navigation;
pub use modules::window;
pub use modules::zoom;

// ---- Constants ----

pub const MESSENGER_URL: &str = "https://facebook.com/messages";
pub const MESSENGER_STYLE_ID: &str = "mesa-custom-messenger-css";
pub const MESSENGER_CSS: &str = include_str!("scripts/tweaks.css");
pub const INJECT_JS: &str = include_str!("scripts/tweaks-inject.js");

pub const TITLEBAR_HEIGHT: f64 = 32.;
pub const INITIAL_WIDTH: f64 = 800.;
pub const INITIAL_HEIGHT: f64 = 600.;

#[cfg(target_os = "windows")]
pub const ACRYLIC_COLOR: (u8, u8, u8, u8) = (18, 18, 18, 125);

pub const INJECT_URLS: &[&str] = &["facebook.com/messages"];

pub const ZOOM_MIN: f64 = 0.5;
pub const ZOOM_MAX: f64 = 2.0;
pub const ZOOM_STEP: f64 = 0.1;

// ---- Statics ----

pub static CURRENT_ZOOM: Mutex<f64> = Mutex::new(1.0);
pub static CALL_WINDOW_COUNTER: AtomicU32 = AtomicU32::new(0);
pub static IS_LOGGED_OUT: AtomicBool = AtomicBool::new(true);
pub static CURRENT_THEME: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new("dark".into()));

pub static INJECT_SCRIPT: LazyLock<String> = LazyLock::new(|| {
    let css = serde_json::to_string(MESSENGER_CSS).expect("failed to serialize custom CSS");
    let style_id = serde_json::to_string(MESSENGER_STYLE_ID).expect("failed to serialize style id");

    INJECT_JS
        .replace("__STYLE_ID__", &style_id)
        .replace("__CSS__", &css)
});

// ---- Shared helpers ----

pub fn apply_theme_to_chat(window: &tauri::Window, theme: &str) {
    if let Some(chat) = window.get_webview("chat_window") {
        let theme_json = serde_json::value::to_value(theme)
            .map(|v| v.to_string())
            .unwrap_or_else(|_| "\"dark\"".into());
        let js = include_str!("scripts/apply-theme.js").replace("__THEME_JSON__", &theme_json);
        let _ = chat.eval(&js);
    }
}

// ---- App entry point ----

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

                    let Some(next) = zoom::compute_next_zoom(&shortcut) else {
                        return;
                    };

                    zoom::apply_zoom_via_app(app, next);
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::resize_titlebar,
            commands::set_chat_zoom,
            commands::get_chat_zoom,
            commands::simulate_shortcut,
            commands::focus_chat,
            commands::show_devtools,
            commands::logout,
            commands::check_logged_in,
            commands::set_theme,
            commands::set_backdrop_blur,
        ])
        .setup(|app| {
            let window = window::create_window(app)?;
            window::apply_platform_effects(&window);

            let chat = window::create_chat(&window)?;
            let titlebar = window::create_titlebar(&window)?;

            window::apply_layout(&window, &titlebar, &chat);
            window::setup_resize_handler(&window, &titlebar, &chat);
            zoom::setup_zoom_shortcuts(app, &window);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
