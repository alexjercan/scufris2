//! The blob the pill window is cut down to.
//!
//! There is no compositor on this desktop, so per-pixel alpha is discarded: a
//! window is opaque everywhere it exists. What X can still do is make part of a
//! window not exist. The Shape extension replaces a window's bounding region
//! with a mask, and outside that mask the window is neither drawn nor clicked -
//! the desktop shows through for real, whatever happens to be under there,
//! which is what separates this from painting a picture of the wallpaper.
//!
//! The mask is the ellipse inscribed in the frame, one rectangle per scanline.
//! The geometry is computed here, in physical pixels and without a display, so
//! the only part that needs an X server is the request that carries it.
//!
//! What it costs: the mask is one bit per pixel, so the edge is a hard
//! staircase with no way to anti-alias it. Against a dark desktop the steps are
//! near-black on dark and hard to pick out; against a bright wallpaper they are
//! visible for what they are.

use std::sync::OnceLock;

use x11rb::{
    protocol::{
        shape::{ConnectionExt as _, SK, SO},
        xproto::{ClipOrdering, Rectangle},
    },
    rust_connection::RustConnection,
};

/// The display, once, for every window that gets cut.
///
/// A shape is not a frame: it is set on the window and stays set, so this
/// carries no state beyond the connection. Without a display, or without the
/// Shape extension, there is nothing here and the window stays the rectangle
/// it was built as.
static DISPLAY: OnceLock<Option<Session>> = OnceLock::new();

struct Session {
    connection: RustConnection,
}

/// Cuts one window down to the ellipse inscribed in its frame.
///
/// The frame is in physical pixels, which is what the mask is measured in: a
/// window on a scaled monitor is bigger than the layout that describes it.
pub fn cut(window: u32, width: u32, height: u32) -> Result<(), String> {
    let Some(session) = DISPLAY.get_or_init(Session::open) else {
        return Err("the display cannot shape windows".into());
    };
    session.cut(window, &ellipse(width, height))
}

/// Returns the ellipse inscribed in one frame, as one rectangle per scanline.
///
/// One rectangle per row rather than a drawn bitmap: the request carries the
/// rows themselves, so no pixmap has to be made, filled and freed, and the
/// geometry stays a calculation that can be checked without a display.
pub fn ellipse(width: u32, height: u32) -> Vec<Rectangle> {
    let (Ok(frame_width), Ok(frame_height)) = (i16::try_from(width), i16::try_from(height)) else {
        return Vec::new();
    };
    if frame_width <= 0 || frame_height <= 0 {
        return Vec::new();
    }
    let radius_x = f64::from(frame_width) / 2.0;
    let radius_y = f64::from(frame_height) / 2.0;
    let mut rows = Vec::new();
    for row in 0..frame_height {
        // The row's own middle, so a row is in when its center is in rather
        // than when its top edge grazes the curve.
        let offset = (f64::from(row) + 0.5 - radius_y) / radius_y;
        let span = 1.0 - offset * offset;
        if span <= 0.0 {
            continue;
        }
        let half = radius_x * span.sqrt();
        let left = (radius_x - half).round().clamp(0.0, f64::from(frame_width));
        let right = (radius_x + half).round().clamp(0.0, f64::from(frame_width));
        if right - left < 1.0 {
            continue;
        }
        rows.push(Rectangle {
            x: left as i16,
            y: row,
            width: (right - left) as u16,
            height: 1,
        });
    }
    rows
}

impl Session {
    fn open() -> Option<Self> {
        let (connection, _) = x11rb::connect(None).ok()?;
        // Asked rather than assumed: a server without the extension answers
        // every shape request with an unknown-opcode error, which arrives long
        // after the request that caused it.
        connection.shape_query_version().ok()?.reply().ok()?;
        Some(Self { connection })
    }

