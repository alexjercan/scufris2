//! The keys that are arranged where no window is.
//!
//! The pill never takes the keyboard, and the textbox is not up while the
//! microphone is open, so between `Super+D` and the words arriving there is no
//! window to type at. Two keys cover that gap: the cancel key, which stops a
//! listen the person did not mean to start, and the stop key, which stops
//! Scufris.
//!
//! They are accelerators the display grabs, built from the hotkey's own
//! modifiers: `Super+D` opens the pill, so `Super+Escape` puts it away and
//! `Super+Period` stops what Scufris is doing. Grabbed only while the pill is
//! on screen, because an accelerator held all session is one no other program
//! can ever use.
//!
//! Stop is its own key rather than a second meaning for Escape. Escape puts a
//! pill away and throws away a take, and neither reaches the conversation; stop
//! ends a run that may be part way through changing something. A gesture with
//! that much behind it is not one to arrive at by pressing the dismiss key at
//! the wrong moment.
//!
//! Neither is required. A companion whose hotkey has no modifier is one the
//! person puts away with the tray and stops with `scufris-ctl abort`, and every
//! other key belongs to the textbox.

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

/// The key that stops Scufris.
///
/// The interrupt key of every terminal and half the editors ever written. It
/// belongs to nothing on the desktop, which is what makes it grabbable.
const STOP: &str = "Period";

/// The keys that answer the pill, and how they are arranged while it is up.
pub struct PillKeys {
    /// The accelerator that puts the pill away, when the hotkey leaves one.
    cancel: Option<Shortcut>,
    /// The accelerator that stops Scufris, on the same terms.
    stop: Option<Shortcut>,
    /// Where a wanted grab is asked for, when there is anything to grab.
    grabs: Option<Sender<bool>>,
}

impl PillKeys {
    /// Builds the arrangement this configuration allows.
    pub fn new(handle: AppHandle, hotkey: &str) -> Self {
        let cancel = beside(hotkey, CANCEL);
        let stop = beside(hotkey, STOP);
        let wanted: Vec<Shortcut> = [cancel, stop].into_iter().flatten().collect();
        let grabs = (!wanted.is_empty()).then(|| grabber(handle, wanted));
        Self {
            cancel,
            stop,
            grabs,
        }
    }

    /// Says whether an accelerator is the cancel key.
    ///
    /// The display hands every accelerator to one handler, so this is what
    /// tells the cancel key from the hotkey that opens the pill.
    pub fn cancels(&self, shortcut: &Shortcut) -> bool {
        self.cancel.as_ref() == Some(shortcut)
    }

    /// Says whether an accelerator is the stop key.
    pub fn stops(&self, shortcut: &Shortcut) -> bool {
        self.stop.as_ref() == Some(shortcut)
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
fn grabber(handle: AppHandle, keys: Vec<Shortcut>) -> Sender<bool> {
    let (asked, wanted) = mpsc::channel::<bool>();
    thread::spawn(move || {
        // Which are actually grabbed. A window manager that already holds one
        // of these refuses it, so what was asked for and what is held differ,
        // and they differ one key at a time.
        let mut held: Vec<Shortcut> = Vec::new();
        for want in wanted {
            if want {
                for key in &keys {
                    if held.contains(key) {
                        continue;
                    }
                    match handle.global_shortcut().register(*key) {
                        Ok(()) => held.push(*key),
                        // A window manager that already holds this key refuses
                        // the grab, and that is the good case: its own binding
                        // runs `scufris-ctl` and arrives in the same place.
                        Err(error) => debug!("{key} is somebody else's: {error}"),
                    }
                }
                continue;
            }
            held.retain(|key| match handle.global_shortcut().unregister(*key) {
                Ok(()) => false,
                Err(error) => {
                    warn!("{key} stayed grabbed: {error}");
                    true
                }
            });
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

/// One accelerator beside the activation hotkey, on the hotkey's own modifiers.
///
/// Its modifiers and nothing else: `Super+D` opens the pill, so `Super+Escape`
/// puts it away and `Super+Period` stops it, and the person has one modifier to
/// remember rather than three. A hotkey with no modifier leaves none, because a
/// bare key the display gave the companion is one no other program on the
/// desktop would ever see again.
fn beside(hotkey: &str, key: &str) -> Option<Shortcut> {
    let (modifiers, _) = hotkey.rsplit_once('+')?;
    let accelerator = format!("{modifiers}+{key}");
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

    /// One modifier to remember. Whatever opens the pill is what puts it away
    /// and what stops it, down to a hotkey the person built out of two
    /// modifiers.
    #[test]
    fn the_hotkeys_own_modifiers_are_the_ones_that_answer_it() {
        assert_eq!(beside("Super+D", CANCEL), Some(accelerator("Super+Escape")));
        assert_eq!(beside("Super+D", STOP), Some(accelerator("Super+Period")));
        assert_eq!(
            beside("Control+Alt+G", CANCEL),
            Some(accelerator("Control+Alt+Escape"))
        );
        assert_eq!(
            beside("Control+Alt+G", STOP),
            Some(accelerator("Control+Alt+Period"))
        );
    }

    /// The two keys are two keys. Stop reaches the conversation and cancel
    /// never does, so nothing may deliver one by pressing the other.
    #[test]
    fn stopping_scufris_is_not_the_key_that_puts_the_pill_away() {
        assert_ne!(beside("Super+D", CANCEL), beside("Super+D", STOP));
    }

    /// A bare accelerator is global. Granting the companion one would take
    /// Escape off the desktop for every other program, for the whole session,
    /// and nothing here is worth that: the textbox holds its own Escape, a
    /// listen can always be put away with the tray, and a run can always be
    /// stopped with `scufris-ctl abort`.
    #[test]
    fn a_hotkey_with_no_modifier_leaves_the_desktops_bare_keys_alone() {
        assert_eq!(beside("F9", CANCEL), None);
        assert_eq!(beside("F9", STOP), None);
    }
}
