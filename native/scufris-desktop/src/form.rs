//! The small window a panel borrows when it needs words.
//!
//! A widget window never holds the keyboard. It is built unfocusable and stays
//! that way, because widgets arrive in the middle of a sentence and one that
//! could take the keys would take them from whoever was typing - see
//! [`crate::widgets::windows`] for why that is a law rather than a preference.
//! So a panel that wants a task written, a weight logged or a note kept cannot
//! carry a field of its own.
//!
//! This is the field it borrows instead: one window, one form, up only while
//! there is a question on it. The panel says what to ask and what to do with
//! the answer; this holds the words and hands the finished action back to the
//! panel's own backend, which is the only place it can go. Nothing typed here
//! ever reaches the conversation - the textbox is that door and this is not it.
//!
//! Built to the HUD's recipe, and for the HUD's reasons: claimed keyboard
//! before every raise because i3 reads a window's hints when it maps it, the
//! window the keys came from recorded so they can go back, and a refusal to
//! come up over a take in the textbox. That last one is not politeness. The
//! textbox is up only while there is a decision about words in it, it holds the
//! keyboard for exactly that long, and a window that took the keys off it would
//! leave the pill's state machine editing a box nobody can answer.

use std::{
    collections::BTreeMap,
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicU32, Ordering},
    },
};

use serde::Serialize;
use serde_json::{Map, Value};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tracing::{debug, warn};

use crate::{
    display::{self, Verdict},
    focus::{self, FocusTracker},
    textbox,
};

/// Stable window label. `capabilities/default.json` names it too: a window the
/// capability does not cover cannot invoke the commands its page is made of.
pub const LABEL: &str = "form";

/// The question, pushed when one is asked.
pub const ASK_EVENT: &str = "scufris://ask";

/// Box width in logical pixels. `form.css` lays out to exactly this.
///
/// Narrower than the transcript box, which holds a paragraph somebody spoke.
/// What goes in here is a task, a weight or a heading - short things, and a
/// field far wider than its answer reads as a field that expected more.
pub const WIDTH: f64 = 420.0;

/// The tallest box that will be built, in logical pixels.
///
/// The height follows the fields, so an ask with a long block in it is a taller
/// window. This is where that stops: a box taller than the HUD is one that
/// covers the panel that asked for it.
const CEILING: f64 = 560.0;

/// Room above the first line and below the last, in logical pixels.
const PAD: f64 = 14.0;

/// The title line, and the gap under it.
const CROWN: f64 = 15.0;
const CROWN_GAP: f64 = 12.0;

/// One field's label line, and the gap under it.
const NAME: f64 = 14.0;
const NAME_GAP: f64 = 4.0;

/// One line inside a field, and the padding and border around them all.
const LINE: f64 = 20.0;
const BOX: f64 = 14.0;

/// The gap between two fields.
const FIELD_GAP: f64 = 10.0;

/// The keys line at the foot, and the gap over it.
const KEYS_GAP: f64 = 10.0;
const KEYS: f64 = 14.0;

/// The most fields one ask may carry.
///
/// Four is a form; more is a dialog, and a panel that needs a dialog is a panel
/// asking for something `today` should be asked for directly.
const FIELDS: usize = 4;

/// The most lines one field may be.
const LINES: u32 = 12;

/// The most characters a title or a label may carry onto the box.
const WORDS: usize = 64;

/// The X window the display knows the box by, once it has made one.
static WINDOW: AtomicU32 = AtomicU32::new(0);

/// One field on the form, as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Field {
    /// The key the answer arrives under.
    pub name: String,
    /// What is printed over the field.
    pub label: String,
    /// What the field starts with.
    pub value: String,
    /// How many lines the field is.
    pub lines: u32,
    /// Grey words in an empty field.
    pub hint: String,
}

/// One question, as a widget asked it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ask {
    /// What the box is titled.
    pub title: String,
    /// The fields, in the order they are asked.
    pub fields: Vec<Field>,
    /// The action the answers are laid into. Never sent to the page: what the
    /// answers mean is between the widget and its backend.
    #[serde(skip)]
    pub action: Map<String, Value>,
}

