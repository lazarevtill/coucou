// The island must never be activated by a click or reached with Alt-Tab: a
// window you cannot see that holds the keyboard swallows what you type, and its
// buttons (Allow among them) answer to Enter and Space.
//
// Setting the extended style once is not enough. tao, under Tauri, recomputes
// the whole extended style from its own flags whenever one of them changes —
// click-through is toggled all the time — which drops WS_EX_NOACTIVATE and
// WS_EX_TOOLWINDOW and puts WS_EX_APPWINDOW back. So the style is pinned where
// every change passes: WM_STYLECHANGING.

use ::windows::core::BOOL;
use ::windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use ::windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use ::windows::Win32::System::Threading::GetCurrentProcessId;
use ::windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use ::windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongPtrW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, SetForegroundWindow, SetWindowLongPtrW, GWL_EXSTYLE, GW_OWNER, STYLESTRUCT,
    WM_STYLECHANGING, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

/// Identifies our subclass on the island's window.
const PIN_ID: usize = 0xC0C0;

/// The extended style the island keeps: a tool window, never an app window (out
/// of Alt-Tab and the taskbar), and never activated — unless `activating`, while
/// a text field in it needs the keyboard.
pub fn pinned(ex: u32, activating: bool) -> u32 {
    let ex = (ex | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0;
    if activating {
        ex & !WS_EX_NOACTIVATE.0
    } else {
        ex | WS_EX_NOACTIVATE.0
    }
}

unsafe extern "system" fn keep_pinned(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    activating: usize,
) -> LRESULT {
    if msg == WM_STYLECHANGING && wparam.0 as i32 == GWL_EXSTYLE.0 {
        let change = &mut *(lparam.0 as *mut STYLESTRUCT);
        change.styleNew = pinned(change.styleNew, activating != 0);
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// Pins the style of `hwnd` from now on. Must run on the thread that owns the
/// window (Windows does not subclass across threads). Calling it again only
/// changes `activating`.
pub fn pin(hwnd: HWND, activating: bool) -> bool {
    unsafe {
        if !SetWindowSubclass(hwnd, Some(keep_pinned), PIN_ID, activating as usize).as_bool() {
            return false;
        }
        // Rewriting the style sends it through the pin once now.
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, pinned(ex, activating) as isize);
        true
    }
}

// ── The keyboard, once the chat is done with it ──────────────────────────────
//
// Allowing activation again is not enough on the way out: a window that stops
// being activatable stays the active one. The island folded away would keep the
// keyboard, with the chat's hidden text field still focused. So it does what
// Windows does when the active window goes away: the window in front gets it.

/// What choosing the next window needs to know about a top-level window.
#[derive(Debug, Clone, Copy)]
pub struct TopWindow {
    pub hwnd: isize,
    /// Visible, not cloaked (a suspended app's window is visible but not
    /// there), not minimised.
    pub shown: bool,
    pub ex_style: u32,
    pub owned: bool,
    /// One of Coucou's own windows.
    pub ours: bool,
}

/// A window you could switch to: not a tool window, not one that refuses
/// activation, and not owned by another unless it asks to be listed.
pub fn is_app_window(ex_style: u32, owned: bool) -> bool {
    ex_style & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) == 0 && (!owned || ex_style & WS_EX_APPWINDOW.0 != 0)
}

/// The first window in front of you that is not ours. `windows` is in Z-order,
/// front first.
pub fn next_in_line(windows: &[TopWindow]) -> Option<isize> {
    windows.iter().find(|w| w.shown && !w.ours && is_app_window(w.ex_style, w.owned)).map(|w| w.hwnd)
}

/// The island let go of the keyboard: if it still has it, the window in front
/// gets it.
pub fn give_back_foreground(island: HWND) {
    unsafe {
        if GetForegroundWindow() != island {
            return; // you have already gone somewhere else
        }
        if let Some(next) = next_in_line(&top_windows()) {
            let _ = SetForegroundWindow(HWND(next as *mut _));
        }
    }
}

/// Every top-level window, front first.
fn top_windows() -> Vec<TopWindow> {
    unsafe extern "system" fn collect(hwnd: HWND, list: LPARAM) -> BOOL {
        let list = &mut *(list.0 as *mut Vec<TopWindow>);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut cloaked = 0u32;
        let cloaked = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&mut cloaked as *mut u32).cast(), 4).is_ok() && cloaked != 0;
        list.push(TopWindow {
            hwnd: hwnd.0 as isize,
            shown: IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() && !cloaked,
            ex_style: GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32,
            owned: GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid()),
            ours: pid == GetCurrentProcessId(),
        });
        true.into()
    }
    let mut list: Vec<TopWindow> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut list as *mut Vec<TopWindow> as isize));
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn top(hwnd: isize, shown: bool, ex_style: u32, owned: bool, ours: bool) -> TopWindow {
        TopWindow { hwnd, shown, ex_style, owned, ours }
    }

    #[test]
    fn the_keyboard_goes_to_the_first_window_in_front_that_is_not_ours() {
        let normal = WS_EX_APPWINDOW.0 | WS_EX_TOPMOST.0;
        let z = [
            top(1, true, WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0, false, true), // the island
            top(2, false, 0, false, true),                                       // the hidden flyout
            top(3, false, 0, false, false),                                      // cloaked or minimised
            top(4, true, WS_EX_TOOLWINDOW.0, false, false),                      // a tool window
            top(5, true, WS_EX_NOACTIVATE.0, false, false),                      // refuses activation
            top(6, true, 0, true, false),                                        // a dialog's owner chain
            top(7, true, normal, false, false),                                  // the terminal
            top(8, true, 0, false, false),                                       // the browser behind it
        ];
        assert_eq!(next_in_line(&z), Some(7));
        assert_eq!(next_in_line(&z[..6]), None, "nothing to give it to");
        assert!(is_app_window(WS_EX_APPWINDOW.0, true), "an owned window that asks to be listed");
    }

    use ::windows::core::w;
    use ::windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_EX_LAYERED, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    };

    fn ex_of(hwnd: HWND) -> u32 {
        unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 }
    }

    /// What tao writes when it toggles click-through: its own idea of the
    /// style, which knows nothing of ours.
    fn rewrite_like_tao(hwnd: HWND) {
        let theirs = WS_EX_APPWINDOW.0 | WS_EX_TRANSPARENT.0 | WS_EX_LAYERED.0;
        unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, theirs as isize) };
    }

    #[test]
    fn the_pinned_style_is_a_tool_window_that_is_never_activated() {
        let ex = pinned(WS_EX_APPWINDOW.0 | WS_EX_TRANSPARENT.0, false);
        assert_eq!(ex & WS_EX_APPWINDOW.0, 0, "an app window is listed in Alt-Tab");
        assert_ne!(ex & WS_EX_TOOLWINDOW.0, 0);
        assert_ne!(ex & WS_EX_NOACTIVATE.0, 0);
        assert_ne!(ex & WS_EX_TRANSPARENT.0, 0, "click-through is not ours to change");

        let typing = pinned(WS_EX_NOACTIVATE.0, true);
        assert_eq!(typing & WS_EX_NOACTIVATE.0, 0, "the chat's text field needs the keyboard");
        assert_ne!(typing & WS_EX_TOOLWINDOW.0, 0, "still out of Alt-Tab while typing");
    }

    #[test]
    fn a_rewritten_style_keeps_the_island_out_of_reach() {
        unsafe {
            let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("pin test"), WS_POPUP, 0, 0, 10, 10, None, None, None, None)
                .expect("a test window");

            assert!(pin(hwnd, false));
            rewrite_like_tao(hwnd);
            let ex = ex_of(hwnd);
            assert_eq!(ex & WS_EX_APPWINDOW.0, 0, "back in Alt-Tab after a rewrite: {ex:#x}");
            assert_ne!(ex & WS_EX_TOOLWINDOW.0, 0, "{ex:#x}");
            assert_ne!(ex & WS_EX_NOACTIVATE.0, 0, "activatable after a rewrite: {ex:#x}");
            assert_ne!(ex & WS_EX_TRANSPARENT.0, 0, "the rewrite's own bits stay: {ex:#x}");

            // The chat opens: activation allowed, and it survives a rewrite too.
            assert!(pin(hwnd, true));
            rewrite_like_tao(hwnd);
            let ex = ex_of(hwnd);
            assert_eq!(ex & WS_EX_NOACTIVATE.0, 0, "{ex:#x}");
            assert_ne!(ex & WS_EX_TOOLWINDOW.0, 0, "{ex:#x}");

            // And closes again.
            assert!(pin(hwnd, false));
            rewrite_like_tao(hwnd);
            assert_ne!(ex_of(hwnd) & WS_EX_NOACTIVATE.0, 0);

            let _ = DestroyWindow(hwnd);
        }
    }
}
