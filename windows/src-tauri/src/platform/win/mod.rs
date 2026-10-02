// The Windows implementation of `Platform`.

pub mod focus;
mod env;
mod spawn;

use super::{Candidate, FocusMethod, FocusOutcome, Platform};
use crate::launch::{Env, Launch};

pub struct Windows;

static ENV: env::RealEnv = env::RealEnv;

impl Platform for Windows {
    fn focus(&self, candidates: &[Candidate], hints: &[String]) -> (FocusOutcome, FocusMethod) {
        focus::focus(candidates, hints)
    }

    fn process_alive(&self, pid: u32, exe: &str) -> bool {
        focus::is_running(pid, exe)
    }

    fn launch_env(&self) -> &dyn Env {
        &ENV
    }

    fn spawn(&self, launch: &Launch) -> std::io::Result<()> {
        spawn::spawn(launch)
    }
}