impl Ask {
    /// Reads one ask, or says why it is not one.
    ///
    /// Every bound is here rather than on the page. What arrives is a widget's
    /// word, widgets are the one part of this companion that can come from
    /// outside it - `SCUFRIS_WIDGET_PATH` installs a directory - and a page
    /// that trusted a count would be a window sized by whoever wrote the
    /// widget.
    pub fn parse(value: Value) -> Result<Self, String> {
        let Value::Object(mut asked) = value else {
            return Err("an ask is an object".into());
        };
        let action = match asked.remove("action") {
            Some(Value::Object(action)) => action,
            _ => return Err("an ask carries an action object".into()),
        };
        let title = match asked.get("title") {
            Some(Value::String(title)) => clip(title),
            _ => return Err("an ask carries a title".into()),
        };
        let Some(Value::Array(fields)) = asked.get("fields") else {
            return Err("an ask carries fields".into());
        };
        if fields.is_empty() {
            return Err("an ask with no fields asks nothing".into());
        }
        if fields.len() > FIELDS {
            return Err(format!("an ask carries at most {FIELDS} fields"));
        }
        let fields = fields
            .iter()
            .map(field)
            .collect::<Result<Vec<Field>, String>>()?;
        Ok(Self {
            title,
            fields,
            action,
        })
    }

    /// The height of the box that asks this, in logical pixels.
    pub fn height(&self) -> f64 {
        let mut total = PAD * 2.0 + CROWN + CROWN_GAP + KEYS_GAP + KEYS;
        for (index, field) in self.fields.iter().enumerate() {
            if index > 0 {
                total += FIELD_GAP;
            }
            total += NAME + NAME_GAP + f64::from(field.lines) * LINE + BOX;
        }
        total.min(CEILING)
    }

    /// Lays the answers into the action the widget sent, and nothing else.
    ///
    /// Only the fields that were asked for. The page answers with what it was
    /// given, but a page that made up a key would otherwise be a page that can
    /// name any argument the backend reads.
    fn fill(&self, answers: &BTreeMap<String, String>) -> Value {
        let mut action = self.action.clone();
        for field in &self.fields {
            let said = answers.get(&field.name).map_or("", String::as_str);
            action.insert(field.name.clone(), Value::String(tidy(said, field.lines)));
        }
        Value::Object(action)
    }
}

/// Reads one field, or says why it is not one.
fn field(value: &Value) -> Result<Field, String> {
    let Value::Object(asked) = value else {
        return Err("a field is an object".into());
    };
    let Some(Value::String(name)) = asked.get("name") else {
        return Err("a field carries a name".into());
    };
    if name.is_empty() {
        return Err("a field's name is what its answer is called".into());
    }
    let label = match asked.get("label") {
        Some(Value::String(label)) => clip(label),
        _ => name.clone(),
    };
    let lines = match asked.get("lines") {
        Some(Value::Number(lines)) => lines.as_u64().unwrap_or(1).clamp(1, u64::from(LINES)) as u32,
        _ => 1,
    };
    Ok(Field {
        name: name.clone(),
        label,
        value: match asked.get("value") {
            Some(Value::String(value)) => value.clone(),
            _ => String::new(),
        },
        lines,
        hint: match asked.get("hint") {
            Some(Value::String(hint)) => clip(hint),
            _ => String::new(),
        },
    })
}

/// Cuts one printed word down to what the box has room for.
fn clip(text: &str) -> String {
    text.chars().take(WORDS).collect()
}

