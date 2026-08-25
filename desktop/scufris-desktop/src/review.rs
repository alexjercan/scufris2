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

use tauri::{
    AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::pill;

/// Stable window label. `capabilities/default.json` names it too: a window the
/// capability does not cover cannot listen for the presentation it renders.
pub const LABEL: &str = "review";

/// Box width in logical pixels.
///
/// The window is exactly what `review.css` lays out, for the same reason the
/// pill window is: without a compositor there is no alpha to hide a margin in.
pub const WIDTH: f64 = 460.0;

/// Box height in logical pixels.
///
/// Three lines of transcript, the caret, and the hint line. A take longer than
/// that scrolls under a fade rather than resizing the window: equal min and max
/// hints cannot be changed while the window is up without re-applying them.
pub const HEIGHT: f64 = 108.0;

/// Gap between the bottom of the box and the top of the orb window, in logical
/// pixels.
const GAP: f64 = 12.0;

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

/// Shows the box above the orb without ever touching the keyboard.
fn show(app: &AppHandle) -> Result<(), String> {
    let window = ensure(app).map_err(|error| format!("the transcript box is missing: {error}"))?;
    // Placement is part of being read: the orb window is bottom-center on
    // whichever monitor the window manager reports, and the box belongs
    // directly above it.
    if let Err(error) = place(&window) {
        tracing::warn!("the transcript box could not be placed: {error}");
    }
    window
        .show()
        .map_err(|error| format!("the transcript box could not be shown: {error}"))?;
    match window.is_visible() {
        Ok(true) => {}
        Ok(false) => return Err("the transcript box did not come up".into()),
        Err(error) => {
            return Err(format!(
                "the transcript box could not confirm that it is up: {error}"
            ));
        }
    }
    if let Err(error) = window.set_always_on_top(true) {
        tracing::warn!("the transcript box could not be kept on top: {error}");
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
    match window.is_visible() {
        Ok(false) => Ok(()),
        Ok(true) => Err("the transcript box is still up".into()),
        Err(error) => Err(format!(
            "the transcript box could not confirm that it is down: {error}"
        )),
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
    use super::*;

    #[test]
    fn the_frame_is_the_size_the_page_lays_out() {
        // review.css lays the box out at exactly these logical pixels, and the
        // window cannot be resized once it is up.
        assert_eq!(WIDTH, 460.0);
        assert_eq!(HEIGHT, 108.0);
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
