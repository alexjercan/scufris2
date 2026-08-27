//! The conversation window, and the way in for typed words.
//!
//! The pill says what Scufris is doing and the textbox holds one take. Neither
//! of them shows what was actually said, and until this window there was one
//! place that did: a terminal running `scufris-ctl debug`. That is a whole Pi
//! session and it stays - it is the deep tool - but reading the last four lines
//! should not cost a terminal.
//!
//! So this is a frontend surface like the widget shelf is: it draws the
//! transcript stream the service pushes to every frontend, and it types back on
//! the same socket. [`crate::conversation::Conversation`] holds every decision
//! it makes; this holds the window and the wiring.
//!
//! It has no accelerator of its own. The companion already holds one key for
//! the whole session - the activation hotkey - and a second would be one more
//! key no other program on this desktop could ever use. The way in is the
//! command socket instead: `scufris-ctl hud` from a window manager binding, or
//! the tray. That is the same door `scufris-ctl open` came in by, and it costs
//! the desktop nothing.

use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicU32, Ordering},
};

use scufris_control::service::TranscriptEntry;
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tracing::{debug, warn};

use crate::{
    app::Backend,
    conversation::{Conversation, Notice},
    display::{self, Verdict},
    focus::FocusTracker,
};

/// Stable window label. `capabilities/default.json` names it too: a window the
/// capability does not cover cannot listen for the events it renders.
pub const LABEL: &str = "hud";

/// One more line of the conversation, pushed as it is said.
pub const SAID_EVENT: &str = "scufris://said";

/// Everything at once, replacing whatever the page is showing.
pub const RESET_EVENT: &str = "scufris://conversation";

/// What the window is waiting for, pushed when that changes.
pub const NOTICE_EVENT: &str = "scufris://notice";

/// Window width in logical pixels.
///
/// Wide enough for a paragraph at a comfortable measure and no wider: the
/// window is read, and a line of prose that runs the width of a monitor is one
/// the eye loses its place in. `hud.css` lays out to exactly this.
pub const WIDTH: f64 = 760.0;

/// Window height in logical pixels.
///
/// Room for a dozen short lines above the field. Longer conversations scroll
/// inside the window rather than resizing it: equal min and max hints are what
/// makes a tiling window manager float this, and they cannot be changed while
/// the window is up.
pub const HEIGHT: f64 = 560.0;

/// The X window the display knows the HUD by, once it has made one.
static WINDOW: AtomicU32 = AtomicU32::new(0);

/// What one page is handed when it says hello.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Backlog {
    /// Everything said so far, oldest first.
    pub lines: Vec<TranscriptEntry>,
    /// What the window is waiting for right now.
    pub notice: Notice,
}

/// Returns the physical position of the window, centered on its monitor.
///
/// Centered rather than pinned to an edge: this is the window the person reads
/// and types in, so it belongs where they are already looking. The pill and its
/// shelf keep the bottom of the screen, and this stands clear of both by being
/// nowhere near them.
pub fn center(
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: u32,
    monitor_height: u32,
    scale: f64,
) -> PhysicalPosition<i32> {
    let width = (WIDTH * scale).round() as i32;
    let height = (HEIGHT * scale).round() as i32;
    let x = monitor_x + ((monitor_width as i32 - width) / 2).max(0);
    let y = monitor_y + ((monitor_height as i32 - height) / 2).max(0);
    PhysicalPosition::new(x, y)
}

/// The conversation window and the socket it types back on.
pub struct Hud {
    app: AppHandle,
    state: Mutex<Conversation>,
    /// Where a typed line goes. The same port the pill submits through, because
    /// a typed line and a spoken one are the same message to the service.
    backend: OnceLock<Arc<dyn Backend>>,
    /// The window to give the keyboard back to when this one goes down.
    focus: FocusTracker,
}

impl Hud {
    /// Builds the window and the conversation it will show.
    ///
    /// The window is made now rather than on the first open, for the reason the
    /// textbox is: the page has to be listening before the lines it renders
    /// arrive, and a window built at that moment would miss the ones that
    /// brought it up.
    pub fn start(app: AppHandle, prefix: impl Into<String>) -> tauri::Result<Arc<Self>> {
        ensure(&app)?;
        Ok(Arc::new(Self {
            app,
            state: Mutex::new(Conversation::new(prefix)),
            backend: OnceLock::new(),
            focus: FocusTracker::new(),
        }))
    }

    /// Gives the HUD the socket it submits on.
    pub fn attach(&self, backend: Arc<dyn Backend>) {
        if self.backend.set(backend).is_err() {
            debug!("the HUD already had a way to submit");
        }
    }

