// "Open this project in…": which editor, which terminal, and what exactly to run.
//
// Planning is pure and takes its view of the machine from an `Env`, so every
// decision below is tested without launching anything. Spawning lives in
// platform/ (see platform/windows.rs).
//
// The rules this file keeps, all from CLAUDE.md:
//   * no `cmd /C` and no shell parsing anywhere: the folder is always its own
//     argument, so `&`, `^` and `%` in a folder name stay part of a name;
//   * the folder must be an absolute directory that exists;
//   * Explorer is the fallback when the chosen launcher is missing or fails.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Editor {
    Cursor,
    Vscode,
    None,
}

impl Editor {
    /// Unknown or missing values fall back to the default rather than failing,
    /// so a settings.json written by another build never breaks "open".
    pub fn from_setting(value: &str) -> Self {
        match value {
            "vscode" => Editor::Vscode,
            "none" => Editor::None,
            _ => Editor::Cursor,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    WindowsTerminal,
    Shell,
    None,
}

impl Terminal {
    pub fn from_setting(value: &str) -> Self {
        match value {
            "shell" => Terminal::Shell,
            "none" => Terminal::None,
            _ => Terminal::WindowsTerminal,
        }
    }
}

/// Which launcher a `Launch` goes through. Reported back to the island.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Cursor,
    Vscode,
    WindowsTerminal,
    Shell,
    Explorer,
}

impl Via {
    pub fn name(self) -> &'static str {
        match self {
            Via::Cursor => "cursor",
            Via::Vscode => "vscode",
            Via::WindowsTerminal => "windowsTerminal",
            Via::Shell => "shell",
            Via::Explorer => "explorer",
        }
    }
}

