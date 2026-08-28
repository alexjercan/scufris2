//! Focus capture and restoration for the window the pill covers.
//!
//! The pill takes keyboard focus while it is open, so cancelling or submitting
//! must give focus back to whatever the user was using. The tracker records the
//! active window before the pill appears and asks the window manager to
//! activate it again afterwards. Without an X display it does nothing.

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager};
use x11rb::{
    CURRENT_TIME,
    connection::Connection,
    protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, InputFocus, Window,
    },
    rust_connection::RustConnection,
};

use crate::{form, hud, pill, textbox, widgets};

/// Source indication used by `_NET_ACTIVE_WINDOW` for pager-like clients.
const SOURCE_PAGER: u32 = 2;

/// How far up the window tree the keyboard is followed to its top level.
const DEPTH: usize = 32;

/// Remembers and restores the window that had focus before the pill opened.
pub struct FocusTracker {
    connection: Option<Session>,
    previous: Mutex<Option<Window>>,
}

struct Session {
    connection: RustConnection,
    root: Window,
    active_window: u32,
}

impl FocusTracker {
    /// Connects to the display, or returns an inert tracker when there is none.
    pub fn new() -> Self {
        Self {
            connection: Session::open(),
            previous: Mutex::new(None),
        }
    }

    /// Records the currently active window, unless it is one of `mine`.
    ///
    /// A window of the companion's own is never somewhere to give the desktop
    /// back to. The window manager names the transcript box as the active
    /// window for as long as the box is up - it is the container i3 believes is
    /// focused, whatever the box says about the keyboard - so a capture taken
    /// while a review is on screen would record the box and hand the person's
    /// keys to a window that refuses them.
    ///
    /// When the active window is the companion's, the keyboard is asked
    /// instead. A click on a panel makes i3 name that panel the active window
    /// while the keyboard stays exactly where it was, because the panel is
    /// built refusing it. A capture taken then - which is every capture the
    /// panels' form window takes, since a tick is what raises it - would find
    /// only our own panel and record nothing, and the box would take the
    /// person's keys out of their editor with nowhere to put them back.
    ///
    /// What cannot be bettered is left alone. A display with no active window
    /// and no keyboard to name, and both of them turning out to be the
    /// companion's, leave the last window worth returning to exactly where it
    /// was: the pill promised to give the desktop back, and forgetting where it
    /// came from is not a way to keep that promise.
    pub fn capture(&self, mine: &[Window]) {
        let Some(session) = &self.connection else {
            return;
        };
        let Some(active) =
            somewhere_to_return(session.active_window(), || session.keyboard_window(), mine)
        else {
            return;
        };
        *self
            .previous
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(active);
    }

    /// Activates the window recorded by the last [`FocusTracker::capture`].
    ///
    /// Having nothing to go back to is not a failure: without a display, or
    /// before the first capture, there is no window this could return to and
    /// nothing has gone wrong. Only a display that refuses the request is
    /// reported, because that is the case where the person's window stays
    /// behind the pill.
    pub fn restore(&self) -> Result<(), String> {
        let Some(session) = &self.connection else {
            return Ok(());
        };
        let previous = self
            .previous
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        let Some(window) = previous else {
            return Ok(());
        };
        session.activate(window)
    }
}

/// Every window the companion has put on the display.
///
/// The one list. There is a tracker on the pill's side and a tracker on the
/// conversation window's, and the first time they kept a list each, the second
/// one was short: it named only its own window, on the reasoning that nothing
/// else of the companion's could be active while it was up. `capture` asks
/// `_NET_ACTIVE_WINDOW` first, and i3 marks the pill active when the pill maps
/// even though the pill is built refusing the keyboard. So the pill was
/// recorded, `restore` activated it, and the person's keys went to the one
/// window here that cannot take them. The keyboard is the second answer, and it
/// is filtered through this same list for the same reason.
///
/// A window of the companion's own is never somewhere to give the desktop back
/// to. Deriving the list from the app rather than passing one keeps that true
/// for a surface nobody has added yet.
pub fn own_windows(app: &AppHandle) -> Vec<Window> {
    let mut mine: Vec<Window> = [
        pill::known_window(),
        textbox::known_window(),
        hud::known_window(),
        form::known_window(),
    ]
    .into_iter()
    .flatten()
    .collect();
    if let Some(widgets) = app.try_state::<Arc<widgets::Widgets>>() {
        mine.extend(widgets.windows());
    }
    mine
}

/// Answers which of the display's answers is a window to go back to.
///
/// Separate from the connection so the one rule that matters - never the
/// companion's own windows - can be checked without a display.
fn worth_returning_to(active: Option<Window>, mine: &[Window]) -> Option<Window> {
    active.filter(|window| !mine.contains(window))
}

/// Answers where the desktop goes back to, from the two things the display can
/// say about it.
///
/// The active window first: it is what a window manager means by "where the
/// person is", and it is right every time the pill or the conversation window
/// is raised from a key. The keyboard only when that answer is our own, which
/// is the panel case - i3 names a clicked panel active, and the keys never left
/// the window the person was typing in. Asked lazily, because reading it costs
/// a round trip and a walk up the window tree that the common case never needs.
fn somewhere_to_return(
    active: Option<Window>,
    keyboard: impl FnOnce() -> Option<Window>,
    mine: &[Window],
) -> Option<Window> {
    worth_returning_to(active, mine).or_else(|| worth_returning_to(keyboard(), mine))
}

