use std::sync::atomic::Ordering;
use tauri::Manager;
use tauri::Url;
#[cfg(target_os = "macos")]
use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri_plugin_opener::open_path;

use crate::{
    BACKDROP_BLUR_ENABLED, CURRENT_THEME, CURRENT_ZOOM, GLOBAL_THEME_ENABLED, INITIAL_WIDTH,
    MESSENGER_URL, WINDOW_NARROW, apply_theme_to_chat,
};

#[tauri::command]
pub fn resize_titlebar(window: tauri::Window, height: f64) {
    if let Some(titlebar) = window.get_webview("top_bar") {
        let width = window
            .inner_size()
            .map(|s| {
                let scale = window.scale_factor().unwrap_or(1.0);
                s.to_logical::<f64>(scale).width
            })
            .unwrap_or(INITIAL_WIDTH);
        let _ = titlebar.set_size(tauri::LogicalSize::new(width, height));
    }
}

#[tauri::command]
pub fn set_chat_zoom(window: tauri::Window, zoom: f64) {
    if let Some(chat) = window.get_webview("chat_window") {
        let _ = chat.set_zoom(zoom);
        if let Ok(mut current) = CURRENT_ZOOM.lock() {
            *current = zoom;
        }
    }
}

#[tauri::command]
pub fn get_chat_zoom() -> f64 {
    CURRENT_ZOOM.lock().map(|z| *z).unwrap_or(1.0)
}

#[tauri::command]
pub fn simulate_shortcut(window: tauri::Window, action: String) {
    let Some(chat) = window.get_webview("chat_window") else {
        return;
    };

    // The command acts on the focused element, so make sure the chat owns
    // focus before running it.
    let _ = chat.set_focus();

    let command = match action.as_str() {
        "undo" | "redo" | "cut" | "copy" | "paste" | "delete" | "selectAll" => action.as_str(),
        _ => return,
    };

    let js = format!(
        "(() => {{ try {{ return document.execCommand('{command}') }} catch (error) {{ console.warn('[mesa] edit command failed', error); return false }} }})()"
    );

    let action_for_log = action;
    let _ = chat.eval_with_callback(js, move |result| {
        if result.trim() != "true" {
            eprintln!("[mesa] edit command {action_for_log} failed: {result}");
        }
    });
}

#[tauri::command]
pub fn focus_chat(window: tauri::Window) {
    if let Some(chat) = window.get_webview("chat_window") {
        let _ = chat.set_focus();
    }
}

#[tauri::command]
pub fn show_devtools(window: tauri::Window) {
    if let Some(chat) = window.get_webview("chat_window") {
        chat.open_devtools();
    }
}

#[tauri::command]
pub fn logout(window: tauri::Window) {
    if let Some(chat) = window.get_webview("chat_window") {
        // Persist the sidebar state before the webview data is wiped; the
        // durable copy restores it after logging back in.
        let app = window.app_handle().clone();
        let _ = chat.eval_with_callback(crate::SIDEBAR_STATE_SCRIPT, move |result| {
            if let Some((width, collapsed)) = crate::parse_sidebar_state(result.trim()) {
                app.state::<crate::settings::SettingsStore>()
                    .update_sidebar(width, collapsed);
            }

            let Some(chat) = app
                .get_window("main")
                .and_then(|window| window.get_webview("chat_window"))
            else {
                return;
            };

            // Kill the SPA by clearing the document in-place. This removes
            // all JavaScript state and event listeners (including any
            // beforeunload handler) without triggering a navigation.
            let _ = chat.eval(
                "document.open(); document.write('<!DOCTYPE html><html><body>Logging out...</body></html>'); document.close();",
            );

            // Clear all cookies and storage. The SPA is already dead so
            // it can't detect or interfere with this.
            let _ = chat.clear_all_browsing_data();

            // With the SPA dead and cookies cleared, Facebook redirects to
            // /login.php, which is in the allowlist, so the login page
            // loads right in the webview.
            if let Ok(url) = Url::parse(MESSENGER_URL) {
                let _ = chat.navigate(url);
            }
        });
    }
}

#[tauri::command]
pub fn check_logged_in(window: tauri::Window) -> bool {
    if let Some(chat) = window.get_webview("chat_window")
        && let Ok(url) = chat.url()
    {
        let s = url.as_str();
        return !(s == "about:blank" || s.contains("/login"));
    }
    false
}