/// A program and its arguments — never a command line to be parsed by a shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// Working directory for the new process, when it is not passed as an argument.
    pub cwd: Option<PathBuf>,
    /// A console program that has to open its own window.
    pub new_console: bool,
    pub via: Via,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    /// The terminal setting is "none".
    Disabled,
    NotFound(&'static str),
    /// Windows Terminal reads `;` in its command line as a command separator, so
    /// a folder containing one cannot be handed to it safely.
    UnsafePath(&'static str),
    BadFolder(String),
}

/// What the planner may ask about the machine.
pub trait Env {
    /// Every `<stem><ext>` found on PATH, in PATH order (PATHEXT applied).
    fn find_all(&self, stem: &str) -> Vec<PathBuf>;
    fn file_exists(&self, path: &Path) -> bool;
    /// An environment variable holding a directory (`LOCALAPPDATA`, `ProgramFiles`…).
    fn dir(&self, name: &str) -> Option<PathBuf>;
}

/// True when any directory in the path is called `cursor`. Cursor ships its own
/// `code` shim, and asking for VS Code must never end up opening Cursor.
fn in_cursor_dir(path: &Path) -> bool {
    path.to_string_lossy()
        .split(['\\', '/'])
        .any(|segment| segment.eq_ignore_ascii_case("cursor"))
}

/// `<dir>\<rest>` when that file exists.
fn installed(env: &dyn Env, dir: &str, rest: &str) -> Option<PathBuf> {
    let candidate = env.dir(dir)?.join(rest);
    env.file_exists(&candidate).then_some(candidate)
}

pub fn plan_editor(editor: Editor, env: &dyn Env, folder: &Path) -> Option<Launch> {
    let (program, via) = match editor {
        Editor::None => return None,
        Editor::Cursor => (
            env.find_all("cursor")
                .into_iter()
                .next()
                .or_else(|| installed(env, "LOCALAPPDATA", r"Programs\cursor\resources\app\bin\cursor.cmd"))?,
            Via::Cursor,
        ),
        Editor::Vscode => (
            env.find_all("code")
                .into_iter()
                .find(|p| !in_cursor_dir(p))
                .or_else(|| installed(env, "LOCALAPPDATA", r"Programs\Microsoft VS Code\bin\code.cmd"))
                .or_else(|| installed(env, "ProgramFiles", r"Microsoft VS Code\bin\code.cmd"))?,
            Via::Vscode,
        ),
    };
    // An empty folder means "just the editor": no argument at all, not an empty one.
    let args = if folder.as_os_str().is_empty() { vec![] } else { vec![folder.as_os_str().to_owned()] };
    Some(Launch { program, args, cwd: None, new_console: false, via })
}

pub fn plan_terminal(terminal: Terminal, env: &dyn Env, folder: &Path) -> Result<Launch, LaunchError> {
    match terminal {
        Terminal::None => Err(LaunchError::Disabled),
        Terminal::WindowsTerminal => {
            let wt = env.find_all("wt").into_iter().next().ok_or(LaunchError::NotFound("wt.exe"))?;
            if folder.to_string_lossy().contains(';') {
                return Err(LaunchError::UnsafePath("Windows Terminal reads ';' as a command separator"));
            }
            let (shell, _) = resolve_shell(env).ok_or(LaunchError::NotFound("a PowerShell"))?;
            Ok(Launch {
                program: wt,
                args: vec!["-d".into(), folder.as_os_str().to_owned(), shell.into_os_string()],
                cwd: None,
                new_console: false,
                via: Via::WindowsTerminal,
            })
        }
        Terminal::Shell => {
            let (shell, _) = resolve_shell(env).ok_or(LaunchError::NotFound("a PowerShell"))?;
            Ok(Launch {
                program: shell,
                args: vec!["-NoExit".into()],
                cwd: Some(folder.to_path_buf()),
                new_console: true,
                via: Via::Shell,
            })
        }
    }
}

pub fn plan_explorer(folder: &Path) -> Launch {
    Launch {
        program: PathBuf::from("explorer.exe"),
        args: vec![folder.as_os_str().to_owned()],
        cwd: None,
        new_console: false,
        via: Via::Explorer,
    }
}

/// The shell new terminals start: PowerShell 7 when it is installed, Windows
/// PowerShell otherwise. Returns the executable and whether it is `pwsh`.
pub fn resolve_shell(env: &dyn Env) -> Option<(PathBuf, bool)> {
    if let Some(pwsh) = env.find_all("pwsh").into_iter().next() {
        return Some((pwsh, true));
    }
    if let Some(pwsh) = installed(env, "ProgramFiles", r"PowerShell\7\pwsh.exe") {
        return Some((pwsh, true));
    }
    if let Some(ps) = installed(env, "SystemRoot", r"System32\WindowsPowerShell\v1.0\powershell.exe") {
        return Some((ps, false));
    }
    env.find_all("powershell").into_iter().next().map(|p| (p, false))
}

/// What the settings window shows next to the editor and terminal choices.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchInfo {
    pub cursor: bool,
    pub vscode: bool,
    pub windows_terminal: bool,
    /// `pwsh` or `powershell` — the shell a new terminal will start. `None` when
    /// neither could be found.
    pub shell: Option<String>,
}

pub fn describe(env: &dyn Env) -> LaunchInfo {
    // Planning for an empty folder is only a way to ask "could this launch?".
    let can = |editor| plan_editor(editor, env, Path::new("")).is_some();
    LaunchInfo {
        cursor: can(Editor::Cursor),
        vscode: can(Editor::Vscode),
        windows_terminal: !env.find_all("wt").is_empty(),
        shell: resolve_shell(env).map(|(_, pwsh)| if pwsh { "pwsh" } else { "powershell" }.to_string()),
    }
}

/// A folder that is safe to hand to a launcher: absolute, an existing directory,
/// with trailing separators removed (a root such as `C:\` keeps its one).
pub fn validate_folder(path: &str) -> Result<PathBuf, LaunchError> {
    let bad = |why: &str| LaunchError::BadFolder(format!("{path:?}: {why}"));
    if path.is_empty() || !Path::new(path).is_absolute() {
        return Err(bad("not an absolute path"));
    }
    let trimmed = path.trim_end_matches(['\\', '/']);
    let normalized = if trimmed.ends_with(':') { format!("{trimmed}\\") } else { trimmed.to_string() };
    if normalized.is_empty() {
        return Err(bad("empty"));
    }
    let folder = PathBuf::from(normalized);
    if !folder.is_dir() {
        return Err(bad("not an existing directory"));
    }
    Ok(folder)
}

