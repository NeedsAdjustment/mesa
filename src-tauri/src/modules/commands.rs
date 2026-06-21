use tauri::Manager;
#[cfg(target_os = "macos")]
use tauri::window::{Effect, EffectState, EffectsBuilder};
#[cfg(target_os = "windows")]
use window_vibrancy::apply_acrylic;

use crate::{CURRENT_THEME, CURRENT_ZOOM, INITIAL_WIDTH, apply_theme_to_chat};

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
pub fn focus_chat(window: tauri::Window) {
    if let Some(chat) = window.get_webview("chat_window") {
        let _ = chat.set_focus();
    }
}

#[tauri::command]
pub fn show_devtools(window: tauri::Window) {
    if let Some(chat) = window.get_webview("chat_window") {
        let _ = chat.open_devtools();
    }
}

#[tauri::command]
pub fn logout(window: tauri::Window) {
    if let Some(chat) = window.get_webview("chat_window") {
        let _ = chat.clear_all_browsing_data();
        let _ = chat.eval(r#"window.location.href = 'https://www.facebook.com/messages/'"#);
    }
}

#[tauri::command]
pub fn check_logged_in(window: tauri::Window) -> bool {
    if let Some(chat) = window.get_webview("chat_window") {
        if let Ok(url) = chat.url() {
            let s = url.as_str();
            return !(s == "about:blank" || s.contains("/login"));
        }
    }
    false
}

#[tauri::command]
pub fn set_theme(window: tauri::Window, theme: String) {
    if let Ok(mut current) = CURRENT_THEME.lock() {
        *current = theme.clone();
    }
    apply_theme_to_chat(&window, &theme);
}

#[tauri::command]
pub fn set_backdrop_blur(window: tauri::Window, enabled: bool) {
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
