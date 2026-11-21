use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex};

use tauri::Manager;
use tauri_plugin_global_shortcut::ShortcutState;

mod modules;
pub use modules::accent;
pub use modules::commands;
pub use modules::navigation;
pub use modules::settings;
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

pub const ZOOM_MIN: f64 = 0.5;
pub const ZOOM_MAX: f64 = 2.0;
pub const ZOOM_STEP: f64 = 0.1;

// ---- Statics ----

pub static CURRENT_ZOOM: Mutex<f64> = Mutex::new(1.0);
pub static CALL_WINDOW_COUNTER: AtomicU32 = AtomicU32::new(0);
pub static IS_LOGGED_OUT: AtomicBool = AtomicBool::new(true);
pub static IS_LOGGING_OUT: AtomicBool = AtomicBool::new(false);
pub static BACKDROP_BLUR_ENABLED: AtomicBool = AtomicBool::new(false);
pub static WINDOW_NARROW: AtomicBool = AtomicBool::new(false);
pub static GLOBAL_THEME_ENABLED: AtomicBool = AtomicBool::new(false);
pub static CURRENT_THEME: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new("dark".into()));

pub static INJECT_SCRIPT: LazyLock<String> = LazyLock::new(|| {
    let css = serde_json::to_string(MESSENGER_CSS).expect("failed to serialize custom CSS");
    let style_id = serde_json::to_string(MESSENGER_STYLE_ID).expect("failed to serialize style id");

    INJECT_JS
        .replace("__STYLE_ID__", &style_id)
        .replace("__CSS__", &css)
});

pub static SIDEBAR_RESIZE_SCRIPT: &str = include_str!("scripts/sidebar-resize.js");
pub static NAV_HANDLER_SCRIPT: &str = include_str!("scripts/nav-handler.js");

// ---- Shared helpers ----

pub fn apply_theme_script(theme: &str) -> String {
    let theme_json = serde_json::value::to_value(theme)
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "\"dark\"".into());
    let accent = if GLOBAL_THEME_ENABLED.load(Ordering::Relaxed) {
        accent::accent_rgb()
    } else {
        accent::FALLBACK_ACCENT
    };
    let accent_json = format!("\"#{:02x}{:02x}{:02x}\"", accent.0, accent.1, accent.2);

    include_str!("scripts/apply-theme.js")
        .replace("__THEME_JSON__", &theme_json)
        .replace("__ACCENT_JSON__", &accent_json)
}

pub fn apply_theme_to_chat(window: &tauri::Window, theme: &str) {
    if let Some(chat) = window.get_webview("chat_window") {
        let _ = chat.eval(apply_theme_script(theme));
    }
}

