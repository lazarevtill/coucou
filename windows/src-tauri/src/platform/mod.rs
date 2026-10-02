// Everything that differs between operating systems, behind one set of names.
//
// The rest of the app calls `platform::…` and never touches Win32 or a Linux
// API directly. Each OS file exposes the same functions; the compiler picks one.

use std::path::PathBuf;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use self::linux::*;

/// Wall-clock time in the user's time zone, for log lines and backup names.
pub struct LocalTime {
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// The user's home directory, where `.claude/settings.json` lives.
pub fn home_dir() -> PathBuf {
    std::env::var_os(HOME_VAR)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

// ── Finding and focusing the window a session lives in ───────────────────────
//
// Each OS file provides:
//   focus_window(&[Candidate], &[String]) -> (FocusOutcome, FocusMethod)
//   process_alive(pid, exe) -> bool
//   find_all_on_path(stem) -> Vec<PathBuf>
//   spawn_launch(&Launch) -> io::Result<()>

/// A process whose windows could be a session's. When `exe` is set the process
/// must really be running that image: pids arrive from outside and are recycled.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub pid: u32,
    pub exe: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusOutcome {
    /// In front, and it is the window whose title matched.
    Focused,
    /// In front, but nothing told this window apart from the process's others.
    FocusedUnsure,
    /// The OS would not let us; the taskbar button is flashing instead.
    Flashed,
    /// No visible window belongs to any candidate (or this platform cannot look).
    NoWindow,
}

/// Which step of the ladder got the window to the front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMethod {
    /// The plain request was enough.
    Plain,
    /// Needed the brief input-queue attach (Windows).
    Attached,
    /// Neither — or nothing was tried.
    None,
}
