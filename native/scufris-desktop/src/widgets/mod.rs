//! The widgets runtime: a sibling of the pill, not a part of it.
//!
//! The pill owns the foreground conversation and one window. Widgets are the
//! other thing on screen: small panels that Scufris opens beside the pill while
//! it talks, and that the person can take over and keep. Nothing here reaches
//! the pill's state machine, and nothing there reaches this.
//!
//! The split follows the pill's: [`runtime`] decides and is pure, this module
//! carries the decisions out. What it carries them out against is [`pool`] -
//! warm shell windows - and the daemon link, which is where the answers go.

pub mod backends;
pub mod catalog;
pub mod pool;
pub mod runtime;
pub mod windows;

use std::{
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, Weak,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use scufris_control::service::{Posture, WidgetCommand, WidgetReport};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, ipc::Channel};
use tracing::{debug, info, warn};

use crate::{
    display,
    link::ServiceLink,
    state::Assistant,
    widgets::{
        backends::{Backends, News, Order},
        catalog::{Catalog, CatalogError, Source},
        pool::{Pool, ShellMsg},
        runtime::{Act, Cmd, Runtime, Still},
    },
};

// The widgets `build.rs` compiled into this binary, as `(directory, manifest,
// module)` triples. Generated rather than walked at startup: what ships is what
// was built, and a widget that did not compile is a build failure rather than a
// companion that is missing one.
include!(concat!(env!("OUT_DIR"), "/widgets.rs"));

/// How often the aging clock is asked how much time has gone by.
///
/// A second is far finer than the grace it measures, and a widget retiring one
/// second late is a widget nobody saw retire late. What the cadence must not be
/// is finer: this wakes a thread forever, and the pill's own idle cost is the
/// budget it has to stay inside.
const SWEEP: Duration = Duration::from_secs(1);

/// How often the backends hand over what they wrote.
///
/// Four times a second. Slow enough that a backend writing far faster than
/// anybody can read costs one message rather than hundreds - a webview handed
/// a raw tick stream is the documented way to make one hold gigabytes - and
/// fast enough that a number on screen still reads as live.
const BEAT: Duration = Duration::from_millis(250);

/// The runtime, its windows, its backends, and the way back to the daemon.
pub struct Widgets {
    catalog: Catalog,
    pool: Pool,
    runtime: Mutex<Runtime>,
    /// Held across one decision and everything that decision decides.
    ///
    /// The runtime decides under its own lock and releases it before the host
    /// carries the decisions out, which is what keeps a window move off the
    /// runtime's lock. Three threads arrive here - the daemon's reader, the
    /// aging clock by way of the event loop, and Tauri's command pool - and two
    /// of them interleaving a shelf reflow with a widget opening would put two
    /// windows in one column. This is the queue that stops it.
    ///
    /// It is held across the window work, which is where the waiting is: a first
    /// placement asks the display whether the window came up. That wait is the
    /// same one the companion always paid; what is new is that nothing else
    /// decides while it runs.
    turn: Mutex<()>,
    backends: Backends,
    link: OnceLock<Arc<ServiceLink>>,
    /// Whether the service welcomed this companion before there was a link to
    /// answer on.
    owed_catalog: AtomicBool,
    /// The last assistant state the companion showed. The turn boundary is read
    /// off the change rather than off a message of its own.
    assistant: Mutex<Assistant>,
    app: AppHandle,
}

impl Widgets {
    /// Reads the installed widgets and warms the first shells.
    ///
    /// A catalog that will not build stops the companion here. See
    /// [`catalog::Catalog::build`] for why that is better than starting.
    pub fn start(app: AppHandle) -> Result<Arc<Self>, CatalogError> {
        let names = backends::names();
        let mut catalog = Catalog::build(INSTALLED, &names)?;
        // After the shipped ones, because they are the ones that win. A root on
        // the search path adds to the fleet; it does not replace part of it.
        if let Ok(path) = std::env::var(catalog::WIDGET_PATH) {
            let found = catalog::search(&path);
            let sources: Vec<_> = found.iter().map(catalog::External::source).collect();
            catalog.extend(&sources, &names);
        }
        let widgets = Arc::new(Self {
            catalog,
            pool: Pool::new(app.clone()),
            runtime: Mutex::new(Runtime::new()),
            turn: Mutex::new(()),
            backends: Backends::new(),
            link: OnceLock::new(),
            owed_catalog: AtomicBool::new(false),
            assistant: Mutex::new(Assistant::Idle),
            app,
        });
        widgets.pool.warm();
        widgets.age();
        widgets.beat();
        Ok(widgets)
    }

