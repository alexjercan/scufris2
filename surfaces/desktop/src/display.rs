//! What the display says a window is doing, as opposed to what it was told to.
//!
//! Showing a window is a message, not an act. `show` hands the toolkit a
//! request that only the event loop carries out, and the event loop carries it
//! out by handing the X server another one. A window asked about itself the
//! moment the request returns therefore answers about the world before the
//! request: the pill says "not up" while it is coming up, and "still up" while
//! it is going down, and a window that has just been told to take the keyboard
//! has not been given it yet because the display has not been asked.
//!
//! So the questions are put to the display itself, over a connection of this
//! module's own, and they are put again until the request has had time to reach
//! it. Three answers come back, and the third one is the point: a window
//! nothing can speak for has not failed at anything.
//!
//! Waiting is only possible away from the thread the event loop runs on. That
//! thread is the one that would have to carry the request out, so a wait there
//! could do nothing but run out of time and blame the window for it. It is
//! told which thread it is, and answers `Unsure` at once instead.

use std::{
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread::{self, ThreadId},
    time::{Duration, Instant},
};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::WebviewWindow;
use x11rb::{
    connection::Connection,
    protocol::xproto::{ClientMessageEvent, ConnectionExt, EventMask, MapState, Window},
    rust_connection::RustConnection,
};

/// How long a window is given to come up or go down before the display's answer
/// is taken as final.
///
/// Generous on purpose: the request crosses the event loop, the toolkit's own
/// queue and the X server before anything about it is true, and the cost of
/// waiting too long is a slow verdict while the cost of not waiting long enough
/// is a false one.
const PATIENCE: Duration = Duration::from_millis(500);

/// How long a window is given to take the keyboard.
///
/// Shorter than [`PATIENCE`], because this is the one wait a person can feel: a
/// window manager that means to hand the keyboard over has done it within a
/// frame or two, and one that does not mean to never will. Waiting the full
/// patience for it would only delay the microphone behind a pill that is
/// already up.
const KEYBOARD_PATIENCE: Duration = Duration::from_millis(250);

/// How long between two asks.
const ASK_AGAIN: Duration = Duration::from_millis(10);

/// How far up the window tree the keyboard is followed.
///
/// The toolkit puts windows inside the window it gives out, and the keyboard
/// can land on any of them. Bounded because a tree is only walked upwards on
/// trust.
const DEPTH: usize = 8;

/// What the display answered about one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The display says so.
    Yes,
    /// The display says otherwise, and has had long enough to say something
    /// else.
    No,
    /// Nothing could say. Not a failure: a question nobody was in a position to
    /// answer.
    Unsure,
}

/// The thread the event loop runs on.
static EVENT_LOOP: OnceLock<ThreadId> = OnceLock::new();

/// Whether that loop has started running.
static RUNNING: AtomicBool = AtomicBool::new(false);

/// The display, once, for every window that gets asked about.
static DISPLAY: OnceLock<Option<Session>> = OnceLock::new();

/// Records the calling thread as the one the event loop runs on.
pub fn runs_the_event_loop() {
    let _ = EVENT_LOOP.set(thread::current().id());
}

/// Records that the event loop has started.
///
/// Before it has, a window request is only queued, and a window asked about
/// itself from another thread waits for an answer that thread cannot produce.
/// Nothing is asked and nothing is waited for until this has been said.
pub fn the_event_loop_is_running() {
    RUNNING.store(true, Ordering::SeqCst);
}

/// Answers whether a window can be asked about itself at all from here.
fn reachable() -> bool {
    RUNNING.load(Ordering::SeqCst) || on_the_event_loop()
}

/// Answers whether a request can be waited for from here.
fn patient() -> bool {
    RUNNING.load(Ordering::SeqCst) && !on_the_event_loop()
}

/// Answers whether this is the thread the event loop runs on.
///
/// Public because waiting is not the only thing this thread must not do. It is
/// the thread every window request is carried out on, so a wait taken here for
/// anything a window request could be behind is a wait for itself. Whoever
/// holds such a thing has to be able to ask.
pub fn on_the_event_loop() -> bool {
    EVENT_LOOP
        .get()
        .is_some_and(|thread| *thread == thread::current().id())
}

