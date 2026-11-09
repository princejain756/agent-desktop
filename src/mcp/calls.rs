//! Tool calls in flight, keyed by JSON-RPC request id, so `notifications/cancelled` can stop one.

use serde_json::Value;
use std::collections::{HashMap, hash_map::Entry};
use std::io;
use std::process::{Child, ChildStderr, ChildStdout, Command};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

const TERMINATE_GRACE: Duration = Duration::from_secs(3);
const EXIT_POLL: Duration = Duration::from_millis(20);

/// Output pipes of a started call.
pub(crate) type Pipes = (Option<ChildStdout>, Option<ChildStderr>);

/// Each admitted request id, with the child process its call started once it has started one.
enum Slot {
    /// Admitted and not cancelled; carries the child once the call has spawned it.
    Running(Option<Child>),
    /// Cancelled; the id stays occupied until its own call finishes so it cannot be re-admitted while
    /// that call is still reading its child's pipes.
    Cancelled,
}

/// Each admitted request id: `Running` or `Cancelled`, occupied either way until `finish`.
#[derive(Default)]
pub(crate) struct Calls(Mutex<HashMap<String, Slot>>);

impl Calls {
    /// Admits `id` before its call starts; false when a call with that id is already in flight.
    pub(crate) fn admit(&self, id: &Value) -> bool {
        match self.lock().entry(id.to_string()) {
            Entry::Occupied(_) => false,
            Entry::Vacant(entry) => {
                entry.insert(Slot::Running(None));
                true
            }
        }
    }

    /// Spawns `command` for `id` and returns its pipes, or `None` when `id` is not admitted or was cancelled.
    /// The registry stays locked while the child spawns, so a cancel lands either before it, and nothing
    /// starts, or after it, and the child is killed.
    pub(crate) fn start(&self, id: &Value, command: &mut Command) -> Option<io::Result<Pipes>> {
        let mut calls = self.lock();
        let Some(Slot::Running(slot)) = calls.get_mut(&id.to_string()) else {
            return None;
        };
        Some(command.spawn().map(|mut child| {
            let pipes = (child.stdout.take(), child.stderr.take());
            *slot = Some(child);
            pipes
        }))
    }

    /// Takes back the child of `id` after its output closed, or `None` when `id` was cancelled.
    pub(crate) fn take(&self, id: &Value) -> Option<Child> {
        match self.lock().get_mut(&id.to_string()) {
            Some(Slot::Running(slot)) => slot.take(),
            _ => None,
        }
    }

    /// Cancels `id`; an unknown id is ignored. The id stays occupied until its own call's `finish`,
    /// so a reused id is refused while that call is still running. A started child is stopped on its
    /// own thread by `terminate`.
    pub(crate) fn cancel(&self, id: &Value) {
        let child = match self.lock().get_mut(&id.to_string()) {
            Some(slot) => match std::mem::replace(slot, Slot::Cancelled) {
                Slot::Running(child) => child,
                Slot::Cancelled => None,
            },
            None => None,
        };
        if let Some(child) = child {
            std::thread::spawn(move || terminate(child));
        }
    }

    /// Releases `id` once its call has returned; false when it was cancelled, so no response is due.
    pub(crate) fn finish(&self, id: &Value) -> bool {
        matches!(self.lock().remove(&id.to_string()), Some(Slot::Running(_)))
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Slot>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Asks `child` to exit with SIGTERM, which lets the CLI release held keys and buttons first (it waits up to
/// 2 s for that on Linux), and kills it if it is still running after `TERMINATE_GRACE`. Without SIGTERM
/// (Windows) it kills at once.
fn terminate(mut child: Child) {
    if request_exit(&child) {
        let deadline = Instant::now() + TERMINATE_GRACE;
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(EXIT_POLL),
                Err(_) => break,
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
fn request_exit(child: &Child) -> bool {
    libc::pid_t::try_from(child.id())
        .is_ok_and(|pid| unsafe { libc::kill(pid, libc::SIGTERM) } == 0)
}

#[cfg(not(unix))]
fn request_exit(_child: &Child) -> bool {
    false
}

#[cfg(test)]
#[path = "calls_tests.rs"]
mod tests;
