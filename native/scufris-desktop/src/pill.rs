//! The always-on-top bottom-center orb window.
//!
//! The pill is the orb: a frame with nothing in it but the dotted thought orb,
//! whose shape and accent carry the whole state. It must never cover the
//! desktop the user keeps working in. Positioning is a pure calculation so the
//! bottom-center placement is testable without a display.
//!
//! What is on screen is not the frame but a blob inside it: the window is cut
//! down to the ellipse inscribed in its own rectangle, so the corners are not
//! drawn and the desktop is visible around the orb. See [`crate::blob`] for
//! what that costs.
//!
//! Arriving is motion and nothing else. The window carries no alpha, so it
//! cannot fade, and its size is pinned by equal min and max hints, so it cannot
//! grow while it is up. What is left is where it is: the window rises into its
//! resting spot from below, carries a little past it, and settles back, while
//! the page pops the orb inside the frame and squashes it once as it lands.

use std::{
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    thread,
    time::Duration,
};

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

use crate::{
    app::{Hidden, Shown},
    blob,
    display::{self, Verdict},
};

/// Stable window label.
pub const LABEL: &str = "pill";

/// Pill width in logical pixels.
///
/// The window is exactly what `pill.css` lays out. The two cannot be adjusted
/// apart: a frame narrower than the layout clips the orb, and a wider one
/// leaves ground nobody asked for around it. Fifteen pixels either side of the
/// 160 pixel orb, which is what the mic-level scale needs at its loudest.
///
/// Two and a half times the 76 by 92 frame the orb study landed on. Every
/// distance in the frame carries the same factor, so the pill is the same
/// drawing at a size that can be read from across the desk.
pub const WIDTH: f64 = 190.0;

/// Pill height in logical pixels.
///
/// Taller than it is wide by one line: the listening timer's row is reserved in
/// every state rather than resized into, because equal min and max hints cannot
/// be changed while the window is up without re-applying them, and a frame that
/// resizes under the orb moves the orb.
///
/// The frame is not what is seen. [`crate::blob`] cuts the window down to the
/// ellipse inscribed in it, and the layout keeps the orb and the timer inside
/// that curve.
pub const HEIGHT: f64 = 230.0;

/// Gap between the pill and the bottom edge of the screen, in logical pixels.
///
/// A distance to the screen, not a part of the frame, so it did not grow with
/// the frame: the pill is bigger, and it stands off the edge by as much as it
/// ever did.
pub const BOTTOM_MARGIN: f64 = 72.0;

/// How far below its resting spot the pill starts an entrance, in logical
/// pixels. About a third of the frame: the travel is a proportion of the thing
/// that travels, so it grew with it and the entrance still takes as long.
const RISE: f64 = 70.0;

/// How many positions one entrance steps through.
const RISE_STEPS: u32 = 13;

/// How long one entrance step waits, which is about sixty a second. Thirteen
/// of them is a little over two tenths of a second: long enough to read as
/// arriving, short enough that nobody waits for it.
const RISE_STEP: Duration = Duration::from_millis(16);

/// How hard the entrance carries past its resting spot before settling back.
/// That recoil is what the eye reads as the pill landing rather than stopping.
const RECOIL: f64 = 1.5;

/// Event telling the page that the window has started rising, so the orb can
/// pop and squash inside the frame while it travels.
const ENTRANCE_EVENT: &str = "scufris://entrance";

/// The newest entrance. A tween whose generation has moved on stops stepping:
/// the window belongs to whoever placed it last.
static ENTRANCE: AtomicU64 = AtomicU64::new(0);

/// What the page reported about `prefers-reduced-motion`. It starts as though
/// the person asked for stillness: until the page has said otherwise, a window
/// that travels is a window that guessed.
static STILLNESS: AtomicBool = AtomicBool::new(true);

/// Whether the window has been cut down to its blob.
///
/// The cut is set on the X window and stays set, so it is asked for once. It
/// cannot be asked for before the window is realized, which is what showing it
/// does, and a cut that did not happen leaves this false so the next show tries
/// again.
static SHAPED: AtomicBool = AtomicBool::new(false);

