// Everything that only means something on one operating system, behind one small
// interface, so the rest of the app never names Win32 and a Linux port has a
// single place to fill in (a Unix socket for the relay, Secret Service for keys,
// X11/Wayland for focusing a window).
//
// Only what the Windows build needs today is here. The relay pipe, the
// Credential Manager and the island window still live in pipe.rs, secrets.rs and
// island.rs, and will move behind this trait when they are next touched.

use crate::launch::{Env, Launch};

mod win;

pub use win::focus::{Candidate, Method as FocusMethod, Outcome as FocusOutcome};

pub trait Platform: Send + Sync {
    /// Brings the best window of the first live candidate to the front, and says
    /// which step of the ladder it took to get there.
    fn focus(&self, candidates: &[Candidate], hints: &[String]) -> (FocusOutcome, FocusMethod);
    /// Is `pid` still running `exe`? Pids are recycled, so a bare pid proves nothing.
    fn process_alive(&self, pid: u32, exe: &str) -> bool;
    /// How the launcher sees this machine (PATH, install folders).
    fn launch_env(&self) -> &dyn Env;
    /// Starts a planned launch. Never through a shell.
    fn spawn(&self, launch: &Launch) -> std::io::Result<()>;
}

pub fn current() -> &'static dyn Platform {
    &win::Windows
}
