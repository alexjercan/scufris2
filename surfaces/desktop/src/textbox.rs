//! The focused transcript window above the orb.
//!
//! The orb says what the companion is doing; it cannot say what the words are.
//! So the words get a window of their own, and only while there is a decision
//! to make about them: it rises above the orb for a draft and for a transcript
//! whose fate is unknown, and leaves with the decision.
//!
//! It is the one window here that holds the keyboard, and it holds it only for
//! as long as there are words in it. Enter sends, Escape discards, Ctrl+C
//! copies, and every ordinary editing key works because they arrive in a
//! focused window rather than being rescued from outside one. The pill never
//! takes the keyboard at all: an indicator that took the person's keys to show
//! them a state would be a worse trade than any state is worth.
//!
//! Claiming the keyboard is said again before every raise, not once at build
//! time. A window manager that unmanages a hidden window and manages it again
//! on the next show - i3 does - reads the window's hints when it is mapped, so
//! the claim has to be on the window before it goes up rather than after.

use std::sync::atomic::{AtomicU32, Ordering};

use tauri::{
    AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::{
    app::Shown,
    display::{self, Verdict},
    pill,
};

/// Stable window label. `capabilities/default.json` names it too: a window the
/// capability does not cover cannot listen for the presentation it renders.
pub const LABEL: &str = "textbox";

/// Box width in logical pixels.
///
/// The window is exactly what `textbox.css` lays out, for the same reason the
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
/// inside the field rather than resizing the window: equal min and max hints
/// cannot be changed while the window is up without re-applying them.
pub const HEIGHT: f64 = 140.0;

/// Gap between the bottom of the box and the top of the orb window, in logical
/// pixels. Twice what it was, because what it separates the box from is now
/// two and a half times the size.
///
/// Public because the widget shelf stands above the box and is measured from
/// here: the shelf and the box share this band, and a gap the shelf guessed at
/// would put a panel over the words the person has to read.
pub const GAP: f64 = 24.0;

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

/// Returns the textbox window, creating it hidden on first use.
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
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("textbox.html".into()))
        .title("Scufris transcript")
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(WIDTH, HEIGHT)
        .max_inner_size(WIDTH, HEIGHT)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        // Built down and built refusing the keyboard, and both for the same
        // reason: the window exists from the first start and is empty until
        // there are words in it. What it says about the keyboard from then on
        // is only ever what `raise` and `hide` last said.
        .visible(false)
        .focused(false)
        .focusable(false)
        .build()
}

/// Answers what the display knows the box by, once it has made a window.
///
/// Nothing before the first raise, which is also the first moment a window
/// manager could hand this window anything.
pub fn known_window() -> Option<u32> {
    match WINDOW.load(Ordering::SeqCst) {
        0 => None,
        id => Some(id),
    }
}

/// Answers whether the box is holding the keyboard right now.
///
/// The display is asked rather than the window: the window answers with what
/// the toolkit last noticed, and a phase whose keys have nowhere else to go
/// cannot rest on that.
pub fn focused(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL)
        .is_some_and(|window| display::keyboard(&window, &WINDOW) == Verdict::Yes)
}

/// Answers whether the box is on screen, as far as the toolkit knows.
///
/// Only used to tell a box that is already down from one that is up, so that
/// taking it down twice does not give the keyboard back twice. A window that
/// will not say counts as down, which is the answer that does nothing.
pub fn up(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

/// The window operations one raise needs.
///
/// A trait so the order they run in is testable without a display. The order is
/// the whole of the keyboard contract: claiming the keyboard has to reach the
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
    /// Asks for the keyboard.
    fn take_keyboard(&self) -> Result<(), String>;
    /// Answers what the display says about the box holding the keyboard.
    fn holds_keyboard(&self) -> Verdict;
}

impl Frame for WebviewWindow {
    fn accept_focus(&self, accept: bool) -> Result<(), String> {
        self.set_focusable(accept)
            .map_err(|error| format!("the textbox would not take the keyboard: {error}"))
    }

    fn place(&self) -> Result<(), String> {
        place(self).map_err(|error| format!("the textbox could not be placed: {error}"))
    }