// ---- App entry point ----

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }

                    let Some(next) = zoom::compute_next_zoom(shortcut) else {
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
            commands::show_downloads,
            commands::open_download,
            commands::set_theme,
            commands::set_backdrop_blur,
            commands::set_global_theme,
            commands::get_global_theme,
            commands::set_window_narrow,
            commands::get_auto_updates,
            commands::set_auto_updates,
        ])
        .setup(|app| {
            let settings = settings::SettingsStore::load(app.handle());
            let sidebar = settings.sidebar();
            let global_theme = settings.global_theme();
            GLOBAL_THEME_ENABLED.store(global_theme, Ordering::Relaxed);
            app.manage(settings);

            // Sampling the desktop wallpaper decodes a full-size JPEG, which
            // can take seconds. Never do that on the main thread: serve the
            // fallback colour until the warm-up thread caches the real one.
            if global_theme {
                spawn_accent_warmup(app.handle().clone());
            }

            let window = window::create_window(app)?;

            let chat = window::create_chat(&window, &sidebar, global_theme)?;
            let titlebar = window::create_titlebar(&window)?;

            window::apply_layout(&window, &titlebar, &chat);
            window::setup_resize_handler(&window, &titlebar, &chat);
            zoom::setup_zoom_shortcuts(app, &window);

            spawn_sidebar_settings_poll(app.handle().clone());

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

static ACCENT_WARMING: AtomicBool = AtomicBool::new(false);

/// Sample the desktop wallpaper for the global theme accent on a background
/// thread. The app serves the fallback colour until this finishes, then
/// re-applies the chat theme and acrylic tint with the real accent.
pub fn spawn_accent_warmup(app: tauri::AppHandle) {
    if accent::is_cached()
        || ACCENT_WARMING.swap(true, Ordering::Relaxed)
    {
        return;
    }

    std::thread::spawn(move || {
        accent::compute_and_cache();
        ACCENT_WARMING.store(false, Ordering::Relaxed);

        // The startup injection ran with the fallback colour while the
        // wallpaper decoded here. Re-apply now that the cache is warm.
        let app_for_apply = app.clone();
        let _ = app.run_on_main_thread(move || {
            let Some(window) = app_for_apply.get_window("main") else {
                return;
            };

            let theme = CURRENT_THEME
                .lock()
                .map(|theme| theme.clone())
                .unwrap_or_else(|_| "dark".into());
            apply_theme_to_chat(&window, &theme);

            if BACKDROP_BLUR_ENABLED.load(Ordering::Relaxed) {
                commands::apply_blur_effect(&window, true);
            }
        });
    });
}

fn spawn_sidebar_settings_poll(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));

            if crate::IS_LOGGING_OUT.load(std::sync::atomic::Ordering::Relaxed) {
                continue;
            }

            let Some(window) = app.get_window("main") else {
                continue;
            };
            let Some(chat) = window.get_webview("chat_window") else {
                continue;
            };
            let Ok(url) = chat.url() else {
                continue;
            };
            if !navigation::is_messages_page(&url) {
                continue;
            }

            let app_for_callback = app.clone();
            let _ = chat.eval_with_callback(
                "(() => { try { const width = localStorage.getItem('mesa:sidebar-width'); const collapsedKey = localStorage.getItem('mesa:sidebar-collapsed'); if (width === null && collapsedKey === null) return null; const collapsed = document.documentElement.classList.contains('sidebar-collapsed'); return [collapsed ? 1 : 0, width ? Number(width) : null] } catch (_) { return null } })()",
                move |result| {
                    let Some((width, collapsed)) = parse_sidebar_poll_result(result.trim())
                    else {
                        return;
                    };

                    app_for_callback
                        .state::<settings::SettingsStore>()
                        .update_sidebar(width, collapsed);
                },
            );
        }
    });
}

fn parse_sidebar_poll_result(result: &str) -> Option<(Option<u32>, bool)> {
    let value: serde_json::Value = serde_json::from_str(result.trim()).ok()?;
    let value = match value {
        serde_json::Value::String(inner) => {
            serde_json::from_str::<serde_json::Value>(&inner).ok()?
        }
        value => value,
    };

    let values = value.as_array()?;
    let collapsed = values.first()?.as_u64()? != 0;
    let width = values
        .get(1)
        .and_then(|value| value.as_f64())
        .map(|width| width.round() as u32)
        .filter(|width| *width > 0);

    Some((width, collapsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inject_script_placeholders_are_replaced() {
        assert!(INJECT_SCRIPT.contains(MESSENGER_STYLE_ID));
        assert!(!INJECT_SCRIPT.contains("__STYLE_ID__"));
        assert!(!INJECT_SCRIPT.contains("__CSS__"));
    }

    #[test]
    fn parses_sidebar_poll_results() {
        assert_eq!(
            parse_sidebar_poll_result("[0,280]"),
            Some((Some(280), false))
        );
        assert_eq!(parse_sidebar_poll_result("[1,null]"), Some((None, true)));
        assert_eq!(
            parse_sidebar_poll_result("\"[0, 300.4]\""),
            Some((Some(300), false))
        );
        assert_eq!(parse_sidebar_poll_result("null"), None);
        assert_eq!(parse_sidebar_poll_result(""), None);
    }
}
