// Starts a planned launch: a program and its arguments, never a command line.
// `cmd /C` is deliberately nowhere near this — see launch.rs.

use std::os::windows::process::CommandExt;
use std::process::Command;

use crate::launch::{Launch, Via};

/// Keeps a `.cmd` launcher from flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// A console program that is meant to be seen gets its own window.
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

pub fn spawn(launch: &Launch) -> std::io::Result<()> {
    let mut command = Command::new(&launch.program);
    command.args(&launch.args);
    if let Some(cwd) = &launch.cwd {
        command.current_dir(cwd);
    }
    let flags = match launch.via {
        Via::Cursor | Via::Vscode => CREATE_NO_WINDOW,
        Via::Shell => CREATE_NEW_CONSOLE,
        // wt.exe and explorer.exe are GUI programs and need no flag.
        Via::WindowsTerminal | Via::Explorer => 0,
    };
    command.creation_flags(flags).spawn().map(|_| ())
}
