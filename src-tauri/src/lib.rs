use std::sync::LazyLock;

use tauri::{
    Manager, WebviewUrl, WebviewBuilder, LogicalPosition, LogicalSize,
    WindowBuilder, WindowEvent,
};

#[allow(unused_imports)]
use window_vibrancy::{apply_acrylic, apply_vibrancy, NSVisualEffectMaterial};

const MESSENGER_URL: &str = "https://facebook.com/messages";
const MESSENGER_STYLE_ID: &str = "mesa-custom-messenger-css";
const MESSENGER_CSS: &str = include_str!("messenger.css");
const INJECT_JS: &str = include_str!("inject.js");

const TITLEBAR_HEIGHT: f64 = 32.;
const INITIAL_WIDTH: f64 = 800.;
const INITIAL_HEIGHT: f64 = 600.;

#[cfg(target_os = "windows")]
const ACRYLIC_COLOR: (u8, u8, u8, u8) = (18, 18, 18, 125);

const INJECT_URLS: &[&str] = &[
    "facebook.com/messages",
];

static INJECT_SCRIPT: LazyLock<String> = LazyLock::new(|| {
    let css = serde_json::to_string(MESSENGER_CSS).expect("failed to serialize custom CSS");
    let style_id =
        serde_json::to_string(MESSENGER_STYLE_ID).expect("failed to serialize style id");

    INJECT_JS
        .replace("{STYLE_ID}", &style_id)
        .replace("{CSS}", &css)
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
    if let Err(e) =
        apply_vibrancy(window, NSVisualEffectMaterial::HudWindow, None, None)
    {
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
    let chat = window.add_child(
        WebviewBuilder::new(
            "chat_window",
            WebviewUrl::External(
                MESSENGER_URL
                    .parse()
                    .expect("valid messenger url"),
            ),
        )
        .transparent(true)
        .on_page_load(|window, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished
                && should_inject_css(payload.url())
            {
                let _ = window.eval(&*INJECT_SCRIPT);
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
    // Two clones are needed: `on_window_event` consumes self, so one clone is
    // consumed by the method call, and the other is captured by the closure.
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

#[tauri::command]
fn resize_titlebar(window: tauri::Window, height: f64) {
    if let Some(titlebar) = window.get_webview("top_bar") {
        let width = window.inner_size().map(|s| {
            let scale = window.scale_factor().unwrap_or(1.0);
            s.to_logical::<f64>(scale).width
        }).unwrap_or(INITIAL_WIDTH);
        let _ = titlebar.set_size(LogicalSize::new(width, height));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![resize_titlebar])
        .setup(|app| {
            let window = create_window(app)?;
            apply_platform_effects(&window);

            let chat = create_chat(&window)?;
            let titlebar = create_titlebar(&window)?;

            apply_layout(&window, &titlebar, &chat);
            setup_resize_handler(&window, &titlebar, &chat);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
