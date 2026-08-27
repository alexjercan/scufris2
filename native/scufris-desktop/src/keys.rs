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
//! `Super+Delete` stops what Scufris is doing. Grabbed only while the pill is
//! on screen, because an accelerator held all session is one no other program
//! can ever use.
//!
//! The deployment can name either of them instead. Deriving them is the
//! default rather than the rule: one modifier to remember is the right thing
//! to ship, and a desktop where `Super+Escape` already means something is a
//! reason to move the key rather than to lose it. [`NONE`] takes a key off the
//! companion entirely, which is the answer for a desktop that wants both of
//! these keys for itself.
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
/// `Period` was the first choice and was wrong: `Super+.` is the emoji picker
/// on Windows, and the rofi and Hyprland desktops that copied it mean the same
/// thing by it. A default that collides with a convention that widespread is a
/// default that is configured away on every machine.
///
/// `Delete` belongs to nothing modified, which is what makes it grabbable, and
/// what it means is close enough: end the thing that is running. It is spelled
/// out in full because the accelerator parser knows `Delete` and does not know
/// `Del`.
const STOP: &str = "Delete";

/// What a deployment writes to take one key off the companion.
///
/// Not an accelerator and never parsed as one. A person who wants `Super+Q` to
/// stay theirs says so here rather than by finding a key nothing uses.
pub const NONE: &str = "none";

/// What the deployment said about the keys beside the hotkey.
///
/// Each is what was configured, and nothing is what was not: an unset key is
/// derived from the hotkey, which is what ships.
#[derive(Debug, Clone, Copy, Default)]
pub struct Wanted<'a> {
    /// The accelerator that puts the pill away.
    pub cancel: Option<&'a str>,
    /// The accelerator that stops Scufris.
    pub stop: Option<&'a str>,
}

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
    pub fn new(handle: AppHandle, hotkey: &str, wanted: Wanted<'_>) -> Self {
        let cancel = chosen(wanted.cancel, hotkey, CANCEL);
        let stop = chosen(wanted.stop, hotkey, STOP);
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

/// The accelerator one key ends up on: what was asked for, or what it derives
/// to, or nothing.
///
/// An accelerator that will not parse is warned about and dropped rather than
/// quietly derived. Falling back would leave the person with a working key on
/// the wrong accelerator, which is harder to notice than a key that does
/// nothing and says why in the log.
fn chosen(wanted: Option<&str>, hotkey: &str, key: &str) -> Option<Shortcut> {
    match wanted {
        Some(NONE) => None,
        Some(accelerator) => parse(accelerator),
        None => beside(hotkey, key),
    }
}

/// One accelerator beside the activation hotkey, on the hotkey's own modifiers.
///
/// Its modifiers and nothing else: `Super+D` opens the pill, so `Super+Escape`
/// puts it away and `Super+Delete` stops it, and the person has one modifier to
/// remember rather than three. A hotkey with no modifier leaves none, because a
/// bare key the display gave the companion is one no other program on the
/// desktop would ever see again.
fn beside(hotkey: &str, key: &str) -> Option<Shortcut> {
    let (modifiers, _) = hotkey.rsplit_once('+')?;
    parse(&format!("{modifiers}+{key}"))
}

/// One accelerator, or nothing and a line in the log saying why.
fn parse(accelerator: &str) -> Option<Shortcut> {
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
        assert_eq!(beside("Super+D", STOP), Some(accelerator("Super+Delete")));
        assert_eq!(
            beside("Control+Alt+G", CANCEL),
            Some(accelerator("Control+Alt+Escape"))
        );
        assert_eq!(
            beside("Control+Alt+G", STOP),
            Some(accelerator("Control+Alt+Delete"))
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

    /// Deriving is the default and not the rule. What ships is one modifier to
    /// remember; what a person configures is what they get.
    #[test]
    fn a_key_the_deployment_named_is_the_key_that_is_grabbed() {
        assert_eq!(
            chosen(None, "Super+D", CANCEL),
            Some(accelerator("Super+Escape")),
            "an unset key derives"
        );
        assert_eq!(
            chosen(Some("Control+Shift+Q"), "Super+D", CANCEL),
            Some(accelerator("Control+Shift+Q"))
        );
        // A configured key owes the hotkey nothing, down to sharing no
        // modifier with it.
        assert_eq!(
            chosen(Some("Alt+F4"), "F9", STOP),
            Some(accelerator("Alt+F4")),
            "a hotkey with no modifier still allows a named key"
        );
    }

    /// A desktop that wants the key for itself says so, rather than hunting for
    /// an accelerator the companion will fail to parse.
    #[test]
    fn a_key_turned_off_is_grabbed_by_nothing() {
        assert_eq!(chosen(Some(NONE), "Super+D", CANCEL), None);
        assert_eq!(chosen(Some(NONE), "Super+D", STOP), None);
    }

    /// Never derived from. A working key on an accelerator the person did not
    /// ask for is harder to notice than a key that does nothing and says why.
    #[test]
    fn an_accelerator_that_will_not_parse_leaves_no_key_rather_than_the_default() {
        assert_eq!(chosen(Some("Hyper+Nonsense"), "Super+D", CANCEL), None);
        assert_eq!(chosen(Some(""), "Super+D", STOP), None);
    }
}