    /// Takes one line of the conversation and shows it.
    ///
    /// Everything the service pushes arrives here, including the replay it
    /// sends a frontend on connect and including lines somebody typed in a
    /// terminal. The window does not have to be up: a page that is loaded and
    /// down still appends, so opening it is never a wait for content.
    pub fn said(&self, entry: TranscriptEntry) {
        self.lock().said(entry.clone());
        if let Err(error) = self.app.emit_to(LABEL, SAID_EVENT, entry) {
            debug!("the HUD did not take a line: {error}");
        }
    }

    /// The service is back, and the replay of its ring is about to arrive.
    ///
    /// Everything held is thrown away first. The replay is the whole ring, so
    /// keeping what is here would put every line on screen twice.
    pub fn reconnected(&self) {
        self.lock().restart();
        self.show_all();
    }

    /// The service went away, so nothing in flight is coming back.
    pub fn dropped(&self, trouble: impl Into<String>) {
        if self.lock().dropped(trouble) {
            self.tell();
        }
    }

    /// Everything the page has missed, asked for once when it loads.
    pub fn backlog(&self) -> Backlog {
        let state = self.lock();
        Backlog {
            lines: state.lines(),
            notice: state.notice(),
        }
    }

    /// Hands the page everything at once, replacing what it is showing.
    fn show_all(&self) {
        if let Err(error) = self.app.emit_to(LABEL, RESET_EVENT, self.backlog()) {
            debug!("the HUD did not take the conversation: {error}");
        }
    }

    /// Puts the window up if it is down, and down if it is up.
    ///
    /// One verb rather than two, because the way in is one key binding. A person
    /// who pressed it to read the last answer presses it again to go back to
    /// what they were doing.
    pub fn toggle(&self) -> Result<(), String> {
        if up(&self.app) {
            self.hide()
        } else {
            self.show()
        }
    }

    /// Puts the window up with the keyboard.
    pub fn show(&self) -> Result<(), String> {
        let window = ensure(&self.app).map_err(|error| format!("the HUD is missing: {error}"))?;
        // Before the raise, and never over ourselves: the window recorded here
        // is the one the keyboard goes back to, and recording this one would
        // hand the person their keys back into the window they just closed.
        if !holds_keyboard(&self.app) {
            self.focus.capture(&self.windows());
        }
        raise(&window)
    }

    /// Takes the window down and gives the keyboard back.
    pub fn hide(&self) -> Result<(), String> {
        let Some(window) = self.app.get_webview_window(LABEL) else {
            return Ok(());
        };
        if !up(&self.app) {
            // Already down. Restoring focus anyway would take the keys off
            // whatever the person moved to.
            return Ok(());
        }
        // Refusing the keyboard first: a hidden window that still says it wants
        // keys is one a window manager can hand them to on the next map.
        if let Err(error) = window.set_focusable(false) {
            warn!("the HUD would not give the keyboard up: {error}");
        }
        window
            .hide()
            .map_err(|error| format!("the HUD could not be hidden: {error}"))?;
        match display::went_down(&window, &WINDOW) {
            // Down, or on its way with nobody in a position to watch it go.
            Verdict::Yes | Verdict::Unsure => self.focus.restore(),
            Verdict::No => Err("the HUD is still up".into()),
        }
    }

    /// Sends one typed line to the service.
    ///
    /// Nothing is appended here on the way out. The line comes back as a
    /// transcript entry when the service takes it, which is what puts it on
    /// screen - so the window shows the conversation rather than this process's
    /// hopes about it.
    pub fn typed(&self, text: String) {
        let Some(id) = self.lock().typed(&text) else {
            // Blank, or a second Enter on a line that is still in flight. The
            // words are still in the field either way.
            return;
        };
        self.tell();
        let sent = match self.backend.get() {
            Some(backend) => backend.submit(id.clone(), text),
            None => Err("Scufris is not reachable.".into()),
        };
        if let Err(trouble) = sent {
            self.refused(&id, trouble);
        }
    }

    /// The service took a line. Nothing happens unless it was this window's.
    pub fn accepted(&self, id: &str) {
        if self.lock().accepted(id) {
            self.tell();
        }
    }

    /// The service would not take a line, or it never left this process.
    pub fn refused(&self, id: &str, trouble: impl Into<String>) {
        if self.lock().refused(id, trouble) {
            self.tell();
        }
    }