/// The X window the display knows the pill by, once it has made one.
static WINDOW: AtomicU32 = AtomicU32::new(0);

/// Returns the physical bottom-center position of the pill on one monitor.
pub fn bottom_center(
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: u32,
    monitor_height: u32,
    scale: f64,
) -> PhysicalPosition<i32> {
    let width = (WIDTH * scale).round() as i32;
    let height = (HEIGHT * scale).round() as i32;
    let margin = (BOTTOM_MARGIN * scale).round() as i32;
    let x = monitor_x + ((monitor_width as i32 - width) / 2).max(0);
    let y = (monitor_y + monitor_height as i32 - height - margin).max(monitor_y);
    PhysicalPosition::new(x, y)
}

/// Returns the pill window, creating it hidden on first use.
pub fn ensure(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(LABEL) {
        return Ok(window);
    }
    // Opaque on purpose: the page fills the window with the panel, and the
    // desktop shows around it because the window is cut to a blob, not because
    // anything blends. Nothing here depends on a compositor.
    //
    // The size is pinned with min == max hints on a window left resizable,
    // because that is the one combination GTK honors: a non-resizable GTK
    // window ignores the requested size and grows to the webview's natural
    // 200 logical pixels, and GTK clamps size hints to that natural size for
    // it too. The equal hints keep the person from resizing the pill and are
    // also what makes a tiling window manager float it.
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Scufris")
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(WIDTH, HEIGHT)
        .max_inner_size(WIDTH, HEIGHT)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false)
        .focused(false)
        // The same reason as the box, from the other side. The toolkit's
        // one-shot restore would hand this window its right to the keyboard
        // back after its first paint, whenever that lands and whatever posture
        // the pill is in - including the handoff posture, whose whole point is
        // that the person's keys are their own. Built unfocusable, what this
        // window says about the keyboard is only ever what `open` last said.
        .focusable(false)
        .build()?;
    // The blob is cut from the event loop, because that is where the window
    // starts existing on the display: a window that has just been mapped
    // reports where the window manager put it, and every report is another
    // chance to cut a pill that is still a rectangle.
    let target = window.clone();
    window.on_window_event(move |event| {
        // A mask is measured in the frame's own pixels, so a frame that changed
        // size is wearing one that no longer fits and has to be cut again. The
        // size hints keep the person from causing this; a monitor whose scale
        // changes under the pill does it anyway.
        if matches!(
            event,
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }
        ) {
            SHAPED.store(false, Ordering::SeqCst);
        }
        shape(&target);
    });
    Ok(window)
}

/// Answers what the display knows the pill by, once it has made a window.
///
/// Nothing before the first show, which is also the first moment anything could
/// mistake this window for the person's.
pub fn known_window() -> Option<u32> {
    match WINDOW.load(Ordering::SeqCst) {
        0 => None,
        id => Some(id),
    }
}

/// Answers whether the keyboard has been forced onto the pill.
///
/// A dead end rather than a state. This window refuses the keyboard on every
/// show and has no key handlers at all, so a window manager that hands it focus
/// anyway has taken the person's keys as surely as one that focused nothing -
/// and unlike a window they moved to themselves, there is nobody to take them
/// from.
pub fn holds_the_keyboard(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL)
        .is_some_and(|window| display::keyboard(&window, &WINDOW) == Verdict::Yes)
}

/// The two window operations whose order is the keyboard contract.
///
/// A trait so the order they run in is testable without a display, for the same
/// reason the textbox has one: the order is the whole contract.
trait Opening {
    /// Says that the pill wants nothing from the keyboard.
    ///
    /// Applied straight to the window instead of being queued behind the event
    /// loop, which is the whole reason it can be used as an ordering: the
    /// answer is on the window before the show that follows it has even been
    /// asked for.
    fn refuse_keyboard(&self) -> Result<(), String>;
    /// Puts the pill on screen.
    fn show(&self) -> Result<(), String>;
}

impl Opening for WebviewWindow {
    fn refuse_keyboard(&self) -> Result<(), String> {
        self.set_focusable(false)
            .map_err(|error| format!("the pill would not refuse the keyboard: {error}"))
    }

