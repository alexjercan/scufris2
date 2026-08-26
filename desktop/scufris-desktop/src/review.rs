//! The transient transcript window above the orb.
//!
//! The orb says what the companion is doing; it cannot say what the words are.
//! So the words get a window of their own, and only while there is a decision
//! to make about them: it rises above the orb for review and for an uncertain
//! transcript, and leaves with the decision.
//!
//! It is display-only. Every key still belongs to the orb window - Enter sends,
//! Escape discards, a second Enter forces an uncertain send - so this window is
//! built and shown exactly like a passive pill and is never focused. A box that
//! took the keyboard would take those keys away from the window that acts on
//! them.
//!
//! Refusing the keyboard is said again before every raise, not once at build
//! time. The window is built refusing it, and the toolkit hands that refusal
//! back on its own: tao restores `accept-focus` from a one-shot draw handler,
//! so the box advertises `WM_HINTS.input = True` from its first appearance
//! onwards. A window manager that unmanages a hidden window and manages it
//! again on the next show - i3 does - then reads those hints and gives the
//! newly mapped box the keyboard. The first review is unaffected because the
//! box has not been drawn yet; every later one loses the orb its keys, and
//! nothing on the orb can ask for them back, because a review has no further
//! change to make and an activation does nothing in it.

use std::sync::atomic::AtomicU32;

use tauri::{
    AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::{
    display::{self, Verdict},
    pill,
};

/// Stable window label. `capabilities/default.json` names it too: a window the
/// capability does not cover cannot listen for the presentation it renders.
pub const LABEL: &str = "review";

/// Box width in logical pixels.
///
/// The window is exactly what `review.css` lays out, for the same reason the
/// pill window is: without a compositor there is no alpha to hide a margin in.
/// This one keeps its corners - it is a box of words, and an ellipse would cut
/// the ends off the lines it exists to show.
///
/// Wider than the 460 the orb study landed on, by as much as the type grew:
/// the same words per line, in letters that match a pill two and a half times
/// the size it was.
pub const WIDTH: f64 = 620.0;

/// Box height in logical pixels.
///
/// Three lines of transcript and the hint line. A take longer than that scrolls
/// under a fade rather than resizing the window: equal min and max hints cannot
/// be changed while the window is up without re-applying them.
pub const HEIGHT: f64 = 140.0;

/// Gap between the bottom of the box and the top of the orb window, in logical
/// pixels. Twice what it was, because what it separates the box from is now
/// two and a half times the size.
const GAP: f64 = 24.0;

/// The states whose words the person has to decide about.
///
/// `review.ts` holds the same two, as the hints it can print. Everything else -
/// idle, listening, sent, working - is the orb alone, which is the whole point
/// of the bare orb: chrome exists only when there is something to act on.
///
/// A retained transcript is deliberately not among them, which is the accepted
/// design and the one place it costs something: those words are editable and
/// this leaves them unread. Adding "retained" here and a third hint to
/// `review.ts` is the whole change if that is wanted.
const RAISED: [&str; 2] = ["review", "uncertain"];

/// The X window the display knows the box by, once it has made one.
static WINDOW: AtomicU32 = AtomicU32::new(0);

/// Returns the physical position of the box, centered above the orb window.
pub fn above_pill(
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: u32,
    monitor_height: u32,
    scale: f64,
) -> PhysicalPosition<i32> {
    let pill = pill::bottom_center(monitor_x, monitor_y, monitor_width, monitor_height, scale);
    let width = (WIDTH * scale).round() as i32;
    let height = (HEIGHT * scale).round() as i32;
    let gap = (GAP * scale).round() as i32;
    let x = monitor_x + ((monitor_width as i32 - width) / 2).max(0);
    let y = (pill.y - height - gap).max(monitor_y);
    PhysicalPosition::new(x, y)
}

/// Returns the review window, creating it hidden on first use.
///
/// Created with the pill at startup rather than on the first transcript: the
/// page has to be loaded and listening before the presentation that fills it
/// arrives, and a window built at that moment would miss it.
pub fn ensure(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(LABEL) {
        return Ok(window);
    }
    // The same recipe as the pill: opaque, undecorated, on top, out of the
    // taskbar, and pinned by equal min and max size hints on a resizable
    // window, which is the one combination GTK honors and what makes a tiling
    // window manager float it.
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("review.html".into()))
        .title("Scufris transcript")
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(WIDTH, HEIGHT)
        .max_inner_size(WIDTH, HEIGHT)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false)
        .focused(false)
        // Built unfocusable, which is not the same as built unfocused and is
        // the only one of the two that lasts. A window built merely unfocused
        // is given its right to the keyboard back by the toolkit after its
        // first paint, from a one-shot draw handler installed at build time.
        // For this window that paint lands in the middle of the process's
        // first review, while the window manager is already holding an offer of
        // the keyboard out to it, and the box answers an offer it should have
        // let lapse. Built unfocusable, the toolkit installs no handler at all
        // and the refusal below is the only thing that ever speaks for it.
        .focusable(false)
        .build()
}