    fn show(&self) -> Result<(), String> {
        WebviewWindow::show(self)
            .map_err(|error| format!("the textbox could not be shown: {error}"))
    }

    fn seen(&self) -> Verdict {
        display::came_up(self, &WINDOW)
    }

    fn keep_on_top(&self) -> Result<(), String> {
        self.set_always_on_top(true)
            .map_err(|error| format!("the textbox could not be kept on top: {error}"))
    }

    fn take_keyboard(&self) -> Result<(), String> {
        self.set_focus()
            .map_err(|error| format!("the textbox could not take the keyboard: {error}"))
    }

    fn holds_keyboard(&self) -> Verdict {
        display::took_the_keyboard(self, &WINDOW)
    }
}

/// Puts the box over the orb with the keyboard, and reports what that achieved.
///
/// The caller is told what the window is doing rather than what it was asked to
/// do: a request that has not been carried out yet is not one that failed, and
/// a box that is up without the keyboard is a box the person cannot answer.
pub fn show(app: &AppHandle) -> Result<Shown, String> {
    let window = ensure(app).map_err(|error| format!("the textbox is missing: {error}"))?;
    raise(&window)
}

/// Puts one box on screen, in the only order that gets it the keys.
fn raise(frame: &impl Frame) -> Result<Shown, String> {
    // First, and every time. See the module note: a window manager reads what
    // a window says about the keyboard when it maps it, so a claim made after
    // the show is a claim made for the next one. A box that will not take the
    // keyboard is worse than no box at all - the words would be unanswerable
    // and the person's keys would land in it anyway - so this one refuses
    // rather than reports.
    frame.accept_focus(true)?;
    // Placement is part of being read: the orb window is bottom-center on
    // whichever monitor the window manager reports, and the box belongs
    // directly above it.
    if let Err(error) = frame.place() {
        tracing::warn!("{error}");
    }
    frame.show()?;
    match frame.seen() {
        Verdict::Yes => {}
        // Asked for and not yet carried out. A request the display has not
        // reached is not a box that refused to come up, and saying so would be
        // the false verdict this reporting exists to avoid.
        Verdict::Unsure => {
            let _ = frame.take_keyboard();
            return Ok(Shown::Unsure(
                "nothing could say whether the textbox came up".into(),
            ));
        }
        Verdict::No => return Err("the textbox did not come up".into()),
    }
    if let Err(error) = frame.keep_on_top() {
        // A box that may be behind another window is one the person may not be
        // reading, and the words in it are what they have to answer.
        return Ok(Shown::Doubtful(error));
    }
    if let Err(error) = frame.take_keyboard() {
        return Ok(Shown::Seen(Some(error)));
    }
    // Asking for the keyboard is not holding it. A window manager may accept
    // the request and hand the keyboard elsewhere, or later, or never. The
    // runtime asks again on its own and says so once if it runs out of asking,
    // so nothing is said here.
    match frame.holds_keyboard() {
        Verdict::Yes => Ok(Shown::Ready),
        Verdict::No | Verdict::Unsure => Ok(Shown::Seen(None)),
    }
}

