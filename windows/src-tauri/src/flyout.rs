// The tray flyout: a small window above the notification area, the way Windows
// opens its own tray icons — Mica, round corners, gone when it loses focus. It
// shows the Claude Code sessions, the permission card, the integrations, and
// lets you jump to a session's window.
//
// The island's page stays the one owner of the sessions and of the permission
// card; the flyout only draws the snapshot it is given (see `publish_shell` in
// lib.rs) and sends what you click back to the island.

use std::sync::Mutex;
use std::time::Instant;

use tauri::window::{Effect, EffectsBuilder};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

use crate::shell::{self, PRect};

pub const LABEL: &str = "flyout";
/// Logical size, like the Windows 11 quick-settings flyout.
const SIZE: (f64, f64) = (380.0, 560.0);
/// Logical gap to the taskbar and to the screen's edges.
const MARGIN: f64 = 12.0;

#[derive(Default)]
pub struct Flyout {
    hidden_at: Mutex<Option<Instant>>,
    /// Where the tray icon was when last clicked, in physical pixels.
    icon: Mutex<Option<PRect>>,
}

fn page_url(app: &AppHandle) -> WebviewUrl {
    #[cfg(dev)]
    if let Some(mut base) = app.config().build.dev_url.clone() {
        base.set_path("/flyout.html");
        return WebviewUrl::External(base);
    }
    let _ = app;
    WebviewUrl::App("flyout.html".into())
}

/// Created hidden at launch, like the settings window, and only shown and hidden
/// afterwards (see `create_settings_window` in lib.rs for why). Not focused when
/// created: a hidden window that takes the keyboard swallows what you type, and
/// this one can hold Allow.
pub fn create(app: &AppHandle, browser_args: &str) {
    let built = WebviewWindowBuilder::new(app, LABEL, page_url(app))
        .additional_browser_args(browser_args)
        .focused(false)
        .title("Coucou")
        .inner_size(SIZE.0, SIZE.1)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .effects(EffectsBuilder::new().effect(Effect::Mica).build())
        .build();
    match built {
        Ok(win) => {
            crate::platform::round_corners(&win);
            let handle = app.clone();
            win.on_window_event(move |event| match event {
                // Like every Windows flyout: click anywhere else and it goes.
                tauri::WindowEvent::Focused(false) => hide(&handle),
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    hide(&handle);
                }
                _ => {}
            });
        }
        Err(err) => crate::log::line(format!("flyout window failed: {err}")),
    }
}

/// The tray icon was clicked: open, or close if it is open.
pub fn toggle(app: &AppHandle, icon: Option<PRect>) {
    let state = app.state::<Flyout>();
    if let Some(icon) = icon {
        *state.icon.lock().unwrap() = Some(icon);
    }
    let Some(win) = app.get_webview_window(LABEL) else { return };
    let visible = win.is_visible().unwrap_or(false);
    let hidden_ago = state.hidden_at.lock().unwrap().map(|t| t.elapsed());
    if visible {
        hide(app);
    } else if shell::should_open(visible, hidden_ago) {
        show(app);
    }
}

pub fn show(app: &AppHandle) {
    let Some(win) = app.get_webview_window(LABEL) else { return };
    if let Some(place) = placement(app) {
        let _ = win.set_size(PhysicalSize::new(place.size.0, place.size.1));
        // Windows can make an undecorated window with a shadow taller than asked
        // (measured: +37 px at 125 %), so the place is worked out from the size
        // the window really has.
        let real = win.outer_size().map(|s| (s.width, s.height)).unwrap_or(place.size);
        let (x, y) = shell::flyout_origin(place.icon, place.work, real, place.margin);
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
    let _ = win.show();
    let _ = win.set_focus();
    let _ = app.emit("flyout-visible", true);
}

pub fn hide(app: &AppHandle) {
    let Some(win) = app.get_webview_window(LABEL) else { return };
    if !win.is_visible().unwrap_or(false) {
        return;
    }
    *app.state::<Flyout>().hidden_at.lock().unwrap() = Some(Instant::now());
    let _ = win.hide();
    let _ = app.emit("flyout-visible", false);
}

pub fn is_visible(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL).and_then(|w| w.is_visible().ok()).unwrap_or(false)
}

struct Placement {
    icon: PRect,
    work: PRect,
    /// Physical size to ask for.
    size: (u32, u32),
    margin: i32,
}

/// What placing the flyout next to the tray icon needs, in physical pixels, on
/// the display the icon is on. Without a click to go by (the menu, a toast), the
/// primary display's corner.
fn placement(app: &AppHandle) -> Option<Placement> {
    let icon = *app.state::<Flyout>().icon.lock().unwrap();
    let monitors = app.available_monitors().ok()?;
    let monitor = icon
        .and_then(|i| {
            let (cx, cy) = (i.x + i.w as i32 / 2, i.y + i.h as i32 / 2);
            monitors.iter().find(|m| {
                let (p, s) = (m.position(), m.size());
                cx >= p.x && cx < p.x + s.width as i32 && cy >= p.y && cy < p.y + s.height as i32
            })
        })
        .cloned()
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let work = PRect { x: area.position.x, y: area.position.y, w: area.size.width, h: area.size.height };
    let size = ((SIZE.0 * scale).round() as u32, (SIZE.1 * scale).round() as u32);
    // No icon: pretend it sits in the corner where the clock usually is.
    let icon = icon.unwrap_or(PRect { x: work.x + work.w as i32 - 1, y: work.y + work.h as i32, w: 1, h: 1 });
    Some(Placement { icon, work, size, margin: (MARGIN * scale).round() as i32 })
}
