use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex};

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Position, Size};

use crate::{DOCK_HEIGHT, DOCK_WIDTH, INITIAL_HEIGHT, INITIAL_WIDTH, MAX_DOCK_HEIGHT};

pub const TRAY_ID: &str = "mesa-tray";
const MENU_OPEN_ID: &str = "open";
const MENU_UNDOCK_ID: &str = "undock";
const MENU_QUIT_ID: &str = "quit";

pub static TRAY_MODE_ENABLED: AtomicBool = AtomicBool::new(false);

/// Whether the window is currently parked against the tray icon. Position
/// updates (repin on resize, glides) only make sense once it is.
static DOCKED: AtomicBool = AtomicBool::new(false);

/// Generation counter for in-flight glides; bumping it cancels them.
static GLIDE_GENERATION: AtomicU32 = AtomicU32::new(0);

/// Total glide duration. More steps within it means a smoother animation, not
/// a longer one.
const GLIDE_DURATION_MS: f64 = 220.0;

/// Shortest allowed glide step. A floor for pathological refresh-rate
/// reports, so the animation can never turn into an unthrottled stampede.
const GLIDE_MIN_STEP_MS: f64 = 4.0;

/// Window bounds from before tray mode was enabled, so switching the mode
/// off returns the window to where it was.
static NORMAL_BOUNDS: LazyLock<Mutex<Option<((i32, i32), (u32, u32))>>> =
    LazyLock::new(|| Mutex::new(None));

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl ScreenRect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn center_x(&self) -> i32 {
        self.x + self.width / 2
    }

    pub fn center_y(&self) -> i32 {
        self.y + self.height / 2
    }
}

/// Compute where the docked panel should sit on screen.
///
/// The tray icon always lies outside the monitor's work area, on exactly one
/// side of it. Whichever side that is becomes the edge the panel is pinned
/// against, and the panel is centred on the icon along that edge. All inputs
/// and the return value are in physical pixels.
pub fn docked_position(tray: ScreenRect, work_area: ScreenRect, panel: ScreenRect) -> ScreenRect {
    // How far the tray icon's centre reaches past each work-area edge. The
    // taskbar is on whichever edge the icon sits beyond.
    let beyond_left = (work_area.x - tray.center_x()).max(0);
    let beyond_top = (work_area.y - tray.center_y()).max(0);
    let beyond_right = (tray.center_x() - (work_area.x + work_area.width)).max(0);
    let beyond_bottom = (tray.center_y() - (work_area.y + work_area.height)).max(0);

    // The panel never overflows the work area.
    let width = panel.width.min(work_area.width);
    let height = panel.height.min(work_area.height);

    let clamp_x = |x: i32| {
        x.clamp(
            work_area.x,
            (work_area.x + work_area.width - width).max(work_area.x),
        )
    };
    let clamp_y = |y: i32| {
        y.clamp(
            work_area.y,
            (work_area.y + work_area.height - height).max(work_area.y),
        )
    };

    let (x, y) = if beyond_bottom >= beyond_top
        && beyond_bottom >= beyond_left
        && beyond_bottom >= beyond_right
    {
        // Taskbar at the bottom (the common case): panel sits just above it.
        let y = work_area.y + work_area.height - height;
        (clamp_x(tray.center_x() - width / 2), y)
    } else if beyond_top > beyond_left && beyond_top > beyond_right {
        // Taskbar at the top: panel hangs just below it.
        (clamp_x(tray.center_x() - width / 2), work_area.y)
    } else if beyond_left >= beyond_right {
        // Taskbar on the left: panel starts at the work area's left edge.
        let x = work_area.x;
        (x, clamp_y(tray.center_y() - height / 2))
    } else {
        // Taskbar on the right: panel flush against the work area's right edge.
        let x = work_area.x + work_area.width - width;
        (x, clamp_y(tray.center_y() - height / 2))
    };

    ScreenRect::new(x, y, width, height)
}

/// The tray glyph follows the taskbar's theme: white on dark, black on light.
fn tray_icon(theme: tauri::Theme) -> tauri::image::Image<'static> {
    let bytes: &'static [u8] = match theme {
        tauri::Theme::Dark => include_bytes!("../../icons/tray-white.png"),
        _ => include_bytes!("../../icons/tray-black.png"),
    };
    tauri::image::Image::from_bytes(bytes).expect("embedded tray icon should be valid")
}