    /// Stops every backend. The companion is going away.
    ///
    /// A backend is its own process group so that stopping one takes its
    /// children with it, which is also what stops it from dying with the
    /// companion on its own. Somebody has to say so, and this is that.
    pub fn halt(&self) {
        self.backends.halt();
    }

    /// Starts the clock that hands the runtime the time that has gone by.
    ///
    /// The runtime counts elapsed time rather than reading a clock, which is
    /// what lets the whole grace be a unit test and what lets a stopped clock
    /// be a field rather than an arithmetic correction. Somebody has to do the
    /// reading, and it is this thread. It holds a weak handle so that it ends
    /// with the runtime rather than keeping it alive.
    ///
    /// The sweep itself runs on the event loop rather than here. The runtime
    /// decides under its own lock and the host carries the decisions out after
    /// releasing it, so a thread that performed its own acts could interleave a
    /// shelf reflow with a widget opening. The chrome ticks already reach the
    /// runtime from the event loop; the clock joins them rather than becoming a
    /// third place that window moves come from.
    fn age(self: &Arc<Self>) {
        let waking = Arc::downgrade(self);
        thread::spawn(move || {
            let mut last = Instant::now();
            loop {
                thread::sleep(SWEEP);
                let now = Instant::now();
                // Measured rather than assumed: a machine that was asleep, or
                // busy, did not spend one second per wake-up, and the grace is
                // a promise about time rather than about wake-ups.
                let elapsed = now.duration_since(last);
                last = now;
                let Some(widgets) = Weak::upgrade(&waking) else {
                    return;
                };
                let clock = Arc::clone(&widgets);
                if let Err(error) = widgets
                    .app
                    .run_on_main_thread(move || clock.decide(Cmd::Sweep { elapsed }))
                {
                    debug!("the widget clock could not reach the event loop: {error}");
                }
            }
        });
    }

    /// Starts the beat that hands over what the backends wrote.
    ///
    /// Separate from the aging sweep because it is four times as fast and
    /// because it reads something else, and on the event loop for the reason
    /// the sweep is: the runtime decides under its own lock and the host
    /// carries the decisions out after releasing it, so a thread performing its
    /// own acts would be one more place window moves come from.
    fn beat(self: &Arc<Self>) {
        let waking = Arc::downgrade(self);
        thread::spawn(move || {
            let mut last = Instant::now();
            loop {
                thread::sleep(BEAT);
                let now = Instant::now();
                let elapsed = now.duration_since(last);
                last = now;
                let Some(widgets) = Weak::upgrade(&waking) else {
                    return;
                };
                let news = widgets.backends.drain(elapsed);
                if news.is_empty() {
                    continue;
                }
                let carrier = Arc::clone(&widgets);
                if let Err(error) = widgets.app.run_on_main_thread(move || carrier.hear(news)) {
                    debug!("a widget backend could not reach the event loop: {error}");
                }
            }
        });
    }

    /// Carries one beat of backend news into the runtime.
    fn hear(&self, news: Vec<News>) {
        for one in news {
            match one {
                News::Data { surface, data } => self.decide(Cmd::Feed { surface, data }),
                News::Health { surface, health } => {
                    self.decide(Cmd::Health { surface, health });
                }
            }
        }
    }

    /// Records that the person used one surface's restart tick.
    pub fn restarted(&self, surface: String) {
        self.decide(Cmd::Restart { surface });
    }

    /// Carries one widget's action toward whatever feeds it.
    pub fn sent(&self, surface: String, action: Value) {
        self.decide(Cmd::Sent { surface, action });
    }

