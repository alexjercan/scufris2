//! Companion configuration resolved from the environment.
//!
//! Every outside effect the companion can start is named here. The chat and
//! restart hooks are absolute executables supplied by the deployment, so the
//! companion never builds a command line or reaches for a shell.

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

use thiserror::Error;

/// Endpoint used when the deployment configures none.
pub const DEFAULT_STT_ENDPOINT: &str = "http://127.0.0.1:10301/inference";

/// Activation accelerator used when the deployment configures none.
pub const DEFAULT_HOTKEY: &str = "Super+D";

/// State file used when the deployment configures none.
pub const DEFAULT_STATE_FILE: &str = ".local/state/scufris-desktop/pending.json";

/// Restarts allowed inside [`RESTART_WINDOW_SECONDS`].
pub const MAX_RESTARTS: usize = 3;

/// Length of the bounded restart window.
pub const RESTART_WINDOW_SECONDS: u64 = 600;

/// Resolved companion configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Daemon control socket the companion connects to.
    pub socket: PathBuf,
    /// whisper-server-compatible transcription endpoint.
    pub stt_endpoint: String,
    /// Accelerator that opens the pill and starts recording.
    pub hotkey: String,
    /// Executable that opens the full popup chat, when one is configured.
    pub chat_command: Option<PathBuf>,
    /// Executable that restarts the owned backend service, when configured.
    pub restart_command: Option<PathBuf>,
    /// File holding an accepted transcript that has not been acknowledged.
    pub state_file: PathBuf,
}

/// Failure to resolve the companion configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// No socket path could be resolved.
    #[error("{0}")]
    Socket(#[from] scufris_control::ControlPathError),
    /// A configured endpoint was not an absolute HTTP or HTTPS URL.
    #[error("SCUFRIS_STT_ENDPOINT must be an http or https URL")]
    Endpoint,
    /// A configured hook was not an absolute path.
    #[error("{0} must be an absolute executable path")]
    Command(&'static str),
    /// No durable state file could be resolved.
    #[error("SCUFRIS_DESKTOP_STATE_FILE, XDG_STATE_HOME, or HOME must be set")]
    StateFile,
}

impl Config {
    /// Resolves the configuration from the current process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::resolve(
            env::var_os("SCUFRIS_DESKTOP_SOCKET"),
            env::var_os("SCUFRIS_STT_ENDPOINT"),
            env::var_os("SCUFRIS_DESKTOP_HOTKEY"),
            env::var_os("SCUFRIS_DESKTOP_CHAT_COMMAND"),
            env::var_os("SCUFRIS_DESKTOP_RESTART_COMMAND"),
            State {
                configured: env::var_os("SCUFRIS_DESKTOP_STATE_FILE"),
                xdg: env::var_os("XDG_STATE_HOME"),
                home: env::var_os("HOME"),
            },
        )
    }

    fn resolve(
        socket: Option<OsString>,
        endpoint: Option<OsString>,
        hotkey: Option<OsString>,
        chat: Option<OsString>,
        restart: Option<OsString>,
        state: State,
    ) -> Result<Self, ConfigError> {
        let socket = match non_empty(socket) {
            Some(value) => PathBuf::from(value),
            None => scufris_control::socket_path()?,
        };
        let stt_endpoint = match non_empty(endpoint) {
            Some(value) => value.to_string_lossy().into_owned(),
            None => DEFAULT_STT_ENDPOINT.to_string(),
        };
        if !stt_endpoint.starts_with("http://") && !stt_endpoint.starts_with("https://") {
            return Err(ConfigError::Endpoint);
        }
        let hotkey = non_empty(hotkey)
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| DEFAULT_HOTKEY.to_string());
        Ok(Self {
            socket,
            stt_endpoint,
            hotkey,
            chat_command: absolute(chat, "SCUFRIS_DESKTOP_CHAT_COMMAND")?,
            restart_command: absolute(restart, "SCUFRIS_DESKTOP_RESTART_COMMAND")?,
            state_file: state.resolve()?,
        })
    }

    /// Renders the resolved configuration for `--print-config`.
    pub fn describe(&self) -> String {
        let optional = |value: &Option<PathBuf>| {
            value
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string())
        };
        format!(
            "socket={}\nstate_file={}\nstt_endpoint={}\nhotkey={}\nchat_command={}\nrestart_command={}\n",
            self.socket.display(),
            self.state_file.display(),
            self.stt_endpoint,
            self.hotkey,
            optional(&self.chat_command),
            optional(&self.restart_command),
        )
    }
}

/// The three inputs that can name the durable state file, in priority order.
struct State {
    configured: Option<OsString>,
    xdg: Option<OsString>,
    home: Option<OsString>,
}

impl State {
    fn resolve(self) -> Result<PathBuf, ConfigError> {
        if let Some(path) = non_empty(self.configured) {
            return Ok(PathBuf::from(path));
        }
        if let Some(path) = non_empty(self.xdg) {
            return Ok(PathBuf::from(path).join("scufris-desktop/pending.json"));
        }
        non_empty(self.home)
            .map(|path| PathBuf::from(path).join(DEFAULT_STATE_FILE))
            .ok_or(ConfigError::StateFile)
    }
}