    fn cut(&self, window: u32, rectangles: &[Rectangle]) -> Result<(), String> {
        if rectangles.is_empty() {
            // A mask with nothing in it is a window nobody can see. The
            // rectangle it already is beats that.
            return Err("the blob has no area".into());
        }
        // Unsorted: the rows are banded, but saying so buys one sort of a
        // couple of hundred rectangles, once, and costs an X error if the
        // description is ever wrong.
        //
        // Only the bounding shape is set. The input shape follows it while it
        // has never been set itself, so the cut-away corners stop taking
        // clicks as well as stop being drawn, which is what a window that is
        // not there should do.
        self.connection
            .shape_rectangles(
                SO::SET,
                SK::BOUNDING,
                ClipOrdering::UNSORTED,
                window,
                0,
                0,
                rectangles,
            )
            .map_err(|error| format!("the blob could not be sent: {error}"))?
            .check()
            .map_err(|error| format!("the display refused the blob: {error}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pill;

    /// The pill frame at the scale the study describes it in.
    fn frame() -> (u32, u32) {
        (pill::WIDTH as u32, pill::HEIGHT as u32)
    }

    #[test]
    fn the_mask_covers_the_ellipse_inscribed_in_the_frame() {
        let (width, height) = frame();
        let rows = ellipse(width, height);
        let covered: f64 = rows.iter().map(|row| f64::from(row.width)).sum();
        let area = std::f64::consts::PI * f64::from(width as u16) * f64::from(height as u16) / 4.0;
        // A scanline mask is the ellipse to within a row either side, so the
        // area is the honest check that it is an ellipse and not, say, the
        // rectangle it was cut from.
        assert!(
            (covered - area).abs() / area < 0.01,
            "the mask covers {covered} where the ellipse is {area}"
        );
    }

    #[test]
    fn nothing_in_the_mask_leaves_the_frame() {
        let (width, height) = frame();
        for row in ellipse(width, height) {
            assert!(row.x >= 0, "a row starts at {}", row.x);
            assert!(
                i64::from(row.x) + i64::from(row.width) <= i64::from(width),
                "a row ends past the frame"
            );
            assert!(row.y >= 0 && i64::from(row.y) < i64::from(height));
            assert_eq!(row.height, 1);
        }
    }

    #[test]
    fn the_corners_are_the_part_that_is_cut_away() {
        let (width, height) = frame();
        let rows = ellipse(width, height);
        let covers = |x: i16, y: i16| {
            rows.iter()
                .any(|row| row.y == y && row.x <= x && x < row.x + row.width as i16)
        };
        let last_x = width as i16 - 1;
        let last_y = height as i16 - 1;
        for (x, y) in [(0, 0), (last_x, 0), (0, last_y), (last_x, last_y)] {
            assert!(!covers(x, y), "the corner {x},{y} is still there");
        }
        // The middle of every edge is exactly what an inscribed ellipse keeps.
        assert!(covers(width as i16 / 2, 0));
        assert!(covers(width as i16 / 2, last_y));
        assert!(covers(0, height as i16 / 2));
        assert!(covers(last_x, height as i16 / 2));
    }

    #[test]
    fn the_mask_is_one_piece_and_the_same_on_both_sides() {
        let (width, height) = frame();
        let rows = ellipse(width, height);
        assert!(!rows.is_empty());
        for pair in rows.windows(2) {
            let [above, below] = pair else { unreachable!() };
            assert_eq!(
                below.y,
                above.y + 1,
                "the mask has a hole at row {}",
                above.y + 1
            );
        }
        // Rows mirror about the middle, so nothing leans and the widest row is
        // the one across the middle.
        let first = rows.first().expect("a mask row");
        let last = rows.last().expect("a mask row");
        assert_eq!(first.width, last.width);
        assert_eq!(first.x, last.x);
        assert_eq!(
            i32::from(first.y) + i32::from(last.y),
            i32::from(height as u16) - 1
        );
        let widest = rows.iter().map(|row| row.width).max().expect("a mask row");
        assert_eq!(u32::from(widest), width);
    }

    #[test]
    fn a_frame_with_no_area_is_left_alone() {
        assert!(ellipse(0, 0).is_empty());
        assert!(ellipse(190, 0).is_empty());
        assert!(ellipse(0, 230).is_empty());
        // Bigger than X can describe: the window keeps its corners rather than
        // being cut to a mask that does not mean what it says.
        assert!(ellipse(70000, 70000).is_empty());
    }

    #[test]
    fn a_scaled_monitor_gets_a_scaled_mask() {
        let (width, height) = frame();
        let rows = ellipse(width * 2, height * 2);
        let widest = rows.iter().map(|row| row.width).max().expect("a mask row");
        assert_eq!(u32::from(widest), width * 2);
        assert_eq!(
            rows.len(),
            rows.last().expect("a mask row").y as usize + 1 - rows[0].y as usize
        );
    }
}
