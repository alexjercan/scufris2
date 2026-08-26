//! The shell windows, built to the pill's recipe.
//!
//! A widget window is the pill's window with a different page in it: opaque,
//! undecorated, always on top, out of the taskbar, and pinned by equal min and
//! max size hints on a resizable window - the one combination GTK honors, and
//! what makes a tiling window manager float it. See [`crate::pill::ensure`] for
//! why each of those is not a preference.
//!
//! One delta, and it is the whole point of the recipe here: a widget window is
//! built unfocusable and stays unfocusable. Widgets arrive in the middle of a
//! sentence. A window built merely unfocused is handed its right to the
//! keyboard back by the toolkit after its first paint - tao restores
//! `accept-focus` from a one-shot draw handler - so from its second mapping
//! onward it advertises `WM_HINTS.input = True` and i3 gives it the keyboard.
//! That is the keys of whoever was typing. Clicks do not need focus, and the
//! two chrome ticks are clicks, so nothing here is lost by refusing it.
//!
//! The title carries a `scufris-widget` prefix. Tauri builds every window under
//! one `WM_CLASS`, so the title is what an i3 rule can match on; that is the
//! defense in depth behind the unfocusable window, not instead of it.

use std::sync::atomic::AtomicU32;

use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::{
    display::{self, Verdict},
    widgets::runtime::{Monitor, Size},
};

/// The prefix every widget window label carries.
///
/// `capabilities/default.json` matches it with a `widget-*` glob. A window no
/// capability covers cannot invoke the commands its chrome is made of, and
/// nothing says so until a tick does nothing.
pub const LABEL_PREFIX: &str = "widget-";

/// The prefix every widget window title carries, for i3 rules to match.
pub const TITLE_PREFIX: &str = "scufris-widget";

/// Builds one hidden shell window.
///
/// The size is a placeholder until the shell becomes a widget: a pooled window
/// has no widget yet, so it has no size of its own to be built at.
pub fn build(app: &AppHandle, label: &str) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(label) {
        return Ok(window);
    }
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("shell.html".into()))
        .title(TITLE_PREFIX)
        .inner_size(PLACEHOLDER.width, PLACEHOLDER.height)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false)
        .focused(false)
        // The law, not the preference. See the module documentation.
        .focusable(false)
        .build()
}

/// The size a pooled shell waits at, in logical pixels. Small enough that a
/// window which somehow reaches the screen before it becomes anything is not
/// something the person has to move out of the way.
const PLACEHOLDER: Size = Size {
    width: 120.0,
    height: 60.0,
};

/// Pins one window to the size its widget lays out.
///
/// Equal min and max hints, applied in that order around the resize: a window
/// still carrying the placeholder's maximum cannot grow into a bigger widget,
/// and one still carrying the placeholder's minimum cannot shrink into a
/// smaller one, so both hints are lifted before the size is asked for.
pub fn fit(window: &WebviewWindow, size: Size) -> Result<(), String> {
    let wanted = LogicalSize::new(size.width, size.height);
    window
        .set_min_size(None::<LogicalSize<f64>>)
        .and_then(|()| window.set_max_size(None::<LogicalSize<f64>>))
        .and_then(|()| window.set_size(wanted))
        .and_then(|()| window.set_min_size(Some(wanted)))
        .and_then(|()| window.set_max_size(Some(wanted)))
        .map_err(|error| format!("the widget window would not take its size: {error}"))
}

/// Puts one widget window on screen at a place, and says whether it got there.
///
/// Position comes after the show, which is the opposite of the pill: i3 places
/// a floating window when it maps it, so a position set before the map is a
/// position i3 overwrites. The window refuses the keyboard throughout - it was
/// built that way and nothing here asks otherwise.
pub fn show(window: &WebviewWindow, at: PhysicalPosition<i32>) -> Result<(), String> {
    window
        .show()
        .map_err(|error| format!("the widget window could not be shown: {error}"))?;
    // Asked fresh rather than remembered. The pill and the review box each keep
    // the name the display knows them by, because each of them is one window
    // that is asked about again and again; there are many widget windows, and
    // one remembered name shared between them would answer for the wrong one.
    let up = display::came_up(window, &AtomicU32::new(0));
    window
        .set_position(at)
        .map_err(|error| format!("the widget window would not be placed: {error}"))?;
    if let Err(error) = window.set_always_on_top(true) {
        tracing::warn!("a widget window could not be kept on top: {error}");
    }
    match up {
        Verdict::No => Err("the widget window did not come up".into()),
        _ => Ok(()),
    }
}

/// Takes one widget window off the screen, leaving the widget mounted.
///
/// The page keeps its DOM and the widget keeps its state: this is a window
/// unmapped, not a widget destroyed. What it costs is the placement, which i3
/// decides again the next time the window maps - so a window that comes back
/// comes back through [`show`].
pub fn conceal(window: &WebviewWindow) -> Result<(), String> {
    window
        .hide()
        .map_err(|error| format!("the widget window would not go down: {error}"))
}

/// Takes one widget window off the screen for good.
pub fn close(window: &WebviewWindow) -> Result<(), String> {
    window
        .destroy()
        .map_err(|error| format!("the widget window would not close: {error}"))
}

/// Returns the monitor one window is on, as the placement math sees it.
pub fn monitor(window: &WebviewWindow) -> Option<Monitor> {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())?;
    let position = monitor.position();
    let size = monitor.size();
    Some(Monitor {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        scale: monitor.scale_factor(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_widget_window_label_matches_the_capability_glob() {
        // capabilities/default.json lists "widget-*". A label outside it is a
        // window whose chrome ticks invoke nothing, and nothing says so.
        let capability = include_str!("../../capabilities/default.json");
        assert!(
            capability.contains(&format!("\"{LABEL_PREFIX}*\"")),
            "the default capability does not cover {LABEL_PREFIX}* windows"
        );
    }

    #[test]
    fn the_content_policy_lets_a_shell_window_import_its_widget() {
        // The widget module is served over scufris-widget://. A policy that
        // does not name the scheme blocks the import at runtime, long after
        // the build that would have caught a missing file.
        let config = include_str!("../../tauri.conf.json");
        let csp = config
            .lines()
            .find(|line| line.contains("\"csp\""))
            .expect("tauri.conf.json declares a content security policy");
        let script = csp
            .split(';')
            .find(|part| part.contains("script-src"))
            .expect("the policy names script-src");
        assert!(
            script.contains("scufris-widget:"),
            "script-src does not allow the widget scheme: {script}"
        );
    }
}