    /// Records the assistant state the daemon reported.
    ///
    /// Two things are read off it. Falling back to idle after working or
    /// speaking is one turn of the conversation ending, which is what tells an
    /// exhibit the subject has moved on: it costs no message of its own, and
    /// the daemon already sends this one. And time spent speaking is time the
    /// person is listening rather than reading, so the grace does not run.
    pub fn assistant(&self, state: Assistant) {
        let previous = {
            let mut held = self
                .assistant
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let previous = *held;
            *held = state;
            previous
        };
        if state == previous {
            return;
        }
        self.decide(Cmd::Freeze {
            reason: Still::Speech,
            stopped: state == Assistant::Speaking,
        });
        let turned = matches!(previous, Assistant::Working | Assistant::Speaking)
            && state == Assistant::Idle;
        if turned {
            self.decide(Cmd::TurnEnded);
        }
    }

    /// Records the pointer arriving over one surface, or leaving it.
    pub fn hover(&self, surface: String, over: bool) {
        self.decide(Cmd::Hover { surface, over });
    }

    /// Records the microphone opening or closing.
    ///
    /// A person who is talking is not reading the screen, so the grace an
    /// exhibit has left is not spent while they do.
    pub fn recording(&self, recording: bool) {
        self.decide(Cmd::Freeze {
            reason: Still::Microphone,
            stopped: recording,
        });
    }

    /// Takes the runtime's own widgets down with the pill, or brings them back.
    ///
    /// The pill and everything the runtime put beside it are one layer, and the
    /// gesture that puts the pill away puts the layer away. Nothing is retired
    /// and no widget is unmounted: the panels come back exactly as they were,
    /// and the grace they had left is the grace they still have.
    pub fn conceal(&self, hidden: bool) {
        self.decide(Cmd::Conceal { hidden });
    }

    /// Gives the runtime the link its answers travel on.
    ///
    /// Separate from [`Widgets::start`] because the link is started with a
    /// closure that already routes commands here: one of the two has to exist
    /// first, and it is this one.
    pub fn attach(&self, link: Arc<ServiceLink>) {
        if self.link.set(link).is_err() {
            return;
        }
        // The link's reader thread starts inside `ServiceLink::start`, which
        // returns before this runs, so a welcome can arrive with nowhere to
        // answer it. What that welcome wanted is the catalog, and this is the
        // first moment there is a way to send it.
        if self.owed_catalog.swap(false, Ordering::SeqCst) {
            self.announce();
        }
    }

    /// Tells the daemon what widgets are installed.
    ///
    /// Sent once per connection, right after the daemon welcomes the hello. The
    /// daemon types its widget tool from this, so a companion that never sends
    /// it is a companion whose widgets cannot be named.
    pub fn announce(&self) {
        if self.link.get().is_none() {
            // The welcome beat the attach. Remembered rather than dropped: this
            // is the one message a session cannot do without, and losing it
            // leaves the model with no widget it is allowed to name for as long
            // as the session lasts.
            self.owed_catalog.store(true, Ordering::SeqCst);
            return;
        }
        self.report(WidgetReport::Catalog {
            widgets: self.catalog.entries(),
        });
    }

    /// Returns every widget window the display has named.
    ///
    /// For the focus tracker. A widget shell is built unfocusable and stays
    /// that way, so a capture that recorded one as the window to go back to
    /// would hand the person's keys to a window that refuses them.
    pub fn windows(&self) -> Vec<u32> {
        self.pool.shown()
    }

    /// Records that one shell page has loaded and is listening.
    pub fn ready(&self, label: String, channel: Channel<ShellMsg>) {
        self.pool.ready(label, channel);
    }

    /// Returns the module the window holding this surface is allowed to import.
    ///
    /// One surface, one module, and nothing else is reachable. The scheme
    /// handler asks with the label of the window that made the request, so a
    /// page cannot ask for a widget it is not holding however it writes the URL.
    pub fn module(&self, surface: &str) -> Option<String> {
        let widget = self.runtime().surface(surface)?.widget.clone();
        self.catalog.get(&widget).map(|found| found.script.clone())
    }

    /// Carries out one command from the daemon.
    pub fn command(&self, command: WidgetCommand) {
        match command {
            WidgetCommand::Open {
                id,
                widget,
                posture,
                data,
            } => self.open(Some(id), widget, posture, data),
            WidgetCommand::Update { id, surface, data } => {
                self.decide(Cmd::Update { id, surface, data });
            }
            WidgetCommand::Close { id, surface } => self.decide(Cmd::Close { id, surface }),
            WidgetCommand::Clear { id } => self.decide(Cmd::Clear { id }),
        }
    }

