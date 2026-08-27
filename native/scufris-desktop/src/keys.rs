//! The one key that is arranged where no window is.
//!
//! The pill never takes the keyboard, and the textbox is not up while the
//! microphone is open, so between `Super+D` and the words arriving there is no
//! window to type at. One key covers that gap: the cancel key, which stops a
//! listen the person did not mean to start.
//!
//! It is an accelerator the display grabs, built from the hotkey's own
//! modifiers: `Super+D` opens the pill, so `Super+Escape` puts it away. Grabbed
//! only while the pill is on screen, because an accelerator held all session is
//! one no other program can ever use.
//!
//! It is not required. A companion whose hotkey has no modifier is one the
//! person puts away with the tray, and every other key belongs to the textbox.

use std::{
    sync::mpsc::{self, Sender},
    thread,
};

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use tracing::{debug, warn};

use crate::app::Keys;

/// The key that stops a listen.
const CANCEL: &str = "Escape";

/// The key that answers the pill, and how it is arranged while it is up.
pub struct PillKeys {
    /// The accelerator this companion can grab, when the hotkey leaves one.
    cancel: Option<Shortcut>,
    /// Where a wanted grab is asked for, when there is anything to grab.
    grabs: Option<Sender<bool>>,
}

impl PillKeys {
    /// Builds the arrangement this configuration allows.
    pub fn new(handle: AppHandle, hotkey: &str) -> Self {
        let cancel = cancel(hotkey);
        let grabs = cancel.map(|shortcut| grabber(handle, shortcut));
        Self { cancel, grabs }
    }

    /// Says whether an accelerator is the cancel key.
    ///
    /// The display hands every accelerator to one handler, so this is what
    /// tells the cancel key from the hotkey that opens the pill.
    pub fn cancels(&self, shortcut: &Shortcut) -> bool {
        self.cancel.as_ref() == Some(shortcut)
    }

    /// Asks for the accelerator, or gives it back.
    fn grab(&self, wanted: bool) {
        let Some(grabs) = &self.grabs else {
            return;
        };
        // The thread outlives every caller. A send that fails is a process on
        // its way out, and the display drops every grab with the connection
        // holding it, so there is nothing left to give back.
        if let Err(error) = grabs.send(wanted) {
            debug!("the accelerator was left as it was: {error}");
        }
    }
}

/// Starts the thread that takes the accelerator and gives it back.
///
/// On a thread of its own, and never on the one that asked. The display hands
/// every accelerator to one handler on the same thread it takes grabs on, so a
/// grab asked for from inside that handler waits on a thread that is waiting
/// for the handler to return. The hotkey that opens the pill arrives on exactly
/// that thread, which makes it the ordinary road in rather than a corner.
///
/// One thread rather than one per change, because the last posture asked for is
/// the one the key must be left arranged for, and two threads racing would
/// leave it arranged for either.
fn grabber(handle: AppHandle, cancel: Shortcut) -> Sender<bool> {
    let (asked, wanted) = mpsc::channel::<bool>();
    thread::spawn(move || {
        // Whether it is actually grabbed. A window manager that already holds
        // this key refuses it, so what was asked for and what is held differ.
        let mut held = false;
        for want in wanted {
            if want == held {
                continue;
            }
            if want {
                match handle.global_shortcut().register(cancel) {
                    Ok(()) => held = true,
                    // A window manager that already holds this key refuses the
                    // grab, and that is the good case: its own binding runs
                    // `scufris-ctl` and arrives in the same place.
                    Err(error) => debug!("{cancel} is somebody else's: {error}"),
                }
                continue;
            }
            match handle.global_shortcut().unregister(cancel) {
                Ok(()) => held = false,
                Err(error) => warn!("{cancel} stayed grabbed: {error}"),
            }
        }
    });
    asked
}

impl Keys for PillKeys {
    fn stand(&self, on_screen: bool) {
        // Safe for as long as the pill is on screen, which is what gives the
        // key back the moment the pill is put away.
        self.grab(on_screen);
    }
}

/// The accelerator that stops a listen, built from the activation hotkey.
///
/// The hotkey's own modifiers and nothing else: `Super+D` opens the pill, so
/// `Super+Escape` puts it away, and the person has one modifier to remember
/// rather than two. A hotkey with no modifier leaves none, because a bare
/// Escape the display gave the companion is an Escape no other program on the
/// desktop would ever see again.
fn cancel(hotkey: &str) -> Option<Shortcut> {
    let (modifiers, _) = hotkey.rsplit_once('+')?;
    let accelerator = format!("{modifiers}+{CANCEL}");
    match accelerator.parse::<Shortcut>() {
        Ok(shortcut) => Some(shortcut),
        Err(error) => {
            warn!("{accelerator} is not an accelerator: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accelerator(text: &str) -> Shortcut {
        text.parse().expect("it is an accelerator")
    }

    /// One modifier to remember. Whatever opens the pill is what puts it away,
    /// down to a hotkey the person built out of two modifiers.
    #[test]
    fn the_hotkeys_own_modifiers_are_the_ones_that_answer_it() {
        assert_eq!(cancel("Super+D"), Some(accelerator("Super+Escape")));
        assert_eq!(
            cancel("Control+Alt+G"),
            Some(accelerator("Control+Alt+Escape"))
        );
    }

    /// A bare accelerator is global. Granting the companion one would take
    /// Escape off the desktop for every other program, for the whole session,
    /// and nothing here is worth that: the textbox holds its own Escape, and a
    /// listen can always be put away with the tray.
    #[test]
    fn a_hotkey_with_no_modifier_leaves_the_desktops_bare_keys_alone() {
        assert_eq!(cancel("F9"), None);
    }
}