    fn show(&self) -> Result<(), String> {
        WebviewWindow::show(self).map_err(|error| format!("the pill could not be shown: {error}"))
    }
}

/// Says that the pill wants nothing from the keyboard, then puts it on screen.
///
/// In that order, and every time. A window manager reads whether a window will
/// take the keyboard once, when it takes the window over, and it takes it over
/// when the window is mapped. The toolkit builds the pill refusing the keyboard
/// and hands the refusal back after the first draw, so a pill mapped without
/// this said again is one a window manager may hand the person's keys to - and
/// this window has no key handlers at all, so they would land nowhere.
///
/// The refusal is a warning rather than a refusal to come up. The pill is the
/// privacy indicator and an indicator that will not go up is worse than one
/// that went up saying the wrong thing about the keyboard, which the runtime
/// watches for anyway.
fn open(frame: &impl Opening) -> Result<(), String> {
    if let Err(error) = frame.refuse_keyboard() {
        tracing::warn!("{error}");
    }
    frame.show()
}

/// Cuts the window down to its blob, once there is a window to cut.
///
/// Nothing rests on this. A window that cannot be cut is the rectangle it was
/// built as, carrying the same orb it always did, which is what shipped before
/// there was a blob at all - so a failure is said once per attempt and the pill
/// goes up either way.
///
/// Showing a window is a message to the event loop rather than an act, so a
/// window that has just been asked to come up has usually not reached the
/// display yet - and at startup, before the loop has run at all, it certainly
/// has not. That is not a failure and is not said out loud: every show asks
/// again once the display has confirmed the window, and so does every event the
/// window reports.
fn shape(window: &WebviewWindow) {
    if SHAPED.load(Ordering::SeqCst) {
        return;
    }
    match cut(window) {
        Ok(true) => SHAPED.store(true, Ordering::SeqCst),
        Ok(false) => {}
        Err(reason) => tracing::warn!("the pill stays a rectangle: {reason}"),
    }
}

/// Asks the display to cut the pill's own window down to the blob, and answers
/// whether there was a window to cut.
fn cut(window: &WebviewWindow) -> Result<bool, String> {
    // No name means the display has not made the window yet, or is not an X
    // display at all. Neither is worth saying out loud: a window that cannot be
    // cut is the rectangle it was built as, which is what shipped before there
    // was a blob.
    let Some(id) = display::identify(window, &WINDOW) else {
        return Ok(false);
    };
    // The window's own size in physical pixels, which is what a mask is
    // measured in: on a scaled monitor the frame is bigger than the layout.
    let size = window
        .inner_size()
        .map_err(|error| format!("the pill would not say how big it is: {error}"))?;
    blob::cut(id, size.width, size.height).map(|()| true)
}

/// Records what the page reports about `prefers-reduced-motion`.
///
/// The window is the half of the entrance that travels, and only the page can
/// read the preference, so the page says once what it found and the host obeys
/// it. Until it has, the window stays still: the page reports as it loads,
/// which is long before the first activation, so nothing is lost by waiting
/// and a person who asked for no motion is not shown one anyway.
pub fn set_reduced_motion(reduced: bool) {
    STILLNESS.store(reduced, Ordering::SeqCst);
}

/// Answers whether the person asked for as little motion as possible.
fn still() -> bool {
    STILLNESS.load(Ordering::SeqCst)
}

/// Stops any entrance that is still stepping.
fn cancel_entrance() {
    ENTRANCE.fetch_add(1, Ordering::SeqCst);
}

/// Eases one entrance step, from 0 at the start to 1 at the resting spot.
///
/// Fast out of the start, past the resting spot, then back onto it. One is
/// exactly one, so the last step lands on the position [`bottom_center`]
/// chose rather than near it.
fn ease(progress: f64) -> f64 {
    let back = progress - 1.0;
    1.0 + (RECOIL + 1.0) * back * back * back + RECOIL * back * back
}

/// Returns where the pill belongs on its monitor, and that monitor's scale.
fn resting(window: &WebviewWindow) -> tauri::Result<Option<(PhysicalPosition<i32>, f64)>> {
    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return Ok(None);
    };
    let position = monitor.position();
    let size = monitor.size();
    let scale = monitor.scale_factor();
    Ok(Some((
        bottom_center(position.x, position.y, size.width, size.height, scale),
        scale,
    )))
}