fn non_empty(value: Option<OsString>) -> Option<OsString> {
    value.filter(|value| !value.is_empty())
}

fn absolute(value: Option<OsString>, name: &'static str) -> Result<Option<PathBuf>, ConfigError> {
    let Some(value) = non_empty(value) else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !Path::new(&path).is_absolute() {
        return Err(ConfigError::Command(name));
    }
    Ok(Some(path))
}

/// Bounded restart budget for the owned backend service.
#[derive(Debug, Default)]
pub struct RestartBudget {
    attempts: Vec<u64>,
}

impl RestartBudget {
    /// Records one restart attempt at `now` and reports whether it is allowed.
    pub fn allow(&mut self, now: u64) -> bool {
        self.attempts
            .retain(|attempt| now.saturating_sub(*attempt) < RESTART_WINDOW_SECONDS);
        if self.attempts.len() >= MAX_RESTARTS {
            return false;
        }
        self.attempts.push(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> State {
        State {
            configured: Some(OsString::from(
                "/run/user/1000/scufris-desktop/pending.json",
            )),
            xdg: None,
            home: None,
        }
    }

    fn resolve(
        socket: &str,
        endpoint: Option<&str>,
        chat: Option<&str>,
    ) -> Result<Config, ConfigError> {
        Config::resolve(
            Some(OsString::from(socket)),
            endpoint.map(OsString::from),
            None,
            chat.map(OsString::from),
            None,
            state(),
        )
    }

    #[test]
    fn defaults_target_the_bundled_loopback_endpoint() {
        let config = resolve("/run/user/1000/scufris/daemon.sock", None, None).unwrap();
        assert_eq!(config.stt_endpoint, DEFAULT_STT_ENDPOINT);
        assert_eq!(config.hotkey, DEFAULT_HOTKEY);
        assert_eq!(config.chat_command, None);
        assert_eq!(config.restart_command, None);
    }

    #[test]
    fn a_configured_endpoint_overrides_the_bundled_one() {
        let config = resolve(
            "/run/user/1000/scufris/daemon.sock",
            Some("http://127.0.0.1:9000/inference"),
            None,
        )
        .unwrap();
        assert_eq!(config.stt_endpoint, "http://127.0.0.1:9000/inference");
    }

    #[test]
    fn non_http_endpoints_and_relative_hooks_are_rejected() {
        assert!(matches!(
            resolve("/socket", Some("file:///etc/passwd"), None),
            Err(ConfigError::Endpoint)
        ));
        assert!(matches!(
            resolve("/socket", None, Some("scufris-chat")),
            Err(ConfigError::Command("SCUFRIS_DESKTOP_CHAT_COMMAND"))
        ));
    }

    #[test]
    fn the_description_names_every_outside_effect() {
        let config = resolve(
            "/run/user/1000/scufris/daemon.sock",
            None,
            Some("/nix/store/x/bin/scufris-chat"),
        )
        .unwrap();
        assert_eq!(
            config.describe(),
            concat!(
                "socket=/run/user/1000/scufris/daemon.sock\n",
                "state_file=/run/user/1000/scufris-desktop/pending.json\n",
                "stt_endpoint=http://127.0.0.1:10301/inference\n",
                "hotkey=Super+D\n",
                "chat_command=/nix/store/x/bin/scufris-chat\n",
                "restart_command=none\n",
            )
        );
    }

    #[test]
    fn the_durable_state_file_falls_back_from_the_override_to_xdg_to_home() {
        let resolved = |configured: Option<&str>, xdg: Option<&str>, home: Option<&str>| {
            State {
                configured: configured.map(OsString::from),
                xdg: xdg.map(OsString::from),
                home: home.map(OsString::from),
            }
            .resolve()
        };
        assert_eq!(
            resolved(Some("/tmp/pending.json"), Some("/state"), Some("/home/a")).unwrap(),
            PathBuf::from("/tmp/pending.json")
        );
        assert_eq!(
            resolved(None, Some("/state"), Some("/home/a")).unwrap(),
            PathBuf::from("/state/scufris-desktop/pending.json")
        );
        assert_eq!(
            resolved(None, None, Some("/home/a")).unwrap(),
            PathBuf::from("/home/a").join(DEFAULT_STATE_FILE)
        );
        assert!(matches!(
            resolved(None, None, None),
            Err(ConfigError::StateFile)
        ));
    }

    #[test]
    fn restarts_are_bounded_inside_the_window_and_recover_after_it() {
        let mut budget = RestartBudget::default();
        assert!(budget.allow(0));
        assert!(budget.allow(1));
        assert!(budget.allow(2));
        assert!(!budget.allow(3));
        assert!(budget.allow(RESTART_WINDOW_SECONDS + 1));
    }
}
