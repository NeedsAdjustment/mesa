use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex};

use tauri::Manager;
use tauri_plugin_global_shortcut::ShortcutState;

mod modules;
pub use modules::accent;
pub use modules::backdrop;
pub use modules::commands;
pub use modules::navigation;
pub use modules::settings;
pub use modules::tray;
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

/// Docked chat panel size in tray mode, in logical pixels. The width matches
/// the window minimum so Messenger uses its narrow layout.
pub const DOCK_WIDTH: f64 = 316.;
pub const DOCK_HEIGHT: f64 = 520.;

/// Upper bound for the docked panel height. Only exists because the width
/// has to be pinned through a max constraint; effectively unlimited.
pub const MAX_DOCK_HEIGHT: f64 = 1_000_000.0;

pub const ZOOM_MIN: f64 = 0.5;
pub const ZOOM_MAX: f64 = 2.0;
pub const ZOOM_STEP: f64 = 0.1;

// ---- Statics ----

pub static CURRENT_ZOOM: Mutex<f64> = Mutex::new(1.0);
pub static CALL_WINDOW_COUNTER: AtomicU32 = AtomicU32::new(0);
pub static IS_LOGGED_OUT: AtomicBool = AtomicBool::new(true);
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
pub static CONTEXT_MENU_SCRIPT: &str = include_str!("scripts/context-menu.js");

/// Reads the sidebar state out of the chat document: the live collapse class
/// plus the stored width, as `[collapsed ? 1 : 0, width | null]`.
pub(crate) const SIDEBAR_STATE_SCRIPT: &str = "(() => { try { const width = localStorage.getItem('mesa:sidebar-width'); const collapsedKey = localStorage.getItem('mesa:sidebar-collapsed'); if (width === null && collapsedKey === null) return null; const collapsed = document.documentElement.classList.contains('sidebar-collapsed'); return [collapsed ? 1 : 0, width ? Number(width) : null] } catch (_) { return null } })()";

/// Reads the sidebar state out of the chat's localStorage rather than the DOM
/// class: at page load the sidebar script has only just installed, so the
/// class may not reflect the restored state yet.
pub(crate) const SIDEBAR_STORAGE_SCRIPT: &str = "(() => { try { const width = localStorage.getItem('mesa:sidebar-width'); const collapsed = localStorage.getItem('mesa:sidebar-collapsed'); if (width === null && collapsed === null) return null; return [collapsed === 'true' ? 1 : 0, width ? Number(width) : null] } catch (_) { return null } })()";

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

/// Points the titlebar's `--facebook-blue` at the wallpaper accent while the
/// normalised chat theme is on; otherwise the base.css default applies.
pub fn apply_accent_to_titlebar(app: &tauri::AppHandle) {
    let Some(titlebar) = app.get_webview("top_bar") else {
        return;
    };

    let js = if GLOBAL_THEME_ENABLED.load(Ordering::Relaxed) {
        let (r, g, b) = accent::accent_rgb();
        format!(
            "document.documentElement.style.setProperty('--facebook-blue', '#{r:02x}{g:02x}{b:02x}')"
        )
    } else {
        "document.documentElement.style.removeProperty('--facebook-blue')".into()
    };
    let _ = titlebar.eval(js);
}

// ---- App entry point ----

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                // The app owns visibility: launching into tray mode must stay
                // hidden until the first tray click, so the plugin only
                // manages geometry.
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        )
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
            commands::set_sidebar_state,
            commands::get_auto_updates,
            commands::set_auto_updates,
            commands::get_tray_mode,
            commands::set_tray_mode,
        ])
        .setup(|app| {
            let settings = settings::SettingsStore::load(app.handle());
            let sidebar = settings.sidebar();
            let global_theme = settings.global_theme();
            let tray_mode = settings.tray_mode();
            GLOBAL_THEME_ENABLED.store(global_theme, Ordering::Relaxed);
            tray::TRAY_MODE_ENABLED.store(tray_mode, Ordering::Relaxed);
            app.manage(settings);

            // The wallpaper decode is seconds-slow; it must not run on the
            // main thread.
            if global_theme {
                spawn_accent_warmup(app.handle().clone());
            }

            // In tray mode the app starts as just a tray icon; the panel is
            // shown (and docked) on the first tray click.
            let window = window::create_window(app, tray_mode)?;

            // The tray icon exists for the whole session; it is only visible
            // while tray mode is on. Built after the window so the glyph can
            // follow the system theme.
            tray::create_tray(app.handle(), tray_mode)?;

            let chat = window::create_chat(&window, &sidebar, global_theme)?;
            window::disable_chat_devtools(&chat);
            let titlebar = window::create_titlebar(&window)?;

            window::apply_layout(&window, &titlebar, &chat);
            window::setup_resize_handler(&window, &titlebar, &chat);
            tray::setup_close_handler(&window);
            tray::setup_dock_resize_handler(app.handle(), &window);
            zoom::setup_zoom_shortcuts(app, &window);

            // WebView2 controller creation re-shows the host window even
            // when it was built hidden, so enforce the tray-mode launch
            // state once the webviews exist.
            if tray_mode {
                let _ = window.hide();
            }

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

        // Re-apply the theme and tint with the real accent now that the
        // cache is warm.
        let app_for_apply = app.clone();
        let _ = app.run_on_main_thread(move || {
            apply_accent_to_titlebar(&app_for_apply);

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

pub(crate) fn parse_sidebar_state(result: &str) -> Option<(Option<u32>, bool)> {
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
    fn parses_sidebar_state() {
        assert_eq!(parse_sidebar_state("[0,280]"), Some((Some(280), false)));
        assert_eq!(parse_sidebar_state("[1,null]"), Some((None, true)));
        assert_eq!(
            parse_sidebar_state("\"[0, 300.4]\""),
            Some((Some(300), false))
        );
        assert_eq!(parse_sidebar_state("null"), None);
        assert_eq!(parse_sidebar_state(""), None);
    }
}