/// Re-point the tray glyph when the system theme changes.
pub fn set_tray_theme(app: &AppHandle, theme: tauri::Theme) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let _ = tray.set_icon(Some(tray_icon(theme)));
}

/// Build the tray icon. It is created hidden unless tray mode is active; its
/// visibility is toggled by [`apply_tray_mode`].
pub fn create_tray(app: &AppHandle, visible: bool) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id(MENU_OPEN_ID, "Open Mesa").build(app)?;
    let undock_item = MenuItemBuilder::with_id(MENU_UNDOCK_ID, "Undock from Tray").build(app)?;
    let quit = MenuItemBuilder::with_id(MENU_QUIT_ID, "Quit Mesa").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&open)
        .item(&undock_item)
        .separator()
        .item(&quit)
        .build()?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Mesa")
        .menu(&menu)
        // Left-click toggles the panel; the menu stays on right-click.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_OPEN_ID => show_docked(app),
            MENU_UNDOCK_ID => undock(app),
            MENU_QUIT_ID => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_docked(tray.app_handle());
            }
        });

    let theme = app
        .get_window("main")
        .and_then(|window| window.theme().ok())
        .unwrap_or(tauri::Theme::Light);
    builder = builder.icon(tray_icon(theme));

    let tray = builder.build(app)?;
    tray.set_visible(visible)
}

/// Switch tray mode on or off. Persists nothing itself; the caller owns the
/// setting.
pub fn apply_tray_mode(app: &AppHandle, enabled: bool) {
    let Some(window) = app.get_window("main") else {
        return;
    };

    TRAY_MODE_ENABLED.store(enabled, Ordering::Relaxed);

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_visible(enabled);
    }

    if enabled {
        remember_normal_bounds(&window);
        let _ = window.set_skip_taskbar(true);
        // The panel width is fixed; the height may grow but not shrink.
        let _ = window.set_min_size(Some(tauri::LogicalSize::new(DOCK_WIDTH, DOCK_HEIGHT)));
        let _ = window.set_max_size(Some(tauri::LogicalSize::new(DOCK_WIDTH, MAX_DOCK_HEIGHT)));
        schedule_dock_glide(app);
    } else {
        let _ = window.set_skip_taskbar(false);
        let _ = window.set_min_size(Some(tauri::LogicalSize::new(DOCK_WIDTH, 400.0)));
        let _ = window.set_max_size(None::<tauri::LogicalSize<f64>>);
        DOCKED.store(false, Ordering::Relaxed);
        // Cancel any in-flight dock glide before animating back.
        GLIDE_GENERATION.fetch_add(1, Ordering::Relaxed);
        restore_with_glide(&window);
    }
}

/// Read the tray icon's rect as plain physical numbers, comparable across
/// polls.
fn read_tray_rect(tray: &TrayIcon) -> Option<(i32, i32, i32, i32)> {
    let rect = tray.rect().ok()??;
    // tray-icon reports physical pixels; the fixed scale keeps the values
    // comparable, which is all the polling needs.
    let position = rect.position.to_physical::<i32>(1.0);
    let size = rect.size.to_physical::<i32>(1.0);
    Some((position.x, position.y, size.width, size.height))
}

/// Wait until Explorer has laid the tray icon out: the rect is zero until
/// then, and docking against it parks the panel at a screen edge. The
/// deadline keeps a missing icon from stalling the dock.
fn wait_for_settled_tray_rect(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };

    let start = std::time::Instant::now();
    let deadline = start + std::time::Duration::from_secs(2);
    let mut last = read_tray_rect(&tray);

    loop {
        std::thread::sleep(std::time::Duration::from_millis(60));

        let current = read_tray_rect(&tray);
        if let (Some(previous), Some(current)) = (last, current) {
            let valid = current.2 > 0 && current.3 > 0;
            if valid && previous == current {
                eprintln!(
                    "[mesa] dock: tray rect settled after {:?}",
                    start.elapsed()
                );
                return;
            }
        }
        last = current;

        if std::time::Instant::now() >= deadline {
            eprintln!("[mesa] dock: tray rect never settled, docking anyway");
            return;
        }
    }
}