/// Trims one answer, and flattens it when the field was one line.
///
/// A one-line field is one line whatever is pasted into it. The answers go on
/// to `today`, and a task carrying a newline is a journal entry with a task
/// text that is not a task text.
fn tidy(said: &str, lines: u32) -> String {
    if lines > 1 {
        return said.trim().to_string();
    }
    said.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A window or a monitor, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Returns where a box of this size goes over the panel that asked for it.
///
/// Over the panel rather than anywhere fixed: the person clicked a tick on one
/// small window and the answer to the click belongs where they are looking. It
/// never leaves the monitor, and a panel near an edge pushes the box inward
/// rather than off.
pub fn over(panel: Rect, monitor: Rect, box_width: i32, box_height: i32) -> PhysicalPosition<i32> {
    let x = panel.x + (panel.width - box_width) / 2;
    let y = panel.y + (panel.height - box_height) / 2;
    let right = (monitor.x + monitor.width - box_width).max(monitor.x);
    let bottom = (monitor.y + monitor.height - box_height).max(monitor.y);
    PhysicalPosition::new(x.clamp(monitor.x, right), y.clamp(monitor.y, bottom))
}

/// Returns where the box for a question of this height goes.
///
/// The size comes from the question and not from the window. Placement runs
/// before the show - a window manager places a floating window when it maps it,
/// so afterwards is too late - and a window that has never been mapped answers
/// no size at all. Asking it put the box half its own width and height down and
/// to the right of where it belongs, which is what a live run found.
pub fn box_over(panel: Rect, monitor: Rect, height: f64, scale: f64) -> PhysicalPosition<i32> {
    let width = (WIDTH * scale).round() as i32;
    let tall = (height * scale).round() as i32;
    over(panel, monitor, width, tall)
}

/// The question on the box, and who it is being asked for.
struct Pending {
    /// The surface whose backend gets the answer.
    surface: String,
    ask: Ask,
}

/// The box, and the one question it is holding.
pub struct Form {
    app: AppHandle,
    pending: Mutex<Option<Pending>>,
    /// The window to give the keyboard back to when this one goes down.
    focus: FocusTracker,
}

impl Form {
    /// Returns a form with no question on it.
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            pending: Mutex::new(None),
            focus: FocusTracker::new(),
        }
    }

    /// Puts one question on screen for one surface.
    ///
    /// Refused rather than queued when the box is already asking something. The
    /// words in it are the person's and a second question would type over them,
    /// and the panel says so on its badge - which is what a refused tick looks
    /// like everywhere else here.
    pub fn open(&self, surface: String, ask: Ask, panel: Option<Rect>) -> Result<(), String> {
        if textbox::up(&self.app) {
            return Err("the textbox has a take in it".into());
        }
        {
            let mut pending = self.held();
            if pending.is_some() && up(&self.app) {
                return Err("the box is busy".into());
            }
            *pending = Some(Pending {
                surface,
                ask: ask.clone(),
            });
        }
        let window = ensure(&self.app).map_err(|error| format!("the form is missing: {error}"))?;
        // Pushed before the raise, so the box is never on screen empty. A page
        // that has not finished loading misses this and asks for it instead;
        // see [`Form::asked`].
        if let Err(error) = self.app.emit_to(LABEL, ASK_EVENT, &ask) {
            debug!("the form did not take a question: {error}");
        }
        self.fit(&window, ask.height())?;
        // Before the raise, and never over ourselves: the window recorded here
        // is the one the keyboard goes back to, and recording this one would
        // hand the person their keys back into the window they just closed.
        if !holds_keyboard(&self.app) {
            self.focus.capture(&focus::own_windows(&self.app));
        }
        raise(&window, panel, ask.height())
    }

    /// The question the box is holding, for a page that has just loaded.
    pub fn asked(&self) -> Option<Ask> {
        self.held().as_ref().map(|pending| pending.ask.clone())
    }

    /// Takes the answers, puts the box away, and says where they go.
    ///
    /// The action is built from the ask this process is holding rather than
    /// from anything the page said about it: the page is handed labels and
    /// hands back words, and what those words mean was decided when the panel
    /// asked.
    pub fn submit(&self, answers: &BTreeMap<String, String>) -> Option<(String, Value)> {
        let pending = self.held().take()?;
        let action = pending.ask.fill(answers);
        self.put_away();
        Some((pending.surface, action))
    }

    /// Puts the box away with nothing answered.
    pub fn cancel(&self) {
        self.held().take();
        self.put_away();
    }

    /// Drops a question asked for a surface that is going away.
    ///
    /// A box left asking about a panel that has been closed is a box whose
    /// answer has nowhere to land.
    pub fn forget(&self, surface: &str) {
        let mine = self
            .held()
            .as_ref()
            .is_some_and(|pending| pending.surface == surface);
        if mine {
            self.cancel();
        }
    }

    /// Gives the box the size this question needs, before it maps.
    ///
    /// Equal minimum and maximum hints, lifted before the resize and put back
    /// after it: they are what makes a tiling window manager float this, and a
    /// window still carrying the last question's maximum cannot grow into a
    /// longer one.
    fn fit(&self, window: &WebviewWindow, height: f64) -> Result<(), String> {
        let wanted = LogicalSize::new(WIDTH, height);
        window
            .set_min_size(None::<LogicalSize<f64>>)
            .and_then(|()| window.set_max_size(None::<LogicalSize<f64>>))
            .and_then(|()| window.set_size(wanted))
            .and_then(|()| window.set_min_size(Some(wanted)))
            .and_then(|()| window.set_max_size(Some(wanted)))
            .map_err(|error| format!("the form would not take its size: {error}"))
    }

    /// Takes the box down and gives the keyboard back.
    fn put_away(&self) {
        let Some(window) = self.app.get_webview_window(LABEL) else {
            return;
        };
        if !up(&self.app) {
            // Already down. Restoring focus anyway would take the keys off
            // whatever the person moved to.
            return;
        }
        // Asked before the window gives the keyboard up, because afterwards
        // there is nothing left to read. Only a window that had the keys gives
        // them back.
        let held = holds_keyboard(&self.app);
        // Refusing the keyboard first: a hidden window that still says it wants
        // keys is one a window manager can hand them to on the next map.
        if let Err(error) = window.set_focusable(false) {
            warn!("the form would not give the keyboard up: {error}");
        }
        if let Err(error) = window.hide() {
            warn!("the form could not be hidden: {error}");
            return;
        }
        match display::went_down(&window, &WINDOW) {
            Verdict::Yes | Verdict::Unsure if held => {
                if let Err(error) = self.focus.restore() {
                    debug!("the keyboard did not go back: {error}");
                }
            }
            Verdict::Yes | Verdict::Unsure => {}
            Verdict::No => warn!("the form is still up"),
        }
    }

    fn held(&self) -> MutexGuard<'_, Option<Pending>> {
        self.pending
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