/// Puts the box where the state wants it: up for the states that carry a
/// decision, down for every other.
///
/// The window is only ever chrome around the orb, so this is reported rather
/// than enforced. The orb is what the person must be able to see, and it is
/// already up by the time this runs.
pub fn follow(app: &AppHandle, state: &str) -> Result<(), String> {
    if RAISED.contains(&state) {
        show(app)
    } else {
        hide(app)
    }
}

/// The window operations one raise needs.
///
/// A trait so the order they run in is testable without a display. The order is
/// the whole of the keyboard contract: refusing the keyboard has to reach the
/// window before the window manager sees it, and a window manager sees it when
/// it is mapped.
trait Frame {
    /// Says whether the box may hold the keyboard.
    fn accept_focus(&self, accept: bool) -> Result<(), String>;
    /// Puts the box where it belongs.
    fn place(&self) -> Result<(), String>;
    /// Puts the box on screen.
    fn show(&self) -> Result<(), String>;
    /// Answers what the display says about the box being up.
    fn seen(&self) -> Verdict;
    /// Keeps the box over the desktop.
    fn keep_on_top(&self) -> Result<(), String>;
}

impl Frame for WebviewWindow {
    fn accept_focus(&self, accept: bool) -> Result<(), String> {
        self.set_focusable(accept)
            .map_err(|error| format!("the transcript box would not refuse the keyboard: {error}"))
    }

    fn place(&self) -> Result<(), String> {
        place(self).map_err(|error| format!("the transcript box could not be placed: {error}"))
    }

    fn show(&self) -> Result<(), String> {
        WebviewWindow::show(self)
            .map_err(|error| format!("the transcript box could not be shown: {error}"))
    }

    fn seen(&self) -> Verdict {
        display::came_up(self, &WINDOW)
    }

    fn keep_on_top(&self) -> Result<(), String> {
        self.set_always_on_top(true)
            .map_err(|error| format!("the transcript box could not be kept on top: {error}"))
    }
}

/// Shows the box above the orb without ever touching the keyboard.
fn show(app: &AppHandle) -> Result<(), String> {
    let window = ensure(app).map_err(|error| format!("the transcript box is missing: {error}"))?;
    raise(&window)
}

/// Puts one box on screen, in the only order that keeps the orb its keys.
fn raise(frame: &impl Frame) -> Result<(), String> {
    // First, and every time. See the module note: the toolkit gives the box
    // back its right to the keyboard after the first draw, so a raise that did
    // not say this again would hand the orb's keys to a window that has no use
    // for them and no way to give them back. A box that will not refuse them is
    // worse than no box at all - unread words leave the person their keys - so
    // this is the one thing here that refuses rather than reports.
    frame.accept_focus(false)?;
    // Placement is part of being read: the orb window is bottom-center on
    // whichever monitor the window manager reports, and the box belongs
    // directly above it.
    if let Err(error) = frame.place() {
        tracing::warn!("{error}");
    }
    frame.show()?;
    match frame.seen() {
        // Up, or asked for and not yet carried out. A request the display has
        // not reached is not a box that refused to come up, and saying so would
        // be the false verdict this reporting exists to avoid.
        Verdict::Yes | Verdict::Unsure => {}
        Verdict::No => return Err("the transcript box did not come up".into()),
    }
    if let Err(error) = frame.keep_on_top() {
        tracing::warn!("{error}");
    }
    Ok(())
}

