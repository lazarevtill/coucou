//! The chain of processes above the relay: hook <- bash <- claude <- shell <- the
//! terminal or editor hosting the session.
//!
//! This is what lets Coucou find the window a session lives in. Only PIDs and
//! executable *file names* are collected — never a path, a command line or an
//! environment (the paths carry the user name).
//!
//! The walk itself is pure and takes its process table as a closure, so it is
//! tested against chains copied from a real machine. Only `snapshot_lookup`
//! touches Win32.

use std::collections::HashSet;

/// One ancestor of the relay process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ancestor {
    pub pid: u32,
    /// File name only, e.g. `WindowsTerminal.exe`.
    pub exe: String,
}

/// What the walker needs to know about a process.
#[derive(Debug, Clone)]
pub struct Proc {
    pub ppid: u32,
    pub exe: String,
    /// Creation time in any monotonic unit; `None` when it could not be read.
    pub created: Option<u64>,
}

/// More than this and the chain is noise, not information.
pub const MAX_ANCESTORS: usize = 12;

/// Windows' own plumbing. Every chain ends in these, and none of them is ever
/// the window a session lives in, so they are not worth sending.
const SYSTEM_ROOTS: &[&str] = &[
    "explorer.exe",
    "svchost.exe",
    "services.exe",
    "wininit.exe",
    "winlogon.exe",
    "sihost.exe",
    "userinit.exe",
    "smss.exe",
    "csrss.exe",
    "system",
];

/// Walks from `start` upwards, nearest ancestor first, `start` itself excluded.
pub fn walk(lookup: &dyn Fn(u32) -> Option<Proc>, start: u32, max: usize) -> Vec<Ancestor> {
    let mut out = Vec::new();
    let Some(first) = lookup(start) else { return out };

    let mut seen = HashSet::from([start]);
    let mut pid = first.ppid;
    let mut child_created = first.created;

    while out.len() < max {
        // 0 is "no parent"; a repeated pid is a cycle.
        if pid == 0 || !seen.insert(pid) {
            break;
        }
        let Some(parent) = lookup(pid) else { break };
        if SYSTEM_ROOTS.iter().any(|root| parent.exe.eq_ignore_ascii_case(root)) {
            break;
        }
        // A parent cannot have been created after its child. When it was, the real
        // parent has exited and Windows has recycled its number.
        if let (Some(p), Some(c)) = (parent.created, child_created) {
            if p > c {
                break;
            }
        }
        out.push(Ancestor { pid, exe: parent.exe.clone() });
        child_created = parent.created;
        pid = parent.ppid;
    }
    out
}

/// The ancestors of this process, nearest first.
pub fn ancestors() -> Vec<Ancestor> {
    let table = sys::process_table();
    walk(
        &|pid| {
            table.get(&pid).map(|(ppid, exe)| Proc {
                ppid: *ppid,
                exe: exe.clone(),
                created: sys::creation_time(pid),
            })
        },
        std::process::id(),
        MAX_ANCESTORS,
    )
}