/// What the display knows one window by.
enum Named {
    /// The window the display knows it by.
    Known(Window),
    /// There is no window on the display yet. A window that does not exist is
    /// not up and holds nothing, which is what makes this an answer.
    Absent,
    /// This is not an X window, so the display cannot answer for it and the
    /// toolkit is all there is.
    Elsewhere,
}

/// Returns the X window one Tauri window is, once the display has made one.
///
/// The window is remembered: it is made once and outlives every hide, and
/// asking for it again is a round trip through the event loop.
pub fn identify(window: &WebviewWindow, known: &AtomicU32) -> Option<u32> {
    match name(window, known) {
        Named::Known(id) => Some(id),
        Named::Absent | Named::Elsewhere => None,
    }
}

fn name(window: &WebviewWindow, known: &AtomicU32) -> Named {
    let remembered = known.load(Ordering::SeqCst);
    if remembered != 0 {
        return Named::Known(remembered);
    }
    if !reachable() {
        return Named::Absent;
    }
    let Ok(handle) = window.window_handle() else {
        return Named::Absent;
    };
    let RawWindowHandle::Xlib(xlib) = handle.as_raw() else {
        return Named::Elsewhere;
    };
    match u32::try_from(xlib.window) {
        Ok(0) | Err(_) => Named::Absent,
        Ok(id) => {
            known.store(id, Ordering::SeqCst);
            Named::Known(id)
        }
    }
}

/// Answers what the display says about one window being up, right now.
pub fn up(window: &WebviewWindow, known: &AtomicU32) -> Verdict {
    if !reachable() {
        return Verdict::Unsure;
    }
    match name(window, known) {
        Named::Absent => Verdict::No,
        Named::Elsewhere => match window.is_visible() {
            // Only the positive answer is worth anything: the toolkit records
            // a window as shown when it passes the request on, and as hidden
            // until then.
            Ok(true) => Verdict::Yes,
            _ => Verdict::Unsure,
        },
        Named::Known(id) => match session() {
            Some(session) => session.mapped(id),
            None => match window.is_visible() {
                Ok(true) => Verdict::Yes,
                _ => Verdict::Unsure,
            },
        },
    }
}

/// Answers what the display says about one window holding the keyboard, right
/// now.
pub fn keyboard(window: &WebviewWindow, known: &AtomicU32) -> Verdict {
    if !reachable() {
        return Verdict::Unsure;
    }
    match name(window, known) {
        Named::Absent => Verdict::No,
        Named::Elsewhere => match window.is_focused() {
            Ok(true) => Verdict::Yes,
            _ => Verdict::Unsure,
        },
        Named::Known(id) => match session() {
            Some(session) => session.keyboard(id),
            None => match window.is_focused() {
                Ok(true) => Verdict::Yes,
                _ => Verdict::Unsure,
            },
        },
    }
}

/// Answers whether the keyboard is on no window at all, right now.
///
/// The X server answers a focus of `None` or `PointerRoot` when no client holds
/// the keyboard: the first drops every key on the floor, and the second sends
/// them wherever the pointer happens to rest. Either way nobody was given them
/// and nobody can be typing, which is what makes this worth asking - a keyboard
/// no window holds can be taken without taking it from anyone.
///
/// A display nothing can be asked of answers `Unsure`, and `Unsure` is never a
/// reason to take anything.
pub fn nobody_holds_the_keyboard() -> Verdict {
    if !reachable() {
        return Verdict::Unsure;
    }
    match session() {
        Some(session) => session.unheld(),
        None => Verdict::Unsure,
    }
}

/// Puts one window on every workspace, or brings it back to the current one.
///
/// `_NET_WM_STATE_STICKY`, asked for the way the specification says a client
/// asks: a message to the root window rather than a property written directly,
/// because once a window is mapped the state belongs to the window manager. i3
/// honors it for floating windows, and a widget window is always floating.
///
/// The window has to be on screen. A window manager unmanages a window when it
/// unmaps, and takes its state with it, so a window that goes down and comes
/// back has to be asked again.
pub fn sticky(window: &WebviewWindow, known: &AtomicU32, wanted: bool) -> Result<(), String> {
    let Named::Known(id) = name(window, known) else {
        return Err("the display could not name a window to keep on every workspace".into());
    };
    match session() {
        Some(session) => session.sticky(id, wanted),
        None => Err("no display answered a request to keep a window on every workspace".into()),
    }
}