/// Takes the box off screen, and confirms that it is down.
///
/// An always-on-top box left over a decision that is finished is worse than one
/// that never came up, so a box that will not go down is reported.
pub fn hide(app: &AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window(LABEL) else {
        return Ok(());
    };
    window
        .hide()
        .map_err(|error| format!("the transcript box could not be hidden: {error}"))?;
    match display::went_down(&window, &WINDOW) {
        // Down, or on its way and nobody in a position to watch it go.
        Verdict::Yes | Verdict::Unsure => Ok(()),
        Verdict::No => Err("the transcript box is still up".into()),
    }
}

/// Puts the box above the orb on the monitor the window manager describes.
fn place(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return Ok(());
    };
    let position = monitor.position();
    let size = monitor.size();
    window.set_position(above_pill(
        position.x,
        position.y,
        size.width,
        size.height,
        monitor.scale_factor(),
    ))
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::HashMap};

    use super::*;

    /// One box that records what it was told, in order.
    #[derive(Default)]
    struct RecordedFrame {
        operations: RefCell<Vec<String>>,
        /// Operations that answer with a failure instead of doing anything.
        refuse: HashMap<&'static str, &'static str>,
        /// What the display says about the box after it was shown.
        seen: Option<Verdict>,
    }

    impl RecordedFrame {
        fn refusing(operation: &'static str, reason: &'static str) -> Self {
            Self {
                refuse: HashMap::from([(operation, reason)]),
                ..Self::default()
            }
        }

        fn seeing(seen: Verdict) -> Self {
            Self {
                seen: Some(seen),
                ..Self::default()
            }
        }

        fn attempt(&self, operation: &'static str) -> Result<(), String> {
            self.operations.borrow_mut().push(operation.to_string());
            match self.refuse.get(operation) {
                Some(reason) => Err((*reason).to_string()),
                None => Ok(()),
            }
        }

        fn operations(&self) -> Vec<String> {
            self.operations.borrow().clone()
        }
    }

    impl Frame for RecordedFrame {
        fn accept_focus(&self, accept: bool) -> Result<(), String> {
            self.operations
                .borrow_mut()
                .push(format!("accept-focus {accept}"));
            match self.refuse.get("accept-focus") {
                Some(reason) => Err((*reason).to_string()),
                None => Ok(()),
            }
        }
        fn place(&self) -> Result<(), String> {
            self.attempt("place")
        }
        fn show(&self) -> Result<(), String> {
            self.attempt("show")
        }
        fn seen(&self) -> Verdict {
            let _ = self.attempt("seen");
            self.seen.unwrap_or(Verdict::Yes)
        }
        fn keep_on_top(&self) -> Result<(), String> {
            self.attempt("keep-on-top")
        }
    }

    /// Every raise refuses the keyboard, and refuses it before the box is on
    /// screen, because a window manager reads that refusal when the window is
    /// mapped. Saying it once at build time is not enough: the toolkit restores
    /// the window's right to the keyboard after its first draw, so from the
    /// second raise on an unsaid refusal hands the orb's keys to a box that
    /// cannot use them and cannot give them back.
    #[test]
    fn every_raise_refuses_the_keyboard_before_the_box_is_on_screen() {
        let frame = RecordedFrame::default();
        assert_eq!(raise(&frame), Ok(()));
        assert_eq!(raise(&frame), Ok(()));
        assert_eq!(
            frame.operations(),
            [
                "accept-focus false",
                "place",
                "show",
                "seen",
                "keep-on-top",
                "accept-focus false",
                "place",
                "show",
                "seen",
                "keep-on-top",
            ]
        );
    }

    /// Placement and always-on-top are chrome around the orb and are reported.
    /// The keyboard is not: a box that would hold it takes every key the
    /// person has, so it does not come up at all.
    #[test]
    fn a_box_that_will_not_refuse_the_keyboard_never_comes_up() {
        let frame =
            RecordedFrame::refusing("accept-focus", "the box would not refuse the keyboard");
        assert_eq!(
            raise(&frame),
            Err("the box would not refuse the keyboard".into())
        );
        assert_eq!(frame.operations(), ["accept-focus false"]);

        let frame = RecordedFrame::refusing("place", "the box could not be placed");
        assert_eq!(raise(&frame), Ok(()));
        assert!(frame.operations().iter().any(|step| step == "show"));
    }

    /// A box that has been asked to come up and has not reached the display yet
    /// is not a box that refused to. Reporting one as the other is what put
    /// "the transcript box did not come up" in the log of every start.
    #[test]
    fn a_box_the_display_cannot_speak_for_is_not_reported_as_refusing() {
        let frame = RecordedFrame::seeing(Verdict::Unsure);
        assert_eq!(raise(&frame), Ok(()));
        // Still kept on top: the raise carries on with everything that does not
        // rest on the answer.
        assert_eq!(
            frame.operations(),
            ["accept-focus false", "place", "show", "seen", "keep-on-top"]
        );

        let frame = RecordedFrame::seeing(Verdict::No);
        assert_eq!(
            raise(&frame),
            Err("the transcript box did not come up".into())
        );
    }

    #[test]
    fn the_frame_is_the_size_the_page_lays_out() {
        // review.css lays the box out at exactly these logical pixels, and the
        // window cannot be resized once it is up.
        assert_eq!(WIDTH, 620.0);
        assert_eq!(HEIGHT, 140.0);
        // 16 above and 14 below the three 26.1 pixel lines, and the hint line
        // ten under them: room for three lines and no room for a fourth, which
        // is what puts the fade at the bottom of a long take.
        let lines = HEIGHT - (16.0 + 14.0) - (18.0 + 10.0);
        assert!(lines >= 3.0 * 26.1, "the box cannot hold three lines");
        assert!(lines < 4.0 * 26.1, "the box holds a fourth line");
    }

    #[test]
    fn only_a_decision_about_words_raises_the_box() {
        assert!(RAISED.contains(&"review"));
        assert!(RAISED.contains(&"uncertain"));
        for bare in [
            "idle",
            "listening",
            "transcribing",
            "sent",
            "retained",
            "working",
            "speaking",
            "attention",
            "error",
            "disconnected",
        ] {
            assert!(!RAISED.contains(&bare), "{bare} raises the transcript box");
        }
    }

    #[test]
    fn the_box_sits_directly_above_the_pill() {
        let pill = pill::bottom_center(0, 0, 1920, 1080, 1.0);
        let position = above_pill(0, 0, 1920, 1080, 1.0);
        assert_eq!(position.x, (1920 - WIDTH as i32) / 2);
        assert_eq!(position.y + HEIGHT as i32 + GAP as i32, pill.y);
    }

    #[test]
    fn placement_follows_the_monitor_offset_and_scale() {
        let pill = pill::bottom_center(1920, -120, 2560, 1440, 2.0);
        let position = above_pill(1920, -120, 2560, 1440, 2.0);
        assert_eq!(position.x, 1920 + (2560 - (WIDTH * 2.0) as i32) / 2);
        assert_eq!(
            position.y + (HEIGHT * 2.0) as i32 + (GAP * 2.0) as i32,
            pill.y
        );
    }

    #[test]
    fn a_monitor_too_short_for_the_box_never_places_it_off_screen() {
        assert_eq!(above_pill(0, 0, 320, 120, 1.0), PhysicalPosition::new(0, 0));
        assert_eq!(
            above_pill(40, 60, 320, 120, 1.0),
            PhysicalPosition::new(40, 60)
        );
    }
}
