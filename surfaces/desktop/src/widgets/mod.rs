//! The widgets runtime: a sibling of the pill, not a part of it.
//!
//! The pill owns the foreground conversation and one window. Widgets are the
//! other thing on screen: small panels that Scufris opens beside the pill while
//! it talks, and that the person can take over and keep. Nothing here reaches
//! the pill's state machine, and nothing there reaches this.
//!
//! The split follows the pill's: [`runtime`] decides and is pure, this module
//! carries the decisions out. What it carries them out against is [`pool`] -
//! warm shell windows - and the service link, which is where the answers go.

pub mod backends;
pub mod catalog;
pub mod pool;
pub mod protocol;
pub mod runtime;
pub mod turn;
pub mod windows;

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard, Weak, atomic::AtomicU32},
    thread,
    time::{Duration, Instant},
};

use scufris_control::service::{WidgetCall, WidgetDefinition};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, ipc::Channel};
use tracing::{debug, info, warn};

use crate::{
    display,
    form::{self, Form},
    state::Assistant,
    widgets::{
        backends::{Backends, News, Order},
        catalog::{Catalog, CatalogError, Source},
        pool::{Pool, ShellMsg},
        protocol::{Posture, WidgetReport},
        runtime::{Act, Cmd, Runtime, Still},
        turn::Turn,
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

/// The runtime, its windows, its backends, and the way back to the service.
pub struct Widgets {
    catalog: Catalog,
    pool: Pool,
    runtime: Mutex<Runtime>,
    /// Held across one decision and everything that decision decides.
    ///
    /// The runtime decides under its own lock and releases it before the host
    /// carries the decisions out, which is what keeps a window move off the
    /// runtime's lock. Three threads arrive here - the service's reader, the
    /// aging clock by way of the event loop, and Tauri's command pool - and two
    /// of them interleaving a shelf reflow with a widget opening would put two
    /// windows in one column. This is the queue that stops it.
    ///
    /// It is held across the window work, which is where the waiting is: a first
    /// placement asks the display whether the window came up, and asks the
    /// toolkit which monitor the window landed on. The event loop is what
    /// answers both, so it is the one thread that never waits here - it hands
    /// what it cannot decide now to a thread that can. See [`turn`].
    turn: Turn<Asked>,
    backends: Backends,
    /// The one window a panel may borrow to take words in. Held here because
    /// the answer is an action for a surface's backend, and this is what knows
    /// which surfaces there are.
    form: Form,
    /// The last assistant state the companion showed. The turn boundary is read
    /// off the change rather than off a message of its own.
    assistant: Mutex<Assistant>,
    app: AppHandle,
}

/// One thing the widget runtime was asked for, as it was asked for.
///
/// A [`Cmd`] is what the runtime is given. An open only becomes one after a
/// shell has been reserved to carry it, and reserving one is itself a wait. So
/// the two travel together: either can be handed to the thread that is allowed
/// to wait, and an open handed over waits for its shell there rather than on
/// the event loop.
enum Asked {
    Command(Cmd),
    Open {
        id: Option<String>,
        widget: String,
        posture: Posture,
        data: Value,
    },
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
            turn: Turn::new(),
            backends: Backends::new(),
            form: Form::new(app.clone()),
            assistant: Mutex::new(Assistant::Idle),
            app,
        });
        widgets.pool.warm();
        widgets.staff();
        widgets.age();
        widgets.beat();
        Ok(widgets)
    }

    /// Gives the turn's queue a thread of its own.
    ///
    /// Before the clock and the beat, because both of them decide from the
    /// event loop and the event loop is what the queue exists for. It holds a
    /// weak handle like they do, so it ends with the runtime rather than
    /// keeping it alive.
    fn staff(self: &Arc<Self>) {
        let waking = Arc::downgrade(self);
        self.turn.staff(move |asked| {
            let Some(widgets) = Weak::upgrade(&waking) else {
                return;
            };
            widgets.now(asked);
        });
    }

    /// Carries out one thing that was asked for, waiting for the turn.
    ///
    /// Whoever gets here is allowed to wait: either a thread that arrived with
    /// its own decision, or the queue's, which exists so that the event loop
    /// never has to.
    fn now(&self, asked: Asked) {
        match asked {
            Asked::Command(cmd) => {
                let _turn = self.turn.wait();
                let acts = self.runtime().apply(&self.catalog, cmd);
                self.settle(acts);
            }
            Asked::Open {
                id,
                widget,
                posture,
                data,
            } => self.opening(id, widget, posture, data),
        }
    }

    /// Hands one thing over to the thread that can wait for its turn.
    fn hand(&self, asked: Asked) {
        if !self.turn.later(asked) {
            debug!("a widget decision had nowhere to wait for its turn");
        }
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

    /// Puts one widget's question on the form box.
    pub fn asked(&self, surface: String, ask: Value) {
        self.decide(Cmd::Ask { surface, ask });
    }

    /// Sends the answered form on, as the action the panel asked for.
    ///
    /// The two halves are kept apart on purpose: the box knows the words and
    /// nothing about surfaces, and this knows the surfaces and nothing about
    /// the words. What joins them is the ask the box was opened with.
    pub fn answered(&self, answers: &BTreeMap<String, String>) {
        let Some((surface, action)) = self.form.submit(answers) else {
            debug!("a form was answered with no question on it");
            return;
        };
        self.sent(surface, action);
    }

    /// Asks the backend what a field could be, while it is being typed.
    ///
    /// The same road an answer takes, because it is the same kind of thing: an
    /// action on the backend of the panel that asked. What comes back is an
    /// ordinary reading, which reaches the box through [`form::Form::saw`].
    pub fn looking(&self, field: &str, text: &str) {
        let Some((surface, action)) = self.form.look(field, text) else {
            debug!(field, "a form looked up a field nothing asked about");
            return;
        };
        self.sent(surface, action);
    }

    /// The question the box is holding, for a form page that has just loaded.
    pub fn asking(&self) -> Option<form::Ask> {
        self.form.asked()
    }

    /// Puts the form box away with nothing written.
    pub fn dropped(&self) {
        self.form.cancel();
    }

    /// Records the assistant state the service reported.
    ///
    /// Two things are read off it. Falling back to idle after working or
    /// speaking is one turn of the conversation ending, which is what tells an
    /// exhibit the subject has moved on: it costs no message of its own, and
    /// the service already sends this one. And time spent speaking is time the
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

    /// Widget definitions registered in `surface.hello` on every connection.
    pub fn definitions(&self) -> Vec<WidgetDefinition> {
        self.catalog.entries()
    }

    /// Executes one live atomic response call as best-effort presentation.
    pub fn call(&self, call: WidgetCall) {
        self.open(Some(call.id), call.name, Posture::Exhibit, call.arguments);
    }

    /// Returns every widget window the display has named.
    ///
    /// For the focus tracker. A widget shell is built unfocusable and stays
    /// that way, so a capture that recorded one as the window to go back to
    /// would hand the person's keys to a window that refuses them.
    pub fn windows(&self) -> Vec<u32> {
        self.pool.shown()
    }

    /// Answers whether the layer is holding any panel at all.
    ///
    /// Read rather than waited for: this answers a keypress, and the runtime
    /// lock is only ever held for a decision, never across the window work. A
    /// panel that is mounted but concealed still counts - it is on the layer,
    /// and raising the layer is what would show it.
    pub fn holding(&self) -> bool {
        self.runtime().holding()
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
    /// Nothing about it goes back to the service. Scufris finds out the way it
    /// finds out about a widget the person closed: by being told, if it ever
    /// asks. The desktop is the person's, and a panel they put up themselves
    /// is not a turn in the conversation.
    ///
    /// It carries no payload of the person's, so what its backend is told is
    /// whatever the widget's own manifest declares - and a widget that declares
    /// nothing has a backend that stands up on its own defaults or is not worth
    /// summoning.
    pub fn summon(&self, widget: String) {
        info!(widget, "summoned from the tray");
        self.open(None, widget, Posture::Instrument, json!({}));
    }

    /// Opens one widget, here or on the thread that is allowed to wait.
    ///
    /// An open is the one decision that waits twice: once for a shell to be
    /// free, which is up to three seconds, and again for the window it reserves
    /// to reach the screen, which the event loop is what carries out. Neither
    /// wait can be taken on that loop, so an open asked for there - the tray's
    /// summon is the one that is - always goes to the queue. It is never worth
    /// trying the turn first: a free turn would not shorten either wait.
    fn open(&self, id: Option<String>, widget: String, posture: Posture, data: Value) {
        if display::on_the_event_loop() {
            self.hand(Asked::Open {
                id,
                widget,
                posture,
                data,
            });
            return;
        }
        self.opening(id, widget, posture, data);
    }

    /// Reserves a shell, then asks the runtime what to do with it.
    ///
    /// The shell is reserved first because its label is the surface identifier:
    /// the pool mints labels and never hands one out twice, so a surface the
    /// service has been told about can never be confused with a later one. A
    /// runtime that then refuses the open leaves the shell unused, and it is
    /// discarded rather than kept, for the same reason.
    fn opening(&self, id: Option<String>, widget: String, posture: Posture, data: Value) {
        let Some(spawn) = self.catalog.get(&widget).map(|found| found.spawn.clone()) else {
            self.refuse(id, "widget_not_found", format!("no widget named {widget}"));
            return;
        };
        // Both roads into an open pass here, which is why the manifest's own
        // keys are laid under the caller's here rather than at either end.
        let data = beneath(spawn.as_ref(), data);
        let Some(surface) = self.pool.take() else {
            self.refuse(
                id,
                "no_shell",
                "no widget window could be made ready".into(),
            );
            return;
        };
        let _turn = self.turn.wait();
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

    /// Asks the runtime for one thing, here or on the thread that can wait.
    ///
    /// The event loop takes the turn only if the turn is free. It cannot wait
    /// for one: whoever holds it is very likely waiting for this loop to place
    /// a window, and a loop waiting for that thread would be waiting for
    /// itself. The decision is not dropped - it goes to the queue, and the
    /// thread working that queue does the waiting instead.
    fn decide(&self, cmd: Cmd) {
        if display::on_the_event_loop() {
            let Some(_turn) = self.turn.free() else {
                self.hand(Asked::Command(cmd));
                return;
            };
            let acts = self.runtime().apply(&self.catalog, cmd);
            self.settle(acts);
            return;
        }
        self.now(Asked::Command(cmd));
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
    /// A placement that fails is not a log line. The service is answered from
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
                    // The box first. It is asking on this backend's behalf, and
                    // what it is waiting for is in the same reading the panel
                    // draws.
                    self.form.saw(&surface, &data);
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
                Act::Ask { surface, ask } => {
                    // A question that cannot be put up is a tick that did
                    // nothing, and a tick that does nothing reads as broken.
                    if let Err(detail) = self.ask(&surface, ask) {
                        self.pool.send(&surface, ShellMsg::Refused { detail });
                    }
                }
                Act::Stick { surface, sticky } => self.stick(&surface, sticky),
                Act::Refuse { surface, detail } => {
                    self.pool.send(&surface, ShellMsg::Refused { detail });
                }
                Act::Retire { surface } => {
                    // A box still asking about a panel that has gone is a box
                    // whose answer has nowhere to land.
                    self.form.forget(&surface);
                    self.pool.discard(&surface);
                }
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

    /// Puts one question on the form box, over the panel that asked it.
    ///
    /// What a question may say is decided here rather than on the box's page: a
    /// widget can come from outside this build - `SCUFRIS_WIDGET_PATH` installs
    /// a directory - and the page would otherwise be a window sized and titled
    /// by whoever wrote the widget.
    fn ask(&self, surface: &str, ask: Value) -> Result<(), String> {
        let ask = form::Ask::parse(ask)?;
        let panel = self
            .app
            .get_webview_window(surface)
            .and_then(|window| form::frame(&window));
        self.form.open(surface.to_string(), ask, panel)
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
        // Widget execution is best-effort presentation in protocol v5. Runtime
        // outcomes remain local and never produce protocol acknowledgements.
        debug!(?report, "widget presentation outcome");
    }

    fn runtime(&self) -> MutexGuard<'_, Runtime> {
        self.runtime
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

/// Answers whether one window label belongs to a widget shell.
pub fn is_shell(label: &str) -> bool {
    label.starts_with(windows::LABEL_PREFIX)
}

/// Lays a widget's declared spawn keys under the ones the open carried.
///
/// The caller's word wins key by key, because the manifest says what the widget
/// is and the open says what is being asked of it: a panel opened on a chosen
/// day is that widget looking at that day, not a different widget.
///
/// A payload that is not an object is passed through as it stands. A widget
/// whose backend reads a bare value has nothing to merge into, and quietly
/// replacing what the caller sent would be worse than handing it over.
fn beneath(declared: Option<&Value>, asked: Value) -> Value {
    let Some(Value::Object(declared)) = declared else {
        return asked;
    };
    let Value::Object(asked) = asked else {
        return asked;
    };
    let mut merged = declared.clone();
    merged.extend(asked);
    Value::Object(merged)
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

    /// The manifest says what the widget is; the open says what is being asked
    /// of it. So a panel opened on a chosen day is that widget looking at that
    /// day, and a summon that carries nothing still gets the widget it asked
    /// for rather than whatever its backend defaults to.
    #[test]
    fn a_widgets_own_keys_lie_under_the_ones_the_open_carried() {
        let declared = json!({"view": "agenda", "days": 30});
        assert_eq!(
            beneath(Some(&declared), json!({"date": "2026-08-30"})),
            json!({"view": "agenda", "days": 30, "date": "2026-08-30"})
        );
        assert_eq!(
            beneath(Some(&declared), json!({})),
            json!({"view": "agenda", "days": 30}),
            "a summon carries nothing and still knows which widget it is"
        );
        assert_eq!(
            beneath(Some(&declared), json!({"days": 7})),
            json!({"view": "agenda", "days": 7}),
            "the caller's word wins key by key"
        );
        assert_eq!(
            beneath(None, json!({"seconds": 300})),
            json!({"seconds": 300}),
            "a widget that declares nothing changes nothing"
        );
        assert_eq!(
            beneath(Some(&declared), json!(5)),
            json!(5),
            "a payload with nothing to merge into is handed over as it stands"
        );
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
                widget: "cpu".into(),
                posture: Posture::Exhibit,
                data: serde_json::json!({ "every": 1 }),
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
