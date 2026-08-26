//! The companion's own command socket, and the verbs the desktop sends it.
//!
//! Two protocols live in this crate and they face opposite ways. The daemon
//! protocol is the conversation: the companion and the Scufris daemon agree on
//! what was said. This one is the desktop: a key binding, a script, or a
//! terminal telling the pill what to do.
//!
//! Which is the whole point of it. The pill's keys used to need focus, so
//! using them meant taking the keyboard away from whatever the person was
//! typing in. A verb on a socket needs nothing: the window manager reads the
//! key and says what happened, and the pill is never the focused window.
//!
//! One LF-terminated JSON line each way, bounded the way the daemon protocol
//! is. The answer is written before the connection closes, so a caller that
//! got an answer knows the companion took the verb.

use std::{env, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ControlPathError, in_runtime_dir};

/// Wire version of the command protocol.
///
/// Its own number rather than the daemon protocol's. The two change for
/// different reasons, and `scufris-ctl` on a person's PATH can be older than
/// the companion it is talking to.
pub const COMMAND_VERSION: u32 = 1;

/// Socket name below [`crate::SOCKET_DIRECTORY_NAME`].
pub const COMMAND_FILE_NAME: &str = "desktop.sock";

/// Returns the companion's command socket path for the current user session.
pub fn command_socket_path() -> Result<PathBuf, ControlPathError> {
    in_runtime_dir(env::var_os("XDG_RUNTIME_DIR"), COMMAND_FILE_NAME)
}

/// One versioned verb sent to the companion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    /// Wire version used to encode the verb.
    pub v: u32,
    /// The verb itself.
    #[serde(flatten)]
    pub verb: Verb,
}

impl Command {
    /// Creates a command carrying the current version.
    pub fn new(verb: Verb) -> Self {
        Self {
            v: COMMAND_VERSION,
            verb,
        }
    }
}

/// What the desktop can ask the pill to do.
///
/// The same three things the pill's own keys do, and nothing else. This socket
/// is a way to press those keys without holding the keyboard; it is not a
/// second way to drive the companion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum Verb {
    /// Bring the pill up and start recording. The activation hotkey.
    Open,
    /// Escape. Cancels what is running, or puts a resting pill away.
    Cancel,
    /// Enter. Accepts what the pill is showing, with whatever is in its field.
    Accept,
}

impl Verb {
    /// Returns the verb one word names, or nothing if no verb does.
    pub fn named(word: &str) -> Option<Self> {
        match word {
            "open" => Some(Self::Open),
            "cancel" => Some(Self::Cancel),
            "accept" => Some(Self::Accept),
            _ => None,
        }
    }

    /// Returns the word that names this verb.
    pub fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Cancel => "cancel",
            Self::Accept => "accept",
        }
    }
}

/// One versioned answer from the companion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    /// Wire version used to encode the answer.
    pub v: u32,
    /// What happened.
    #[serde(flatten)]
    pub outcome: Outcome,
}

impl Answer {
    /// Creates an answer carrying the current version.
    pub fn new(outcome: Outcome) -> Self {
        Self {
            v: COMMAND_VERSION,
            outcome,
        }
    }
}

/// What became of one verb.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum Outcome {
    /// The verb reached the pill.
    ///
    /// Not that it changed anything. Escape in a phase with nothing to cancel
    /// is still an Escape that arrived, and the caller is a key binding that
    /// has nothing useful to do with the difference.
    Taken,
    /// The verb did not reach the pill, and why.
    Refused {
        /// What went wrong, for the person reading their terminal.
        detail: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_survives_the_round_trip_by_the_name_it_is_typed_with() {
        for word in ["open", "cancel", "accept"] {
            let verb = Verb::named(word).expect("the word names a verb");
            assert_eq!(verb.name(), word);
            let line = serde_json::to_string(&Command::new(verb)).expect("it encodes");
            let read: Command = serde_json::from_str(&line).expect("it decodes");
            assert_eq!(read, Command::new(verb));
            assert!(line.contains(&format!("\"verb\":\"{word}\"")), "{line}");
        }
        assert_eq!(Verb::named("quit"), None);
    }

    #[test]
    fn an_answer_says_which_of_the_two_things_happened() {
        let taken = serde_json::to_string(&Answer::new(Outcome::Taken)).expect("it encodes");
        assert_eq!(taken, r#"{"v":1,"answer":"taken"}"#);
        let refused = Answer::new(Outcome::Refused {
            detail: "the pill is not up".into(),
        });
        let line = serde_json::to_string(&refused).expect("it encodes");
        assert_eq!(
            serde_json::from_str::<Answer>(&line).expect("it decodes"),
            refused
        );
    }

    #[test]
    fn the_command_socket_sits_beside_the_daemon_socket_and_is_not_it() {
        let run = Some(std::ffi::OsString::from("/run/user/1000"));
        let command =
            in_runtime_dir(run.clone(), COMMAND_FILE_NAME).expect("the runtime directory is set");
        let daemon =
            in_runtime_dir(run, crate::SOCKET_FILE_NAME).expect("the runtime directory is set");
        assert_eq!(command.parent(), daemon.parent());
        assert_ne!(command, daemon);
        assert!(command.ends_with(COMMAND_FILE_NAME));
    }

    #[test]
    fn a_session_with_no_runtime_directory_has_no_command_socket() {
        assert!(in_runtime_dir(None, COMMAND_FILE_NAME).is_err());
        assert!(
            in_runtime_dir(Some(std::ffi::OsString::new()), COMMAND_FILE_NAME).is_err(),
            "an empty runtime directory is no runtime directory"
        );
    }
}