#[tauri::command]
pub fn show_downloads(window: tauri::Window) -> Result<(), String> {
    let directory = window.path().download_dir().map_err(|e| e.to_string())?;
    open_path(directory.to_string_lossy().into_owned(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_download(path: String) -> Result<(), String> {
    if !std::path::Path::new(&path).exists() {
        return Err("file no longer exists".into());
    }
    open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn acrylic_tint() -> Option<(u8, u8, u8, u8)> {
    if !GLOBAL_THEME_ENABLED.load(Ordering::Relaxed) {
        return None;
    }

    let (r, g, b) = crate::accent::accent_rgb();
    let light = CURRENT_THEME
        .lock()
        .map(|theme| theme.eq_ignore_ascii_case("light"))
        .unwrap_or(false);
    let alpha = if light { 44 } else { 64 };

    Some((r, g, b, alpha))
}

pub(crate) fn apply_blur_effect(window: &tauri::Window, enabled: bool) {
    if enabled {
        #[cfg(target_os = "macos")]
        let _ = window.set_effects(Some(
            EffectsBuilder::new()
                .effect(Effect::HudWindow)
                .state(EffectState::Active)
                .build(),
        ));
        #[cfg(target_os = "windows")]
        crate::backdrop::apply(window, true, acrylic_tint());
    } else {
        #[cfg(target_os = "windows")]
        crate::backdrop::apply(window, false, None);
        #[cfg(not(target_os = "windows"))]
        let _ = window.set_effects(None);
    }
}

#[tauri::command]
pub fn set_theme(window: tauri::Window, theme: String) {
    if let Ok(mut current) = CURRENT_THEME.lock() {
        *current = theme.clone();
    }
    apply_theme_to_chat(&window, &theme);

    if BACKDROP_BLUR_ENABLED.load(Ordering::Relaxed) {
        apply_blur_effect(&window, true);
    }
}

#[tauri::command]
pub fn set_global_theme(window: tauri::Window, enabled: bool) {
    GLOBAL_THEME_ENABLED.store(enabled, Ordering::Relaxed);
    window
        .app_handle()
        .state::<crate::settings::SettingsStore>()
        .update_global_theme(enabled);

    // Start the accent warm-up if it hasn't run yet.
    if enabled && !crate::accent::is_cached() {
        crate::spawn_accent_warmup(window.app_handle().clone());
    }

    if let Some(chat) = window.get_webview("chat_window") {
        let js = if enabled {
            "window.__mesaGlobalTheme = true; document.documentElement.classList.add('mesa-global-theme')"
        } else {
            "window.__mesaGlobalTheme = false; document.documentElement.classList.remove('mesa-global-theme')"
        };
        let _ = chat.eval(js);
    }

    let theme = CURRENT_THEME
        .lock()
        .map(|theme| theme.clone())
        .unwrap_or_else(|_| "dark".into());
    apply_theme_to_chat(&window, &theme);

    if BACKDROP_BLUR_ENABLED.load(Ordering::Relaxed) {
        apply_blur_effect(&window, true);
    }
}

#[tauri::command]
pub fn get_global_theme() -> bool {
    GLOBAL_THEME_ENABLED.load(Ordering::Relaxed)
}

#[tauri::command]
pub fn get_auto_updates(app: tauri::AppHandle) -> bool {
    app.state::<crate::settings::SettingsStore>()
        .auto_updates()
}

#[tauri::command]
pub fn set_auto_updates(app: tauri::AppHandle, enabled: bool) {
    app.state::<crate::settings::SettingsStore>()
        .update_auto_updates(enabled);
}

#[tauri::command]
pub fn get_tray_mode() -> bool {
    crate::tray::TRAY_MODE_ENABLED.load(Ordering::Relaxed)
}

#[tauri::command]
pub fn set_tray_mode(app: tauri::AppHandle, enabled: bool) {
    crate::tray::TRAY_MODE_ENABLED.store(enabled, Ordering::Relaxed);
    app.state::<crate::settings::SettingsStore>()
        .update_tray_mode(enabled);
    crate::tray::apply_tray_mode(&app, enabled);
}

#[tauri::command]
pub fn set_window_narrow(window: tauri::Window, narrow: bool) {
    WINDOW_NARROW.store(narrow, Ordering::Relaxed);

    if let Some(chat) = window.get_webview("chat_window") {
        let js = if narrow {
            "document.documentElement.classList.add('narrow-window')"
        } else {
            "document.documentElement.classList.remove('narrow-window')"
        };
        let _ = chat.eval(js);
    }
}

#[tauri::command]
pub fn set_backdrop_blur(window: tauri::Window, enabled: bool) {
    BACKDROP_BLUR_ENABLED.store(enabled, Ordering::Relaxed);
    apply_blur_effect(&window, enabled);

    // Communicate blur state to the chat webview so CSS can conditionally apply
    // and match webview background: transparent when blur is on, titlebar colour when off
    if let Some(chat) = window.get_webview("chat_window") {
        let js = if enabled {
            "document.documentElement.classList.add('backdrop-blur')"
        } else {
            "document.documentElement.classList.remove('backdrop-blur')"
        };
        let _ = chat.eval(js);
    }
}
