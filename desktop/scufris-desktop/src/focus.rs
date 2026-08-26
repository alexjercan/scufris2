//! Focus capture and restoration for the window the pill covers.
//!
//! The pill takes keyboard focus while it is open, so cancelling or submitting
//! must give focus back to whatever the user was using. The tracker records the
//! active window before the pill appears and asks the window manager to
//! activate it again afterwards. Without an X display it does nothing.

use std::sync::Mutex;

use x11rb::{
    CURRENT_TIME,
    connection::Connection,
    protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, InputFocus, Window,
    },
    rust_connection::RustConnection,
};

/// Source indication used by `_NET_ACTIVE_WINDOW` for pager-like clients.
const SOURCE_PAGER: u32 = 2;

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
    /// What cannot be bettered is left alone. A display with no active window
    /// to name, and a window that turns out to be the companion's, both leave
    /// the last window worth returning to exactly where it was: the pill
    /// promised to give the desktop back, and forgetting where it came from is
    /// not a way to keep that promise.
    pub fn capture(&self, mine: &[Window]) {
        let Some(session) = &self.connection else {
            return;
        };
        let Some(active) = worth_returning_to(session.active_window(), mine) else {
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

/// Answers which of the display's answers is a window to go back to.
///
/// Separate from the connection so the one rule that matters - never the
/// companion's own windows - can be checked without a display.
fn worth_returning_to(active: Option<Window>, mine: &[Window]) -> Option<Window> {
    active.filter(|window| !mine.contains(window))
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
        assert_eq!(worth_returning_to(Some(review), &[pill, review]), None);
        assert_eq!(worth_returning_to(Some(pill), &[pill, review]), None);
        assert_eq!(
            worth_returning_to(Some(0x500030), &[pill, review]),
            Some(0x500030)
        );
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
}
