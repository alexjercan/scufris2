//! The always-on-top bottom-center orb window.
//!
//! The pill is the orb: a small square frame with nothing in it but the dotted
//! thought orb, whose shape and accent carry the whole state. It must never
//! cover the desktop the user keeps working in. Positioning is a pure
//! calculation so the bottom-center placement is testable without a display.
//!
//! Arriving is motion and nothing else. The window carries no alpha, so it
//! cannot fade, and its size is pinned by equal min and max hints, so it cannot
//! grow while it is up. What is left is where it is: the window rises into its
//! resting spot from below, carries a little past it, and settles back, while
//! the page pops the orb inside the frame and squashes it once as it lands.

use std::{
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    thread,
    time::Duration,
};

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::app::Shown;

/// Stable window label.
pub const LABEL: &str = "pill";

/// Pill width in logical pixels.
///
/// The window is exactly what `pill.css` lays out. The two cannot be adjusted
/// apart: a frame narrower than the layout clips the orb, and a wider one
/// leaves a black margin around it. Six pixels either side of the 64 pixel orb,
/// which is what the mic-level scale needs at its loudest.
pub const WIDTH: f64 = 76.0;

/// Pill height in logical pixels.
///
/// The window is exactly the opaque panel. A window with margins around the
/// panel needs per-pixel alpha, which bare X11 without a compositor cannot
/// do: the alpha is discarded and the margins render as black.
///
/// Taller than it is wide by one line: the listening timer's row is reserved in
/// every state rather than resized into, because equal min and max hints cannot
/// be changed while the window is up without re-applying them, and a frame that
/// resizes under the orb moves the orb.
pub const HEIGHT: f64 = 92.0;

/// Gap between the pill and the bottom edge of the screen, in logical pixels.
pub const BOTTOM_MARGIN: f64 = 72.0;

/// How far below its resting spot the pill starts an entrance, in logical
/// pixels. About a third of the frame: the smaller the window, the shorter the
/// travel that still reads as arriving rather than flying in.
const RISE: f64 = 28.0;

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
    // Opaque on purpose: the page fills the window with the panel, so nothing
    // depends on compositing being available.
    //
    // The size is pinned with min == max hints on a window left resizable,
    // because that is the one combination GTK honors: a non-resizable GTK
    // window ignores the requested size and grows to the webview's natural
    // 200 logical pixels, and GTK clamps size hints to that natural size for
    // it too. The equal hints keep the person from resizing the pill and are
    // also what makes a tiling window manager float it.
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
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
        .build()
}

/// Answers whether the pill window currently holds the keyboard.
pub fn focused(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL)
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false)
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
    // Anything but a confirmed "down" counts as up. A window that cannot say
    // where it is is not one to start moving around.
    if !matches!(window.is_visible(), Ok(false)) || still() {
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

/// Places the pill, shows it, focuses it, and reports what that achieved.
///
/// The pill is the recording privacy indicator, so the caller is told what the
/// window is doing rather than what it was asked to do: an operation that
/// returns without error has still only asked.
///
/// The entrance is asked for last and waited for never: a pill that took the
/// keyboard took it before the first step, and the person may type into one
/// that is still climbing.
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

/// Shows the pill, asks for the keyboard, and reports what that achieved.
///
/// The pill is the recording privacy indicator, so the caller is told what the
/// window is doing rather than what it was asked to do: an operation that
/// returns without error has still only asked. What the window says about
/// itself afterwards is the answer, and a window that cannot answer counts as
/// not having done it.
fn reveal(window: &WebviewWindow, mut doubt: String) -> Result<Shown, String> {
    window
        .show()
        .map_err(|error| format!("the pill could not be shown: {error}"))?;
    match window.is_visible() {
        Ok(true) => {}
        Ok(false) => return Err("the pill did not come up".into()),
        Err(error) => return Err(format!("the pill could not confirm that it is up: {error}")),
    }
    if let Err(error) = window.set_always_on_top(true) {
        // A pill that may be behind another window is not something to rest a
        // privacy indicator on: the person may be looking at what is in front.
        doubt = format!("the pill could not be kept on top: {error}");
    }
    if !doubt.is_empty() {
        return Ok(Shown::Doubtful(doubt));
    }
    if let Err(error) = window.set_focus() {
        return Ok(Shown::Seen(format!(
            "the pill could not take the keyboard: {error}"
        )));
    }
    // Asking for the keyboard is not holding it. A window manager may accept
    // the request and hand the keyboard elsewhere, or later, or never.
    match window.is_focused() {
        Ok(true) => Ok(Shown::Ready),
        Ok(false) => Ok(Shown::Seen("the pill did not take the keyboard".into())),
        Err(error) => Ok(Shown::Seen(format!(
            "the pill could not confirm that it has the keyboard: {error}"
        ))),
    }
}

/// Places the pill and shows it without touching the keyboard.
///
/// The passive pill only reports the turn it started, so being up is all it
/// owes: placement or always-on-top falling short is worth a report, not a
/// refusal, because nothing that rests on being seen rests on this posture.
pub fn show_passive(app: &AppHandle) -> Result<(), String> {
    let window = ensure(app).map_err(|error| format!("the pill window is missing: {error}"))?;
    let rise = arrange(&window).unwrap_or_else(|error| {
        tracing::warn!("the passive pill could not be placed: {error}");
        None
    });
    window
        .show()
        .map_err(|error| format!("the pill could not be shown: {error}"))?;
    match window.is_visible() {
        Ok(true) => {}
        Ok(false) => return Err("the pill did not come up".into()),
        Err(error) => return Err(format!("the pill could not confirm that it is up: {error}")),
    }
    if let Err(error) = window.set_always_on_top(true) {
        tracing::warn!("the passive pill could not be kept on top: {error}");
    }
    // The same arrival, with the keyboard left exactly where the person put it.
    if let Some(rise) = rise {
        glide(&window, rise);
    }
    Ok(())
}

/// Hides the pill without destroying it, and confirms that it is down.
///
/// An always-on-top pill that is still up must never be recorded as down: that
/// record is what would stop it ever being taken down again.
pub fn hide(app: &AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window(LABEL) else {
        return Ok(());
    };
    // A pill on its way down has nothing left to rise into.
    cancel_entrance();
    window
        .hide()
        .map_err(|error| format!("the pill could not be hidden: {error}"))?;
    match window.is_visible() {
        Ok(false) => Ok(()),
        Ok(true) => Err("the pill is still up".into()),
        Err(error) => Err(format!(
            "the pill could not confirm that it is down: {error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_is_the_size_the_page_lays_out() {
        // pill.css lays the orb and its reserved timer row out at exactly these
        // logical pixels, and the window cannot be resized once it is up. A
        // frame and a layout that drift apart are a clipped orb or a black
        // margin around it, so the numbers are pinned here rather than only
        // derived from themselves.
        assert_eq!(WIDTH, 76.0);
        assert_eq!(HEIGHT, 92.0);
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