/// A monitor's refresh rate in Hz, or 0 when it cannot be determined.
#[cfg(target_os = "windows")]
fn monitor_refresh_hz(monitor: &tauri::Monitor) -> u32 {
    use windows::Win32::Graphics::Gdi::{
        DISPLAY_DEVICEW, DEVMODEW, DISPLAY_DEVICE_ACTIVE, ENUM_CURRENT_SETTINGS,
        EnumDisplayDevicesW, EnumDisplaySettingsW,
    };

    let position = monitor.position();
    let size = monitor.size();

    // Display settings are addressed by display device, so walk them and
    // match the one whose current-mode geometry equals the monitor's.
    let mut index: u32 = 0;
    let mut device = DISPLAY_DEVICEW::default();
    device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;

    while unsafe { EnumDisplayDevicesW(windows::core::PCWSTR::null(), index, &mut device, 0) }
        .as_bool()
    {
        index += 1;

        if device.StateFlags.0 & DISPLAY_DEVICE_ACTIVE.0 == 0 {
            continue;
        }

        let mut mode = DEVMODEW::default();
        mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        if unsafe {
            EnumDisplaySettingsW(
                windows::core::PCWSTR(device.DeviceName.as_ptr()),
                ENUM_CURRENT_SETTINGS,
                &mut mode,
            )
        }
        .as_bool()
        {
            let mode_position = unsafe { mode.Anonymous1.Anonymous2.dmPosition };
            if mode_position.x == position.x
                && mode_position.y == position.y
                && mode.dmPelsWidth == size.width
                && mode.dmPelsHeight == size.height
            {
                let frequency = mode.dmDisplayFrequency;
                // 0 and 1 mean "default/unknown" rather than a real rate.
                return if frequency >= 10 { frequency } else { 0 };
            }
        }
    }

    0
}

#[cfg(target_os = "windows")]
fn monitor_hz(monitor: Option<&tauri::Monitor>) -> u32 {
    monitor.map(monitor_refresh_hz).unwrap_or(0)
}

#[cfg(not(target_os = "windows"))]
fn monitor_hz(_monitor: Option<&tauri::Monitor>) -> u32 {
    0
}

/// Step interval for the glide, derived from the displays involved.
///
/// Sample at the faster of the two: the compositor drops whatever it cannot
/// present. Unknown rates assume 60Hz.
fn glide_interval_ms(departure_hz: u32, destination_hz: u32) -> f64 {
    let hz = departure_hz.max(destination_hz);
    let hz = if hz == 0 { 60 } else { hz };
    (1000.0 / hz as f64).max(GLIDE_MIN_STEP_MS)
}

/// Glide the panel to the tray icon once it has settled, morphing position
/// and size in one motion.
fn schedule_dock_glide(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        wait_for_settled_tray_rect(&app);

        let Some(window) = app.get_window("main") else {
            return;
        };
        if !TRAY_MODE_ENABLED.load(Ordering::Relaxed) {
            return;
        }

        let Some(dock) = dock_panel_target(&app, &window) else {
            eprintln!("[mesa] dock: tray rect unavailable, keeping current geometry");
            return;
        };

        let Ok(position) = window.outer_position() else {
            return;
        };
        let Ok(size) = window.inner_size() else {
            return;
        };

        // Sample at the faster of the departure and taskbar displays.
        let interval = glide_interval_ms(
            monitor_hz(window.current_monitor().ok().flatten().as_ref()),
            monitor_hz(
                window
                    .monitor_from_point(
                        (dock.x + dock.width / 2) as f64,
                        (dock.y + dock.height / 2) as f64,
                    )
                    .ok()
                    .flatten()
                    .as_ref(),
            ),
        );

        let generation = GLIDE_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
        glide_window(
            &window,
            (
                position.x as f64,
                position.y as f64,
                size.width as f64,
                size.height as f64,
            ),
            (
                dock.x as f64,
                dock.y as f64,
                dock.width as f64,
                dock.height as f64,
            ),
            interval,
            generation,
            true,
        );
    });
}

