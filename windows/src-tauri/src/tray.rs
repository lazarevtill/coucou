// Notification-area icon. A left click opens the flyout, like Windows' own tray
// icons; the right-click menu keeps Open, Settings, Pause and Quit. The icon wears
// a coloured dot for what is going on (see shell::TrayState) and its tooltip says
// who needs you.

use std::sync::Mutex;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter};

use crate::island::WINDOW_LABEL;
use crate::shell::{self, PRect, TrayState};

const TRAY_ID: &str = "coucou";

/// What the icon shows now, so an unchanged snapshot costs nothing.
static SHOWN: Mutex<Option<(TrayState, String)>> = Mutex::new(None);

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Coucou", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(app, &[&open, &sep1, &settings, &pause, &sep2, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Coucou")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = event {
                crate::flyout::toggle(tray.app_handle(), Some(physical(rect)));
            }
        })
        .on_menu_event(|app: &AppHandle, event| match event.id.as_ref() {
            "quit" => app.exit(0),
            "settings" => crate::show_settings_window(app),
            id => {
                let _ = app.emit_to(WINDOW_LABEL, "tray", id.to_string());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

/// The tray reports the icon's rectangle in physical pixels on Windows.
fn physical(rect: tauri::Rect) -> PRect {
    let p = rect.position.to_physical::<i32>(1.0);
    let s = rect.size.to_physical::<u32>(1.0);
    PRect { x: p.x, y: p.y, w: s.width, h: s.height }
}

/// Puts the state's badge on the icon and sets the tooltip.
pub fn show_state(app: &AppHandle, state: TrayState, tooltip: &str) {
    let tooltip = shell::tooltip(tooltip);
    {
        let mut shown = SHOWN.lock().unwrap();
        if shown.as_ref() == Some(&(state, tooltip.clone())) {
            return;
        }
        *shown = Some((state, tooltip.clone()));
    }
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    if let Some(base) = app.default_window_icon() {
        let (w, h) = (base.width(), base.height());
        let rgba = match state.badge() {
            Some(color) => shell::badged(base.rgba(), w, h, color),
            None => base.rgba().to_vec(),
        };
        let _ = tray.set_icon(Some(Image::new_owned(rgba, w, h)));
    }
    let _ = tray.set_tooltip(Some(tooltip));
}
