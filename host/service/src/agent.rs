//! The `pi --mode rpc` child, and how it is started and stopped.
//!
//! One child at a time, by L1. It gets its own process group, because stopping
//! the agent means stopping what the agent started: tmux panes, sub-agents,
//! whatever a tool left running. Signalling the group is the only way to say
//! that, and it is what `scufris-desktop` already does for widget backends.
//!
//! Stopping is a sequence rather than a signal. Closing stdin is how an RPC
//! client says goodbye, so the agent gets that first and a bound to act on it.
//! `SIGTERM` follows, then `SIGKILL`, then the exit status is collected. A
//! child that is never waited for is a zombie, and a supervisor that leaks
//! those is a supervisor that stops being able to count its own restarts.

use std::{
    ffi::OsString,
    io::{self, Write},
    os::unix::process::CommandExt,
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use serde::Serialize;
use tracing::{debug, warn};

use crate::config::Config;

/// How long the agent has to go on its own terms after stdin closes.
const GOODBYE: Duration = Duration::from_secs(5);

/// How often the wait for a stopping agent looks again.
const GOODBYE_STEP: Duration = Duration::from_millis(50);

/// One running agent.
pub struct Agent {
    child: Child,
    /// The process group to signal, which is the child's own pid.
    leader: i32,
    /// Held so it can be closed first and by whoever stops the agent.
    stdin: Arc<Mutex<Option<ChildStdin>>>,
}

/// The streams a freshly started agent hands back for reading.
pub struct Streams {
    /// Events.
    pub stdout: ChildStdout,
    /// Diagnostics.
    pub stderr: ChildStderr,
}

impl Agent {
    /// Starts one agent in RPC mode on the configured session directory.
    pub fn start(config: &Config) -> io::Result<(Self, Streams)> {
        std::fs::create_dir_all(&config.session_dir)?;
        let mut command = Command::new(&config.agent);
        command
            .args(config.agent_args())
            .current_dir(&config.working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Its own group, so one signal reaches everything the agent started.
        command.process_group(0);
        let mut child = command.spawn()?;
        let leader = i32::try_from(child.id())
            .map_err(|_| io::Error::other("the agent has no representable pid"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("the agent has no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("the agent has no stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("the agent has no stderr"))?;
        debug!(pid = leader, agent = %config.agent.display(), "the agent started");
        Ok((
            Self {
                child,
                leader,
                stdin: Arc::new(Mutex::new(Some(stdin))),
            },
            Streams { stdout, stderr },
        ))
    }

    /// The handle a command is written through.
    ///
    /// Cloned out rather than borrowed, so a caller writes to the agent
    /// without holding the service's own lock across the write.
    pub fn writer(&self) -> Writer {
        Writer {
            stdin: Arc::clone(&self.stdin),
        }
    }

    /// Stops the agent and collects its exit status.
    ///
    /// Blocking, and bounded by [`GOODBYE`]. The caller is either shutting the
    /// service down or handing the session to a terminal, and both of those
    /// have to be finished before the next thing starts.
    pub fn stop(mut self) -> ExitStatus {
        // Closing stdin is how an RPC client says goodbye. A well behaved
        // agent exits on it and never sees a signal.
        drop(
            self.stdin
                .lock()
                .unwrap_or_else(|held| held.into_inner())
                .take(),
        );
        let mut waited = Duration::ZERO;
        let mut termed = false;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return status,
                Ok(None) => {}
                Err(error) => {
                    warn!(%error, "the agent could not be waited for");
                    return ExitStatus::default();
                }
            }
            if waited >= GOODBYE {
                break;
            }
            // Half the bound on goodbye, half on SIGTERM. An agent that
            // ignores a closed stdin usually answers the signal.
            if !termed && waited >= GOODBYE / 2 {
                self.signal(Signal::SIGTERM);
                termed = true;
            }
            thread::sleep(GOODBYE_STEP);
            waited = waited.saturating_add(GOODBYE_STEP);
        }
        warn!(pid = self.leader, "the agent would not stop, killing it");
        self.signal(Signal::SIGKILL);
        self.child.wait().unwrap_or_default()
    }

    /// Signals the whole group the agent leads.
    fn signal(&self, signal: Signal) {
        if let Err(error) = killpg(Pid::from_raw(self.leader), signal) {
            // ESRCH is the ordinary case of a group that is already gone.
            debug!(pid = self.leader, ?signal, %error, "the agent group would not take the signal");
        }
    }
}

/// Writes commands to one agent.
#[derive(Clone)]
pub struct Writer {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
}

impl Writer {
    /// Sends one command as a single LF-terminated line.
    pub fn send(&self, command: &impl Serialize) -> io::Result<()> {
        let mut line = serde_json::to_vec(command)?;
        line.push(b'\n');
        let mut held = self.stdin.lock().unwrap_or_else(|held| held.into_inner());
        let stdin = held
            .as_mut()
            .ok_or_else(|| io::Error::other("the agent is not running"))?;
        stdin.write_all(&line)?;
        stdin.flush()
    }
}

/// The arguments one agent is started with, for logging.
pub fn described(config: &Config) -> String {
    let mut parts: Vec<OsString> = vec![config.agent.clone().into_os_string()];
    parts.extend(config.agent_args());
    parts
        .iter()
        .map(|part| part.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn config(agent: &str, session_dir: PathBuf) -> Config {
        Config {
            agent: PathBuf::from(agent),
            session_dir,
            surface_socket: PathBuf::from("/run/user/1000/scufris/surface.sock"),
            agent_socket: PathBuf::from("/run/user/1000/scufris/agent.sock"),
            control_socket: PathBuf::from("/run/user/1000/scufris/control.sock"),
            content_socket: PathBuf::from("/run/user/1000/scufris/content.sock"),
            attachment_dir: PathBuf::from("/home/test/.local/share/scufris/attachments"),
            working_dir: std::env::temp_dir(),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("scufris-agent-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    /// One stand-in agent running the given shell program.
    ///
    /// Built by hand rather than through [`Agent::start`], because what the
    /// service starts is fixed by its configuration and these tests are about
    /// what happens to a child once it is running. `/bin/sh` is the one
    /// program a build sandbox is guaranteed to have.
    fn fake(program: &str) -> Agent {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", program])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = command.spawn().expect("the shell starts");
        let leader = i32::try_from(child.id()).expect("the pid fits");
        let stdin = child.stdin.take().expect("it has stdin");
        Agent {
            child,
            leader,
            stdin: Arc::new(Mutex::new(Some(stdin))),
        }
    }

    #[test]
    fn an_agent_that_is_not_there_fails_to_start_rather_than_being_reported_as_running() {
        let started = Agent::start(&config("/nonexistent/scufris", scratch("missing")));
        let Err(error) = started else {
            panic!("there is no such program");
        };
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn starting_the_agent_makes_the_session_directory_it_was_given() {
        let session_dir = scratch("made");
        let _ = Agent::start(&config("/nonexistent/scufris", session_dir.clone()));
        assert!(
            session_dir.is_dir(),
            "the directory is made before the agent is asked to store anything in it"
        );
        std::fs::remove_dir_all(&session_dir).expect("the directory is removed");
    }

    #[test]
    fn an_agent_that_reads_its_stdin_stops_when_stdin_closes() {
        // An agent that never answers and exits on end of input, which is
        // exactly the goodbye path being asserted here: no signal is needed.
        let agent = fake("while IFS= read -r line; do :; done");
        let writer = agent.writer();
        writer
            .send(&serde_json::json!({ "type": "get_state" }))
            .expect("the line is written");
        let started = std::time::Instant::now();
        let status = agent.stop();
        assert!(status.success(), "{status}");
        assert!(
            started.elapsed() < GOODBYE / 2,
            "a polite agent is never signalled, took {:?}",
            started.elapsed()
        );
        // The handle is closed with the agent, so a late command is refused
        // rather than written into a pipe with nobody on the other end.
        assert!(writer.send(&serde_json::json!({})).is_err());
    }

    #[test]
    fn an_agent_that_ignores_goodbye_is_signalled_and_then_killed() {
        // One that traps the polite signals and never exits on its own. It has
        // to be stopped, and it has to be stopped in bounded time.
        let agent = fake("trap '' TERM INT; while :; do sleep 1; done");
        let started = std::time::Instant::now();
        let status = agent.stop();
        assert!(!status.success(), "{status}");
        assert!(
            started.elapsed() < GOODBYE * 3,
            "stopping is bounded, took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn the_agent_is_started_in_rpc_mode_on_its_own_session_directory() {
        let described = described(&config("/bin/scufris", PathBuf::from("/srv/sessions")));
        assert_eq!(
            described,
            "/bin/scufris --session-dir /srv/sessions --continue --mode rpc"
        );
    }
}
