//! The companion's own command socket, and the verb the desktop sends it.
//!
//! Two protocols live in this crate and they face opposite ways. The service
//! protocol is the conversation: the companion and `scufris-service` agree on
//! what was said. This one is the desktop: a key binding, a script, or a
//! terminal asking the companion to listen.
//!
//! Two verbs, and both of them are a way in. The companion's windows hold their
//! own keys once they are up - the textbox and the HUD are focused windows, so
//! Enter and Escape are ordinary keys in them and need nothing from outside.
//! What is left is getting them up: a window manager binding that opens the
//! pill and starts the microphone, or one that shows the conversation, without
//! the companion having to grab a key for either.
//!
//! Grabbing is what this socket exists to avoid. The companion holds one
//! accelerator for the whole session already; every further one is a key no
//! other program on the desktop can ever use again, and a binding the person
//! writes in their own window manager configuration costs it nothing.
//!
//! One LF-terminated JSON line each way, bounded the way the service protocol
//! is. The answer is written before the connection closes, so a caller that
//! got an answer knows the companion took the verb.

use std::{env, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ControlPathError, chosen_runtime_dir, in_runtime_dir};

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
    in_runtime_dir(
        chosen_runtime_dir(),
        env::var_os("XDG_RUNTIME_DIR"),
        COMMAND_FILE_NAME,
    )
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

/// What the desktop can ask the companion to do.
///
/// Two windows, and nothing else. These are the companion's keys arriving from
/// a window manager rather than from a grab; this is not a second way to drive
/// the conversation. Everything that carries words goes to the service socket,
/// where `send` and `abort` already live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum Verb {
    /// Bring the pill up and start recording. The activation hotkey.
    Open,
    /// Show the conversation, or put it away if it is already showing.
    ///
    /// One verb rather than a show and a hide, because what sends it is one key
    /// binding: a person who pressed it to read the last answer presses it again
    /// to go back to what they were doing.
    Hud,
    /// Bring the workspace up: the pill, and the panels standing beside it.
    ///
    /// The microphone is not touched. This is the companion's other door, and
    /// it is a verb rather than a second accelerator because a grab held all
    /// session is a chord taken from every other program on the desktop. One
    /// key is what the companion is worth; whether this one is worth another is
    /// the desktop's to decide, and it decides by binding this.
    Show,
    /// Put the workspace away, leaving whatever is on it mounted.
    ///
    /// A show and a hide rather than one toggle, unlike [`Verb::Hud`]. What
    /// sends these is not always a key: a script that means to leave the screen
    /// clear has to be able to say so without first asking what is on it.
    Hide,
}

impl Verb {
    /// Returns the verb one word names, or nothing if no verb does.
    pub fn named(word: &str) -> Option<Self> {
        match word {
            "open" => Some(Self::Open),
            "hud" => Some(Self::Hud),
            "show" => Some(Self::Show),
            "hide" => Some(Self::Hide),
            _ => None,
        }
    }

    /// Returns the word that names this verb.
    pub fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Hud => "hud",
            Self::Show => "show",
            Self::Hide => "hide",
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
    /// The verb reached the companion.
    ///
    /// Not that it changed anything. An activation in a phase that ignores one
    /// is still an activation that arrived, and the caller is a key binding
    /// that has nothing useful to do with the difference.
    Taken,
    /// The verb did not reach the companion, and why.
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
        for word in ["open", "hud"] {
            let verb = Verb::named(word).expect("the word names a verb");
            assert_eq!(verb.name(), word);
            let line = serde_json::to_string(&Command::new(verb)).expect("it encodes");
            let read: Command = serde_json::from_str(&line).expect("it decodes");
            assert_eq!(read, Command::new(verb));
            assert!(line.contains(&format!("\"verb\":\"{word}\"")), "{line}");
        }
        // Two verbs and two windows. A binding that sends one must never get
        // the other: the pill opens the microphone and the HUD does not.
        assert_ne!(Verb::Open, Verb::Hud);

        assert_eq!(Verb::named("quit"), None);
        // The two the textbox took over. A caller that still types them is
        // told they are not verbs rather than being answered.
        assert_eq!(Verb::named("accept"), None);
        assert_eq!(Verb::named("cancel"), None);
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
    fn the_command_socket_sits_beside_the_service_socket_and_is_not_it() {
        let run = Some(std::ffi::OsString::from("/run/user/1000"));
        let command = in_runtime_dir(None, run.clone(), COMMAND_FILE_NAME)
            .expect("the runtime directory is set");
        let service = in_runtime_dir(None, run, crate::service::SURFACE_FILE_NAME)
            .expect("the runtime directory is set");
        assert_eq!(command.parent(), service.parent());
        assert_ne!(command, service);
        assert!(command.ends_with(COMMAND_FILE_NAME));
    }

    #[test]
    fn a_session_with_no_runtime_directory_has_no_command_socket() {
        assert!(in_runtime_dir(None, None, COMMAND_FILE_NAME).is_err());
        assert!(
            in_runtime_dir(None, Some(std::ffi::OsString::new()), COMMAND_FILE_NAME).is_err(),
            "an empty runtime directory is no runtime directory"
        );
    }
}