    /// Records that the person closed one surface with its own chrome tick.
    pub fn dismissed(&self, surface: String) {
        self.decide(Cmd::Dismissed { surface });
    }

    /// Records that the person used one surface's pin tick.
    pub fn pinned(&self, surface: String) {
        self.decide(Cmd::Pin { surface });
    }

    /// Returns the widgets the tray offers, as `(identifier, name)`.
    pub fn summonable(&self) -> Vec<(String, String)> {
        self.catalog.summonable()
    }

    /// Opens one instrument because the person asked the tray for it.
    ///
    /// Nothing about it goes back to the daemon. Scufris finds out the way it
    /// finds out about a widget the person closed: by being told, if it ever
    /// asks. The desktop is the person's, and a panel they put up themselves
    /// is not a turn in the conversation.
    ///
    /// It carries no payload, which is why only a widget with a backend behind
    /// it can be summoned: that backend has to stand up on its own defaults.
    pub fn summon(&self, widget: String) {
        info!(widget, "summoned from the tray");
        self.open(None, widget, Posture::Instrument, json!({}));
    }

    /// Reserves a shell, then asks the runtime what to do with it.
    ///
    /// The shell is reserved first because its label is the surface identifier:
    /// the pool mints labels and never hands one out twice, so a surface the
    /// daemon has been told about can never be confused with a later one. A
    /// runtime that then refuses the open leaves the shell unused, and it is
    /// discarded rather than kept, for the same reason.
    fn open(&self, id: Option<String>, widget: String, posture: Posture, data: Value) {
        if self.catalog.get(&widget).is_none() {
            self.refuse(id, "widget_not_found", format!("no widget named {widget}"));
            return;
        }
        let Some(surface) = self.pool.take() else {
            self.refuse(
                id,
                "no_shell",
                "no widget window could be made ready".into(),
            );
            return;
        };
        let _turn = self.turn();
        let acts = self.runtime().apply(
            &self.catalog,
            Cmd::Open {
                id,
                surface: surface.clone(),
                widget,
                posture,
                data,
            },
        );
        let adopted = acts
            .iter()
            .any(|act| matches!(act, Act::Adopt { surface: opened, .. } if opened == &surface));
        self.settle(acts);
        if !adopted {
            self.pool.discard(&surface);
        }
    }

    /// Answers a refused open, when there is a request waiting on an answer.
    ///
    /// The runtime's own refusals go the same way; this is the pair of them
    /// that happen before the runtime is ever asked.
    fn refuse(&self, id: Option<String>, code: &str, detail: String) {
        match id {
            Some(id) => self.report(WidgetReport::Failed {
                id,
                code: code.into(),
                detail,
            }),
            None => warn!(code, "a summoned widget was refused: {detail}"),
        }
    }

    fn decide(&self, cmd: Cmd) {
        let _turn = self.turn();
        let acts = self.runtime().apply(&self.catalog, cmd);
        self.settle(acts);
    }

    /// Carries out one batch, and then whatever that batch's own failures
    /// decide.
    ///
    /// Two rounds and no more. A surface whose window never came up is retired,
    /// retiring reflows the shelf, and a reflow is only ever a move - which is
    /// warned about rather than failed - so the second round has nothing left to
    /// lose.
    fn settle(&self, acts: Vec<Act>) {
        let mut acts = acts;
        for _ in 0..2 {
            let lost = self.perform(acts);
            if lost.is_empty() {
                return;
            }
            acts = lost
                .into_iter()
                .flat_map(|surface| self.runtime().apply(&self.catalog, Cmd::Lost { surface }))
                .collect();
        }
    }

