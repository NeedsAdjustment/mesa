use tauri::Manager;
use tauri::WindowEvent;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

use crate::{CURRENT_ZOOM, ZOOM_MAX, ZOOM_MIN, ZOOM_STEP};

pub fn setup_zoom_shortcuts(app: &mut tauri::App, window: &tauri::Window) {
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

pub fn apply_zoom_via_app(app: &tauri::AppHandle, zoom: f64) {
    if let Some(window) = app.get_window("main") {
        if let Some(chat) = window.get_webview("chat_window") {
            let _ = chat.set_zoom(zoom);
        }
    }
    if let Ok(mut current) = CURRENT_ZOOM.lock() {
        *current = zoom;
    }
}

/// Called from the global shortcut handler — maps key codes to zoom levels.
pub fn compute_next_zoom(shortcut: &Shortcut) -> Option<f64> {
    let current = CURRENT_ZOOM.lock().map(|z| *z).unwrap_or(1.0);
    Some(match shortcut.key {
        Code::Equal | Code::NumpadAdd => (current + ZOOM_STEP).min(ZOOM_MAX),
        Code::Minus | Code::NumpadSubtract => (current - ZOOM_STEP).max(ZOOM_MIN),
        Code::Digit0 | Code::Numpad0 => 1.0,
        _ => return None,
    })
}