/// A machine described by data, shared by every test that needs to plan a launch.
/// The default mirrors the one this was built on: `code` on PATH is Cursor's
/// shim, `wt.exe` is an app alias, there is no pwsh.
#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    pub struct Fake {
        pub path: HashMap<&'static str, Vec<&'static str>>,
        pub files: HashSet<&'static str>,
        pub dirs: HashMap<&'static str, &'static str>,
    }

    impl Env for Fake {
        fn find_all(&self, stem: &str) -> Vec<PathBuf> {
            self.path.get(stem).into_iter().flatten().map(PathBuf::from).collect()
        }
        fn file_exists(&self, p: &Path) -> bool {
            self.files.contains(p.to_str().unwrap())
        }
        fn dir(&self, name: &str) -> Option<PathBuf> {
            self.dirs.get(name).map(PathBuf::from)
        }
    }

    pub const CURSOR_CMD: &str = r"C:\Users\u\AppData\Local\Programs\cursor\resources\app\bin\cursor.cmd";
    pub const CURSOR_CODE: &str = r"c:\Users\u\AppData\Local\Programs\cursor\resources\app\codeBin\code.cmd";
    pub const VSCODE_CMD: &str = r"C:\Users\u\AppData\Local\Programs\Microsoft VS Code\bin\code.cmd";
    pub const WT: &str = r"C:\Users\u\AppData\Local\Microsoft\WindowsApps\wt.exe";
    pub const PS5: &str = r"C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe";
    pub const PWSH: &str = r"C:\Program Files\PowerShell\7\pwsh.exe";

    pub fn this_machine() -> Fake {
        let mut f = Fake::default();
        f.path.insert("cursor", vec![CURSOR_CMD]);
        f.path.insert("code", vec![CURSOR_CODE]);
        f.path.insert("wt", vec![WT]);
        f.path.insert("powershell", vec![PS5]);
        f.files.extend([CURSOR_CMD, CURSOR_CODE, WT, PS5]);
        f.dirs.insert("LOCALAPPDATA", r"C:\Users\u\AppData\Local");
        f.dirs.insert("ProgramFiles", r"C:\Program Files");
        f.dirs.insert("SystemRoot", r"C:\WINDOWS");
        f
    }
}

#[cfg(test)]
mod tests {
    use super::fake::*;
    use super::*;