    /// Carries out one batch of decisions, and answers with the surfaces whose
    /// windows never reached the screen.
    ///
    /// A placement that fails is not a log line. The daemon is answered from
    /// this same batch, and a panel reported open and never shown is one Scufris
    /// talks about and nobody can read, so the surfaces that failed come back
    /// for the caller to retire.
    fn perform(&self, acts: Vec<Act>) -> Vec<String> {
        let mut lost: Vec<(String, String)> = Vec::new();
        for act in acts {
            match act {
                Act::Adopt {
                    surface,
                    widget,
                    name,
                    data,
                    slot,
                    size,
                    hidden,
                } => {
                    self.pool.send(
                        &surface,
                        ShellMsg::Become {
                            surface: surface.clone(),
                            widget,
                            name,
                            data,
                        },
                    );
                    let placed = if hidden {
                        // Sized and loaded behind the layer. It comes up with
                        // the rest of them when the pill does.
                        self.fit(&surface, size)
                    } else {
                        self.place(&surface, slot, size, true)
                    };
                    if let Err(detail) = placed {
                        lost.push((surface, detail));
                    }
                }
                Act::Conceal {
                    surface,
                    hidden,
                    slot,
                    size,
                } => {
                    if hidden {
                        self.take_down(&surface);
                    } else if let Err(detail) = self.place(&surface, slot, size, true) {
                        // Through the first-show path, because i3 places a
                        // floating window when it maps it: a window coming back
                        // has to be placed after the map, exactly as it was the
                        // first time. And a window that will not come back is a
                        // widget the person cannot see, so it goes rather than
                        // staying in the runtime as a panel nobody can read.
                        lost.push((surface, detail));
                    }
                }
                Act::Move {
                    surface,
                    slot,
                    size,
                } => {
                    if let Err(error) = self.place(&surface, slot, size, false) {
                        // A window in the wrong place is worse than one in the
                        // right place and much better than one that is gone.
                        warn!(surface, "{error}");
                    }
                }
                Act::Update { surface, data } => {
                    self.pool.send(&surface, ShellMsg::Update { data });
                }
                Act::Life { surface, life } => {
                    self.pool.send(&surface, ShellMsg::Life { state: life });
                }
                Act::Health { surface, health } => {
                    self.pool.send(&surface, ShellMsg::Health { state: health });
                }
                Act::Subscribe {
                    surface,
                    backend,
                    spawn,
                    cadence,
                    shared,
                    restart,
                } => {
                    let Some(installed) = backends::installed(&backend) else {
                        // The catalog refuses a widget that names a backend
                        // nothing installs, so reaching this means the table
                        // and the catalog disagree.
                        warn!(surface, backend, "no such widget backend");
                        continue;
                    };
                    let order = Order {
                        backend: installed,
                        spawn: &spawn,
                        cadence,
                        shared,
                    };
                    if restart {
                        self.backends.restart(surface, order);
                    } else {
                        self.backends.subscribe(surface, order);
                    }
                }
                Act::Unsubscribe { surface } => self.backends.unsubscribe(&surface),
                Act::Send { surface, action } => self.backends.send(&surface, &action),
                Act::Stick { surface, sticky } => self.stick(&surface, sticky),
                Act::Refuse { surface, detail } => {
                    self.pool.send(&surface, ShellMsg::Refused { detail });
                }
                Act::Retire { surface } => self.pool.discard(&surface),
                Act::Report(WidgetReport::Opened { id, surface })
                    if lost.iter().any(|(gone, _)| gone == &surface) =>
                {
                    // The open is answered where it happened. A `widget_opened`
                    // for a window the display refused would leave Scufris
                    // talking about a panel nobody can see, and holding a
                    // surface identifier for one that is about to be retired.
                    let detail = lost
                        .iter()
                        .find(|(gone, _)| gone == &surface)
                        .map_or_else(String::new, |(_, why)| why.clone());
                    warn!(surface, "a widget never reached the screen: {detail}");
                    self.report(WidgetReport::Failed {
                        id,
                        code: "not_shown".into(),
                        detail,
                    });
                }
                Act::Report(body) => self.report(body),
            }
        }
        lost.into_iter().map(|(surface, _)| surface).collect()
    }

    /// Gives one widget window the size its widget lays out, and nothing else.
    fn fit(&self, surface: &str, size: runtime::Size) -> Result<(), String> {
        let Some(window) = self.app.get_webview_window(surface) else {
            return Err(format!("{surface} has no window"));
        };
        windows::fit(&window, size)
    }

    /// Puts one widget window on every workspace, or brings it back to this one.
    ///
    /// An exhibit belongs to the layer that follows the person around, so it is
    /// on every workspace the way i3's own scratchpad is. Pinning it is what
    /// brings it down onto the workspace they are looking at. Nothing here
    /// touches i3's real scratchpad: the whole mechanism is one window state.
    fn stick(&self, surface: &str, sticky: bool) {
        let Some(window) = self.app.get_webview_window(surface) else {
            warn!(surface, "a widget surface has no window");
            return;
        };
        if let Err(error) = display::sticky(&window, &AtomicU32::new(0), sticky) {
            // A desktop that will not take the state is a widget on one
            // workspace, which is worse than on all of them and much better
            // than not being up at all.
            warn!(surface, "{error}");
        }
    }