/// Waits for one window to come up, and answers what the display said.
pub fn came_up(window: &WebviewWindow, known: &AtomicU32) -> Verdict {
    until(window, known, up, Verdict::Yes, PATIENCE)
}

/// Waits for one window to go down, and answers what the display said.
pub fn went_down(window: &WebviewWindow, known: &AtomicU32) -> Verdict {
    until(window, known, up, Verdict::No, PATIENCE)
}

/// Waits for one window to take the keyboard, and answers what the display
/// said.
pub fn took_the_keyboard(window: &WebviewWindow, known: &AtomicU32) -> Verdict {
    until(window, known, keyboard, Verdict::Yes, KEYBOARD_PATIENCE)
}

/// Asks until the display gives the wanted answer, or until it has had long
/// enough not to.
fn until(
    window: &WebviewWindow,
    known: &AtomicU32,
    ask: fn(&WebviewWindow, &AtomicU32) -> Verdict,
    wanted: Verdict,
    patience: Duration,
) -> Verdict {
    let mut answer = ask(window, known);
    if answer == wanted {
        return Verdict::Yes;
    }
    if !patient() {
        return Verdict::Unsure;
    }
    let deadline = Instant::now() + patience;
    while Instant::now() < deadline {
        thread::sleep(ASK_AGAIN);
        answer = ask(window, known);
        if answer == wanted {
            return Verdict::Yes;
        }
    }
    match answer {
        // Nothing was ever in a position to answer, so the time proves nothing.
        Verdict::Unsure => Verdict::Unsure,
        _ => Verdict::No,
    }
}

fn session() -> Option<&'static Session> {
    DISPLAY.get_or_init(Session::open).as_ref()
}

/// One connection of this module's own.
///
/// Its own, rather than the toolkit's, because the toolkit's belongs to the
/// event loop and these questions are asked from everywhere else.
struct Session {
    connection: RustConnection,
    /// The root window of the screen this connection opened on. A client asks
    /// the window manager for a state by messaging the root, not the window.
    root: Window,
}

impl Session {
    fn open() -> Option<Self> {
        let (connection, screen) = x11rb::connect(None).ok()?;
        let root = connection.setup().roots.get(screen)?.root;
        Some(Self { connection, root })
    }

    /// Asks the window manager to put one window on every workspace, or to stop.
    fn sticky(&self, window: Window, wanted: bool) -> Result<(), String> {
        let state = self.atom("_NET_WM_STATE")?;
        let flag = self.atom("_NET_WM_STATE_STICKY")?;
        // The specification's own numbers: 0 removes a state, 1 adds one, and
        // the fourth word says a normal application is asking rather than a
        // pager.
        let action = u32::from(wanted);
        let message = ClientMessageEvent::new(32, window, state, [action, flag, 0, 1, 0]);
        self.connection
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT,
                message,
            )
            .map_err(|error| format!("the window manager would not take a sticky state: {error}"))?
            .ignore_error();
        self.connection
            .flush()
            .map_err(|error| format!("a sticky state did not reach the display: {error}"))
    }

    fn atom(&self, name: &str) -> Result<u32, String> {
        self.connection
            .intern_atom(false, name.as_bytes())
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.atom)
            .ok_or_else(|| format!("the display does not know {name}"))
    }

    /// Answers whether the server has one window on screen.
    fn mapped(&self, window: Window) -> Verdict {
        let Some(reply) = self
            .connection
            .get_window_attributes(window)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
        else {
            return Verdict::Unsure;
        };
        if reply.map_state == MapState::VIEWABLE {
            Verdict::Yes
        } else {
            Verdict::No
        }
    }

    /// Answers whether the keyboard is in one window.
    fn keyboard(&self, window: Window) -> Verdict {
        let Some(reply) = self
            .connection
            .get_input_focus()
            .ok()
            .and_then(|cookie| cookie.reply().ok())
        else {
            return Verdict::Unsure;
        };
        if holds(reply.focus, window, |child| self.parent(child)) {
            Verdict::Yes
        } else {
            Verdict::No
        }
    }

    /// Answers whether the keyboard is on no window at all.
    fn unheld(&self) -> Verdict {
        let Some(reply) = self
            .connection
            .get_input_focus()
            .ok()
            .and_then(|cookie| cookie.reply().ok())
        else {
            return Verdict::Unsure;
        };
        if unheld(reply.focus) {
            Verdict::Yes
        } else {
            Verdict::No
        }
    }

    fn parent(&self, window: Window) -> Option<Window> {
        let reply = self.connection.query_tree(window).ok()?.reply().ok()?;
        if reply.parent == 0 || reply.parent == reply.root {
            return None;
        }
        Some(reply.parent)
    }
}