impl Default for FocusTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    fn open() -> Option<Self> {
        let (connection, screen) = x11rb::connect(None).ok()?;
        let root = connection.setup().roots.get(screen)?.root;
        let active_window = connection
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")
            .ok()?
            .reply()
            .ok()?
            .atom;
        Some(Self {
            connection,
            root,
            active_window,
        })
    }

    fn active_window(&self) -> Option<Window> {
        let reply = self
            .connection
            .get_property(false, self.root, self.active_window, AtomEnum::WINDOW, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        reply
            .value32()
            .and_then(|mut values| values.next())
            .filter(|window| *window != 0)
    }

    /// The top-level window the keyboard is really in.
    ///
    /// The X server answers the focus of `None` or `PointerRoot` when nobody
    /// holds the keyboard, and neither is a window to go back to. What it does
    /// name is often an inner window of the client's, so it is walked up to the
    /// top level the window manager and `mine` both speak in terms of.
    fn keyboard_window(&self) -> Option<Window> {
        let reply = self.connection.get_input_focus().ok()?.reply().ok()?;
        let mut window = reply.focus;
        if window == 0 || window == 1 || window == self.root {
            return None;
        }
        // Bounded rather than `loop`: a display that answered a cycle would
        // otherwise hang the raise this is being asked for.
        for _ in 0..DEPTH {
            let tree = self.connection.query_tree(window).ok()?.reply().ok()?;
            if tree.parent == 0 || tree.parent == tree.root {
                return Some(window);
            }
            window = tree.parent;
        }
        None
    }

    fn activate(&self, window: Window) -> Result<(), String> {
        let event = ClientMessageEvent::new(
            32,
            window,
            self.active_window,
            [SOURCE_PAGER, CURRENT_TIME, 0, 0, 0],
        );
        let sent = self.connection.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT,
            event,
        );
        if sent.is_err() {
            self.connection
                .set_input_focus(InputFocus::PARENT, window, CURRENT_TIME)
                .map_err(|error| format!("the previous window would not take focus: {error}"))?;
        }
        self.connection
            .flush()
            .map_err(|error| format!("the display did not accept the focus change: {error}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tracker_without_a_display_stays_inert() {
        // The build sandbox has no X display, so this exercises the fallback the
        // companion uses whenever focus restoration is impossible.
        let tracker = FocusTracker {
            connection: None,
            previous: Mutex::new(None),
        };
        assert!(tracker.connection.is_none());
        tracker.capture(&[]);
        assert_eq!(tracker.restore(), Ok(()));
    }

    #[test]
    fn a_window_of_the_companions_own_is_never_somewhere_to_go_back_to() {
        // The window manager names the transcript box as the active window for
        // as long as the box is up. Recording it would send the person's keys
        // back to the one window that refuses them.
        let pill = 0x400010;
        let review = 0x400020;
        // And every widget window. A shell is built unfocusable and stays that
        // way, so a capture that recorded one would send the person's keys to
        // the one kind of window here that is certain to refuse them.
        let widget = 0x400030;
        let mine = [pill, review, widget];
        assert_eq!(worth_returning_to(Some(review), &mine), None);
        assert_eq!(worth_returning_to(Some(pill), &mine), None);
        assert_eq!(worth_returning_to(Some(widget), &mine), None);
        assert_eq!(worth_returning_to(Some(0x500030), &mine), Some(0x500030));
    }

    #[test]
    fn a_display_with_nothing_active_leaves_the_last_window_alone() {
        // A window that took the keyboard and died leaves the display with no
        // active window to name. That is the moment the pill most needs to
        // remember where the person was, and nothing here is worth overwriting
        // it with: a capture only ever records a window.
        assert_eq!(worth_returning_to(None, &[]), None);
        assert_eq!(worth_returning_to(None, &[0x400010]), None);
    }

    #[test]
    fn a_panel_named_active_is_answered_by_the_keyboard_instead() {
        // Clicking a panel makes i3 name that panel the active window while the
        // keyboard stays in the editor, because the panel refuses it. That is
        // every capture the form window takes, so the active window alone would
        // record nothing and the box would have nowhere to give the keys back
        // to.
        let panel = 0x400030;
        let editor = 0x500030;
        let mine = [panel];
        assert_eq!(
            somewhere_to_return(Some(panel), || Some(editor), &mine),
            Some(editor)
        );
    }

    #[test]
    fn the_active_window_is_asked_first_and_the_keyboard_only_after_it() {
        // A key raises the pill while the person's window is both active and
        // holding the keyboard, which is the common case and must not pay for
        // the panel one. Nothing else is read when the first answer stands.
        let editor = 0x500030;
        assert_eq!(
            somewhere_to_return(Some(editor), || panic!("the keyboard was read"), &[]),
            Some(editor)
        );
        // And a display holding the keyboard nowhere leaves the last window
        // alone, the same as one naming nothing active.
        assert_eq!(somewhere_to_return(None, || None, &[]), None);
        assert_eq!(
            somewhere_to_return(Some(0x400030), || Some(0x400010), &[0x400030, 0x400010]),
            None
        );
    }
}