/// Takes the box off screen and gives the keyboard up with it.
///
/// An always-on-top box left over a decision that is finished is worse than one
/// that never came up, so a box that will not go down is reported. Refusing the
/// keyboard comes first: a hidden window that still says it wants keys is one a
/// window manager can hand them to on the next map.
pub fn hide(app: &AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window(LABEL) else {
        return Ok(());
    };
    if let Err(error) = window.set_focusable(false) {
        tracing::warn!("the textbox would not give the keyboard up: {error}");
    }
    window
        .hide()
        .map_err(|error| format!("the textbox could not be hidden: {error}"))?;
    match display::went_down(&window, &WINDOW) {
        // Down, or on its way and nobody in a position to watch it go.
        Verdict::Yes | Verdict::Unsure => Ok(()),
        Verdict::No => Err("the textbox is still up".into()),
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
        /// What the display says about the keyboard after it was asked for.
        holds: Option<Verdict>,
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

        fn holding(holds: Verdict) -> Self {
            Self {
                holds: Some(holds),
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
        fn take_keyboard(&self) -> Result<(), String> {
            self.attempt("take-keyboard")
        }
        fn holds_keyboard(&self) -> Verdict {
            let _ = self.attempt("holds-keyboard");
            self.holds.unwrap_or(Verdict::Yes)
        }
    }

    /// Every raise claims the keyboard, and claims it before the box is on
    /// screen, because a window manager reads that claim when the window is
    /// mapped. Saying it once at build time is not enough: the window is built
    /// unfocusable and every hide says so again, so an unsaid claim is a box
    /// the person cannot type in.
    #[test]
    fn every_raise_claims_the_keyboard_before_the_box_is_on_screen() {
        let frame = RecordedFrame::default();
        assert_eq!(raise(&frame), Ok(Shown::Ready));
        assert_eq!(raise(&frame), Ok(Shown::Ready));
        assert_eq!(
            frame.operations(),
            [
                "accept-focus true",
                "place",
                "show",
                "seen",
                "keep-on-top",
                "take-keyboard",
                "holds-keyboard",
                "accept-focus true",
                "place",
                "show",
                "seen",
                "keep-on-top",
                "take-keyboard",
                "holds-keyboard",
            ]
        );
    }

    /// Placement is chrome and is reported. The keyboard is not: a box that
    /// cannot take it is a box holding words nobody can answer, so it does not
    /// come up at all.
    #[test]
    fn a_box_that_cannot_claim_the_keyboard_never_comes_up() {
        let frame = RecordedFrame::refusing("accept-focus", "the box would not take the keyboard");
        assert_eq!(
            raise(&frame),
            Err("the box would not take the keyboard".into())
        );
        assert_eq!(frame.operations(), ["accept-focus true"]);

        let frame = RecordedFrame::refusing("place", "the box could not be placed");
        assert_eq!(raise(&frame), Ok(Shown::Ready));
        assert!(frame.operations().iter().any(|step| step == "show"));
    }

    /// A box that has been asked to come up and has not reached the display yet
    /// is not a box that refused to. Reporting one as the other is what put
    /// "the transcript box did not come up" in the log of every start.
    #[test]
    fn a_box_the_display_cannot_speak_for_is_not_reported_as_refusing() {
        let frame = RecordedFrame::seeing(Verdict::Unsure);
        assert!(matches!(raise(&frame), Ok(Shown::Unsure(_))));
        // The keyboard is still asked for - the request is worth making either
        // way - and nothing is recorded as achieved.
        assert!(
            frame
                .operations()
                .iter()
                .any(|step| step == "take-keyboard")
        );

        let frame = RecordedFrame::seeing(Verdict::No);
        assert_eq!(raise(&frame), Err("the textbox did not come up".into()));
    }

    /// Up is not the same as answerable. A box the window manager left without
    /// the keys is reported as seen, and the runtime is what asks again.
    #[test]
    fn a_box_the_window_manager_left_without_the_keys_is_only_seen() {
        let frame = RecordedFrame::holding(Verdict::No);
        assert_eq!(raise(&frame), Ok(Shown::Seen(None)));

        let frame = RecordedFrame::refusing("keep-on-top", "the box could not be kept on top");
        assert_eq!(
            raise(&frame),
            Ok(Shown::Doubtful("the box could not be kept on top".into()))
        );
    }

    #[test]
    fn the_frame_is_the_size_the_page_lays_out() {
        // textbox.css lays the box out at exactly these logical pixels, and the
        // window cannot be resized once it is up.
        assert_eq!(WIDTH, 620.0);
        assert_eq!(HEIGHT, 140.0);
        // 16 above and 14 below the three 26.1 pixel lines, and the hint line
        // ten under them: room for three lines and no room for a fourth, which
        // is what makes a long take scroll inside the field.
        let lines = HEIGHT - (16.0 + 14.0) - (18.0 + 10.0);
        assert!(lines >= 3.0 * 26.1, "the box cannot hold three lines");
        assert!(lines < 4.0 * 26.1, "the box holds a fourth line");
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
