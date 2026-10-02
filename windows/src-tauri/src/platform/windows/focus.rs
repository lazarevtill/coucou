// Bringing a session's window to the front.
//
// A process can own several windows (Cursor is one process for every editor
// window) and Windows refuses most attempts by a background process to steal the
// foreground, so this does three things in order and stops at the first that
// works, checking after each one rather than trusting a return value:
//
//   1. restore it if minimised, then plain SetForegroundWindow — which Windows
//      allows when we just received the user's click;
//   2. only if that did not take: attach to the foreground window's input queue
//      for the instant of the call, the documented way around the lock;
//   3. only if that did not take either: flash its taskbar button and say so.
//
// No synthetic key presses: an Alt tap would toggle the menu bar in the very
// editor being activated.
//
// The pure part (`pick`) is unit tested; the Win32 part was exercised against
// real Cursor and Windows Terminal windows (see windows/docs/terminals.md).

use crate::platform::{Candidate, FocusMethod, FocusOutcome};
use ::windows::core::BOOL;
use ::windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use ::windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, FlashWindowEx, GetForegroundWindow, GetWindow, GetWindowLongPtrW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow,
    ShowWindow, FLASHWINFO, FLASHW_TIMERNOFG, FLASHW_TRAY, GWL_EXSTYLE, GW_OWNER, SW_RESTORE,
    WS_EX_TOOLWINDOW,
};

/// What `pick` needs to know about a window.
#[derive(Debug, Clone)]
pub struct WinInfo {
    pub title: String,
}

/// Chooses among a process's windows, given in Z-order (front first): the first
/// whose title contains a hint, else the front-most. The bool says whether a hint
/// actually matched. Hints under two characters would match almost any title.
pub fn pick(windows: &[WinInfo], hints: &[String]) -> Option<(usize, bool)> {
    if windows.is_empty() {
        return None;
    }
    let hints: Vec<String> = hints
        .iter()
        .map(|h| h.trim().to_lowercase())
        .filter(|h| h.chars().count() >= 2)
        .collect();
    for (i, w) in windows.iter().enumerate() {
        let title = w.title.to_lowercase();
        if hints.iter().any(|h| title.contains(h.as_str())) {
            return Some((i, true));
        }
    }
    Some((0, false))
}

struct Found {
    hwnd: HWND,
    pid: u32,
    title: String,
}

/// Top-level, visible, unowned, non-tool windows, front-most first.
fn top_level_windows() -> Vec<Found> {
    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let out = &mut *(lparam.0 as *mut Vec<Found>);
        if !IsWindowVisible(hwnd).as_bool() {
            return true.into();
        }
        // An owned window is a dialog or popup of someone else's window.
        if GetWindow(hwnd, GW_OWNER).map(|o| !o.is_invalid()).unwrap_or(false) {
            return true.into();
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOOLWINDOW.0 != 0 {
            return true.into();
        }
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buf);
        if len <= 0 {
            return true.into(); // nothing to show in a taskbar, nothing to jump to
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        out.push(Found { hwnd, pid, title: String::from_utf16_lossy(&buf[..len as usize]) });
        true.into()
    }
    let mut found: Vec<Found> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut found as *mut Vec<Found> as isize));
    }
    found
}

/// The file name of the image a pid is running, or `None` if it is gone.
pub fn image_name(pid: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            ::windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(process);
        if !ok {
            return None;
        }
        let full = String::from_utf16_lossy(&buf[..len as usize]);
        full.rsplit(['\\', '/']).next().map(str::to_string)
    }
}

/// Is `pid` still the process it was when it was recorded?
pub fn is_running(pid: u32, exe: &str) -> bool {
    image_name(pid).map(|n| n.eq_ignore_ascii_case(exe)).unwrap_or(false)
}

fn foreground_is(hwnd: HWND) -> bool {
    unsafe { GetForegroundWindow() == hwnd }
}

/// Brings the best window of the first candidate that has one to the front.
pub fn focus(candidates: &[Candidate], hints: &[String]) -> (FocusOutcome, FocusMethod) {
    let all = top_level_windows();
    for candidate in candidates {
        if let Some(expected) = &candidate.exe {
            if !is_running(candidate.pid, expected) {
                continue;
            }
        }
        let mine: Vec<&Found> = all.iter().filter(|w| w.pid == candidate.pid).collect();
        let infos: Vec<WinInfo> = mine.iter().map(|w| WinInfo { title: w.title.clone() }).collect();
        let Some((index, matched)) = pick(&infos, hints) else { continue };
        let hwnd = mine[index].hwnd;
        return match bring_to_front(hwnd) {
            Some(method) => {
                (if matched { FocusOutcome::Focused } else { FocusOutcome::FocusedUnsure }, method)
            }
            None => {
                flash(hwnd);
                (FocusOutcome::Flashed, FocusMethod::None)
            }
        };
    }
    (FocusOutcome::NoWindow, FocusMethod::None)
}

fn bring_to_front(hwnd: HWND) -> Option<FocusMethod> {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
        if foreground_is(hwnd) {
            return Some(FocusMethod::Plain);
        }

        // The foreground lock held. Share the foreground thread's input state for
        // the length of one call, which is what makes Windows accept the switch.
        let foreground = GetForegroundWindow();
        let their_thread = GetWindowThreadProcessId(foreground, None);
        let ours = GetCurrentThreadId();
        let attached = their_thread != 0 && their_thread != ours && AttachThreadInput(ours, their_thread, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(ours, their_thread, false);
        }
        foreground_is(hwnd).then_some(FocusMethod::Attached)
    }
}

fn flash(hwnd: HWND) {
    let info = FLASHWINFO {
        cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
        hwnd,
        dwFlags: FLASHW_TRAY | FLASHW_TIMERNOFG,
        uCount: 3,
        dwTimeout: 0,
    };
    unsafe {
        let _ = FlashWindowEx(&info);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wins(titles: &[&str]) -> Vec<WinInfo> {
        titles.iter().map(|t| WinInfo { title: (*t).to_string() }).collect()
    }
    fn hints(h: &[&str]) -> Vec<String> {
        h.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_first_window_whose_title_has_a_hint_wins_over_front_ones_that_do_not() {
        // Cursor with three windows; the session belongs to shop-api.
        let w = wins(&["notes.md - docs - Cursor", "main.py - shop-api - Cursor", "x - api - Cursor"]);
        assert_eq!(pick(&w, &hints(&["shop-api"])), Some((1, true)));
    }

    #[test]
    fn matching_ignores_case_and_any_hint_may_match() {
        let w = wins(&["Coucou Terminal Plugins Agent"]);
        assert_eq!(pick(&w, &hints(&["nothing", "COUCOU"])), Some((0, true)));
    }

    #[test]
    fn with_no_match_the_front_most_window_is_used_and_flagged_unsure() {
        let w = wins(&["a - Cursor", "b - Cursor"]);
        assert_eq!(pick(&w, &hints(&["zzz"])), Some((0, false)));
        assert_eq!(pick(&w, &hints(&[])), Some((0, false)));
    }

    #[test]
    fn empty_and_one_character_hints_match_nothing() {
        let w = wins(&["a - Cursor", "b - Cursor"]);
        assert_eq!(pick(&w, &hints(&["", " ", "a"])), Some((0, false)));
    }

    #[test]
    fn no_windows_no_pick() {
        assert_eq!(pick(&[], &hints(&["x"])), None);
    }
}