/// Where one entrance starts and where it must end.
struct Rise {
    /// The resting spot the pill has to end on.
    home: PhysicalPosition<i32>,
    /// How far below it the pill starts, in physical pixels.
    offset: i32,
}

/// Places the pill, and says where it has to rise from when it is arriving.
///
/// Placement is not visibility. A monitor the window manager will not describe
/// leaves the pill where it already was, which is worse than the right place
/// and much better than not being up at all.
///
/// A pill that is already up is only put back where it belongs. The entrance is
/// for arriving: replaying it for a re-render would pull the person's eye back
/// to a pill they are already reading.
fn arrange(window: &WebviewWindow) -> tauri::Result<Option<Rise>> {
    // Whoever places the pill owns it from here. A repair that puts the window
    // back where it belongs must not then be dragged off it by a tween that is
    // still stepping.
    cancel_entrance();
    let Some((home, scale)) = resting(window)? else {
        return Ok(None);
    };
    // Anything but a confirmed "down" counts as up. A window that nothing can
    // say is down is not one to start moving around, and replaying an entrance
    // for a pill the person is already reading pulls their eye back to it.
    if display::up(window, &WINDOW) != Verdict::No || still() {
        window.set_position(home)?;
        return Ok(None);
    }
    let offset = (RISE * scale).round() as i32;
    window.set_position(PhysicalPosition::new(home.x, home.y + offset))?;
    Ok(Some(Rise { home, offset }))
}

/// Rises the pill into its resting spot, on a thread of its own.
///
/// Nothing waits for this. The keyboard is settled before the first step, so
/// the person can type into a pill that is still climbing, and a later
/// placement cancels it wherever it has got to.
fn glide(window: &WebviewWindow, rise: Rise) {
    let generation = ENTRANCE.fetch_add(1, Ordering::SeqCst) + 1;
    // The page pops the orb inside the frame while the frame travels. A page
    // that misses this is a pill that only rises, which is still an arrival.
    // Addressed to this window: the review window pops in place and has no
    // entrance to run from here.
    let _ = window.emit_to(LABEL, ENTRANCE_EVENT, ());
    let window = window.clone();
    thread::spawn(move || {
        for step in 1..=RISE_STEPS {
            thread::sleep(RISE_STEP);
            if ENTRANCE.load(Ordering::SeqCst) != generation {
                return;
            }
            let progress = f64::from(step) / f64::from(RISE_STEPS);
            let offset = (f64::from(rise.offset) * (1.0 - ease(progress))).round() as i32;
            if window
                .set_position(PhysicalPosition::new(rise.home.x, rise.home.y + offset))
                .is_err()
            {
                // A window that will not move is not worth chasing. It is up,
                // which is the part anything rests on.
                return;
            }
        }
    });
}

/// Places the pill and shows it without ever touching the keyboard.
///
/// The pill is the recording privacy indicator, so the caller is told what the
/// window is doing rather than what it was asked to do: an operation that
/// returns without error has still only asked. The display is what says
/// otherwise, and it is given time to: a request that has not been carried out
/// yet is not one that failed.
///
/// The entrance is asked for last and waited for never. Nothing waits on a pill
/// that is still climbing, because nothing is typed into it.
pub fn show(app: &AppHandle) -> Result<Shown, String> {
    let window = ensure(app).map_err(|error| format!("the pill window is missing: {error}"))?;
    // Placement is part of being seen: a pill left at a position nobody chose
    // may be off the screen the person is looking at.
    let (rise, doubt) = match arrange(&window) {
        Ok(rise) => (rise, String::new()),
        Err(error) => (None, format!("the pill could not be placed: {error}")),
    };
    let shown = reveal(&window, doubt);
    if shown.is_ok()
        && let Some(rise) = rise
    {
        glide(&window, rise);
    }
    shown
}

