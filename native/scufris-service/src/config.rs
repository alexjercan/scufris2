//! Where the service gets the three things it needs to run.
//!
//! The agent to spawn, the directory its sessions live in, and the socket to
//! bind. The first two are named by an option or by the environment behind it,
//! and the socket is fixed by the session runtime directory. Nothing is read
//! from a configuration file: a second place to say which `pi` this is would
//! be a second place for it to be wrong.
//!
//! The defaults exist so the binary can be run by hand from a terminal, which
//! is how it gets tested before there is a unit for it.

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

use scufris_control::service::service_socket_path;

/// Environment variable naming the agent program to supervise.
pub const AGENT_VARIABLE: &str = "SCUFRIS_SERVICE_AGENT";

/// Environment variable naming the session directory.
pub const SESSION_DIR_VARIABLE: &str = "SCUFRIS_SERVICE_SESSION_DIR";

/// Program name looked up on `PATH` when [`AGENT_VARIABLE`] is unset.
pub const DEFAULT_AGENT: &str = "scufris";

/// Session directory below the user's data directory, when nothing names one.
pub const DEFAULT_SESSION_SUBDIR: &str = "scufris/sessions";

/// Everything one run of the service is configured with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Absolute path of the agent program. It is `pi` with the Scufris
    /// extensions already on its command line, which is what the `scufris`
    /// launcher is.
    pub agent: PathBuf,
    /// Directory the agent stores and resumes its sessions in.
    pub session_dir: PathBuf,
    /// Socket the service binds.
    pub socket: PathBuf,
    /// Directory the agent runs in.
    pub working_dir: PathBuf,
}

impl Config {
    /// Completes the configuration from this process's environment.
    ///
    /// The two named values are what the command line and the variables behind
    /// it settled on. Everything else has no option, so it is read here.
    pub fn from_environment(
        agent: Option<OsString>,
        session_dir: Option<OsString>,
    ) -> Result<Self, ConfigError> {
        Self::resolve(
            agent,
            session_dir,
            env::var_os("XDG_DATA_HOME"),
            env::var_os("HOME"),
            env::var_os("PATH"),
        )
        .and_then(|partial| {
            Ok(Self {
                socket: service_socket_path().map_err(|error| {
                    ConfigError::Missing(format!("the service socket has no path: {error}"))
                })?,
                ..partial
            })
        })
    }

    /// Resolves everything but the socket from values rather than from the
    /// process environment, so the rules can be tested without a test setting
    /// a variable every other test in the process can see.
    fn resolve(
        agent: Option<OsString>,
        session_dir: Option<OsString>,
        data_home: Option<OsString>,
        home: Option<OsString>,
        path: Option<OsString>,
    ) -> Result<Self, ConfigError> {
        let home = present(home)
            .map(PathBuf::from)
            .ok_or_else(|| ConfigError::Missing("HOME is required".into()))?;
        let agent = match present(agent) {
            Some(named) => {
                let named = PathBuf::from(named);
                if !named.is_absolute() {
                    return Err(ConfigError::Relative(AGENT_VARIABLE, named));
                }
                named
            }
            None => on_path(DEFAULT_AGENT, path).ok_or(ConfigError::NoAgent)?,
        };
        let session_dir = match present(session_dir) {
            Some(named) => {
                let named = PathBuf::from(named);
                if !named.is_absolute() {
                    return Err(ConfigError::Relative(SESSION_DIR_VARIABLE, named));
                }
                named
            }
            None => match present(data_home).map(PathBuf::from) {
                Some(data) if data.is_absolute() => data.join(DEFAULT_SESSION_SUBDIR),
                _ => home.join(".local/share").join(DEFAULT_SESSION_SUBDIR),
            },
        };
        Ok(Self {
            agent,
            session_dir,
            socket: PathBuf::new(),
            working_dir: home,
        })
    }

    /// The command line that starts the agent in RPC mode on this session.
    ///
    /// `--continue` rather than a named session: the newest session in the
    /// directory is the conversation, and after a debug lease that is the file
    /// the terminal was just writing to.
    pub fn agent_args(&self) -> Vec<OsString> {
        vec![
            OsString::from("--session-dir"),
            self.session_dir.clone().into_os_string(),
            OsString::from("--continue"),
            OsString::from("--mode"),
            OsString::from("rpc"),
        ]
    }

    /// The command line that resumes one session in a terminal.
    ///
    /// Given the session the agent was actually using, that exact file rather
    /// than `--continue`, so there is no question about which session the
    /// terminal took over. Before the agent has said which file that is, the
    /// directory and `--continue` are the best answer available and mean the
    /// same thing.
    pub fn debug_args(&self, session: Option<&Path>) -> Vec<String> {
        let mut args = vec![
            "--session-dir".to_string(),
            self.session_dir.display().to_string(),
        ];
        match session {
            Some(file) => {
                args.push("--session".to_string());
                args.push(file.display().to_string());
            }
            None => args.push("--continue".to_string()),
        }
        args
    }
}