/// Leave tray mode by growing and gliding back into the remembered window
/// bounds in a single motion.
fn restore_with_glide(window: &tauri::Window) {
    let bounds = NORMAL_BOUNDS.lock().ok().and_then(|mut bounds| bounds.take());

    let Some(((x, y), (width, height))) = bounds else {
        // No remembered bounds (e.g. the app launched straight into tray
        // mode): fall back to the standard window, centred on its monitor.
        let _ = window.set_size(tauri::LogicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT));
        let _ = window.center();
        return;
    };

    let Ok(position) = window.outer_position() else {
        let _ = window.set_size(Size::Physical(tauri::PhysicalSize::new(width, height)));
        let _ = window.set_position(Position::Physical(tauri::PhysicalPosition::new(x, y)));
        return;
    };
    let Ok(size) = window.inner_size() else {
        let _ = window.set_size(Size::Physical(tauri::PhysicalSize::new(width, height)));
        let _ = window.set_position(Position::Physical(tauri::PhysicalPosition::new(x, y)));
        return;
    };

    // Sample at the faster of the departure and destination displays.
    let interval = glide_interval_ms(
        monitor_hz(window.current_monitor().ok().flatten().as_ref()),
        monitor_hz(
            window
                .monitor_from_point(x as f64 + width as f64 / 2.0, y as f64 + height as f64 / 2.0)
                .ok()
                .flatten()
                .as_ref(),
        ),
    );

    let generation = GLIDE_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    glide_window(
        window,
        (
            position.x as f64,
            position.y as f64,
            size.width as f64,
            size.height as f64,
        ),
        (x as f64, y as f64, width as f64, height as f64),
        interval,
        generation,
        false,
    );
}

/// Morph the window from one `(x, y, width, height)` rect to another,
/// sampled at the displays' refresh rates.
///
/// `mark_docked`: consider the panel docked at the end, so resize repinning
/// only engages at the final geometry.
fn glide_window(
    window: &tauri::Window,
    from: (f64, f64, f64, f64),
    to: (f64, f64, f64, f64),
    interval_ms: f64,
    generation: u32,
    mark_docked: bool,
) {
    let window = window.clone();
    std::thread::spawn(move || {
        let interval = std::time::Duration::from_secs_f64(interval_ms / 1000.0);
        let steps = ((GLIDE_DURATION_MS / interval_ms).round() as i32).max(8);

        for step in 1..=steps {
            std::thread::sleep(interval);
            if GLIDE_GENERATION.load(Ordering::Relaxed) != generation {
                return;
            }

            let eased = ease_out_cubic(step as f64 / steps as f64);

            let _ = window.set_size(Size::Physical(tauri::PhysicalSize::new(
                (from.2 + (to.2 - from.2) * eased).round() as u32,
                (from.3 + (to.3 - from.3) * eased).round() as u32,
            )));
            let _ = window.set_position(Position::Physical(tauri::PhysicalPosition::new(
                (from.0 + (to.0 - from.0) * eased).round() as i32,
                (from.1 + (to.1 - from.1) * eased).round() as i32,
            )));
        }

        if mark_docked {
            DOCKED.store(true, Ordering::Relaxed);
        }
    });
}

fn ease_out_cubic(progress: f64) -> f64 {
    1.0 - (1.0 - progress).powi(3)
}

/// Leave tray mode from the tray icon's menu.
fn undock(app: &AppHandle) {
    app.state::<crate::settings::SettingsStore>()
        .update_tray_mode(false);
    apply_tray_mode(app, false);
}

/// Toggle the docked panel from a tray icon click.
fn toggle_docked(app: &AppHandle) {
    let Some(window) = app.get_window("main") else {
        return;
    };

    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        show_docked(app);
    }
}