    fn args(l: &Launch) -> Vec<String> {
        l.args.iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    // ── editors ───────────────────────────────────────────────────────────────

    #[test]
    fn cursor_is_the_first_path_hit_and_the_folder_is_one_argument() {
        let l = plan_editor(Editor::Cursor, &this_machine(), Path::new(r"C:\a & b\100%^")).unwrap();
        assert_eq!(l.program, PathBuf::from(CURSOR_CMD));
        assert_eq!(args(&l), vec![r"C:\a & b\100%^"]);
        assert_eq!(l.via, Via::Cursor);
        assert!(!l.new_console);
    }

    #[test]
    fn cursor_is_found_in_its_default_install_when_it_is_not_on_path() {
        let mut m = this_machine();
        m.path.remove("cursor");
        let l = plan_editor(Editor::Cursor, &m, Path::new(r"C:\p")).unwrap();
        assert_eq!(l.program, PathBuf::from(CURSOR_CMD));
        m.files.remove(CURSOR_CMD);
        assert!(plan_editor(Editor::Cursor, &m, Path::new(r"C:\p")).is_none());
    }

    #[test]
    fn vs_code_never_picks_cursors_code_shim() {
        // `code` on PATH is Cursor's own shim on this machine; asking for VS Code
        // must not open Cursor.
        let mut m = this_machine();
        m.path.insert("code", vec![CURSOR_CODE, VSCODE_CMD]);
        m.files.insert(VSCODE_CMD);
        let l = plan_editor(Editor::Vscode, &m, Path::new(r"C:\p")).unwrap();
        assert_eq!(l.program, PathBuf::from(VSCODE_CMD));
        assert_eq!(l.via, Via::Vscode);
    }

    #[test]
    fn vs_code_falls_back_to_its_standard_install_or_to_nothing() {
        let mut m = this_machine(); // only Cursor's shim on PATH
        assert!(plan_editor(Editor::Vscode, &m, Path::new(r"C:\p")).is_none());
        m.files.insert(VSCODE_CMD);
        let l = plan_editor(Editor::Vscode, &m, Path::new(r"C:\p")).unwrap();
        assert_eq!(l.program, PathBuf::from(VSCODE_CMD));
        // The machine-wide install is the last resort.
        let mut m = this_machine();
        let system = r"C:\Program Files\Microsoft VS Code\bin\code.cmd";
        m.files.insert(system);
        assert_eq!(
            plan_editor(Editor::Vscode, &m, Path::new(r"C:\p")).unwrap().program,
            PathBuf::from(system)
        );
    }

    #[test]
    fn an_empty_folder_launches_the_editor_alone() {
        let l = plan_editor(Editor::Cursor, &this_machine(), Path::new("")).unwrap();
        assert_eq!(l.program, PathBuf::from(CURSOR_CMD));
        assert!(l.args.is_empty(), "an empty path must not become an empty argument");
    }

    #[test]
    fn editor_none_plans_nothing() {
        assert!(plan_editor(Editor::None, &this_machine(), Path::new(r"C:\p")).is_none());
    }

    // ── terminals ─────────────────────────────────────────────────────────────

    #[test]
    fn windows_terminal_gets_the_folder_and_the_resolved_shell_as_separate_arguments() {
        let l = plan_terminal(Terminal::WindowsTerminal, &this_machine(), Path::new(r"C:\My Projects\app")).unwrap();
        assert_eq!(l.program, PathBuf::from(WT));
        assert_eq!(args(&l), vec!["-d", r"C:\My Projects\app", PS5]);
        assert_eq!(l.via, Via::WindowsTerminal);
        assert_eq!(l.cwd, None);
    }

    #[test]
    fn pwsh_wins_over_windows_powershell_when_it_exists() {
        let mut m = this_machine();
        m.files.insert(PWSH);
        let l = plan_terminal(Terminal::WindowsTerminal, &m, Path::new(r"C:\p")).unwrap();
        assert_eq!(args(&l), vec!["-d", r"C:\p", PWSH]);
    }

    #[test]
    fn shell_resolution_order_is_path_then_install_then_windows_powershell() {
        let mut m = this_machine();
        assert_eq!(resolve_shell(&m), Some((PathBuf::from(PS5), false)));
        m.files.insert(PWSH);
        assert_eq!(resolve_shell(&m), Some((PathBuf::from(PWSH), true)));
        m.path.insert("pwsh", vec![r"C:\tools\pwsh.exe"]);
        m.files.insert(r"C:\tools\pwsh.exe");
        assert_eq!(resolve_shell(&m), Some((PathBuf::from(r"C:\tools\pwsh.exe"), true)));
        // Nothing at all.
        assert_eq!(resolve_shell(&Fake::default()), None);
    }

    #[test]
    fn a_semicolon_is_refused_for_windows_terminal_because_it_splits_commands() {
        let err = plan_terminal(Terminal::WindowsTerminal, &this_machine(), Path::new(r"C:\a;b")).unwrap_err();
        assert!(matches!(err, LaunchError::UnsafePath(_)));
        // The plain shell has no such problem: the folder is a working directory.
        assert!(plan_terminal(Terminal::Shell, &this_machine(), Path::new(r"C:\a;b")).is_ok());
    }

    #[test]
    fn a_missing_windows_terminal_or_shell_is_reported_not_guessed() {
        let mut m = this_machine();
        m.path.remove("wt");
        assert_eq!(
            plan_terminal(Terminal::WindowsTerminal, &m, Path::new(r"C:\p")).unwrap_err(),
            LaunchError::NotFound("wt.exe")
        );
        assert_eq!(
            plan_terminal(Terminal::Shell, &Fake::default(), Path::new(r"C:\p")).unwrap_err(),
            LaunchError::NotFound("a PowerShell")
        );
    }

    #[test]
    fn the_plain_shell_opens_its_own_console_in_the_folder() {
        let l = plan_terminal(Terminal::Shell, &this_machine(), Path::new(r"C:\proj")).unwrap();
        assert_eq!(l.program, PathBuf::from(PS5));
        assert_eq!(args(&l), vec!["-NoExit"]);
        assert_eq!(l.cwd, Some(PathBuf::from(r"C:\proj")));
        assert!(l.new_console);
        assert_eq!(l.via, Via::Shell);
    }

    #[test]
    fn terminal_none_is_disabled() {
        assert_eq!(
            plan_terminal(Terminal::None, &this_machine(), Path::new(r"C:\p")).unwrap_err(),
            LaunchError::Disabled
        );
    }

    #[test]
    fn explorer_is_just_explorer_and_the_folder() {
        let l = plan_explorer(Path::new(r"C:\p"));
        assert_eq!(l.program, PathBuf::from("explorer.exe"));
        assert_eq!(args(&l), vec![r"C:\p"]);
        assert_eq!(l.via, Via::Explorer);
    }

    // ── settings and folders ──────────────────────────────────────────────────

    #[test]
    fn unknown_setting_values_fall_back_to_the_defaults() {
        assert_eq!(Editor::from_setting("cursor"), Editor::Cursor);
        assert_eq!(Editor::from_setting("vscode"), Editor::Vscode);
        assert_eq!(Editor::from_setting("none"), Editor::None);
        assert_eq!(Editor::from_setting(""), Editor::Cursor);
        assert_eq!(Editor::from_setting("emacs"), Editor::Cursor);
        assert_eq!(Terminal::from_setting("windowsTerminal"), Terminal::WindowsTerminal);
        assert_eq!(Terminal::from_setting("shell"), Terminal::Shell);
        assert_eq!(Terminal::from_setting("none"), Terminal::None);
        assert_eq!(Terminal::from_setting("kitty"), Terminal::WindowsTerminal);
    }

    #[test]
    fn only_an_absolute_existing_directory_is_a_valid_folder() {
        let root = std::env::temp_dir().join(format!("coucou-launch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("proj one");
        std::fs::create_dir_all(&dir).unwrap();
        let file = root.join("a.txt");
        std::fs::write(&file, b"x").unwrap();

        assert_eq!(validate_folder(dir.to_str().unwrap()).unwrap(), dir);
        // A trailing separator is dropped.
        let with_slash = format!("{}\\", dir.display());
        assert_eq!(validate_folder(&with_slash).unwrap(), dir);

        for bad in ["", "proj", r"..\proj", "-d", r"\\?\", file.to_str().unwrap(), r"Z:\definitely\not\here"] {
            assert!(validate_folder(bad).is_err(), "{bad:?} must be refused");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn describe_reports_what_this_machine_has_for_the_settings_window() {
        // This machine: Cursor yes, VS Code only as Cursor's shim (so no), wt yes,
        // PowerShell 5.1 only.
        let info = describe(&this_machine());
        assert_eq!(
            serde_json::to_value(&info).unwrap(),
            serde_json::json!({
                "cursor": true, "vscode": false, "windowsTerminal": true, "shell": "powershell"
            })
        );
        let mut with_pwsh = this_machine();
        with_pwsh.files.insert(PWSH);
        assert_eq!(describe(&with_pwsh).shell.as_deref(), Some("pwsh"));
        let bare = describe(&Fake::default());
        assert!(!bare.cursor && !bare.vscode && !bare.windows_terminal && bare.shell.is_none());
    }

    #[test]
    fn a_drive_root_keeps_its_separator() {
        assert_eq!(validate_folder(r"C:\").unwrap(), PathBuf::from(r"C:\"));
    }
}