/// Returns the value unless it is absent or empty.
fn present(value: Option<OsString>) -> Option<OsString> {
    value.filter(|value| !value.is_empty())
}

/// Returns the first executable of that name on `PATH`.
fn on_path(name: &str, path: Option<OsString>) -> Option<PathBuf> {
    env::split_paths(&present(path)?)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

/// The service could not work out what to run.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    /// Something the service cannot invent was not set.
    #[error("{0}")]
    Missing(String),
    /// A variable named a path that was not absolute.
    #[error("{0} must be an absolute path, not {1}")]
    Relative(&'static str, PathBuf),
    /// No agent was named and none was found.
    #[error("no {AGENT_VARIABLE} and no {DEFAULT_AGENT} on PATH")]
    NoAgent,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(value: &str) -> Option<OsString> {
        Some(OsString::from(value))
    }

    fn resolved(agent: Option<OsString>, session: Option<OsString>) -> Config {
        Config::resolve(agent, session, None, os("/home/a"), None).expect("it resolves")
    }

    #[test]
    fn a_named_agent_and_session_directory_are_taken_as_they_are() {
        let config = resolved(os("/nix/store/x/bin/scufris"), os("/srv/sessions"));
        assert_eq!(config.agent, PathBuf::from("/nix/store/x/bin/scufris"));
        assert_eq!(config.session_dir, PathBuf::from("/srv/sessions"));
        assert_eq!(config.working_dir, PathBuf::from("/home/a"));
    }

    #[test]
    fn the_default_session_directory_follows_the_data_home() {
        let config = Config::resolve(
            os("/bin/scufris"),
            None,
            os("/home/a/.data"),
            os("/home/a"),
            None,
        )
        .expect("it resolves");
        assert_eq!(
            config.session_dir,
            PathBuf::from("/home/a/.data/scufris/sessions")
        );
        // An unset or relative data home is no data home. The specification
        // says absolute, and a relative one would put the conversation
        // wherever the service happened to be started from.
        for data in [None, os("relative")] {
            let config = Config::resolve(os("/bin/scufris"), None, data, os("/home/a"), None)
                .expect("it resolves");
            assert_eq!(
                config.session_dir,
                PathBuf::from("/home/a/.local/share/scufris/sessions")
            );
        }
    }

    #[test]
    fn a_relative_path_is_refused_rather_than_resolved_against_the_working_directory() {
        assert_eq!(
            Config::resolve(os("scufris"), None, None, os("/home/a"), None),
            Err(ConfigError::Relative(
                AGENT_VARIABLE,
                PathBuf::from("scufris")
            ))
        );
        assert_eq!(
            Config::resolve(
                os("/bin/scufris"),
                os("sessions"),
                None,
                os("/home/a"),
                None
            ),
            Err(ConfigError::Relative(
                SESSION_DIR_VARIABLE,
                PathBuf::from("sessions")
            ))
        );
    }

    #[test]
    fn a_session_with_no_home_is_refused() {
        assert!(matches!(
            Config::resolve(os("/bin/scufris"), None, None, None, None),
            Err(ConfigError::Missing(_))
        ));
        assert!(matches!(
            Config::resolve(os("/bin/scufris"), None, None, os(""), None),
            Err(ConfigError::Missing(_))
        ));
    }

    #[test]
    fn with_nothing_named_the_agent_is_looked_for_on_the_path() {
        assert_eq!(
            Config::resolve(None, None, None, os("/home/a"), None),
            Err(ConfigError::NoAgent)
        );
        let directory = std::env::temp_dir().join(format!(
            "scufris-config-{}-{}",
            std::process::id(),
            "path-lookup"
        ));
        std::fs::create_dir_all(&directory).expect("the directory is made");
        let program = directory.join(DEFAULT_AGENT);
        std::fs::write(&program, "#!/bin/sh\n").expect("the program is written");
        let path = env::join_paths([PathBuf::from("/nonexistent"), directory.clone()])
            .expect("the path joins");
        let config = Config::resolve(None, None, None, os("/home/a"), Some(path))
            .expect("the agent is found on the path");
        assert_eq!(config.agent, program);
        std::fs::remove_dir_all(&directory).expect("the directory is removed");
    }

    #[test]
    fn the_agent_runs_in_rpc_mode_and_the_terminal_gets_the_same_session() {
        let config = resolved(os("/bin/scufris"), os("/srv/sessions"));
        assert_eq!(
            config.agent_args(),
            [
                "--session-dir",
                "/srv/sessions",
                "--continue",
                "--mode",
                "rpc"
            ]
            .map(OsString::from)
        );
        assert_eq!(
            config.debug_args(Some(Path::new("/srv/sessions/one.jsonl"))),
            [
                "--session-dir",
                "/srv/sessions",
                "--session",
                "/srv/sessions/one.jsonl"
            ]
        );
        // No session yet means the agent has not said which file it is on.
        // `--continue` is the same conversation by a different name.
        assert_eq!(
            config.debug_args(None),
            ["--session-dir", "/srv/sessions", "--continue"]
        );
    }
}