/// Shows the pill and reports what that achieved.
///
/// The keyboard is left exactly where the person put it. This window has no key
/// handlers and never has: refusing the keyboard before the window manager
/// takes the window over is what keeps it from swallowing the keys they are
/// typing into their own window.
fn reveal(window: &WebviewWindow, mut doubt: String) -> Result<Shown, String> {
    // Refusing the keyboard comes first, and every time. See `open`.
    open(window)?;
    let up = display::came_up(window, &WINDOW);
    // Cutting comes as soon as the display has a window to cut, and before any
    // of the answers below can return early: a pill that reported trouble and
    // stayed a rectangle would be the black box this shape exists to avoid.
    shape(window);
    if up == Verdict::No {
        return Err("the pill did not come up".into());
    }
    if let Err(error) = window.set_always_on_top(true) {
        // A pill that may be behind another window is not something to rest a
        // privacy indicator on: the person may be looking at what is in front.
        doubt = format!("the pill could not be kept on top: {error}");
    }
    if !doubt.is_empty() {
        return Ok(Shown::Doubtful(doubt));
    }
    match up {
        Verdict::Yes => Ok(Shown::Seen(None)),
        // Asked for and not yet carried out: not a failure, and nothing here
        // may be recorded as achieved.
        _ => Ok(Shown::Unsure(
            "nothing could say whether the pill came up".into(),
        )),
    }
}

