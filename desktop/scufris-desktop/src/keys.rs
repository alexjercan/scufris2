//! The pill's keys, arranged where the pill window is not.
//!
//! The pill never takes the keyboard from the person's editor, so a key typed
//! at it never arrives. Two ways round that, and the companion arranges both:
//!
//! - A window manager binding mode. i3 and sway hold bare Escape and Return
//!   inside a named mode, which is the only way a bare key reaches the pill
//!   without being taken away from every other program on the desktop. The
//!   window manager enters the mode when the person opens the pill; this leaves
//!   it, so the mode and the pill still agree when the pill closed for a reason
//!   the person never asked for.
//! - Modified accelerators the display grabs, for a desktop with no binding
//!   mode to speak of. `Super+D` opens the pill, so `Super+Escape` and
//!   `Super+Enter` answer it. Grabbed only while the pill is on screen: an
//!   accelerator held all session is one no other program can ever use.
//!
//! Neither is required. A companion with no mode hook and an unmodified hotkey
//! is one the person answers with the mouse and the tray.

use std::{
    path::PathBuf,
    process::Command,
    sync::mpsc::{self, Sender},
    thread,
};

use scufris_control::command::Verb;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use tracing::{debug, warn};

use crate::{app::Keys, state::Posture};

/// The binding mode the window manager holds while the pill wants a bare key.
pub const MODE_HELD: &str = "scufris";

/// The mode it goes back to. i3 and sway both call the ordinary one this.
pub const MODE_FREE: &str = "default";

/// The pill's two keys, and what each one asks for.
const ANSWERS: [(&str, Verb); 2] = [("Escape", Verb::Cancel), ("Enter", Verb::Accept)];

/// The keys that answer the pill, and how they are arranged for each posture.
pub struct PillKeys {
    /// Executable that puts the window manager into one named binding mode.
    mode: Option<PathBuf>,
    /// The accelerators this companion can grab, with the verb each carries.
    answers: Vec<(Shortcut, Verb)>,
    /// Where a wanted grab is asked for, when there is anything to grab.
    grabs: Option<Sender<bool>>,
}

impl PillKeys {
    /// Builds the arrangement this configuration allows.
    pub fn new(handle: AppHandle, mode: Option<PathBuf>, hotkey: &str) -> Self {
        let answers = answers(hotkey);
        let grabs = (!answers.is_empty()).then(|| {
            grabber(
                handle,
                answers.iter().map(|(shortcut, _)| *shortcut).collect(),
            )
        });
        Self {
            mode,
            answers,
            grabs,
        }
    }

    /// Says which of the pill's keys an accelerator is, if it is one.
    ///
    /// The display hands every accelerator to one handler, so this is what
    /// tells the pill's own keys from the hotkey that opens it.
    pub fn verb(&self, shortcut: &Shortcut) -> Option<Verb> {
        self.answers
            .iter()
            .find(|(candidate, _)| candidate == shortcut)
            .map(|(_, verb)| *verb)
    }

    /// Puts the window manager into one binding mode.
    ///
    /// Waited for rather than left to a thread. The last mode asked for is the
    /// one the window manager must be left in, and the last one of all is asked
    /// for as the companion exits: a thread started there races the exit and
    /// loses, leaving the person's Escape key inside a mode nothing owns.
    fn mode(&self, name: &str) {
        let Some(command) = &self.mode else {
            return;
        };
        match Command::new(command).arg(name).status() {
            Ok(status) if status.success() => {
                debug!(mode = name, "the window manager changed mode")
            }
            Ok(status) => warn!(mode = name, "{} answered {status}", command.display()),
            Err(error) => warn!(mode = name, "{} would not run: {error}", command.display()),
        }
    }

    /// Asks for the accelerators, or gives them back.
    fn grab(&self, wanted: bool) {
        let Some(grabs) = &self.grabs else {
            return;
        };
        // The thread outlives every caller. A send that fails is a process on
        // its way out, and the display drops every grab with the connection
        // holding it, so there is nothing left to give back.
        if let Err(error) = grabs.send(wanted) {
            debug!("the accelerators were left as they were: {error}");
        }
    }
}

