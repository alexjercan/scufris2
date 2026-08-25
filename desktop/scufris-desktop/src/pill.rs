//! The always-on-top bottom-center pill window.
//!
//! The pill is deliberately small and undecorated: it must never cover the
//! desktop the user keeps working in. Positioning is a pure calculation so the
//! bottom-center placement is testable without a display.

use tauri::{
    AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::app::Shown;

/// Stable window label.
pub const LABEL: &str = "pill";

/// Pill width in logical pixels.
pub const WIDTH: f64 = 560.0;

/// Pill height in logical pixels.
pub const HEIGHT: f64 = 96.0;

/// Gap between the pill and the bottom edge of the screen, in logical pixels.
pub const BOTTOM_MARGIN: f64 = 72.0;

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
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Scufris")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false)
        .focused(false)
        .build()
}

/// Answers whether the pill window is currently on screen.
pub fn visible(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

/// Places the pill at the bottom center of its monitor.
///
/// Placement is not visibility. A monitor the window manager will not describe
/// leaves the pill where it already was, which is worse than the right place
/// and much better than not being up at all.
fn place(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return Ok(());
    };
    let position = monitor.position();
    let size = monitor.size();
    window.set_position(bottom_center(
        position.x,
        position.y,
        size.width,
        size.height,
        monitor.scale_factor(),
    ))
}

/// Places the pill, shows it, focuses it, and reports what that achieved.
///
/// The pill is the recording privacy indicator, so the caller is told what the
/// window is doing rather than what it was asked to do: an operation that
/// returns without error has still only asked. What the window says about
/// itself afterwards is the answer, and a window that cannot answer counts as
/// not having done it.
pub fn show(app: &AppHandle) -> Result<Shown, String> {
    let window = ensure(app).map_err(|error| format!("the pill window is missing: {error}"))?;
    // Placement is part of being seen: a pill left at a position nobody chose
    // may be off the screen the person is looking at.
    let mut doubt = match place(&window) {
        Ok(()) => String::new(),
        Err(error) => format!("the pill could not be placed: {error}"),
    };
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

/// Hides the pill without destroying it, and confirms that it is down.
///
/// An always-on-top pill that is still up must never be recorded as down: that
/// record is what would stop it ever being taken down again.
pub fn hide(app: &AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window(LABEL) else {
        return Ok(());
    };
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
        let position = bottom_center(0, 0, 320, 120, 1.0);
        assert_eq!(position.x, 0);
        assert_eq!(position.y, 0);
        assert_eq!(
            bottom_center(40, 60, 320, 120, 1.0),
            PhysicalPosition::new(40, 60)
        );
    }
}