/// Returns the form window, creating it hidden on first use.
///
/// Created with the pill at startup rather than on the first question: the page
/// has to be loaded and listening before the question that fills it arrives,
/// and a window built at that moment would miss it.
pub fn ensure(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(LABEL) {
        return Ok(window);
    }
    // The pill's recipe: opaque, undecorated, out of the taskbar, and pinned by
    // equal minimum and maximum size hints on a resizable window, which is the
    // one combination GTK honors and what makes a tiling window manager float
    // it. On top, unlike the HUD: this box stands over the panel that asked for
    // it, and panels are on top themselves.
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("form.html".into()))
        .title("Scufris")
        .inner_size(WIDTH, PAD * 2.0 + CROWN + CROWN_GAP + KEYS_GAP + KEYS)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        // Built down and built refusing the keyboard. What it says about the
        // keyboard from then on is only ever what `raise` and `put_away` last
        // said.
        .visible(false)
        .focused(false)
        .focusable(false)
        .build()
}

/// Answers what the display knows the box by, once it has made a window.
pub fn known_window() -> Option<u32> {
    match WINDOW.load(Ordering::SeqCst) {
        0 => None,
        id => Some(id),
    }
}

/// Answers whether the box is on screen, according to the display.
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

/// Answers whether the box is holding the keyboard right now.
fn holds_keyboard(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL)
        .is_some_and(|window| display::keyboard(&window, &WINDOW) == Verdict::Yes)
}

/// Puts the box on screen, in the order that gets it the keys.
///
/// Claiming the keyboard first, and every time, for the reason
/// [`crate::textbox`] documents at length: a window manager that unmanages a
/// hidden window and manages it again on the next show - i3 does - reads the
/// window's hints when it maps it, so a claim made after the show is a claim
/// made for the next one.
///
/// A box that cannot take the keyboard does not come up. It would be a form
/// nobody could fill in, sitting over the panel that asked for it.
fn raise(window: &WebviewWindow, panel: Option<Rect>, height: f64) -> Result<(), String> {
    window
        .set_focusable(true)
        .map_err(|error| format!("the form would not take the keyboard: {error}"))?;
    if let Err(error) = place(window, panel, height) {
        // Chrome. A box in the wrong place is still a box that can be typed in.
        warn!("the form could not be placed: {error}");
    }
    window
        .show()
        .map_err(|error| format!("the form could not be shown: {error}"))?;
    if display::came_up(window, &WINDOW) == Verdict::No {
        return Err("the form did not come up".into());
    }
    if let Err(error) = window.set_always_on_top(true) {
        warn!("the form could not be kept on top: {error}");
    }
    // Asking is not holding: a window manager may accept the request and hand
    // the keyboard elsewhere, or later. Nothing is reported - the field is
    // there to click, and the panel is not waiting on an answer.
    if let Err(error) = window.set_focus() {
        debug!("the form could not take the keyboard: {error}");
    }
    Ok(())
}