/// Hides the pill without destroying it, and confirms that it is down.
///
/// An always-on-top pill that is still up must never be recorded as down: that
/// record is what would stop it ever being taken down again. A hide nothing
/// could speak for is not that, and is not reported as one.
pub fn hide(app: &AppHandle) -> Result<Hidden, String> {
    let Some(window) = app.get_webview_window(LABEL) else {
        return Ok(Hidden::Down);
    };
    // A pill on its way down has nothing left to rise into.
    cancel_entrance();
    window
        .hide()
        .map_err(|error| format!("the pill could not be hidden: {error}"))?;
    match display::went_down(&window, &WINDOW) {
        Verdict::Yes => Ok(Hidden::Down),
        Verdict::No => Err("the pill is still up".into()),
        Verdict::Unsure => Ok(Hidden::Unsure(
            "nothing could say whether the pill went down".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[derive(Default)]
    struct RecordedOpening {
        operations: RefCell<Vec<String>>,
        /// True when the window will not say what it does with the keyboard.
        mute: bool,
    }

    impl RecordedOpening {
        fn mute() -> Self {
            Self {
                mute: true,
                ..Self::default()
            }
        }

        fn operations(&self) -> Vec<String> {
            self.operations.borrow().clone()
        }
    }

    impl Opening for RecordedOpening {
        fn refuse_keyboard(&self) -> Result<(), String> {
            self.operations.borrow_mut().push("refuse-keyboard".into());
            if self.mute {
                return Err("the pill would not say".into());
            }
            Ok(())
        }

        fn show(&self) -> Result<(), String> {
            self.operations.borrow_mut().push("show".into());
            Ok(())
        }
    }

    /// The pill never holds the keyboard, in any phase. A window manager reads
    /// that when it takes the window over, which is at the mapping, so said
    /// after the show it is read one show too late and the pill swallows the
    /// keys the person is typing into their own window.
    #[test]
    fn the_pill_refuses_the_keys_before_it_is_shown() {
        let frame = RecordedOpening::default();
        assert_eq!(open(&frame), Ok(()));
        assert_eq!(frame.operations(), ["refuse-keyboard", "show"]);
    }

    #[test]
    fn a_pill_that_cannot_refuse_the_keyboard_is_still_put_up() {
        // Nothing rests on the pill but being seen, and being seen is worth
        // more than the refusal it could not say. The runtime watches for a
        // keyboard that landed here anyway.
        let frame = RecordedOpening::mute();
        assert_eq!(open(&frame), Ok(()));
        assert_eq!(frame.operations(), ["refuse-keyboard", "show"]);
    }

    #[test]
    fn the_frame_is_the_size_the_page_lays_out() {
        // pill.css lays the orb and its reserved timer row out at exactly these
        // logical pixels, and the window cannot be resized once it is up. A
        // frame and a layout that drift apart are a clipped orb or ground
        // nobody asked for around it, so the numbers are pinned here rather
        // than only derived from themselves.
        assert_eq!(WIDTH, 190.0);
        assert_eq!(HEIGHT, 230.0);
        // 15 + 160 + 15 across, and 20 + 160 + 5 + 35 + 10 down.
        assert_eq!(WIDTH, 15.0 + 160.0 + 15.0);
        assert_eq!(HEIGHT, 20.0 + 160.0 + 5.0 + 35.0 + 10.0);
    }

    #[test]
    fn the_orb_stays_inside_the_blob_at_its_loudest() {
        // The window is cut to the ellipse inscribed in the frame, so a layout
        // that fits the rectangle is not enough: the orb has to fit the curve,
        // at the 1.12 scale pill.css gives it when the microphone is loudest.
        let radius_x = WIDTH / 2.0;
        let radius_y = HEIGHT / 2.0;
        // Measured from the engine's own frames rather than assumed: across
        // every state the furthest dot, its radius included, lands within 0.44
        // of the box it is drawn in.
        let orb = 160.0 * 0.44 * 1.12;
        // The orb box is 20 from the top of a 160 tall square, so its middle
        // sits above the middle of the frame.
        let lift = radius_y - (20.0 + 80.0);
        for step in 0..=90 {
            let angle = f64::from(step) * std::f64::consts::PI / 180.0;
            let x = orb * angle.cos() / radius_x;
            let y = (orb * angle.sin() + lift) / radius_y;
            assert!(
                x * x + y * y < 1.0,
                "the orb reaches outside the blob at {step} degrees"
            );
        }
    }

    #[test]
    fn the_entrance_carries_past_the_resting_spot_and_still_lands_on_it() {
        assert_eq!(ease(0.0), 0.0);
        // Exactly one, not nearly one: the last step is what leaves the pill
        // where the placement calculation put it.
        assert_eq!(ease(1.0), 1.0);
        let furthest = (1..RISE_STEPS)
            .map(|step| ease(f64::from(step) / f64::from(RISE_STEPS)))
            .fold(0.0_f64, f64::max);
        assert!(furthest > 1.0, "the entrance never overshoots: {furthest}");
        assert!(
            furthest < 1.15,
            "the entrance overshoots far enough to read as a bounce: {furthest}"
        );
    }

    #[test]
    fn the_timer_row_fits_across_the_bottom_of_the_blob() {
        // The row is 35 tall under the orb, and the 20 pixel digits sit in the
        // middle of it: the widest a take can read is "59:59", which is five
        // characters of a monospace face at about 0.6 of its size.
        let radius_x = WIDTH / 2.0;
        let radius_y = HEIGHT / 2.0;
        let row = 20.0 + 160.0 + 5.0;
        let baseline = row + 35.0 / 2.0 + 20.0 / 2.0;
        let across = 2.0 * radius_x * (1.0 - ((baseline - radius_y) / radius_y).powi(2)).sqrt();
        assert!(
            across > 5.0 * 20.0 * 0.6,
            "the blob is only {across} across where the timer is"
        );
    }

    #[test]
    fn the_pill_sits_bottom_center_above_the_screen_edge() {
        let position = bottom_center(0, 0, 1920, 1080, 1.0);
        assert_eq!(position.x, (1920 - WIDTH as i32) / 2);
        assert_eq!(position.y, 1080 - HEIGHT as i32 - BOTTOM_MARGIN as i32);
    }

    #[test]
    fn placement_follows_the_monitor_offset_and_scale() {
        let position = bottom_center(1920, -120, 2560, 1440, 2.0);
        assert_eq!(position.x, 1920 + (2560 - (WIDTH * 2.0) as i32) / 2);
        assert_eq!(
            position.y,
            -120 + 1440 - (HEIGHT * 2.0) as i32 - (BOTTOM_MARGIN * 2.0) as i32
        );
    }

    #[test]
    fn a_monitor_smaller_than_the_pill_never_places_it_off_screen() {
        let position = bottom_center(0, 0, 60, 40, 1.0);
        assert_eq!(position.x, 0);
        assert_eq!(position.y, 0);
        assert_eq!(
            bottom_center(40, 60, 60, 40, 1.0),
            PhysicalPosition::new(40, 60)
        );
    }
}
