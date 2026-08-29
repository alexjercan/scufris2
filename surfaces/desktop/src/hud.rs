//! The conversation window, and the way in for typed words.
//!
//! The pill says what Scufris is doing and the textbox holds one take. Neither
//! of them shows the canonical conversation. Reading the last four lines
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

use scufris_control::service::ConversationMessage;
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tracing::{debug, warn};

use crate::{
    app::Backend,
    conversation::{Conversation, Notice},
    display::{self, Verdict},
    focus::{self, FocusTracker},
    pill, textbox,
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
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Backlog {
    /// Everything said so far, oldest first.
    pub lines: Vec<ConversationMessage>,
    /// What the window is waiting for right now.
    pub notice: Notice,
}

/// Returns the physical position of the window, centered above the pill band.
///
/// Centered rather than pinned to an edge: this is the window the person reads
/// and types in, so it belongs where they are already looking.
///
/// "Nowhere near the pill" was the reasoning and it was not arithmetic. On
/// 1920x1080 a centered window runs to y=820 and the pill starts at y=778, so
/// it covered the pill's top 42 pixels; on 1366x768 it covered 198 of them, and
/// what it covered was the ring that says the microphone is open. So the bottom
/// edge is bounded against the pill rather than assumed clear of it.
///
/// The bound cannot always be met. Below about 1600x900 the window is taller
/// than the room above the pill, and there is no position on the monitor that
/// clears it. That case is what [`Hud::show`] refusing to come up over the
/// textbox is for: placement gets the two apart where there is room, and
/// stacking order is what settles it where there is not.
pub fn center(
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: u32,
    monitor_height: u32,
    scale: f64,
) -> PhysicalPosition<i32> {
    let width = (WIDTH * scale).round() as i32;
    let height = (HEIGHT * scale).round() as i32;
    let gap = (textbox::GAP * scale).round() as i32;
    let pill = pill::bottom_center(monitor_x, monitor_y, monitor_width, monitor_height, scale);
    let x = monitor_x + ((monitor_width as i32 - width) / 2).max(0);
    let centered = monitor_y + ((monitor_height as i32 - height) / 2).max(0);
    // Never lower than one gap above the pill, and never off the top of the
    // monitor: a window with its corner on the screen beats one centered off
    // the top, where the field the person types in is the part that is gone.
    let y = centered.min(pill.y - height - gap).max(monitor_y);
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
    pub fn said(&self, entry: ConversationMessage) {
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

    /// Replay ended with `surface.ready`; the field may become live.
    pub fn ready(&self) {
        self.lock().ready();
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
    ///
    /// Not over the textbox. i3 does not stack floating windows by
    /// `_NET_WM_STATE_ABOVE` - it echoes the state and ignores it, and the last
    /// window mapped is the one on top - so being built `always_on_top(false)`
    /// did not keep this window under the box the way it was written to. It
    /// came up over a take the person was editing, took its keyboard, and left
    /// the state machine editing a box that was neither visible nor holding
    /// keys. Nothing recovered it: the repair chain does not run at
    /// `Screen::Ready` and the watch does not run while this window has the
    /// keys.
    ///
    /// So the box wins that band outright. It is up only while there is a take
    /// in it, it holds the keyboard for exactly that long, and one take is the
    /// shortest-lived thing on this desktop. Refusing here is the whole fix
    /// rather than half of one, because restacking alone would leave the box
    /// visible on top and the keys going to this window.
    pub fn show(&self) -> Result<(), String> {
        if textbox::up(&self.app) {
            return Err("The textbox has a take in it.".into());
        }
        let window = ensure(&self.app).map_err(|error| format!("the HUD is missing: {error}"))?;
        // Before the raise, and never over ourselves: the window recorded here
        // is the one the keyboard goes back to, and recording this one would
        // hand the person their keys back into the window they just closed.
        if !holds_keyboard(&self.app) {
            self.focus.capture(&focus::own_windows(&self.app));
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
        // Asked before the window gives the keyboard up, because afterwards
        // there is nothing left to read. Only a window that had the keys gives
        // them back: open the window from the tray in one editor, click into a
        // browser, then put the window away, and an unguarded restore is a
        // plain focus steal back to the editor. `show` guards the capture the
        // same way and for the same reason.
        let held = holds_keyboard(&self.app);
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
            Verdict::Yes | Verdict::Unsure if held => self.focus.restore(),
            Verdict::Yes | Verdict::Unsure => Ok(()),
            Verdict::No => Err("the HUD is still up".into()),
        }
    }

    /// Sends one typed line to the service, and answers whether it was taken.
    ///
    /// Nothing is appended here on the way out. The line comes back as a
    /// transcript entry when the service takes it, which is what puts it on
    /// screen - so the window shows the conversation rather than this process's
    /// hopes about it.
    ///
    /// The answer is what lets the page keep the promise this refusal is built
    /// on. A second Enter while one line is in flight is refused rather than
    /// queued, and the reason that is acceptable is that the words stay in the
    /// field for the person to send again. The page cleared the field before
    /// asking, so they did not: the sentence went nowhere and nothing said so.
    pub fn typed(&self, text: String) -> bool {
        let Some(id) = self.lock().typed(&text) else {
            // Blank, or a second Enter on a line that is still in flight. The
            // words are still in the field either way.
            return false;
        };
        self.tell();
        let sent = match self.backend.get() {
            Some(backend) => backend.submit(id.clone(), text),
            None => Err("Scufris is not reachable.".into()),
        };
        if let Err(trouble) = sent {
            self.refused(&id, trouble);
        }
        // Taken by the window, which is what the field is cleared on. Whether
        // the service takes it is a later answer and it arrives as a notice.
        true
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
        //
        // This is a request about other people's windows and not about the
        // companion's own. i3 does not stack floating windows by
        // `_NET_WM_STATE_ABOVE`: it echoes the state and ignores it, and the
        // last window mapped is the one on top. So nothing here keeps this
        // window under the pill or the box. `center` bounds the placement
        // against the pill band and `Hud::show` refuses to come up over a take
        // in the box, and between them that is what does.
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

/// Answers whether the HUD is on screen, according to the display.
///
/// The display, not the toolkit. `is_visible` is what this process last asked
/// for, and it survives i3 unmapping the window on a workspace switch, so on
/// any workspace but the one the window was left on the toolkit said "up" and
/// `toggle` took the hide path: one press of the binding showed nothing,
/// answered `taken`, and pulled the person back to the workspace the window was
/// on. A display that cannot be asked leaves the toolkit's flag, and only its
/// positive answer is worth anything - a window is recorded as shown the moment
/// the request is passed on, and as hidden until then.
pub fn up(app: &AppHandle) -> bool {
    let Some(window) = app.get_webview_window(LABEL) else {
        return false;
    };
    match display::up(&window, &WINDOW) {
        Verdict::Yes => true,
        Verdict::No => false,
        Verdict::Unsure => window.is_visible().unwrap_or(false),
    }
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

    /// The bottom edge of the window, and the top edge of the pill under it.
    fn bottom_and_pill(width: u32, height: u32, scale: f64) -> (i32, i32) {
        let window = center(0, 0, width, height, scale);
        let pill = pill::bottom_center(0, 0, width, height, scale);
        (window.y + (HEIGHT * scale).round() as i32, pill.y)
    }

    #[test]
    fn the_window_is_horizontally_centered_on_its_monitor() {
        assert_eq!(center(0, 0, 1920, 1080, 1.0).x, (1920 - WIDTH as i32) / 2);
        // A second monitor to the right, at twice the scale. The window is
        // measured in physical pixels there, so the same logical size takes
        // twice the room and the centering has to be done after the scaling
        // rather than before it.
        assert_eq!(
            center(1920, -120, 2560, 1440, 2.0).x,
            1920 + (2560 - (WIDTH * 2.0) as i32) / 2
        );
    }

    #[test]
    fn the_window_stands_clear_of_the_pill_where_the_monitor_has_room() {
        // B3. Centering blind put the window's bottom edge at y=820 on
        // 1920x1080 with the pill's top edge at y=778, so it covered the pill's
        // top 42 pixels - and what it covered was the ring that says the
        // microphone is open.
        for (width, height) in [(1920, 1080), (2560, 1440), (1600, 900)] {
            let (bottom, pill_top) = bottom_and_pill(width, height, 1.0);
            assert!(
                bottom <= pill_top,
                "{width}x{height}: the window runs to {bottom} and the pill starts at {pill_top}"
            );
        }
        // It only moves as far as it has to. Where there is room to spare the
        // window stays where the eye already is.
        let clear = center(0, 0, 2560, 1440, 1.0).y;
        assert_eq!(clear, (1440 - HEIGHT as i32) / 2);
    }

    #[test]
    fn a_monitor_with_no_room_above_the_pill_puts_the_window_at_the_top() {
        // 560 of window, 230 of pill, a 72 margin under it and a 24 gap above
        // it come to 886, and a 768-tall monitor does not have it. There is no
        // position that clears the pill, so the window takes the top of the
        // monitor: that is the least of it covered, and it is never placed off
        // the screen, where the field the person types in is what goes missing.
        assert_eq!(center(0, 0, 1366, 768, 1.0), PhysicalPosition::new(303, 0));
        assert_eq!(center(40, 60, 640, 480, 1.0), PhysicalPosition::new(40, 60));
        // At twice the scale the window is twice as tall in physical pixels, so
        // a monitor with room at 1x can be out of it at 2x.
        assert_eq!(center(1920, -120, 2560, 1440, 2.0).y, -120);
    }

    #[test]
    fn the_window_never_covers_more_of_the_pill_than_it_has_to() {
        // The residual, stated rather than left to be discovered. Below about
        // 1600x900 the window is taller than the room above the pill and some
        // of the pill is behind it. Bounding the placement is what keeps that
        // to the smallest overlap the monitor allows.
        let (bottom, pill_top) = bottom_and_pill(1366, 768, 1.0);
        assert_eq!(bottom - pill_top, 94);
        // Blind centering covered more than twice as much.
        let blind = (768 - HEIGHT as i32) / 2 + HEIGHT as i32;
        assert_eq!(blind - pill_top, 198);
    }

    #[test]
    fn the_window_is_the_size_the_page_lays_out() {
        // hud.css lays out to exactly these logical pixels, and the window
        // cannot be resized once it is up.
        assert_eq!(WIDTH, 760.0);
        assert_eq!(HEIGHT, 560.0);
    }
}