/// Show the panel, re-docked at the tray icon's current position.
fn show_docked(app: &AppHandle) {
    let Some(window) = app.get_window("main") else {
        return;
    };

    dock_window(app, &window);
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// The tray icon rect and its monitor's work area, in physical pixels.
fn tray_and_work_area(
    app: &AppHandle,
    window: &tauri::Window,
) -> Option<(ScreenRect, ScreenRect)> {
    let tray = app.tray_by_id(TRAY_ID)?;
    let rect = tray.rect().ok()?;
    let rect = rect?;

    let scale = window.scale_factor().unwrap_or(1.0);

    // tray-icon reports physical pixels; convert defensively via the
    // logical/physical helpers so either encoding works.
    let position = rect.position.to_physical::<i32>(scale);
    let size = rect.size.to_physical::<i32>(scale);
    let tray_rect = ScreenRect::new(position.x, position.y, size.width, size.height);

    // The work area of the monitor holding the tray icon.
    let Ok(Some(monitor)) = window.monitor_from_point(
        tray_rect.center_x() as f64,
        tray_rect.center_y() as f64,
    ) else {
        return None;
    };
    let area = monitor.work_area();
    let work_area = ScreenRect::new(
        area.position.x,
        area.position.y,
        area.size.width as i32,
        area.size.height as i32,
    );

    Some((tray_rect, work_area))
}

/// Where the docked panel should sit, at its standard size.
fn dock_panel_target(app: &AppHandle, window: &tauri::Window) -> Option<ScreenRect> {
    let (tray_rect, work_area) = tray_and_work_area(app, window)?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let panel = ScreenRect::new(
        0,
        0,
        (DOCK_WIDTH * scale).round() as i32,
        (DOCK_HEIGHT * scale).round() as i32,
    );
    let dock = docked_position(tray_rect, work_area, panel);

    eprintln!(
        "[mesa] dock: tray=({},{} {}x{}) work=({},{} {}x{}) panel={}x{} scale={scale} -> ({},{}) {}x{}",
        tray_rect.x,
        tray_rect.y,
        tray_rect.width,
        tray_rect.height,
        work_area.x,
        work_area.y,
        work_area.width,
        work_area.height,
        panel.width,
        panel.height,
        dock.x,
        dock.y,
        dock.width,
        dock.height
    );

    Some(dock)
}

/// Size and position the window as a docked chat panel next to the tray icon.
fn dock_window(app: &AppHandle, window: &tauri::Window) {
    let Some(dock) = dock_panel_target(app, window) else {
        eprintln!("[mesa] dock: tray rect unavailable, keeping current geometry");
        return;
    };

    let _ = window.set_size(Size::Physical(tauri::PhysicalSize::new(
        dock.width as u32,
        dock.height as u32,
    )));
    let _ = window.set_position(Position::Physical(tauri::PhysicalPosition::new(
        dock.x,
        dock.y,
    )));
    DOCKED.store(true, Ordering::Relaxed);
}

/// Keep the panel's taskbar edge anchored while its height changes.
///
/// With a bottom taskbar the panel grows upward (the bottom stays flush), and
/// with a top taskbar it grows downward from the taskbar edge.
fn repin_docked(app: &AppHandle, window: &tauri::Window) {
    let Some((tray_rect, work_area)) = tray_and_work_area(app, window) else {
        return;
    };
    let Ok(size) = window.inner_size() else {
        return;
    };

    let panel = ScreenRect::new(0, 0, size.width as i32, size.height as i32);
    let dock = docked_position(tray_rect, work_area, panel);
    let _ = window.set_position(Position::Physical(tauri::PhysicalPosition::new(
        dock.x,
        dock.y,
    )));
}

/// In tray mode a close request hides the panel; the titlebar has no close
/// button, so this covers Alt+F4.
pub fn setup_close_handler(window: &tauri::Window) {
    let window_for_event = window.clone();
    let window_for_close = window.clone();
    window_for_event.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            if TRAY_MODE_ENABLED.load(Ordering::Relaxed) {
                api.prevent_close();
                let _ = window_for_close.hide();
            }
        }
    });
}

/// While tray mode is active, keep the panel anchored to the taskbar edge as
/// its height is resized.
pub fn setup_dock_resize_handler(app: &AppHandle, window: &tauri::Window) {
    let window_for_event = window.clone();
    let app_for_pin = app.clone();
    let window_for_pin = window.clone();
    window_for_event.on_window_event(move |event| {
        if let tauri::WindowEvent::Resized(_) = event
            && DOCKED.load(Ordering::Relaxed)
        {
            repin_docked(&app_for_pin, &window_for_pin);
        }
    });
}

