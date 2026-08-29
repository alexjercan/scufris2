//! Companion configuration resolved from the environment.
//!
//! Every outside effect the companion can start is named here. The chat,
//! restart, and speak hooks are absolute executables supplied by the
//! deployment, so the companion never builds a command line or reaches for a
//! shell.

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

/// What `--print-config` says about a key that is derived from the hotkey.
///
/// Not "none", which is a real answer here and means the opposite: a key the
/// deployment took off the companion. See [`crate::keys::NONE`].
pub const DERIVED: &str = "derived";

/// State file used when the deployment configures none.
pub const DEFAULT_STATE_FILE: &str = ".local/state/scufris-desktop/pending.json";

/// Restarts allowed inside [`RESTART_WINDOW_SECONDS`].
pub const MAX_RESTARTS: usize = 3;

/// Length of the bounded restart window.
pub const RESTART_WINDOW_SECONDS: u64 = 600;

/// Resolved companion configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Service socket the companion connects to.
    pub socket: PathBuf,
    /// Command socket the companion listens on, for the desktop's own verbs.
    ///
    /// Absent when the session names no runtime directory to put one in. The
    /// companion still starts: the socket is how a window manager binding
    /// reaches the pill, and the hotkey and the tray reach it without one.
    pub command_socket: Option<PathBuf>,
    /// whisper-server-compatible transcription endpoint.
    pub stt_endpoint: String,
    /// Accelerator that opens the pill and starts recording.
    pub hotkey: String,
    /// Accelerator that puts the pill away, when the deployment names one.
    ///
    /// Absent is not off: an unnamed key is derived from the hotkey's own
    /// modifiers, which is what ships. `keys::NONE` is how a key is turned off.
    pub cancel_key: Option<String>,
    /// Accelerator that stops Scufris, on the same terms.
    pub stop_key: Option<String>,
    /// Executable that opens the conversation in a terminal, when one is
    /// configured. Usually a wrapper around `scufris-ctl debug`.
    pub chat_command: Option<PathBuf>,
    /// Executable that restarts the owned backend service, when configured.
    pub restart_command: Option<PathBuf>,
    /// Executable that speaks one paragraph read from its standard input, when
    /// one is configured. Without it the companion stays silent, which is a
    /// deployment without a synthesiser rather than a fault.
    pub speak_command: Option<PathBuf>,
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
            env::var_os("SCUFRIS_DESKTOP_COMMAND_SOCKET"),
            env::var_os("SCUFRIS_STT_ENDPOINT"),
            Keys {
                hotkey: env::var_os("SCUFRIS_DESKTOP_HOTKEY"),
                cancel: env::var_os("SCUFRIS_DESKTOP_CANCEL_KEY"),
                stop: env::var_os("SCUFRIS_DESKTOP_STOP_KEY"),
            },
            Hooks {
                chat: env::var_os("SCUFRIS_DESKTOP_CHAT_COMMAND"),
                restart: env::var_os("SCUFRIS_DESKTOP_RESTART_COMMAND"),
                speak: env::var_os("SCUFRIS_DESKTOP_SPEAK_COMMAND"),
            },
            State {
                configured: env::var_os("SCUFRIS_DESKTOP_STATE_FILE"),
                xdg: env::var_os("XDG_STATE_HOME"),
                home: env::var_os("HOME"),
            },
        )
    }

    fn resolve(
        socket: Option<OsString>,
        command_socket: Option<OsString>,
        endpoint: Option<OsString>,
        keys: Keys,
        hooks: Hooks,
        state: State,
    ) -> Result<Self, ConfigError> {
        let socket = match non_empty(socket) {
            Some(value) => PathBuf::from(value),
            None => scufris_control::service::service_socket_path()?,
        };
        let command_socket = match non_empty(command_socket) {
            Some(value) => Some(PathBuf::from(value)),
            // Not an error, unlike the service socket: a companion with no
            // command socket is one the person opens from the tray and the
            // hotkey, and a companion that refused to start over a socket they
            // may never use would be the worse trade.
            None => scufris_control::command::command_socket_path().ok(),
        };
        let stt_endpoint = match non_empty(endpoint) {
            Some(value) => value.to_string_lossy().into_owned(),
            None => DEFAULT_STT_ENDPOINT.to_string(),
        };
        if !stt_endpoint.starts_with("http://") && !stt_endpoint.starts_with("https://") {
            return Err(ConfigError::Endpoint);
        }
        let hotkey = non_empty(keys.hotkey)
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| DEFAULT_HOTKEY.to_string());
        Ok(Self {
            socket,
            command_socket,
            stt_endpoint,
            hotkey,
            cancel_key: word(keys.cancel),
            stop_key: word(keys.stop),
            chat_command: absolute(hooks.chat, "SCUFRIS_DESKTOP_CHAT_COMMAND")?,
            restart_command: absolute(hooks.restart, "SCUFRIS_DESKTOP_RESTART_COMMAND")?,
            speak_command: absolute(hooks.speak, "SCUFRIS_DESKTOP_SPEAK_COMMAND")?,
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
        // Not `optional`: an unnamed key is derived from the hotkey, and
        // "none" is the answer for one the deployment turned off.
        fn derived(value: &Option<String>) -> &str {
            value.as_deref().unwrap_or(DERIVED)
        }
        format!(
            "socket={}\ncommand_socket={}\nstate_file={}\nstt_endpoint={}\nhotkey={}\ncancel_key={}\nstop_key={}\nchat_command={}\nrestart_command={}\nspeak_command={}\n",
            self.socket.display(),
            optional(&self.command_socket),
            self.state_file.display(),
            self.stt_endpoint,
            self.hotkey,
            derived(&self.cancel_key),
            derived(&self.stop_key),
            optional(&self.chat_command),
            optional(&self.restart_command),
            optional(&self.speak_command),
        )
    }
}

