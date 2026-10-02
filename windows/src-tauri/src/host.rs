// Where a Claude Code session lives: which editor or terminal hosts it, and which
// shell it was started from.
//
// The process chain captured by coucou-hook is the truth. The environment
// variables are only a fallback, because they lie: a Windows Terminal opened from
// inside Cursor inherits Cursor's TERM_PROGRAM and git askpass path, while its
// process chain contains WindowsTerminal.exe and no Cursor at all.
//
// Everything here is pure, so it is tested against chains copied from a real
// machine (Windows Terminal, a Cursor terminal, Cursor's own Claude panel).

use serde::{Deserialize, Serialize};

/// One ancestor of the hook process, exactly as the relay sends it.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ChainEntry {
    pub pid: u32,
    pub exe: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostKind {
    Cursor,
    Vscode,
    WindowsTerminal,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellKind {
    Pwsh,
    Powershell,
    Cmd,
    Bash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub kind: HostKind,
    pub shell: Option<ShellKind>,
    pub label: String,
    pub shell_label: Option<String>,
}

/// Everything the relay could tell us about where it runs.
pub struct Signals<'a> {
    pub chain: &'a [ChainEntry],
    /// `cursor`, `vscode` or empty — derived by the relay, never a path.
    pub hint: &'a str,
    pub term_program: &'a str,
    pub has_wt_session: bool,
    /// `ideName` from ~/.claude/ide/<port>.lock, when that file could be read.
    pub ide_name: Option<&'a str>,
}

/// `WindowsTerminal.exe` -> `windowsterminal`.
fn stem(exe: &str) -> String {
    let lower = exe.to_ascii_lowercase();
    lower.strip_suffix(".exe").unwrap_or(&lower).to_string()
}

pub(crate) fn host_of_exe(exe: &str) -> Option<HostKind> {
    match stem(exe).as_str() {
        "cursor" => Some(HostKind::Cursor),
        "code" | "code - insiders" => Some(HostKind::Vscode),
        "windowsterminal" => Some(HostKind::WindowsTerminal),
        _ => None,
    }
}

fn shell_of_exe(exe: &str) -> Option<ShellKind> {
    match stem(exe).as_str() {
        "pwsh" => Some(ShellKind::Pwsh),
        "powershell" => Some(ShellKind::Powershell),
        "cmd" => Some(ShellKind::Cmd),
        "bash" => Some(ShellKind::Bash),
        _ => None,
    }
}

/// The nearest process above the relay that is Claude Code itself.
fn claude_index(chain: &[ChainEntry]) -> Option<usize> {
    chain.iter().position(|e| stem(&e.exe) == "claude")
}

/// The Claude Code process of a session — what has to be alive for the session to be.
pub(crate) fn claude_entry(chain: &[ChainEntry]) -> Option<&ChainEntry> {
    claude_index(chain).map(|i| &chain[i])
}

/// The shell the user typed `claude` into: the nearest shell above Claude Code.
/// The relay's own Git Bash sits *below* Claude Code, so it is never picked.
/// Without a Claude Code entry (a renamed binary) the outermost shell is the best
/// remaining guess.
fn shell_of_chain(chain: &[ChainEntry]) -> Option<ShellKind> {
    match claude_index(chain) {
        Some(i) => chain[i + 1..].iter().find_map(|e| shell_of_exe(&e.exe)),
        None => chain.iter().rev().find_map(|e| shell_of_exe(&e.exe)),
    }
}

fn kind_from_ide_name(name: &str) -> Option<HostKind> {
    let lower = name.to_ascii_lowercase();
    if lower == "cursor" {
        Some(HostKind::Cursor)
    } else if lower.contains("visual studio code") || lower == "vs code" {
        Some(HostKind::Vscode)
    } else {
        None
    }
}

fn host_label(kind: HostKind) -> &'static str {
    match kind {
        HostKind::Cursor => "Cursor",
        HostKind::Vscode => "VS Code",
        HostKind::WindowsTerminal => "Windows Terminal",
        HostKind::Unknown => "Terminal",
    }
}

fn shell_label(shell: ShellKind) -> &'static str {
    match shell {
        ShellKind::Pwsh => "pwsh",
        ShellKind::Powershell => "PowerShell",
        ShellKind::Cmd => "cmd",
        ShellKind::Bash => "bash",
    }
}