fn remember_normal_bounds(window: &tauri::Window) {
    let Ok(position) = window.outer_position() else {
        return;
    };
    // Outer position pairs with `set_position`, inner size with `set_size`.
    let Ok(size) = window.inner_size() else {
        return;
    };

    if let Ok(mut bounds) = NORMAL_BOUNDS.lock() {
        *bounds = Some(((position.x, position.y), (size.width, size.height)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRAY: ScreenRect = ScreenRect::new(1800, 1404, 24, 24);
    const PANEL: ScreenRect = ScreenRect::new(0, 0, 316, 520);

    #[test]
    fn glide_interval_samples_at_the_fastest_display() {
        assert!((glide_interval_ms(240, 60) - 1000.0 / 240.0).abs() < 0.01);
        assert!((glide_interval_ms(60, 144) - 1000.0 / 144.0).abs() < 0.01);
        assert!((glide_interval_ms(60, 60) - 1000.0 / 60.0).abs() < 0.01);
    }

    #[test]
    fn glide_interval_falls_back_and_floors() {
        // Unknown rates assume 60Hz rather than dividing by zero.
        assert!((glide_interval_ms(0, 0) - 1000.0 / 60.0).abs() < 0.01);
        // One known display is enough to drive the faster rate.
        assert!((glide_interval_ms(0, 240) - 1000.0 / 240.0).abs() < 0.01);
        // Pathological reports cannot remove the throttle entirely.
        assert_eq!(glide_interval_ms(1000, 1000), 4.0);
    }

    #[test]
    fn ease_out_cubic_starts_fast_and_ends_exact() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        // Front-loaded: a quarter of the time covers more than half the
        // distance, so the glide settles gently at the dock.
        assert!(ease_out_cubic(0.25) > 0.5);
    }

    #[test]
    fn docks_above_a_bottom_taskbar() {
        // 1080p screen, 48px taskbar at the bottom, tray icon at the far
        // right: the panel clamps so it stays fully on screen.
        let work = ScreenRect::new(0, 0, 1920, 1032);
        let tray = ScreenRect::new(1876, 1400, 24, 24);

        let dock = docked_position(tray, work, PANEL);

        assert_eq!(dock.height, 520);
        assert_eq!(dock.y, 1032 - 520);
        assert_eq!(dock.x, 1604); // 1920 - 316: clamped from the icon's centre
    }

    #[test]
    fn docks_below_a_top_taskbar() {
        let work = ScreenRect::new(0, 48, 1920, 1032);
        let tray = ScreenRect::new(1876, 12, 24, 24);

        let dock = docked_position(tray, work, PANEL);

        assert_eq!(dock.y, 48);
        assert_eq!(dock.x, 1604);
    }

    #[test]
    fn docks_right_of_a_left_taskbar() {
        // Icon low on the taskbar: the panel clamps to the work-area bottom.
        let work = ScreenRect::new(48, 0, 1872, 1080);
        let tray = ScreenRect::new(12, 900, 24, 24);

        let dock = docked_position(tray, work, PANEL);

        assert_eq!(dock.x, 48);
        assert_eq!(dock.y, 560); // 1080 - 520: clamped
    }

    #[test]
    fn docks_left_of_a_right_taskbar() {
        let work = ScreenRect::new(0, 0, 1872, 1080);
        let tray = ScreenRect::new(1884, 900, 24, 24);

        let dock = docked_position(tray, work, PANEL);

        assert_eq!(dock.x, 1872 - 316);
        assert_eq!(dock.y, 560);
    }

    #[test]
    fn clamps_to_the_work_area_at_screen_edges() {
        // Tray icon hugging the left edge of a wide screen: the panel would
        // otherwise start off-screen.
        let work = ScreenRect::new(0, 0, 2560, 1392);
        let tray = ScreenRect::new(4, 1400, 24, 24);

        let dock = docked_position(tray, work, PANEL);

        assert_eq!(dock.x, 0);
        assert_eq!(dock.y, 1392 - 520);
    }

    #[test]
    fn panel_never_taller_than_the_work_area() {
        let work = ScreenRect::new(0, 0, 1920, 400);

        let dock = docked_position(TRAY, work, PANEL);

        assert_eq!(dock.height, 400);
        assert_eq!(dock.y, 0);
    }

    #[test]
    fn panel_never_wider_than_the_work_area() {
        let work = ScreenRect::new(0, 0, 200, 1000);
        let tray = ScreenRect::new(204, 400, 24, 24);

        let dock = docked_position(tray, work, PANEL);

        assert_eq!(dock.width, 200);
        assert_eq!(dock.x, 0);
    }
}