/// Answers whether one focus reply names no window at all.
///
/// `XGetInputFocus` answers 0 for `None` and 1 for `PointerRoot`. Both are
/// answers about the screen rather than about a window, and neither is a client
/// that was given the keyboard.
///
/// Separate from the connection so it can be checked without a display.
fn unheld(focus: Window) -> bool {
    focus <= 1
}

/// Answers whether the keyboard is inside one window: on the window itself, or
/// on anything the toolkit put inside it.
///
/// Separate from the connection so the walk can be checked without a display.
fn holds(focus: Window, window: Window, parent: impl Fn(Window) -> Option<Window>) -> bool {
    let mut at = focus;
    for _ in 0..DEPTH {
        if at == window {
            return true;
        }
        // Nothing and the pointer's root are answers about the screen rather
        // than about a window, and neither of them is this one.
        if at <= 1 {
            return false;
        }
        match parent(at) {
            Some(above) => at = above,
            None => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// One window tree, child to parent.
    fn tree(pairs: &[(Window, Window)]) -> impl Fn(Window) -> Option<Window> {
        let parents: HashMap<Window, Window> = pairs.iter().copied().collect();
        move |child| parents.get(&child).copied()
    }

    #[test]
    fn the_keyboard_is_held_by_the_window_that_has_it() {
        assert!(holds(0x400010, 0x400010, tree(&[])));
        assert!(!holds(0x400010, 0x400020, tree(&[])));
    }

    #[test]
    fn the_keyboard_is_held_by_a_window_the_toolkit_put_it_inside() {
        // The webview is a window of its own inside the frame Tauri hands out,
        // so the keyboard can land on a child and still be the frame's.
        let inside = tree(&[(0x400030, 0x400020), (0x400020, 0x400010)]);
        assert!(holds(0x400030, 0x400010, inside));
    }

    #[test]
    fn a_keyboard_on_another_window_is_not_held_however_far_up_it_goes() {
        let elsewhere = tree(&[(0x500030, 0x500020)]);
        assert!(!holds(0x500030, 0x400010, elsewhere));
    }

    #[test]
    fn nothing_and_the_pointers_root_hold_no_window() {
        // XGetInputFocus answers 0 for None and 1 for PointerRoot, and a
        // headless session with nothing focused answers exactly that.
        assert!(!holds(0, 0x400010, tree(&[])));
        assert!(!holds(1, 0x400010, tree(&[])));
    }

    #[test]
    fn a_keyboard_no_client_was_given_is_held_by_nobody() {
        // The two answers about the screen. A window manager that focuses a
        // window which refuses the keyboard, and then loses that window, leaves
        // the server on one of them, and every key the person presses goes
        // nowhere.
        assert!(unheld(0));
        assert!(unheld(1));
        // Any window, ours or anybody's, is somebody holding it.
        assert!(!unheld(0x400010));
    }

    #[test]
    fn a_tree_that_never_ends_is_not_walked_forever() {
        // Only ever walked upwards on trust, so a cycle is a hang unless the
        // walk is bounded.
        let circle = tree(&[(0x400020, 0x400030), (0x400030, 0x400020)]);
        assert!(!holds(0x400020, 0x400010, circle));
    }

    #[test]
    fn nothing_is_waited_for_before_the_event_loop_runs() {
        // The event loop is what carries a window request out. Until it is
        // running, waiting for one could only run out of time.
        assert!(!RUNNING.load(Ordering::SeqCst));
        assert!(!patient());
        assert!(!reachable());
    }
}