pub fn classify(signals: &Signals) -> HostInfo {
    // Order of trust: the process chain (what actually hosts the session), then
    // the IDE's own lock file, then the relay's hint, then bare environment.
    let kind = signals
        .chain
        .iter()
        .find_map(|e| host_of_exe(&e.exe))
        .or_else(|| signals.ide_name.and_then(kind_from_ide_name))
        .or(match signals.hint {
            "cursor" => Some(HostKind::Cursor),
            "vscode" => Some(HostKind::Vscode),
            _ => None,
        })
        .or((signals.term_program == "vscode").then_some(HostKind::Vscode))
        .or(signals.has_wt_session.then_some(HostKind::WindowsTerminal))
        .unwrap_or(HostKind::Unknown);

    let shell = shell_of_chain(signals.chain);
    HostInfo {
        kind,
        shell,
        label: host_label(kind).to_string(),
        shell_label: shell.map(|s| shell_label(s).to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(exes: &[&str]) -> Vec<ChainEntry> {
        exes.iter()
            .enumerate()
            .map(|(i, exe)| ChainEntry { pid: 1000 + i as u32, exe: (*exe).to_string() })
            .collect()
    }

    fn host(chain: &[ChainEntry], hint: &str, term: &str, wt: bool, ide: Option<&str>) -> HostInfo {
        classify(&Signals { chain, hint, term_program: term, has_wt_session: wt, ide_name: ide })
    }

    #[test]
    fn windows_terminal_running_windows_powershell() {
        // claude run from PowerShell in Windows Terminal; the hook went through Git Bash.
        let c = chain(&["bash.exe", "claude.exe", "powershell.exe", "WindowsTerminal.exe"]);
        let h = host(&c, "", "", true, None);
        assert_eq!(h.kind, HostKind::WindowsTerminal);
        assert_eq!(h.shell, Some(ShellKind::Powershell));
        assert_eq!(h.label, "Windows Terminal");
        assert_eq!(h.shell_label.as_deref(), Some("PowerShell"));
    }

    #[test]
    fn cursors_integrated_terminal() {
        // The ptyHost is a Cursor.exe child of the main Cursor.exe.
        let c = chain(&["bash.exe", "bash.exe", "claude.exe", "powershell.exe", "Cursor.exe", "Cursor.exe"]);
        let h = host(&c, "cursor", "vscode", false, Some("Cursor"));
        assert_eq!(h.kind, HostKind::Cursor);
        assert_eq!(h.shell, Some(ShellKind::Powershell));
        assert_eq!(h.label, "Cursor");
    }

    #[test]
    fn claude_in_a_cursor_panel_has_a_host_but_no_shell() {
        let c = chain(&["bash.exe", "claude.exe", "Cursor.exe", "Cursor.exe"]);
        let h = host(&c, "", "", false, None);
        assert_eq!(h.kind, HostKind::Cursor);
        assert_eq!(h.shell, None);
        assert_eq!(h.shell_label, None);
    }

    #[test]
    fn vs_code_is_told_apart_from_cursor() {
        let c = chain(&["bash.exe", "claude.exe", "powershell.exe", "Code.exe", "Code.exe"]);
        let h = host(&c, "vscode", "vscode", false, None);
        assert_eq!(h.kind, HostKind::Vscode);
        assert_eq!(h.label, "VS Code");
        let c = chain(&["claude.exe", "pwsh.exe", "Code - Insiders.exe"]);
        assert_eq!(host(&c, "", "", false, None).kind, HostKind::Vscode);
    }

    #[test]
    fn the_shell_is_the_one_nearest_above_claude_not_the_outermost() {
        // pwsh started from cmd inside Windows Terminal: the user's shell is pwsh.
        let c = chain(&["bash.exe", "claude.exe", "pwsh.exe", "cmd.exe", "WindowsTerminal.exe"]);
        let h = host(&c, "", "", true, None);
        assert_eq!(h.shell, Some(ShellKind::Pwsh));
        assert_eq!(h.shell_label.as_deref(), Some("pwsh"));
    }

    #[test]
    fn the_process_chain_beats_inherited_environment() {
        // Windows Terminal launched from a Cursor terminal inherits Cursor's
        // TERM_PROGRAM and askpass path. The chain has no Cursor in it.
        let c = chain(&["bash.exe", "claude.exe", "powershell.exe", "WindowsTerminal.exe"]);
        let h = host(&c, "cursor", "vscode", true, Some("Cursor"));
        assert_eq!(h.kind, HostKind::WindowsTerminal);
    }

    #[test]
    fn without_a_host_in_the_chain_the_other_signals_decide_in_order() {
        let c = chain(&["bash.exe", "claude.exe", "powershell.exe"]);
        assert_eq!(host(&c, "", "", false, Some("Cursor")).kind, HostKind::Cursor);
        assert_eq!(host(&c, "", "", false, Some("Visual Studio Code")).kind, HostKind::Vscode);
        assert_eq!(host(&c, "cursor", "vscode", false, None).kind, HostKind::Cursor);
        assert_eq!(host(&c, "vscode", "vscode", false, None).kind, HostKind::Vscode);
        assert_eq!(host(&c, "", "vscode", false, None).kind, HostKind::Vscode);
        assert_eq!(host(&c, "", "", true, None).kind, HostKind::WindowsTerminal);
    }

    #[test]
    fn a_session_with_no_terminal_is_unknown_not_guessed() {
        // claude started by the desktop app or a browser extension: nothing above it.
        let c = chain(&["claude.exe"]);
        let h = host(&c, "", "", false, None);
        assert_eq!(h.kind, HostKind::Unknown);
        assert_eq!(h.shell, None);
        assert_eq!(h.label, "Terminal");
        // And an empty chain (relay older than this build) is the same.
        assert_eq!(host(&[], "", "", false, None).kind, HostKind::Unknown);
    }

    #[test]
    fn executable_names_match_without_regard_to_case_or_extension() {
        let c = chain(&["BASH.EXE", "Claude.exe", "POWERSHELL.EXE", "windowsterminal.exe"]);
        let h = host(&c, "", "", false, None);
        assert_eq!(h.kind, HostKind::WindowsTerminal);
        assert_eq!(h.shell, Some(ShellKind::Powershell));
    }

    #[test]
    fn the_wire_shape_is_what_the_island_reads() {
        let c = chain(&["claude.exe", "powershell.exe", "WindowsTerminal.exe"]);
        let json = serde_json::to_value(host(&c, "", "", true, None)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "windows-terminal",
                "shell": "powershell",
                "label": "Windows Terminal",
                "shellLabel": "PowerShell",
            })
        );
    }
}
