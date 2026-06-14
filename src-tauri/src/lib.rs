use tauri::{
    menu::{Menu, MenuItem, Submenu},
    Manager, Theme, WebviewUrl, WebviewBuilder, LogicalPosition, LogicalSize, WindowBuilder, WindowEvent
};

use window_vibrancy::{apply_acrylic, apply_vibrancy, NSVisualEffectMaterial};

const MESSENGER_URL: &str = "https://www.facebook.com/messages";
const MESSENGER_STYLE_ID: &str = "mesa-custom-messenger-css";
const MESSENGER_CSS: &str = include_str!("messenger.css");

const TITLEBAR_HEIGHT: f64 = 32.;

fn apply_layout(
    window: &tauri::Window,
    titlebar: &tauri::webview::Webview,
    chat: &tauri::webview::Webview,
) {
    let Ok(size) = window.inner_size() else {
        return;
    };
    let Ok(scale_factor) = window.scale_factor() else {
        return;
    };

    let logical = size.to_logical::<f64>(scale_factor);
    let width = logical.width;
    let height = logical.height;
    let chat_height = (height - TITLEBAR_HEIGHT).max(0.0);

    let _ = titlebar.set_position(LogicalPosition::new(0.0, 0.0));
    let _ = titlebar.set_size(LogicalSize::new(width, TITLEBAR_HEIGHT));

    let _ = chat.set_position(LogicalPosition::new(0.0, TITLEBAR_HEIGHT));
    let _ = chat.set_size(LogicalSize::new(width, chat_height));
}

fn should_inject_css(url: &tauri::Url) -> bool {
    matches!(
        url.host_str(),
        Some("facebook.com") | Some("www.facebook.com") | Some("messenger.com") | Some("www.messenger.com")
    )
}

fn build_css_script() -> String {
    let css = serde_json::to_string(MESSENGER_CSS).expect("failed to serialize custom CSS");

    format!(
                r#"(() => {{
  const styleId = {style_id};
  const css = {css};
    const ensureStyleLast = () => {{
        const head = document.head || document.documentElement;
        let style = document.getElementById(styleId);

        if (!style) {{
            style = document.createElement('style');
            style.id = styleId;
            style.type = 'text/css';
            style.textContent = css;
            head.appendChild(style);
            return;
        }}

        if (style.textContent !== css) {{
            style.textContent = css;
        }}

        if (style.parentNode !== head || head.lastElementChild !== style) {{
            head.appendChild(style);
        }}
    }};

    ensureStyleLast();

    if (!window.__mesaCssObserver) {{
        window.__mesaCssObserver = new MutationObserver(() => ensureStyleLast());
        window.__mesaCssObserver.observe(document.documentElement, {{
            childList: true,
            subtree: true,
        }});
    }}
}})();"#,
        style_id = serde_json::to_string(MESSENGER_STYLE_ID).expect("failed to serialize style id"),
        css = css,
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
          let width = 800.;
          let height = 600.;

          let window = WindowBuilder::new(
              app,
              "main"
          )
          .inner_size(width, height)
          .title("Mesa")
          .decorations(false)
          .transparent(true)
          .build()?;

          #[cfg(target_os = "macos")]
          apply_vibrancy(&window, NSVisualEffectMaterial::HudWindow, None, None).expect("Unsupported platform! 'apply_vibrancy' is only supported on macOS");

          #[cfg(target_os = "windows")]
          apply_acrylic(&window, Some((18, 18, 18, 125))).expect("Unsupported platform! 'apply_blur' is only supported on Windows");

          let titlebar = window.add_child(
            WebviewBuilder::new(
              "top_bar",
              WebviewUrl::App("index.html".into()),
            )
            .transparent(true),
            LogicalPosition::new(0.,0.),
            LogicalSize::new(width, TITLEBAR_HEIGHT),
            
          )?;

          let chat = window.add_child(
            WebviewBuilder::new(
              "chat_window",
              WebviewUrl::External(MESSENGER_URL.parse().expect("valid messenger url"))
            )
            .transparent(true)
            .on_page_load(|window, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished
          && should_inject_css(payload.url()){
                let _ = window.eval(build_css_script());
            }
          }),
            LogicalPosition::new(0.,TITLEBAR_HEIGHT),
            LogicalSize::new(width, height - TITLEBAR_HEIGHT)
          )?;

          apply_layout(&window, &titlebar, &chat);

          {
            let window_clone = window.clone();
            let titlebar_clone = titlebar.clone();
            let chat_clone = chat.clone();

            window.on_window_event(move |event| {
                if let WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } = event {
                    apply_layout(&window_clone, &titlebar_clone, &chat_clone);
                }
            });
          }
          Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}