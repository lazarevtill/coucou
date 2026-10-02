// "Open this project in…" — the one entry point behind the island's two buttons.
//
// Validates the folder, plans the launch, spawns it, and falls back to Explorer
// when the chosen launcher is missing, refused the folder, or failed to start.
// What happened is reported back so the island can say so.

use std::path::Path;

use serde::Serialize;

use crate::launch::{self, Editor, Terminal};

/// What opening a project needs from the machine. The real one is `RealHost`;
/// the tests use one that records instead of starting anything.
pub trait Host {
    /// How the launcher sees this machine (PATH, install folders).
    fn env(&self) -> &dyn launch::Env;
    /// Starts a planned launch. Never through a shell.
    fn spawn(&self, launch: &launch::Launch) -> std::io::Result<()>;
    /// Shows the folder in the system file manager.
    fn reveal(&self, folder: &Path) -> std::io::Result<()>;
}

/// This machine, through the platform module.
pub struct RealHost;

impl Host for RealHost {
    fn env(&self) -> &dyn launch::Env {
        &launch::SystemEnv
    }
    fn spawn(&self, launch: &launch::Launch) -> std::io::Result<()> {
        crate::platform::spawn_launch(launch)
    }
    fn reveal(&self, folder: &Path) -> std::io::Result<()> {
        // Best effort, like every other use of it: the file manager is always there.
        crate::platform::reveal_folder(&folder.to_string_lossy());
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Editor,
    Terminal,
}

impl Target {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "editor" => Some(Target::Editor),
            "terminal" => Some(Target::Terminal),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenResult {
    /// `cursor`, `vscode`, `windowsTerminal`, `shell`, `explorer`, or `none`.
    pub via: String,
    /// The file manager was used instead of what the settings asked for.
    pub fell_back: bool,
    /// Why the preferred launcher was not used, or why nothing could be opened.
    pub error: Option<String>,
}

pub fn open_project(
    host: &dyn Host,
    editor: Editor,
    terminal: Terminal,
    target: Target,
    folder: Option<&str>,
) -> OpenResult {
    let nothing = |error: Option<String>| OpenResult { via: "none".into(), fell_back: false, error };

    let folder = match folder.filter(|f| !f.is_empty()) {
        Some(raw) => match launch::validate_folder(raw) {
            Ok(f) => f,
            Err(launch::LaunchError::BadFolder(why)) => return nothing(Some(why)),
            Err(other) => return nothing(Some(format!("{other:?}"))),
        },
        // An editor can be launched on its own; a terminal needs somewhere to be.
        None if target == Target::Editor => std::path::PathBuf::new(),
        None => return nothing(Some("There is no project folder to open.".into())),
    };
    let env = host.env();

    // What the settings ask for. `Ok(None)` is "switched off": do nothing, quietly.
    let planned: Result<Option<launch::Launch>, String> = match target {
        Target::Editor => match editor {
            Editor::None => Ok(None),
            _ => launch::plan_editor(editor, env, &folder)
                .map(Some)
                .ok_or_else(|| "The chosen editor was not found.".to_string()),
        },
        Target::Terminal => match launch::plan_terminal(terminal, env, &folder) {
            Ok(l) => Ok(Some(l)),
            Err(launch::LaunchError::Disabled) => Ok(None),
            Err(launch::LaunchError::NotFound(what)) => Err(format!("{what} was not found.")),
            Err(launch::LaunchError::UnsafePath(why)) => Err(why.to_string()),
            Err(launch::LaunchError::BadFolder(why)) => Err(why),
        },
    };

    let reason = match planned {
        Ok(None) => return nothing(None),
        Ok(Some(l)) => match host.spawn(&l) {
            Ok(()) => return OpenResult { via: l.via.name().into(), fell_back: false, error: None },
            Err(err) => format!("Could not start it: {err}"),
        },
        Err(reason) => reason,
    };

    // The file manager always works on an existing folder, so it answers any of the above.
    match host.reveal(&folder) {
        Ok(()) => OpenResult { via: launch::Via::Explorer.name().into(), fell_back: true, error: Some(reason) },
        Err(err) => nothing(Some(format!("{reason} The file manager failed too: {err}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::fake::*;
    use crate::launch::{Env, Launch, Via};
    use std::path::Path;
    use std::sync::Mutex;

    /// Records what would have been started, and can refuse chosen launchers.
    /// Revealing a folder is recorded as an Explorer launch.
    struct FakePlatform {
        env: Fake,
        spawned: Mutex<Vec<Launch>>,
        refuse: Vec<Via>,
    }

    impl FakePlatform {
        fn new(env: Fake) -> Self {
            Self { env, spawned: Mutex::new(vec![]), refuse: vec![] }
        }
        fn started(&self) -> Vec<Launch> {
            self.spawned.lock().unwrap().clone()
        }
        fn record(&self, launch: Launch) -> std::io::Result<()> {
            if self.refuse.contains(&launch.via) {
                return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "refused"));
            }
            self.spawned.lock().unwrap().push(launch);
            Ok(())
        }
    }

    impl Host for FakePlatform {
        fn env(&self) -> &dyn Env {
            &self.env
        }
        fn spawn(&self, launch: &Launch) -> std::io::Result<()> {
            self.record(launch.clone())
        }
        fn reveal(&self, folder: &Path) -> std::io::Result<()> {
            self.record(Launch {
                program: "explorer.exe".into(),
                args: vec![folder.as_os_str().to_owned()],
                cwd: None,
                new_console: false,
                via: Via::Explorer,
            })
        }
    }

    /// A real directory, because the folder is checked against the file system.
    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("coucou-open-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn open(p: &FakePlatform, ed: Editor, term: Terminal, target: Target, folder: &std::path::Path) -> OpenResult {
        open_project(p, ed, term, target, folder.to_str())
    }

    #[test]
    fn the_editor_button_opens_the_chosen_editor_with_the_folder() {
        let dir = temp_dir("editor");
        let p = FakePlatform::new(this_machine());
        let r = open(&p, Editor::Cursor, Terminal::WindowsTerminal, Target::Editor, &dir);
        assert_eq!(r, OpenResult { via: "cursor".into(), fell_back: false, error: None });
        let started = p.started();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].via, Via::Cursor);
        assert_eq!(started[0].args, vec![dir.as_os_str().to_owned()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_editor_falls_back_to_explorer_and_says_why() {
        let dir = temp_dir("missing");
        let mut env = this_machine();
        env.path.remove("cursor");
        env.files.remove(CURSOR_CMD);
        let p = FakePlatform::new(env);
        let r = open(&p, Editor::Cursor, Terminal::WindowsTerminal, Target::Editor, &dir);
        assert_eq!(r.via, "explorer");
        assert!(r.fell_back);
        assert!(r.error.is_some());
        assert_eq!(p.started().len(), 1);
        assert_eq!(p.started()[0].via, Via::Explorer);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_launcher_that_fails_to_start_also_falls_back() {
        let dir = temp_dir("fails");
        let mut p = FakePlatform::new(this_machine());
        p.refuse = vec![Via::Cursor];
        let r = open(&p, Editor::Cursor, Terminal::WindowsTerminal, Target::Editor, &dir);
        assert_eq!((r.via.as_str(), r.fell_back), ("explorer", true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_opens_when_the_setting_is_none() {
        let dir = temp_dir("none");
        let p = FakePlatform::new(this_machine());
        let r = open(&p, Editor::None, Terminal::None, Target::Editor, &dir);
        assert_eq!(r, OpenResult { via: "none".into(), fell_back: false, error: None });
        let r = open(&p, Editor::None, Terminal::None, Target::Terminal, &dir);
        assert_eq!(r, OpenResult { via: "none".into(), fell_back: false, error: None });
        assert!(p.started().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_terminal_button_opens_windows_terminal_in_the_folder() {
        let dir = temp_dir("terminal");
        let p = FakePlatform::new(this_machine());
        let r = open(&p, Editor::Cursor, Terminal::WindowsTerminal, Target::Terminal, &dir);
        assert_eq!(r.via, "windowsTerminal");
        let started = p.started();
        assert_eq!(started[0].args[0], "-d");
        assert_eq!(started[0].args[1], dir.as_os_str());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_windows_terminal_cannot_take_goes_to_explorer() {
        // `;` is legal in a Windows folder name and is a command separator to wt.exe.
        let dir = temp_dir("semi;colon");
        let p = FakePlatform::new(this_machine());
        let r = open(&p, Editor::Cursor, Terminal::WindowsTerminal, Target::Terminal, &dir);
        assert_eq!((r.via.as_str(), r.fell_back), ("explorer", true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bad_folder_starts_nothing_at_all() {
        let p = FakePlatform::new(this_machine());
        for folder in [Some("relative"), Some(r"Z:\no\such\folder")] {
            for target in [Target::Editor, Target::Terminal] {
                let r = open_project(&p, Editor::Cursor, Terminal::WindowsTerminal, target, folder);
                assert_eq!(r.via, "none", "{folder:?} {target:?}");
                assert!(r.error.is_some(), "{folder:?} {target:?}");
            }
        }
        assert!(p.started().is_empty());
    }

    #[test]
    fn the_editor_alone_can_be_launched_without_a_project() {
        // The idle card's "Open Cursor" has no session, so no folder.
        let p = FakePlatform::new(this_machine());
        for none in [None, Some("")] {
            let r = open_project(&p, Editor::Cursor, Terminal::WindowsTerminal, Target::Editor, none);
            assert_eq!(r, OpenResult { via: "cursor".into(), fell_back: false, error: None });
        }
        let started = p.started();
        assert_eq!(started.len(), 2);
        assert!(started.iter().all(|l| l.args.is_empty()), "no folder, no argument");
    }

    #[test]
    fn a_terminal_needs_a_folder() {
        let p = FakePlatform::new(this_machine());
        for none in [None, Some("")] {
            let r = open_project(&p, Editor::Cursor, Terminal::WindowsTerminal, Target::Terminal, none);
            assert_eq!(r.via, "none");
            assert!(r.error.is_some());
        }
        assert!(p.started().is_empty());
    }

    #[test]
    fn when_even_explorer_fails_the_error_is_reported() {
        let dir = temp_dir("allfail");
        let mut p = FakePlatform::new(this_machine());
        p.refuse = vec![Via::Cursor, Via::Explorer];
        let r = open(&p, Editor::Cursor, Terminal::WindowsTerminal, Target::Editor, &dir);
        assert_eq!((r.via.as_str(), r.fell_back), ("none", false));
        assert!(r.error.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