/// The only Win32 in this module.
mod sys {
    use std::collections::HashMap;

    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// pid -> (parent pid, exe file name) for every process, from one snapshot.
    pub fn process_table() -> HashMap<u32, (u32, String)> {
        let mut table = HashMap::new();
        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return table;
            };
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
            while more {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                let exe = String::from_utf16_lossy(&entry.szExeFile[..len]);
                table.insert(entry.th32ProcessID, (entry.th32ParentProcessID, exe));
                more = Process32NextW(snapshot, &mut entry).is_ok();
            }
            let _ = CloseHandle(snapshot);
        }
        table
    }

    /// Creation time as a FILETIME count, or `None` when the process cannot be
    /// opened (system processes, or one that has just exited).
    pub fn creation_time(pid: u32) -> Option<u64> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let (mut created, mut exited, mut kernel, mut user) =
                (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
            let ok = GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user).is_ok();
            let _ = CloseHandle(process);
            ok.then(|| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A process table: pid -> (ppid, exe, created).
    fn table(rows: &[(u32, u32, &str, Option<u64>)]) -> HashMap<u32, Proc> {
        rows.iter()
            .map(|(pid, ppid, exe, created)| {
                (*pid, Proc { ppid: *ppid, exe: exe.to_string(), created: *created })
            })
            .collect()
    }

    fn chain(t: &HashMap<u32, Proc>, start: u32, max: usize) -> Vec<(u32, String)> {
        walk(&|pid| t.get(&pid).cloned(), start, max)
            .into_iter()
            .map(|a| (a.pid, a.exe))
            .collect()
    }

    #[test]
    fn walks_nearest_first_and_excludes_the_start() {
        // The shape seen on a real machine: claude run from PowerShell in Windows
        // Terminal, with the hook launched through Git Bash.
        let t = table(&[
            (100, 90, "coucou-hook.exe", Some(500)),
            (90, 80, "bash.exe", Some(400)),
            (80, 70, "claude.exe", Some(300)),
            (70, 60, "powershell.exe", Some(200)),
            (60, 50, "WindowsTerminal.exe", Some(100)),
        ]);
        assert_eq!(
            chain(&t, 100, MAX_ANCESTORS),
            vec![
                (90, "bash.exe".to_string()),
                (80, "claude.exe".to_string()),
                (70, "powershell.exe".to_string()),
                (60, "WindowsTerminal.exe".to_string()),
            ]
        );
    }

    #[test]
    fn stops_before_windows_plumbing() {
        let t = table(&[
            (100, 90, "coucou-hook.exe", None),
            (90, 80, "Cursor.exe", None),
            (80, 70, "explorer.exe", None),
            (70, 1, "userinit.exe", None),
        ]);
        assert_eq!(chain(&t, 100, MAX_ANCESTORS), vec![(90, "Cursor.exe".to_string())]);
        // Case does not matter for the stop list.
        let t = table(&[(100, 90, "h.exe", None), (90, 80, "SVCHOST.EXE", None)]);
        assert!(chain(&t, 100, MAX_ANCESTORS).is_empty());
    }

    #[test]
    fn never_returns_more_than_max() {
        let rows: Vec<(u32, u32, &str, Option<u64>)> =
            (1..=30u32).map(|p| (p, p + 1, "node.exe", None)).collect();
        let t = table(&rows);
        assert_eq!(chain(&t, 1, 5).len(), 5);
    }

    #[test]
    fn a_parent_younger_than_its_child_is_a_reused_pid_and_ends_the_chain() {
        // PID 80 now belongs to a process created *after* its supposed child: the
        // real parent died and the number was recycled. Report nothing past it.
        let t = table(&[
            (100, 90, "coucou-hook.exe", Some(500)),
            (90, 80, "bash.exe", Some(400)),
            (80, 70, "unrelated.exe", Some(450)),
            (70, 60, "powershell.exe", Some(200)),
        ]);
        assert_eq!(chain(&t, 100, MAX_ANCESTORS), vec![(90, "bash.exe".to_string())]);
    }

    #[test]
    fn unknown_creation_times_do_not_stop_the_walk() {
        let t = table(&[
            (100, 90, "h.exe", Some(500)),
            (90, 80, "bash.exe", None),
            (80, 70, "claude.exe", Some(300)),
        ]);
        assert_eq!(
            chain(&t, 100, MAX_ANCESTORS),
            vec![(90, "bash.exe".to_string()), (80, "claude.exe".to_string())]
        );
    }

    #[test]
    fn a_cycle_or_a_vanished_parent_ends_the_chain() {
        let cyc = table(&[(100, 90, "h.exe", None), (90, 100, "bash.exe", None)]);
        assert_eq!(chain(&cyc, 100, MAX_ANCESTORS), vec![(90, "bash.exe".to_string())]);

        let gone = table(&[(100, 90, "h.exe", None), (90, 80, "bash.exe", None)]);
        assert_eq!(chain(&gone, 100, MAX_ANCESTORS), vec![(90, "bash.exe".to_string())]);

        let orphan = table(&[(100, 0, "h.exe", None)]);
        assert!(chain(&orphan, 100, MAX_ANCESTORS).is_empty());
        assert!(chain(&orphan, 999, MAX_ANCESTORS).is_empty());
    }
}
