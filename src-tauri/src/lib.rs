use tauri::{
    menu::{Menu, MenuItem, Submenu},
    Manager, Theme, WebviewUrl, WebviewBuilder, LogicalPosition, LogicalSize, WindowBuilder, WindowEvent
};

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
          let app_handle = app.handle();
          let dark_item = MenuItem::new(app_handle, "Dark", true, None::<&str>)?;
          let light_item = MenuItem::new(app_handle, "Light", true, None::<&str>)?;
          let system_item = MenuItem::new(app_handle, "System", true, None::<&str>)?;

          let dark_id = dark_item.id().clone();
          let light_id = light_item.id().clone();
          let system_id = system_item.id().clone();

          let theme_menu = Submenu::with_items(
              app_handle,
              "Theme",
              true,
              &[&dark_item, &light_item, &system_item],
          )?;
          let menu = Menu::with_items(app_handle, &[&theme_menu])?;
          app_handle.set_menu(menu)?;

          let width = 800.;
          let height = 600.;

          let window = WindowBuilder::new(
              app,
              "main"
          )
          .inner_size(width, height)
          .title("Mesa")
          .decorations(false)
          .on_menu_event(move |window, event| {
              let theme = if event.id == dark_id {
                  Some(Theme::Dark)
              } else if event.id == light_id {
                  Some(Theme::Light)
              } else if event.id == system_id {
                  None
              } else {
                  return;
              };

              let _ = window.set_theme(theme);
              window.app_handle().set_theme(theme);
          })
          
          .build()?;

          let titlebar = window.add_child(
            WebviewBuilder::new(
              "top_bar",
              WebviewUrl::App("index.html".into()),
            ),
            LogicalPosition::new(0.,0.),
            LogicalSize::new(width, TITLEBAR_HEIGHT),
            
          )?;

          let chat = window.add_child(
            WebviewBuilder::new(
              "chat_window",
              WebviewUrl::External(MESSENGER_URL.parse().expect("valid messenger url"))
            )
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