/// Puts the box over the panel that asked, or in the middle of the monitor.
fn place(window: &WebviewWindow, panel: Option<Rect>, height: f64) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return Ok(());
    };
    let at = monitor.position();
    let size = monitor.size();
    let screen = Rect {
        x: at.x,
        y: at.y,
        width: size.width as i32,
        height: size.height as i32,
    };
    let anchor = panel.unwrap_or(screen);
    window.set_position(box_over(anchor, screen, height, monitor.scale_factor()))
}

/// Returns where one window is, as the placement math sees it.
pub fn frame(window: &WebviewWindow) -> Option<Rect> {
    let at = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    Some(Rect {
        x: at.x,
        y: at.y,
        width: size.width as i32,
        height: size.height as i32,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn asking(fields: Value) -> Value {
        json!({"title": "New task", "fields": fields, "action": {"action": "add"}})
    }

    #[test]
    fn an_ask_says_what_it_wants_and_what_to_do_with_it() {
        let ask = Ask::parse(asking(json!([{"name": "text", "label": "Task"}])))
            .expect("a well formed ask");
        assert_eq!(ask.title, "New task");
        assert_eq!(ask.fields.len(), 1);
        assert_eq!(ask.fields[0].name, "text");
        assert_eq!(ask.fields[0].lines, 1, "a field is one line unless it says");
        assert_eq!(ask.action.get("action"), Some(&json!("add")));
    }

    /// Every bound is here rather than on the page. A widget can come from
    /// outside this build - `SCUFRIS_WIDGET_PATH` installs a directory - so
    /// what it says about a window is checked before the window is built to it.
    #[test]
    fn an_ask_that_is_not_one_is_refused_rather_than_drawn() {
        for (asked, why) in [
            (json!("text"), "not an object"),
            (json!({"title": "x", "fields": []}), "no action"),
            (asking(json!([])), "no fields"),
            (json!({"fields": [{"name": "a"}], "action": {}}), "no title"),
            (asking(json!([{"label": "Task"}])), "a field with no name"),
        ] {
            assert!(Ask::parse(asked).is_err(), "{why} was accepted");
        }
        let many: Vec<Value> = (0..FIELDS + 1)
            .map(|index| json!({"name": index.to_string()}))
            .collect();
        assert!(Ask::parse(asking(json!(many))).is_err(), "too many fields");
    }

    /// A widget that asked for a hundred lines would be a widget sizing the
    /// companion's window, and one that asked for a title the width of the
    /// monitor would be a widget writing on it.
    #[test]
    fn a_field_is_bounded_by_this_build_rather_than_by_the_widget() {
        let ask = Ask::parse(asking(
            json!([{"name": "body", "lines": 400, "label": "x".repeat(400)}]),
        ))
        .expect("a well formed ask");
        assert_eq!(ask.fields[0].lines, LINES);
        assert_eq!(ask.fields[0].label.chars().count(), WORDS);
        assert!(ask.height() <= CEILING);
    }

    #[test]
    fn the_box_is_as_tall_as_the_form_the_page_lays_out() {
        // form.css lays out to exactly these logical pixels, and the window
        // cannot be resized once it is up.
        let one = Ask::parse(asking(json!([{"name": "text"}]))).expect("an ask");
        assert_eq!(one.height(), 131.0);
        let two = Ask::parse(asking(
            json!([{"name": "heading"}, {"name": "body", "lines": 5}]),
        ))
        .expect("an ask");
        assert_eq!(two.height(), 273.0);
    }

    /// The page is handed labels and hands back words. What those words mean
    /// was decided when the panel asked, so a page that answered with a key
    /// nobody asked for cannot name an argument the backend reads.
    #[test]
    fn only_the_fields_that_were_asked_for_reach_the_backend() {
        let ask = Ask::parse(asking(json!([{"name": "text"}]))).expect("an ask");
        let answers = BTreeMap::from([
            ("text".to_string(), "  Call the dentist ".to_string()),
            ("den".to_string(), "/etc".to_string()),
        ]);
        assert_eq!(
            ask.fill(&answers),
            json!({"action": "add", "text": "Call the dentist"})
        );
    }

    /// A one-line field is one line whatever is pasted into it: the answers go
    /// on to `today`, and a task carrying a newline is a task text that is not
    /// one. A block keeps its shape, because that shape is the note.
    #[test]
    fn a_one_line_field_answers_with_one_line() {
        assert_eq!(tidy("  two\nlines  ", 1), "two lines");
        assert_eq!(tidy("  two\nlines  ", 4), "two\nlines");
    }

    /// A field the person left empty is still sent, as nothing. The backend is
    /// what decides whether a blank answer is worth writing, and one that
    /// arrived with the key missing would be indistinguishable from an older
    /// widget that never asked for it.
    #[test]
    fn a_blank_answer_is_still_an_answer() {
        let ask =
            Ask::parse(asking(json!([{"name": "heading"}, {"name": "body"}]))).expect("an ask");
        assert_eq!(
            ask.fill(&BTreeMap::from([("body".to_string(), "note".to_string())])),
            json!({"action": "add", "heading": "", "body": "note"})
        );
    }

    #[test]
    fn the_box_stands_over_the_panel_that_asked() {
        let screen = Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let panel = Rect {
            x: 1500,
            y: 200,
            width: 340,
            height: 520,
        };
        assert_eq!(
            over(panel, screen, 420, 131),
            PhysicalPosition::new(1460, 394)
        );
    }

    #[test]
    fn the_box_is_sized_from_the_question_and_never_from_the_window() {
        // Placement runs before the show, so the window has never been mapped
        // and answers no size. A live run found the box half its own width and
        // height down and to the right of the panel, which is exactly what
        // centring a box of nothing gives.
        let screen = Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let panel = Rect {
            x: 1556,
            y: 24,
            width: 340,
            height: 520,
        };
        let ask = Ask::parse(asking(json!([{"name": "text"}]))).expect("an ask");
        assert_eq!(ask.height(), 131.0);
        assert_eq!(
            box_over(panel, screen, ask.height(), 1.0),
            PhysicalPosition::new(1500, 218)
        );
        // What asking the window gave instead: the panel's own centre point.
        assert_eq!(over(panel, screen, 0, 0), PhysicalPosition::new(1726, 284));
    }

    #[test]
    fn a_box_on_a_doubled_display_is_measured_in_the_pixels_it_is_placed_in() {
        // Positions are physical and the question's height is logical, so a
        // scaled monitor has to be told apart from a bigger one.
        let screen = Rect {
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
        };
        let panel = Rect {
            x: 400,
            y: 200,
            width: 680,
            height: 1040,
        };
        assert_eq!(
            box_over(panel, screen, 131.0, 2.0),
            PhysicalPosition::new(320, 589)
        );
    }

    #[test]
    fn a_panel_near_an_edge_pushes_the_box_inward_rather_than_off() {
        let screen = Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let corner = Rect {
            x: 1860,
            y: 1040,
            width: 40,
            height: 40,
        };
        assert_eq!(
            over(corner, screen, 420, 131),
            PhysicalPosition::new(1500, 949)
        );
        // A second monitor to the right: the box never lands on the first one.
        let right = Rect {
            x: 1920,
            y: -120,
            width: 2560,
            height: 1440,
        };
        let edge = Rect {
            x: 1920,
            y: -120,
            width: 340,
            height: 520,
        };
        assert_eq!(over(edge, right, 420, 131), PhysicalPosition::new(1920, 74));
    }

    #[test]
    fn a_monitor_smaller_than_the_box_never_places_it_off_screen() {
        let small = Rect {
            x: 40,
            y: 60,
            width: 320,
            height: 100,
        };
        assert_eq!(over(small, small, 420, 131), PhysicalPosition::new(40, 60));
    }
}