/// Starts the thread that takes the accelerators and gives them back.
///
/// On a thread of its own, and never on the one that asked. The display hands
/// every accelerator to one handler on the same thread it takes grabs on, so a
/// grab asked for from inside that handler waits on a thread that is waiting
/// for the handler to return. The hotkey that opens the pill arrives on exactly
/// that thread, which makes it the ordinary road in rather than a corner.
///
/// One thread rather than one per change, because the last posture asked for is
/// the one the keys must be left arranged for, and two threads racing would
/// leave them arranged for either.
fn grabber(handle: AppHandle, answers: Vec<Shortcut>) -> Sender<bool> {
    let (asked, wanted) = mpsc::channel::<bool>();
    thread::spawn(move || {
        // What is actually grabbed. A window manager that already holds one of
        // these refuses it, so what was asked for and what is held differ.
        let mut held: Vec<Shortcut> = Vec::new();
        for want in wanted {
            if !want {
                for shortcut in held.drain(..) {
                    if let Err(error) = handle.global_shortcut().unregister(shortcut) {
                        warn!("{shortcut} stayed grabbed: {error}");
                    }
                }
                continue;
            }
            if !held.is_empty() {
                continue;
            }
            for shortcut in &answers {
                match handle.global_shortcut().register(*shortcut) {
                    Ok(()) => held.push(*shortcut),
                    // A window manager that already holds this key refuses the
                    // grab, and that is the good case: its own binding runs
                    // `scufris-ctl` and arrives in the same place.
                    Err(error) => debug!("{shortcut} is somebody else's: {error}"),
                }
            }
        }
    });
    asked
}

impl Keys for PillKeys {
    fn stand(&self, posture: Posture) {
        // The bare keys belong to a pill the person is answering and to nothing
        // else: a mode held over a passive pill takes Escape out of whatever
        // they went back to typing in.
        self.mode(if posture == Posture::Focused {
            MODE_HELD
        } else {
            MODE_FREE
        });
        // The modified ones are safe for as long as the pill is on screen,
        // which is what puts a resting pill away on a desktop with no mode.
        self.grab(posture != Posture::Off);
    }
}

/// The accelerators that answer the pill, built from the activation hotkey.
///
/// The hotkey's own modifiers and nothing else: `Super+D` opens the pill, so
/// `Super+Escape` and `Super+Enter` answer it, and the person has one modifier
/// to remember rather than two. A hotkey with no modifier leaves none, because
/// a bare Escape the display gave the companion is an Escape no other program
/// on the desktop would ever see again.
fn answers(hotkey: &str) -> Vec<(Shortcut, Verb)> {
    let Some((modifiers, _)) = hotkey.rsplit_once('+') else {
        return Vec::new();
    };
    ANSWERS
        .iter()
        .filter_map(|(key, verb)| {
            let accelerator = format!("{modifiers}+{key}");
            match accelerator.parse::<Shortcut>() {
                Ok(shortcut) => Some((shortcut, *verb)),
                Err(error) => {
                    warn!("{accelerator} is not an accelerator: {error}");
                    None
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accelerator(text: &str) -> Shortcut {
        text.parse().expect("it is an accelerator")
    }

    /// One modifier to remember. Whatever opens the pill is what answers it,
    /// down to a hotkey the person built out of two modifiers.
    #[test]
    fn the_hotkeys_own_modifiers_are_the_ones_that_answer_it() {
        assert_eq!(
            answers("Super+D"),
            vec![
                (accelerator("Super+Escape"), Verb::Cancel),
                (accelerator("Super+Enter"), Verb::Accept),
            ]
        );
        assert_eq!(
            answers("Control+Alt+G"),
            vec![
                (accelerator("Control+Alt+Escape"), Verb::Cancel),
                (accelerator("Control+Alt+Enter"), Verb::Accept),
            ]
        );
    }

    /// A bare accelerator is global. Granting the companion one would take that
    /// key off the desktop for every other program, for the whole session, and
    /// no fallback is worth that: the binding mode exists for exactly the case
    /// where a bare key is wanted, and it holds one only while the pill is up.
    #[test]
    fn a_hotkey_with_no_modifier_leaves_the_desktops_bare_keys_alone() {
        assert!(answers("F9").is_empty());
    }
}