    /// Pushes what the window is waiting for.
    fn tell(&self) {
        let notice = self.lock().notice();
        if let Err(error) = self.app.emit_to(LABEL, NOTICE_EVENT, notice) {
            debug!("the HUD did not take a notice: {error}");
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Conversation> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// The companion's own windows, which are never what the keyboard goes back
    /// to. Only this one exists as far as the tracker is concerned; the pill
    /// refuses the keyboard and the textbox is not up while this is.
    fn windows(&self) -> Vec<u32> {
        known_window().into_iter().collect()
    }
}

/// Returns the HUD window, creating it hidden on first use.
pub fn ensure(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(LABEL) {
        return Ok(window);
    }
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("hud.html".into()))
        .title("Scufris")
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(WIDTH, HEIGHT)
        .max_inner_size(WIDTH, HEIGHT)
        .decorations(false)
        .skip_taskbar(true)
        .shadow(false)
        // Not on top. The pill and the textbox are indicators that have to be
        // seen over whatever is under them; this is a window the person works
        // in, and one they have moved away from belongs behind what they moved
        // to. Toggling it is cheaper than fighting it for the screen.
        .always_on_top(false)
        // Built down and built refusing the keyboard. What it says about the
        // keyboard from then on is only ever what `raise` and `hide` last said.
        .visible(false)
        .focused(false)
        .focusable(false)
        .build()
}

/// Answers what the display knows the HUD by, once it has made a window.
pub fn known_window() -> Option<u32> {
    match WINDOW.load(Ordering::SeqCst) {
        0 => None,
        id => Some(id),
    }
}

/// Answers whether the HUD is on screen, as far as the toolkit knows.
pub fn up(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

/// Answers whether the HUD is holding the keyboard right now.
fn holds_keyboard(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL)
        .is_some_and(|window| display::keyboard(&window, &WINDOW) == Verdict::Yes)
}

/// Puts the window on screen, in the order that gets it the keys.
///
/// Claiming the keyboard first, and every time, for the reason
/// [`crate::textbox`] documents at length: a window manager that unmanages a
/// hidden window and manages it again on the next show - i3 does - reads the
/// window's hints when it maps it, so a claim made after the show is a claim
/// made for the next one.
fn raise(window: &WebviewWindow) -> Result<(), String> {
    window
        .set_focusable(true)
        .map_err(|error| format!("the HUD would not take the keyboard: {error}"))?;
    if let Err(error) = place(window) {
        // Chrome. A window in the wrong place is still a window that can be
        // read and typed in.
        warn!("the HUD could not be placed: {error}");
    }
    window
        .show()
        .map_err(|error| format!("the HUD could not be shown: {error}"))?;
    if display::came_up(window, &WINDOW) == Verdict::No {
        return Err("the HUD did not come up".into());
    }
    // Asking is not holding: a window manager may accept the request and hand
    // the keyboard elsewhere, or later. Nothing is reported either way - unlike
    // the textbox, there are no words here that only this window can answer for,
    // and the field is still there to click.
    if let Err(error) = window.set_focus() {
        debug!("the HUD could not take the keyboard: {error}");
    }
    Ok(())
}

/// Puts the window in the middle of the monitor it is on.
fn place(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return Ok(());
    };
    let position = monitor.position();
    let size = monitor.size();
    window.set_position(center(
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
    fn the_window_sits_in_the_middle_of_its_monitor() {
        let position = center(0, 0, 1920, 1080, 1.0);
        assert_eq!(position.x, (1920 - WIDTH as i32) / 2);
        assert_eq!(position.y, (1080 - HEIGHT as i32) / 2);
    }

    #[test]
    fn placement_follows_the_monitor_offset_and_scale() {
        // A second monitor to the right, at twice the scale. The window is
        // measured in physical pixels there, so the same logical size takes
        // twice the room and the centering has to be done after the scaling
        // rather than before it.
        let position = center(1920, -120, 2560, 1440, 2.0);
        assert_eq!(position.x, 1920 + (2560 - (WIDTH * 2.0) as i32) / 2);
        assert_eq!(position.y, -120 + (1440 - (HEIGHT * 2.0) as i32) / 2);
    }

    #[test]
    fn a_monitor_too_small_for_the_window_never_places_it_off_screen() {
        // Better a window with its corner on the monitor than one centered off
        // the top of it, where the field the person types in would be the part
        // that is gone.
        assert_eq!(center(0, 0, 640, 480, 1.0), PhysicalPosition::new(0, 0));
        assert_eq!(center(40, 60, 640, 480, 1.0), PhysicalPosition::new(40, 60));
    }

    #[test]
    fn the_window_is_the_size_the_page_lays_out() {
        // hud.css lays out to exactly these logical pixels, and the window
        // cannot be resized once it is up.
        assert_eq!(WIDTH, 760.0);
        assert_eq!(HEIGHT, 560.0);
    }
}
