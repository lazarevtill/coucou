//! The exact window a session runs in, when its terminal says so.
//!
//! Windows Terminal runs every one of its windows in a single process, so the
//! process chain alone can only say "some Windows Terminal window". But each tab
//! hosts its programs on a pseudo console whose hidden window is *owned* by the
//! terminal window the tab sits in. Attaching to the session's console for an
//! instant and asking for that owner gives the one right window.
//!
//! Editors (Cursor, VS Code) leave their pseudo consoles unowned, so for them
//! this finds nothing and the app goes by the editor's workspace instead.

use crate::proc::Ancestor;

/// Whose console to look at: the Claude Code process when the chain has one,
/// else the shell Windows Terminal started (a Claude Code run through Node).
pub fn target(chain: &[Ancestor]) -> Option<u32> {
    if let Some(claude) = chain.iter().find(|a| a.exe.eq_ignore_ascii_case("claude.exe")) {
        return Some(claude.pid);
    }
    let terminal = chain.iter().position(|a| a.exe.eq_ignore_ascii_case("WindowsTerminal.exe"))?;
    terminal.checked_sub(1).map(|shell| chain[shell].pid)
}

/// The window that owns `pid`'s console, as a raw handle value. `None` when the
/// process has no console, or nothing owns it (an editor's terminal).
#[cfg(windows)]
pub fn owner_window(pid: u32) -> Option<u64> {
    use windows::Win32::System::Console::{
        AttachConsole, FreeConsole, GetConsoleWindow, GetStdHandle, SetConsoleCtrlHandler, SetStdHandle,
        STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, GA_ROOTOWNER};

    unsafe {
        // The hook protocol runs on pipes that must outlive the console swap, so
        // the standard handles are put back exactly as they were.
        let saved = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE].map(|k| (k, GetStdHandle(k).ok()));
        // A Ctrl+C typed in that tab while we are attached must not stop us.
        let _ = SetConsoleCtrlHandler(None, true);
        let _ = FreeConsole();
        let mut owner = None;
        if AttachConsole(pid).is_ok() {
            let console = GetConsoleWindow();
            if !console.is_invalid() {
                let root = GetAncestor(console, GA_ROOTOWNER);
                // Unowned (an editor's pseudo console) answers with itself.
                if !root.is_invalid() && root != console {
                    owner = Some(root.0 as usize as u64);
                }
            }
            let _ = FreeConsole();
        }
        for (kind, handle) in saved {
            if let Some(handle) = handle {
                let _ = SetStdHandle(kind, handle);
            }
        }
        owner
    }
}

#[cfg(not(windows))]
pub fn owner_window(_pid: u32) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(entries: &[(u32, &str)]) -> Vec<Ancestor> {
        entries.iter().map(|(pid, exe)| Ancestor { pid: *pid, exe: (*exe).to_string() }).collect()
    }

    #[test]
    fn the_claude_code_process_is_the_one_to_ask() {
        // Copied from a live Windows Terminal session (nearest ancestor first).
        let c = chain(&[(1, "bash.exe"), (47728, "claude.exe"), (46520, "powershell.exe"), (46304, "WindowsTerminal.exe")]);
        assert_eq!(target(&c), Some(47728));
        let c = chain(&[(1, "bash.exe"), (2, "Claude.EXE"), (3, "Cursor.exe")]);
        assert_eq!(target(&c), Some(2), "file names compare without case");
    }

    #[test]
    fn without_claude_exe_the_shell_under_windows_terminal_is_asked() {
        let c = chain(&[(1, "bash.exe"), (2, "node.exe"), (3, "pwsh.exe"), (4, "WindowsTerminal.exe")]);
        assert_eq!(target(&c), Some(3));
    }

    #[test]
    fn nothing_to_ask_outside_a_terminal() {
        assert_eq!(target(&chain(&[(1, "bash.exe"), (2, "node.exe"), (3, "Cursor.exe")])), None);
        assert_eq!(target(&chain(&[(4, "WindowsTerminal.exe")])), None, "no shell below the terminal");
        assert_eq!(target(&[]), None);
    }
}
