use std::sync::atomic::Ordering;
use tauri::Manager;
use tauri::Url;
#[cfg(target_os = "macos")]
use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri_plugin_opener::open_path;
#[cfg(target_os = "windows")]
use window_vibrancy::apply_acrylic;

use crate::{
    BACKDROP_BLUR_ENABLED, CURRENT_THEME, CURRENT_ZOOM, INITIAL_WIDTH, MESSENGER_URL,
    STANDARD_THEME_ENABLED, WINDOW_NARROW, apply_theme_to_chat,
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
    crate::IS_LOGGING_OUT.store(true, Ordering::Relaxed);

    if let Some(chat) = window.get_webview("chat_window") {
        // Kill the SPA by clearing the document in-place. This removes
        // all JavaScript state and event listeners (including any
        // beforeunload handler) without triggering a navigation.
        let _ = chat.eval(
            "document.open(); document.write('<!DOCTYPE html><html><body>Logging out...</body></html>'); document.close();",
        );

        // Clear all cookies and storage. The SPA is already dead so
        // it can't detect or interfere with this.
        let _ = chat.clear_all_browsing_data();

        // Navigate to the Messenger URL using the Tauri-level API.
        // No beforeunload to block it (SPA was killed above).
        // Facebook sees no session cookies and redirects to /login.php,
        // which is now in the allowlist — so the login page loads
        // right in the webview.
        if let Ok(url) = Url::parse(MESSENGER_URL) {
            let _ = chat.navigate(url);
        }
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

#[tauri::command]
pub fn set_theme(window: tauri::Window, theme: String) {
    if let Ok(mut current) = CURRENT_THEME.lock() {
        *current = theme.clone();
    }
    apply_theme_to_chat(&window, &theme);
}

#[tauri::command]
pub fn set_standard_theme(window: tauri::Window, enabled: bool) {
    STANDARD_THEME_ENABLED.store(enabled, Ordering::Relaxed);
    window
        .app_handle()
        .state::<crate::settings::SettingsStore>()
        .update_standard_theme(enabled);

    if let Some(chat) = window.get_webview("chat_window") {
        let js = if enabled {
            "window.__mesaStandardTheme = true; document.documentElement.classList.add('mesa-standard-theme')"
        } else {
            "window.__mesaStandardTheme = false; document.documentElement.classList.remove('mesa-standard-theme')"
        };
        let _ = chat.eval(js);
    }
}

#[tauri::command]
pub fn get_standard_theme() -> bool {
    STANDARD_THEME_ENABLED.load(Ordering::Relaxed)
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

    if enabled {
        #[cfg(target_os = "macos")]
        let _ = window.set_effects(Some(
            EffectsBuilder::new()
                .effect(Effect::HudWindow)
                .state(EffectState::Active)
                .build(),
        ));
        #[cfg(target_os = "windows")]
        {
            let _ = apply_acrylic(&window, None);
        }
    } else {
        let _ = window.set_effects(None);
    }

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