    /// Takes one widget window off the screen, leaving the widget mounted.
    fn take_down(&self, surface: &str) {
        let Some(window) = self.app.get_webview_window(surface) else {
            warn!(surface, "a widget surface has no window");
            return;
        };
        if let Err(error) = windows::conceal(&window) {
            warn!(surface, "{error}");
        }
    }

    /// Puts one widget window where its slot says it belongs, and says whether
    /// it got there.
    ///
    /// A first placement that fails is a widget nobody can see, so the failure
    /// travels back to the caller. A later move that fails is a widget in the
    /// wrong place, which the caller only warns about.
    fn place(
        &self,
        surface: &str,
        slot: runtime::Slot,
        size: runtime::Size,
        first: bool,
    ) -> Result<(), String> {
        let Some(window) = self.app.get_webview_window(surface) else {
            return Err(format!("{surface} has no window"));
        };
        if first {
            // Sized before it maps, so it never appears at the placeholder's
            // size - and because equal min and max hints are what make a tiling
            // window manager float it. A window that will not take them is a
            // window i3 tiles into whatever the person was doing, which is worse
            // than no widget at all.
            windows::fit(&window, size)?;
            // Then shown, and only then asked which monitor it is on.
            // `current_monitor` answers from where the window is, and a window
            // that has never mapped is not anywhere: on more than one screen it
            // names the primary and puts the widget where nobody is looking.
            windows::raise(&window, &self.pool.named(surface))?;
        }
        let Some(monitor) = windows::monitor(&window) else {
            return Err("nothing could say which monitor a widget is on".into());
        };
        windows::settle(&window, runtime::place(slot, size, &monitor))
    }

    fn report(&self, report: WidgetReport) {
        let Some(link) = self.link.get() else {
            debug!("a widget answer had no link to travel on");
            return;
        };
        if let Err(error) = link.report(report) {
            debug!("a widget answer did not reach the agent: {error}");
        }
    }

    fn runtime(&self) -> MutexGuard<'_, Runtime> {
        self.runtime
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn turn(&self) -> MutexGuard<'_, ()> {
        self.turn.lock().unwrap_or_else(|error| error.into_inner())
    }
}

/// Answers whether one window label belongs to a widget shell.
pub fn is_shell(label: &str) -> bool {
    label.starts_with(windows::LABEL_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::runtime::Life;

    #[test]
    fn the_pill_and_the_textbox_are_not_widget_shells() {
        // The window event handler routes by label. A prefix that caught the
        // pill would close the pill the first time a widget was dismissed.
        assert!(is_shell("widget-1"));
        assert!(!is_shell(crate::pill::LABEL));
        assert!(!is_shell(crate::textbox::LABEL));
    }

    #[test]
    fn a_pinned_surface_tells_its_chrome_before_anything_moves() {
        // The badge is the only feedback the person gets for the pin tick, and
        // the shelf closing up behind it is what they see next.
        let widgets =
            Catalog::build(INSTALLED, &backends::names()).expect("the shipped widgets install");
        let mut runtime = Runtime::new();
        let acts = runtime.apply(
            &widgets,
            Cmd::Open {
                id: Some("w-1".into()),
                surface: "widget-1".into(),
                widget: "note".into(),
                posture: Posture::Exhibit,
                data: serde_json::json!({ "text": "hello" }),
            },
        );
        assert!(matches!(acts.first(), Some(Act::Adopt { .. })));
        let acts = runtime.apply(
            &widgets,
            Cmd::Pin {
                surface: "widget-1".into(),
            },
        );
        assert_eq!(
            acts.first(),
            Some(&Act::Life {
                surface: "widget-1".into(),
                life: Life::Pinned,
            })
        );
        // And the move to the person's own slot comes after it, so the badge
        // has already changed by the time the window travels.
        assert!(
            acts.iter()
                .any(|act| matches!(act, Act::Move { surface, .. } if surface == "widget-1"))
        );
    }
}