/// The accelerators the deployment names, as the environment gave them.
///
/// Together rather than as three arguments, for the reason [`Hooks`] is: they
/// are one kind of thing, and two of them are read against the third.
struct Keys {
    hotkey: Option<OsString>,
    cancel: Option<OsString>,
    stop: Option<OsString>,
}

/// The executables the deployment supplies, as the environment gave them.
///
/// Together rather than as three arguments: they are one kind of thing - an
/// absolute path to something the companion may run - and they are checked the
/// same way.
struct Hooks {
    chat: Option<OsString>,
    restart: Option<OsString>,
    speak: Option<OsString>,
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

/// One configured word, or nothing when the deployment set none.
///
/// An empty variable is nothing rather than an empty accelerator: a unit file
/// that exports a variable it has no value for is how a key would otherwise be
/// turned off by accident.
fn word(value: Option<OsString>) -> Option<String> {
    non_empty(value).map(|value| value.to_string_lossy().into_owned())
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
            Some(OsString::from("/run/user/1000/scufris/desktop.sock")),
            endpoint.map(OsString::from),
            unset(),
            Hooks {
                chat: chat.map(OsString::from),
                restart: None,
                speak: None,
            },
            state(),
        )
    }

    /// A deployment that named no accelerator at all.
    fn unset() -> Keys {
        Keys {
            hotkey: None,
            cancel: None,
            stop: None,
        }
    }

    #[test]
    fn defaults_target_the_bundled_loopback_endpoint() {
        let config = resolve("/run/user/1000/scufris/service.sock", None, None).unwrap();
        assert_eq!(config.stt_endpoint, DEFAULT_STT_ENDPOINT);
        assert_eq!(config.hotkey, DEFAULT_HOTKEY);
        assert_eq!(config.chat_command, None);
        assert_eq!(config.restart_command, None);
        assert_eq!(config.speak_command, None);
    }

    /// The command socket is what a window manager binding reaches the pill
    /// through, and a session that has nowhere to put one is a session with no
    /// binding to make. So an unresolvable command socket is left absent rather
    /// than refused: the service socket is what the companion cannot do without.
    #[test]
    fn a_command_socket_that_cannot_be_placed_is_absent_rather_than_fatal() {
        let config = Config::resolve(
            Some(OsString::from("/run/user/1000/scufris/service.sock")),
            None,
            None,
            unset(),
            Hooks {
                chat: None,
                restart: None,
                speak: None,
            },
            state(),
        )
        .expect("the companion still resolves");
        assert_eq!(
            config.socket,
            PathBuf::from("/run/user/1000/scufris/service.sock")
        );
        assert_eq!(
            config.command_socket,
            scufris_control::command::command_socket_path().ok(),
            "the runtime directory decides this one, and nothing else does"
        );
    }

    #[test]
    fn a_configured_endpoint_overrides_the_bundled_one() {
        let config = resolve(
            "/run/user/1000/scufris/service.sock",
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
            "/run/user/1000/scufris/service.sock",
            None,
            Some("/nix/store/x/bin/scufris-chat"),
        )
        .unwrap();
        assert_eq!(
            config.describe(),
            concat!(
                "socket=/run/user/1000/scufris/service.sock\n",
                "command_socket=/run/user/1000/scufris/desktop.sock\n",
                "state_file=/run/user/1000/scufris-desktop/pending.json\n",
                "stt_endpoint=http://127.0.0.1:10301/inference\n",
                "hotkey=Super+D\n",
                "cancel_key=derived\n",
                "stop_key=derived\n",
                "chat_command=/nix/store/x/bin/scufris-chat\n",
                "restart_command=none\n",
                "speak_command=none\n",
            )
        );
    }

    /// The two keys beside the hotkey are the deployment's to name, and
    /// `--print-config` tells the three answers apart: an accelerator, "none"
    /// for a key the desktop took back, and "derived" for one left to the
    /// hotkey. Only the words are checked here; what they grab is
    /// [`crate::keys`].
    #[test]
    fn the_keys_beside_the_hotkey_are_reported_as_the_deployment_named_them() {
        let config = Config::resolve(
            Some(OsString::from("/run/user/1000/scufris/service.sock")),
            Some(OsString::from("/run/user/1000/scufris/desktop.sock")),
            None,
            Keys {
                hotkey: Some(OsString::from("Control+Alt+G")),
                cancel: Some(OsString::from("Control+Alt+Q")),
                stop: Some(OsString::from(crate::keys::NONE)),
            },
            Hooks {
                chat: None,
                restart: None,
                speak: None,
            },
            state(),
        )
        .unwrap();
        assert_eq!(config.hotkey, "Control+Alt+G");
        assert_eq!(config.cancel_key.as_deref(), Some("Control+Alt+Q"));
        assert_eq!(config.stop_key.as_deref(), Some(crate::keys::NONE));
        assert!(config.describe().contains("cancel_key=Control+Alt+Q\n"));
        assert!(config.describe().contains("stop_key=none\n"));
    }

    /// An environment that sets a key to the empty string named no key. The
    /// unit file writes every variable whether or not the person filled it in,
    /// so this is the ordinary case rather than a malformed one.
    #[test]
    fn a_key_set_to_nothing_is_a_key_that_was_not_named() {
        let config = Config::resolve(
            Some(OsString::from("/run/user/1000/scufris/service.sock")),
            None,
            None,
            Keys {
                hotkey: Some(OsString::new()),
                cancel: Some(OsString::new()),
                stop: Some(OsString::new()),
            },
            Hooks {
                chat: None,
                restart: None,
                speak: None,
            },
            state(),
        )
        .unwrap();
        assert_eq!(config.hotkey, DEFAULT_HOTKEY);
        assert_eq!(config.cancel_key, None);
        assert_eq!(config.stop_key, None);
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
