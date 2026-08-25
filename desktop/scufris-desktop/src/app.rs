//! Runtime glue between the pill state machine and the outside world.
//!
//! Every decision lives in [`crate::state::Companion`]. This module runs the
//! actions the machine returns and keeps the window and the pill on the phase
//! those actions left behind, which is not the same thing: the person's key and
//! the daemon's answer arrive on different threads, and the phase from the
//! change that ran last is the one they must both end up looking at. Each
//! outside effect is a port, so the failure paths that matter - a microphone
//! that never starts, a capture stream that dies, a submission the daemon never
//! confirms, an answer that overtakes the handoff that asked for it, a companion
//! that is restarted with an accepted transcript still in hand - are all
//! exercised in tests without a display, a microphone, or a backend.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tracing::{debug, error, info, warn};

use crate::{
    audio::{Capture, MAX_RECORDING, Recorder},
    config::RestartBudget,
    daemon::DaemonEvent,
    pending::{Pending, PendingStore},
    state::{Action, Companion, Event, Posture},
    tray,
};

/// How many times a failed removal of the durable transcript is retried.
const CLEAR_ATTEMPTS: usize = 2;

/// How many times a presentation that a surface refused is tried again before
/// the runtime leaves it to the next one.
///
/// The pill and the tray are the only things the companion can say anything
/// with, so a presentation one of them refused is worth another try. It is not
/// worth an unbounded one: every later change publishes again, and a surface
/// that is refusing now is not persuaded by being asked forever.
const RENDER_ATTEMPTS: usize = 3;

/// How many times the runtime asks again for a window that did not reach the
/// state its phase needs.
const REPAIR_ATTEMPTS: usize = 3;

/// How long the runtime waits before asking again for such a window.
const REPAIR_DELAY: Duration = Duration::from_millis(250);

/// Event name carrying the pill presentation to the frontend.
pub const PRESENTATION_EVENT: &str = "scufris://presentation";

/// Event name carrying recording progress to the frontend.
pub const TICK_EVENT: &str = "scufris://tick";

/// Event name asking the frontend to put a transcript on the clipboard.
pub const COPY_EVENT: &str = "scufris://copy";

/// Event name carrying the sound cue enablement to the frontend.
pub const CUES_EVENT: &str = "scufris://cues";

/// Interval between recording progress updates.
const TICK_INTERVAL: Duration = Duration::from_millis(60);

/// How long an accepted transcript waits for its acknowledgment.
pub const ACK_TIMEOUT: Duration = Duration::from_secs(15);

/// Payload the pill frontend renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PresentationPayload {
    /// Stable pill state name.
    pub state: &'static str,
    /// Transcript the pill shows.
    pub text: String,
    /// Short explanation of an error or a retained transcript.
    pub detail: String,
    /// Whether the pill offers an editable field.
    pub editable: bool,
    /// Whether the pill shows the recording indicator.
    pub recording: bool,
}

/// Payload carrying recording duration and microphone level.
#[derive(Debug, Clone, Serialize)]
pub struct TickPayload {
    /// Whole seconds recorded so far.
    pub seconds: u64,
    /// Loudest normalised level since the previous tick.
    pub level: f32,
}

/// What a request to put the pill on screen achieved.
///
/// Asking is not achieving, and the difference decides whether the microphone
/// may open: the pill is the recording privacy indicator, so the runtime is
/// told what the window is doing rather than what it was told to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    /// The pill is up, on top, and holds the keyboard: everything asked for.
    Ready,
    /// The pill is up and on top, so the person can see it, but the keyboard is
    /// somewhere else.
    Seen(String),
    /// The pill is up, but nothing proved the person can see it: it may be
    /// behind whatever they are looking at. Nothing that rests on being seen
    /// may rest on this.
    Doubtful(String),
}

/// The pill window and the tray.
///
/// Every operation whose outcome a later decision depends on reports whether it
/// happened. A window that did not come up, a pill that is still on screen after
/// a hide, a presentation that never reached the pill: each is something the
/// runtime must not record as done, because the record is what stops it being
/// tried again.
pub trait Surface: Send + Sync {
    /// Records the active window, then shows and focuses the pill.
    fn show_pill(&self) -> Result<Shown, String>;
    /// Shows the pill without touching the keyboard, and confirms it is up.
    fn show_pill_passive(&self) -> Result<(), String>;
    /// Hides the pill, and confirms that it is down.
    fn hide_pill(&self) -> Result<(), String>;
    /// Gives focus back to the window the pill covered.
    fn restore_focus(&self) -> Result<(), String>;
    /// Renders one presentation in the pill.
    fn present(&self, payload: PresentationPayload) -> Result<(), String>;
    /// Renders recording progress in the pill.
    ///
    /// The animation carries no decision: the presentation beneath it says what
    /// the companion is doing, and a frame nobody drew is a frame nobody misses.
    fn tick(&self, payload: TickPayload);
    /// Applies one state to the tray icon and its status line.
    fn tray(&self, state: &str, detail: &str) -> Result<(), String>;
    /// Puts one transcript on the clipboard.
    ///
    /// Copying is inert either way, so a clipboard that refuses the text is not
    /// worth interrupting the person for.
    fn copy(&self, text: String);
}

/// Local speech-to-text.
pub trait Transcriber: Send + Sync {
    /// Turns one WAV recording into text, or explains why it could not.
    fn transcribe(&self, wav: Vec<u8>) -> Result<String, String>;
}

/// The daemon end of the control protocol.
pub trait Backend: Send + Sync {
    /// Submits one accepted transcript.
    fn submit(&self, id: String, text: String, force: bool) -> Result<(), String>;
}

/// Where deferred work runs.
pub trait Executor: Send + Sync {
    /// Runs one deferred task that must complete.
    fn spawn(&self, task: Box<dyn FnOnce() + Send + 'static>);
    /// Runs one task after `delay`.
    fn spawn_after(&self, delay: Duration, task: Box<dyn FnOnce() + Send + 'static>);
    /// Runs the pill animation loop, which carries no decision and may be
    /// dropped entirely.
    fn spawn_presentation_loop(&self, task: Box<dyn FnOnce() + Send + 'static>);
}

/// An [`Executor`] that runs each task on its own thread.
#[derive(Debug, Default, Clone, Copy)]
pub struct ThreadExecutor;

impl Executor for ThreadExecutor {
    fn spawn(&self, task: Box<dyn FnOnce() + Send + 'static>) {
        thread::spawn(task);
    }

    fn spawn_after(&self, delay: Duration, task: Box<dyn FnOnce() + Send + 'static>) {
        thread::spawn(move || {
            thread::sleep(delay);
            task();
        });
    }

    fn spawn_presentation_loop(&self, task: Box<dyn FnOnce() + Send + 'static>) {
        thread::spawn(task);
    }
}

/// Everything one change puts on the surfaces.
///
/// The window, the pill, and the tray are one surface, not three: where the
/// pill belongs and what it says are the same change seen from two sides, and
/// what the pill renders is only an indicator while the pill is up.
#[derive(Debug, Clone)]
struct Surfaced {
    /// Where the pill window belongs.
    posture: Posture,
    /// What the pill renders.
    payload: PresentationPayload,
    /// Tray state name for the same moment.
    tray: &'static str,
}

/// One change to the companion, and where it sits among all of them.
#[derive(Debug, Clone)]
struct Decision {
    /// What the surfaces must end up showing.
    surfaced: Surfaced,
    /// Position in the order changes were made.
    version: u64,
}

/// Work one caller left for whichever thread proves its decision.
type Follower = Box<dyn FnOnce(Result<(), String>) + Send + 'static>;

/// One surface the runtime keeps in step with the newest decision about it.
///
/// The surface call runs outside the lock. A pill window operation waits on the
/// event loop, and a thread holding a lock while it waits is a thread the main
/// thread can deadlock against, so a thread that finds another already applying
/// leaves its decision behind instead of waiting; the applying thread carries on
/// until nothing newer is left. Whichever order two threads arrive in, the
/// surface ends where the newest decision put it, and one that lost the race
/// changes nothing.
struct Ordered<T> {
    state: Mutex<Latest<T>>,
}

struct Latest<T> {
    /// The newest decision the surface has not been told about yet, and where
    /// it sits in the order.
    next: Option<(u64, T)>,
    /// The newest version seen, whether or not it reached the surface.
    version: u64,
    /// True while a thread is inside the surface call.
    applying: bool,
    /// Follow-up work from callers that found the surface busy.
    waiting: Vec<Follower>,
}

impl<T> Default for Ordered<T> {
    fn default() -> Self {
        Self {
            state: Mutex::new(Latest {
                next: None,
                version: 0,
                applying: false,
                waiting: Vec::new(),
            }),
        }
    }
}

/// Hands the surface back when an effect leaves through a panic.
///
/// Locks here are recovered from poisoning rather than propagating it, so one
/// failed call must not be able to leave the surface stopped at the decision
/// before it, with every later one waiting for a thread that is gone.
struct Applying<'a, T> {
    ordered: &'a Ordered<T>,
    armed: bool,
}

impl<T> Drop for Applying<'_, T> {
    fn drop(&mut self) {
        if self.armed {
            self.ordered.lock().applying = false;
        }
    }
}

impl<T> Ordered<T> {
    /// Records one decision, applies it unless a newer one arrived first, and
    /// then runs `follow` with what the surface actually ended on.
    ///
    /// `follow` runs on whichever thread proves the newest decision. A caller
    /// that finds another thread already on the surface leaves both its
    /// decision and its follow-up behind rather than waiting: the surface call
    /// waits on the event loop that also runs the pill's own commands, so a
    /// thread that waited for it could be the very thread it is waiting for.
    /// Leaving the work behind is what lets unsafe work still happen only after
    /// the postcondition it needs has been proved, and by the thread that
    /// proved it.
    fn apply(
        &self,
        version: u64,
        value: T,
        mut effect: impl FnMut(&T) -> Result<(), String>,
        follow: Follower,
    ) {
        {
            let mut state = self.lock();
            if version >= state.version {
                state.version = version;
                state.next = Some((version, value));
            }
            if state.applying {
                state.waiting.push(follow);
                return;
            }
            if state.next.is_none() {
                // Older than a decision already applied, and nobody is
                // applying: the surface is where a newer change put it, which
                // is where it belongs.
                drop(state);
                follow(Ok(()));
                return;
            }
            state.applying = true;
        }
        let mut applying = Applying {
            ordered: self,
            armed: true,
        };
        let mut outcome = Ok(());
        let inherited = loop {
            let next = {
                let mut state = self.lock();
                match state.next.take() {
                    Some((_, value)) => value,
                    // Taken and cleared together: a decision recorded after
                    // this would otherwise wait for a thread that has stopped
                    // looking. Nothing is left, so nothing is left to hand back.
                    None => {
                        state.applying = false;
                        applying.armed = false;
                        break std::mem::take(&mut state.waiting);
                    }
                }
            };
            outcome = effect(&next);
        };
        // Everyone gets what the surface ended on. That is not always their own
        // decision, and it is what each of them was really asking about: an
        // older decision whose phase has been left behind has nothing left to
        // be right about.
        follow(outcome.clone());
        for other in inherited {
            other(outcome.clone());
        }
    }

    /// Answers whether a decision is waiting for the surface.
    fn pending(&self) -> bool {
        self.lock().next.is_some()
    }

    fn lock(&self) -> MutexGuard<'_, Latest<T>> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
}

/// What the last window operation proved about the pill window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    /// The pill is down.
    Off,
    /// The pill is up, but nothing proved the person can see it.
    Doubtful,
    /// The pill is up and on top, so the person can see it.
    Seen,
    /// The pill is up, on top, and holds the keyboard.
    Ready,
}

impl Screen {
    /// Answers whether the person can be relied on to see the pill.
    fn visible(self) -> bool {
        matches!(self, Screen::Seen | Screen::Ready)
    }
}

/// Everything the runtime reaches outside itself.
pub struct Ports {
    /// The pill window and the tray.
    pub surface: Arc<dyn Surface>,
    /// The microphone.
    pub recorder: Arc<dyn Recorder>,
    /// Durable storage for an accepted transcript.
    pub pending: Arc<dyn PendingStore>,
    /// Local speech-to-text.
    pub transcriber: Arc<dyn Transcriber>,
    /// Where deferred work runs.
    pub executor: Arc<dyn Executor>,
    /// Prefix that makes this process's submission identifiers unique.
    pub prefix: String,
    /// Executable that opens the full popup chat, when one is configured.
    pub chat_command: Option<PathBuf>,
    /// Executable that restarts the owned backend service, when configured.
    pub restart_command: Option<PathBuf>,
    /// How long an accepted transcript waits for its acknowledgment.
    pub ack_timeout: Duration,
}

/// Companion runtime shared by every Tauri handler.
pub struct App {
    ports: Ports,
    companion: Mutex<Companion>,
    recording: Mutex<Option<Box<dyn Capture>>>,
    backend: Mutex<Option<Arc<dyn Backend>>>,
    restarts: Mutex<RestartBudget>,
    transcription: AtomicU64,
    tick: AtomicU64,
    /// Identifies the capture the runtime currently owns. A stream error from
    /// any other capture is stale and must not disturb the current one.
    capture_generation: AtomicU64,
    /// The capture whose stream failed before its handle could be installed.
    failed_capture: AtomicU64,
    /// Counts the changes made to the companion, so the surfaces can tell a
    /// newer decision from an older one.
    decisions: AtomicU64,
    /// Keeps the window, the pill, and the tray on the newest decision.
    surface: Ordered<Surfaced>,
    /// What the window operations proved the pill is doing, as opposed to what
    /// they were asked to do. Only a change is worth an operation: showing a
    /// ready pill would take focus back from whatever the person moved to, and
    /// restoring focus twice would take it from wherever they moved it.
    screen: Mutex<Screen>,
    /// True while the presentation the pill last took says the microphone is
    /// open. The pill is the privacy indicator: the tray says the same thing,
    /// but a tray icon can be folded away into an overflow menu, so nothing
    /// rests the microphone on it.
    drawn_recording: AtomicBool,
    /// True while a chain of window repairs is already under way, so a window
    /// that keeps falling short does not collect one chain per decision.
    repairing: AtomicBool,
}

impl App {
    /// Creates the runtime over one set of ports.
    pub fn new(ports: Ports) -> Self {
        let companion = Companion::new(ports.prefix.clone());
        Self {
            ports,
            companion: Mutex::new(companion),
            recording: Mutex::new(None),
            backend: Mutex::new(None),
            restarts: Mutex::new(RestartBudget::default()),
            transcription: AtomicU64::new(0),
            tick: AtomicU64::new(0),
            capture_generation: AtomicU64::new(0),
            failed_capture: AtomicU64::new(0),
            decisions: AtomicU64::new(0),
            surface: Ordered::default(),
            // The pill window is built hidden.
            screen: Mutex::new(Screen::Off),
            drawn_recording: AtomicBool::new(false),
            repairing: AtomicBool::new(false),
        }
    }

    /// Returns a per-process identifier prefix.
    ///
    /// Submission identifiers outlive the process that made them and are what
    /// the daemon suppresses duplicates by, so a collision would have a
    /// genuinely new request refused. A process identifier and a clock are not
    /// enough - identifiers are reused and clocks move backwards - so the
    /// prefix is drawn from the operating system's randomness.
    pub fn process_prefix() -> String {
        let mut bytes = [0u8; 16];
        if let Err(error) = getrandom::fill(&mut bytes) {
            // The kernel refusing randomness is not survivable for something
            // whose whole job is to be unique.
            panic!("scufris-desktop cannot obtain randomness: {error}");
        }
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// Recovers any accepted transcript left by a previous process and renders
    /// the initial presentation.
    ///
    /// A record that exists but cannot be read is reported, never mistaken for
    /// an empty store: that is what would let the next save destroy it.
    pub fn start(self: &Arc<Self>) {
        let loaded = self.ports.pending.load();
        let (_, decision) = self.decide(|companion| match loaded {
            Ok(Some(pending)) => companion.restore(pending),
            Ok(None) => {}
            Err(error) => companion.report_store_failure(error.to_string()),
        });
        let runtime = Arc::clone(self);
        self.show(
            decision,
            Box::new(move |outcome| {
                if let Err(reason) = outcome {
                    runtime.abandon(reason);
                }
            }),
        );
    }

    /// Installs the daemon connection.
    pub fn set_backend(&self, backend: Arc<dyn Backend>) {
        *self
            .backend
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(backend);
    }

    /// Records whether the daemon connection is open.
    pub fn set_connected(self: &Arc<Self>, connected: bool) {
        {
            let mut companion = self
                .companion
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if companion.connected() == connected {
                return;
            }
            companion.set_connected(connected);
            if connected {
                info!("daemon connected");
            } else {
                warn!("daemon disconnected");
            }
        }
        // The phase is untouched, so the window has nothing to catch up to.
        self.publish();
    }

    /// Applies one thing the daemon link observed.
    ///
    /// Every answer about a submission carries the identifier it answers, and
    /// the state machine applies it only to that submission: a slow answer for
    /// one transcript must not settle the transcript that replaced it.
    pub fn observe(self: &Arc<Self>, event: DaemonEvent) {
        match event {
            DaemonEvent::Connected(session) => {
                debug!(session = %session, "daemon welcome");
                self.set_connected(true)
            }
            DaemonEvent::Disconnected => self.set_connected(false),
            DaemonEvent::State(state, detail) => self.set_assistant(state, detail),
            DaemonEvent::Acknowledged(id) => {
                debug!(id = %id, "submission acknowledged");
                self.handle(Event::Acknowledged(id))
            }
            DaemonEvent::Refused(id, detail) => {
                debug!(id = %id, detail = %detail, "submission refused");
                self.handle(Event::SubmissionFailed { id, reason: detail })
            }
            DaemonEvent::Uncertain(id, detail) => {
                debug!(id = %id, detail = %detail, "submission uncertain");
                self.handle(Event::SubmissionUncertain { id, reason: detail })
            }
        }
    }

    /// Records the assistant state the daemon reported.
    pub fn set_assistant(self: &Arc<Self>, state: scufris_control::AssistantState, detail: String) {
        {
            let mut companion = self
                .companion
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if companion.assistant() != state {
                info!(
                    from = companion.assistant().name(),
                    to = state.name(),
                    "assistant state"
                );
            }
            companion.set_assistant(state, detail);
        }
        self.publish();
    }

    /// Stops any live recording. The accepted transcript stays on disk.
    pub fn shutdown(&self) {
        self.tick.fetch_add(1, Ordering::Relaxed);
        if let Some(recording) = self.take_recording() {
            recording.discard();
        }
    }

    /// Applies one companion event and runs the resulting actions.
    ///
    /// Actions run in order and stop at the first durable-storage failure, so a
    /// transcript is never submitted before it is safely on disk. A failure
    /// becomes the next event rather than a nested call, so the window follows
    /// the phase each event leaves rather than the one it started in.
    pub fn handle(self: &Arc<Self>, event: Event) {
        let (actions, decision) = self.decide(|companion| companion.apply(event));
        if decision.surfaced.posture != Posture::Focused {
            // A phase that is leaving, or one the person only watches, stays
            // on the surfaces until its actions are done, so the pill the
            // person is looking at is the one they finished with.
            self.carry_out(actions, Some(decision));
            return;
        }
        // A phase the person has to see is on the surfaces before any of its
        // own actions run, and it is on them in fact: this waits for what the
        // window and the pill actually did, on whichever thread did it.
        let runtime = Arc::clone(self);
        self.show(
            decision,
            Box::new(move |outcome| match outcome {
                Ok(()) => runtime.carry_out(actions, None),
                // The window this phase needs is not there, so nothing the
                // phase asks for may run: the microphone must never open behind
                // a privacy indicator that never came up, and words nobody can
                // see are words nobody can correct.
                Err(reason) => runtime.abandon(reason),
            }),
        );
    }

    /// Runs what one phase asked for, and puts it on the surfaces afterwards
    /// when it was not already put there first.
    fn carry_out(self: &Arc<Self>, actions: Vec<Action>, after: Option<Decision>) {
        for action in actions {
            if let Err(failure) = self.run(action) {
                // The failure becomes the next event rather than something
                // nested inside this one, so the surfaces follow the phase it
                // leaves rather than the phase it started in.
                self.handle(failure);
                return;
            }
        }
        if let Some(decision) = after {
            self.show(decision, Box::new(|_| {}));
        }
    }

    /// Gives up an interaction whose pill could not be put on screen.
    ///
    /// Nothing durable is thrown away here. An accepted transcript is on disk
    /// before anything is sent, so it comes back on the next start; what stops
    /// is the microphone, the transcription, and the pretence that there is a
    /// pill to answer in. The tray still says what happened, and the person's
    /// next activation is what tries the window again.
    fn abandon(self: &Arc<Self>, reason: String) {
        error!("{reason}");
        let (actions, decision) = self.decide(|companion| companion.abandon(reason));
        for action in actions {
            // Stopping the microphone and the transcription reaches nothing
            // durable, so nothing here can fail into a phase that would want
            // the window back.
            let _ = self.run(action);
        }
        self.show(decision, Box::new(|_| {}));
    }

    /// Changes the companion and stamps the result with its place in the order.
    fn decide<T>(&self, change: impl FnOnce(&mut Companion) -> T) -> (T, Decision) {
        let mut companion = self
            .companion
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let before = companion.phase_name();
        let value = change(&mut companion);
        let after = companion.phase_name();
        if before != after {
            // Every phase change passes through here under the companion lock,
            // so this is the one place the transition log cannot miss one.
            info!(from = before, to = after, "phase");
        }
        let decision = self.stamp(&companion);
        (value, decision)
    }

    /// Reads what the companion looks like now, stamped with its place in the
    /// order the companion lock made the changes.
    fn snapshot(&self) -> Decision {
        let companion = self
            .companion
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.stamp(&companion)
    }

    fn stamp(&self, companion: &Companion) -> Decision {
        let presentation = companion.presentation();
        Decision {
            surfaced: Surfaced {
                posture: companion.posture(),
                payload: PresentationPayload {
                    state: presentation.state,
                    text: presentation.text,
                    detail: presentation.detail,
                    editable: presentation.editable,
                    recording: presentation.recording,
                },
                tray: companion.tray_state(),
            },
            version: self.decisions.fetch_add(1, Ordering::SeqCst) + 1,
        }
    }

    /// Puts the surfaces where one decision wants them, then runs `follow`.
    ///
    /// The person's key and the daemon's answer change the companion from two
    /// threads, and either can be the one that runs last. Whichever it is owns
    /// the surfaces: an older decision that finds a newer one already recorded
    /// changes nothing rather than hiding a pill that is now asking for the
    /// person, or reopening one that is finished.
    fn show(self: &Arc<Self>, decision: Decision, follow: Follower) {
        let runtime = Arc::clone(self);
        self.surface.apply(
            decision.version,
            decision.surfaced,
            // One decision reaches the surfaces at a time, so this reads and
            // writes what the last one did without racing another.
            move |surfaced| runtime.put(surfaced),
            follow,
        );
    }

    /// Puts the window where one decision wants it and renders that decision.
    ///
    /// Returns a failure only for the window: a pill that is not there is a
    /// phase with nowhere to happen, while a surface that refused to render is
    /// one of two, and the other may well have taken it.
    fn put(self: &Arc<Self>, surfaced: &Surfaced) -> Result<(), String> {
        let placed = match surfaced.posture {
            Posture::Focused => self.raise(),
            Posture::Passive => self.settle(),
            Posture::Off => self.lower(),
        };
        // Both surfaces are told whatever the window did. When the pill cannot
        // be shown, the tray is the only one left that can say anything at all.
        self.draw(surfaced);
        if !self.falls_short(surfaced.posture) {
            // The window is where it belongs, so any chain still running has
            // nothing left to repair. Clearing it here as well as in the chain
            // means a chain whose thread was lost cannot block the next one.
            self.repairing.store(false, Ordering::SeqCst);
        } else if !self.repairing.swap(true, Ordering::SeqCst) {
            self.repair(REPAIR_ATTEMPTS);
        }
        // The microphone is open and nothing on screen is saying so.
        if surfaced.payload.recording && self.capturing() && !self.recording_shown() {
            self.stop_capture();
        }
        placed
    }

    /// Renders one presentation on the pill and on the tray, and records what
    /// the pill took.
    ///
    /// Both are told, whatever the other did: they are two ways of saying the
    /// same thing, and a pill that refuses is exactly when the tray is the only
    /// one left that can say anything at all. Only the one that refused is
    /// asked again, and only a pill that took this presentation is written
    /// down: a pill that refused is still showing whatever it took last, which
    /// is what its record already says.
    fn draw(&self, surfaced: &Surfaced) {
        let mut on_pill = false;
        let mut on_tray = false;
        for attempt in 1..=RENDER_ATTEMPTS {
            let mut trouble = Vec::new();
            if !on_pill {
                match self.ports.surface.present(surfaced.payload.clone()) {
                    Ok(()) => {
                        on_pill = true;
                        self.drawn_recording
                            .store(surfaced.payload.recording, Ordering::SeqCst);
                    }
                    Err(reason) => trouble.push(reason),
                }
            }
            if !on_tray {
                match self
                    .ports
                    .surface
                    .tray(surfaced.tray, &surfaced.payload.detail)
                {
                    Ok(()) => on_tray = true,
                    Err(reason) => trouble.push(reason),
                }
            }
            if trouble.is_empty() {
                return;
            }
            // A newer presentation is worth more than another try at one the
            // companion has already left behind.
            if attempt == RENDER_ATTEMPTS || self.surface.pending() {
                warn!("{}", trouble.join("; "));
                return;
            }
        }
    }

    /// Puts the pill on screen, and records only what that achieved.
    fn raise(&self) -> Result<(), String> {
        if self.screen() == Screen::Ready {
            return Ok(());
        }
        match self.ports.surface.show_pill() {
            Ok(Shown::Ready) => {
                self.set_screen(Screen::Ready);
                Ok(())
            }
            Ok(Shown::Seen(trouble)) => {
                // The person can see the pill, which is what the privacy
                // indicator needs. The keyboard is asked for again, because
                // this is not recorded as the pill being ready.
                warn!("{trouble}");
                self.set_screen(Screen::Seen);
                Ok(())
            }
            Ok(Shown::Doubtful(trouble)) => {
                // Up, but perhaps behind what the person is looking at. The
                // phase still has a window, so it goes on; what it does not
                // have is anything to rest a privacy indicator on.
                warn!("{trouble}");
                self.set_screen(Screen::Doubtful);
                Ok(())
            }
            // Nothing is recorded. What the window was doing before is still
            // the best thing known about it, and the next decision is free to
            // try again.
            Err(reason) => Err(reason),
        }
    }

    /// Keeps the pill on screen without the keyboard, and records only what
    /// that achieved.
    ///
    /// This is the handoff posture: the desktop is the person's again while
    /// the window stays to report the turn. A pill that still holds the
    /// keyboard would swallow the keys the person types into their own
    /// window, so holding it counts as falling short and is repaired.
    fn settle(&self) -> Result<(), String> {
        match self.screen() {
            Screen::Seen | Screen::Doubtful => Ok(()),
            Screen::Ready => match self.ports.surface.restore_focus() {
                Ok(()) => {
                    self.set_screen(Screen::Seen);
                    Ok(())
                }
                // Still holding the keyboard, so still Ready: the repair
                // chain asks again rather than recording a release that did
                // not happen.
                Err(reason) => Err(reason),
            },
            Screen::Off => match self.ports.surface.show_pill_passive() {
                Ok(()) => {
                    self.set_screen(Screen::Seen);
                    Ok(())
                }
                Err(reason) => Err(reason),
            },
        }
    }

    /// Takes the pill off screen, and records only what that achieved.
    fn lower(&self) -> Result<(), String> {
        if self.screen() == Screen::Off {
            return Ok(());
        }
        // Only a pill that holds the keyboard has focus to give back. A
        // passive pill already returned it at the handoff, and asking again
        // would put on record a restoration that never happened.
        if self.screen() == Screen::Ready
            && let Err(reason) = self.ports.surface.restore_focus()
        {
            // The pill is going away either way. Only the window behind it is
            // worse off, and saying so is all there is to do about it.
            warn!("{reason}");
        }
        match self.ports.surface.hide_pill() {
            Ok(()) => {
                self.set_screen(Screen::Off);
                Ok(())
            }
            Err(reason) => {
                // An always-on-top pill that is still up must never be recorded
                // as down: that record is what would stop it ever being taken
                // down again. Focus has gone, so it is up but not ready, and
                // the repair below is what takes it down.
                warn!("{reason}");
                self.set_screen(Screen::Seen);
                Err(reason)
            }
        }
    }

    /// Answers whether the window is not where the phase needs it.
    fn falls_short(&self, posture: Posture) -> bool {
        match posture {
            Posture::Focused => self.screen() != Screen::Ready,
            // A passive pill must be up and must not hold the keyboard.
            Posture::Passive => !matches!(self.screen(), Screen::Seen | Screen::Doubtful),
            Posture::Off => self.screen() != Screen::Off,
        }
    }

    /// Asks again for a window that did not reach the state its phase needs.
    ///
    /// Nobody else can. A pill that would not go down is over the desktop with
    /// the keyboard already given back, so it cannot even be sent an Escape; a
    /// pill that would not take the keyboard cannot be typed into. So the
    /// runtime asks again itself, on the newest phase rather than the one that
    /// fell short, and it stops asking: a window manager that has refused three
    /// times is refusing, and the next decision asks again anyway.
    fn repair(self: &Arc<Self>, left: usize) {
        if left == 0 {
            self.repairing.store(false, Ordering::SeqCst);
            return;
        }
        let runtime = Arc::clone(self);
        self.ports.executor.spawn_after(
            REPAIR_DELAY,
            Box::new(move || {
                let decision = runtime.snapshot();
                let wanted = decision.surfaced.posture;
                let again = Arc::clone(&runtime);
                runtime.show(
                    decision,
                    Box::new(move |_| {
                        if again.falls_short(wanted) {
                            again.repair(left - 1);
                        } else {
                            again.repairing.store(false, Ordering::SeqCst);
                        }
                    }),
                );
            }),
        );
    }

    /// Answers whether the person can read that the microphone is open.
    ///
    /// What the pill rendered only counts while the pill is somewhere the
    /// person can see it. A pill that is down, or that would not stay over the
    /// window they are looking at, proves nothing about what they can read.
    fn recording_shown(&self) -> bool {
        self.drawn_recording.load(Ordering::SeqCst) && self.screen().visible()
    }

    /// Answers whether the microphone is open right now.
    fn capturing(&self) -> bool {
        self.recording
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_some()
    }

    /// Stops a capture that nothing on screen is announcing.
    fn stop_capture(self: &Arc<Self>) {
        let runtime = Arc::clone(self);
        self.ports.executor.spawn(Box::new(move || {
            runtime.handle(Event::RecordingFailed(
                "The recording indicator is not on screen, so recording stopped.".into(),
            ))
        }));
    }

    fn screen(&self) -> Screen {
        *self
            .screen
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn set_screen(&self, screen: Screen) {
        *self
            .screen
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = screen;
    }

    /// Runs one action, returning the event that describes its failure.
    fn run(self: &Arc<Self>, action: Action) -> Result<(), Event> {
        match action {
            Action::StartRecording => {
                // The privacy indicator comes first, always. If no surface the
                // person can read took the recording state, the microphone does
                // not open, whatever the companion asked for.
                if !self.recording_shown() {
                    return Err(Event::RecordingFailed(
                        "The recording indicator is not on screen, so nothing was recorded.".into(),
                    ));
                }
                self.start_recording()
            }
            Action::StopRecording => self.stop_recording(),
            Action::DiscardRecording => {
                self.stop_ticker();
                if let Some(recording) = self.take_recording() {
                    recording.discard();
                }
            }
            Action::CancelTranscription => {
                self.stop_ticker();
                self.transcription.fetch_add(1, Ordering::Relaxed);
                if let Some(recording) = self.take_recording() {
                    recording.discard();
                }
            }
            Action::PersistPending { id, text } => {
                return self
                    .ports
                    .pending
                    .save(&Pending { id, text })
                    .map_err(|error| Event::PersistFailed(error.to_string()));
            }
            Action::ClearPending => self.clear_pending(),
            Action::DiscardPending { id } => return self.discard_pending(&id),
            Action::Submit { id, text, force } => self.submit(id, text, force),
            Action::CopyTranscript { text } => self.ports.surface.copy(text),
        }
        Ok(())
    }

    /// Removes the durable transcript, retrying once.
    ///
    /// A removal that keeps failing is logged rather than reopening the pill the
    /// user just finished with. The record left behind is recovered on the next
    /// start and resent under its original identifier, which the daemon
    /// suppresses idempotently, so it cannot reach the conversation twice.
    fn clear_pending(&self) {
        for attempt in 1..=CLEAR_ATTEMPTS {
            match self.ports.pending.clear() {
                Ok(()) => return,
                Err(error) if attempt == CLEAR_ATTEMPTS => {
                    error!("{error}");
                }
                Err(_) => {}
            }
        }
    }

    /// Throws away a transcript the user explicitly discarded.
    ///
    /// Discarded words must not come back. A removal that cannot happen leaves
    /// a tombstone, which is enough to stop the text being restored; only when
    /// neither is possible does the pill reopen to say so.
    fn discard_pending(&self, id: &str) -> Result<(), Event> {
        let removal = match self.ports.pending.clear() {
            Ok(()) => return Ok(()),
            Err(error) => error.to_string(),
        };
        match self.ports.pending.tombstone(id) {
            Ok(()) => {
                warn!("{removal}; the discarded transcript was tombstoned instead");
                Ok(())
            }
            Err(error) => Err(Event::DiscardFailed(format!(
                "The discarded transcript is still on disk: {error}"
            ))),
        }
    }

    fn take_recording(&self) -> Option<Box<dyn Capture>> {
        // Retiring a capture also retires its right to report a stream error.
        self.capture_generation.fetch_add(1, Ordering::SeqCst);
        self.recording
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }

    fn start_recording(self: &Arc<Self>) {
        let generation = self.capture_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let owner = Arc::clone(self);
        let sink: crate::audio::ErrorSink = Arc::new(move |reason: String| {
            // A capture that has been retired, or replaced by a later
            // activation, must not tear down whatever is recording now.
            if owner.capture_generation.load(Ordering::SeqCst) != generation {
                return;
            }
            owner.failed_capture.store(generation, Ordering::SeqCst);
            let runtime = Arc::clone(&owner);
            let executor = Arc::clone(&runtime.ports.executor);
            executor.spawn(Box::new(move || {
                runtime.handle(Event::RecordingFailed(reason));
            }));
        });
        match self.ports.recorder.start(sink) {
            Ok(recording) => {
                // The stream can fail inside `start`, before there is anywhere
                // to put the handle. Closing it here is what releases the
                // microphone instead of leaving it open and unreachable.
                if self.failed_capture.load(Ordering::SeqCst) == generation
                    || self.capture_generation.load(Ordering::SeqCst) != generation
                {
                    recording.discard();
                    return;
                }
                *self
                    .recording
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = Some(recording);
                self.start_ticker();
            }
            Err(error) => {
                // No capture exists, so nothing may report against this
                // generation afterwards.
                self.capture_generation.fetch_add(1, Ordering::SeqCst);
                // Reported through the executor so the state machine is not
                // re-entered while it is applying the activation.
                let runtime = Arc::clone(self);
                let reason = error.to_string();
                self.ports.executor.spawn(Box::new(move || {
                    runtime.handle(Event::RecordingFailed(reason))
                }));
            }
        }
    }

    fn start_ticker(self: &Arc<Self>) {
        let generation = self.tick.fetch_add(1, Ordering::Relaxed) + 1;
        let runtime = Arc::clone(self);
        self.ports
            .executor
            .spawn_presentation_loop(Box::new(move || {
                while runtime.tick.load(Ordering::Relaxed) == generation {
                    let payload = {
                        let recording = runtime
                            .recording
                            .lock()
                            .unwrap_or_else(|error| error.into_inner());
                        recording.as_ref().map(|recording| TickPayload {
                            seconds: recording.elapsed().as_secs(),
                            level: recording.take_level(),
                        })
                    };
                    let Some(payload) = payload else { return };
                    if payload.seconds >= MAX_RECORDING.as_secs() {
                        runtime.handle(Event::Enter { text: None });
                        return;
                    }
                    runtime.ports.surface.tick(payload);
                    thread::sleep(TICK_INTERVAL);
                }
            }));
    }

    fn stop_ticker(&self) {
        self.tick.fetch_add(1, Ordering::Relaxed);
    }

    fn stop_recording(self: &Arc<Self>) {
        self.stop_ticker();
        let Some(recording) = self.take_recording() else {
            return;
        };
        let generation = self.transcription.fetch_add(1, Ordering::Relaxed) + 1;
        let runtime = Arc::clone(self);
        self.ports.executor.spawn(Box::new(move || {
            let wav = recording.finish();
            let event = match runtime.ports.transcriber.transcribe(wav) {
                Ok(transcript) => Event::Transcribed(transcript),
                Err(reason) => Event::TranscriptionFailed(reason),
            };
            if runtime.transcription.load(Ordering::Relaxed) != generation {
                return;
            }
            runtime.handle(event);
        }));
    }

    fn submit(self: &Arc<Self>, id: String, text: String, force: bool) {
        let backend = self
            .backend
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let Some(backend) = backend else {
            self.fail_submission(id, "The Scufris backend is unavailable.".into());
            return;
        };
        if let Err(reason) = backend.submit(id.clone(), text, force) {
            self.fail_submission(id, reason);
            return;
        }
        // An unanswered submission is retained rather than silently lost, and
        // it is retained as uncertain: the bytes left this process, so the
        // request may have run. Nothing resends it without the person saying
        // so.
        let runtime = Arc::clone(self);
        self.ports.executor.spawn_after(
            self.ports.ack_timeout,
            Box::new(move || {
                runtime.handle(Event::SubmissionUncertain {
                    id,
                    reason: "The backend did not confirm delivery.".into(),
                });
            }),
        );
    }

    fn fail_submission(self: &Arc<Self>, id: String, reason: String) {
        let runtime = Arc::clone(self);
        self.ports.executor.spawn(Box::new(move || {
            runtime.handle(Event::SubmissionFailed { id, reason })
        }));
    }

    /// Puts the surfaces on what the companion looks like now.
    ///
    /// The companion is read under the same lock that changes it and stamped
    /// where it was read, so a snapshot taken before a newer one cannot reach
    /// the surfaces after it and leave the pill rendering a state that has been
    /// left behind.
    pub fn publish(self: &Arc<Self>) {
        // The window is asked for as well, because a presentation the person
        // cannot see is not a presentation. Nothing here waits for that: a
        // presentation is what the companion looks like, not something it is
        // doing. A window that would not come up is left to the repair chain
        // and to the next decision that needs it.
        self.show(
            self.snapshot(),
            Box::new(|outcome| {
                if let Err(reason) = outcome {
                    warn!("{reason}");
                }
            }),
        );
    }

    /// Opens the full popup chat through the configured hook.
    pub fn open_chat(&self) {
        let Some(command) = &self.ports.chat_command else {
            warn!("no chat command is configured");
            return;
        };
        spawn_hook(command);
    }

    /// Restarts the owned backend service inside the bounded restart budget.
    pub fn restart_backend(&self) {
        let Some(command) = &self.ports.restart_command else {
            warn!("no backend restart command is configured");
            return;
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if !self
            .restarts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .allow(now)
        {
            warn!("backend restart budget is exhausted");
            return;
        }
        spawn_hook(command);
    }
}

fn spawn_hook(command: &Path) {
    match Command::new(command).spawn() {
        Ok(mut child) => {
            thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(error) => error!("cannot run {}: {error}", command.display()),
    }
}

/// Returns the tray status line for one presentation.
pub fn status_line(state: &str, detail: &str) -> String {
    tray::tooltip(state, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        audio::{ErrorSink, RecorderError},
        pending::PendingError,
    };

    /// One surface operation that returns an error a fixed number of times
    /// before working, the way a production adapter reports a refusal.
    struct Refusals {
        remaining: AtomicU64,
        reason: &'static str,
    }

    impl Refusals {
        fn new(reason: &'static str) -> Self {
            Self {
                remaining: AtomicU64::new(0),
                reason,
            }
        }

        /// Makes the next `times` attempts fail.
        fn fail(&self, times: u64) {
            self.remaining.store(times, Ordering::SeqCst);
        }

        /// Answers whether this attempt fails, and consumes it if it does.
        fn attempt(&self) -> Result<(), String> {
            if self.remaining.load(Ordering::SeqCst) == 0 {
                return Ok(());
            }
            self.remaining.fetch_sub(1, Ordering::SeqCst);
            Err(self.reason.into())
        }
    }

    struct RecordedSurface {
        presentations: Mutex<Vec<PresentationPayload>>,
        tray: Mutex<Vec<String>>,
        copied: Mutex<Vec<String>>,
        shown: AtomicU64,
        hidden: AtomicU64,
        restored: AtomicU64,
        /// What the pill window is actually doing, as opposed to how many times
        /// it was told to do something.
        on_screen: AtomicBool,
        /// True while the pill holds the keyboard.
        focused: AtomicBool,
        /// Window operations in the order the surface received them, whether
        /// they worked or not.
        window: Mutex<Vec<&'static str>>,
        /// When set, the next attempt to show the pill fails the way a broken
        /// window connection would.
        panic_on_show: AtomicBool,
        /// Show attempts that return an error without the pill coming up.
        refuse_show: Refusals,
        /// Show attempts where the pill comes up but does not take focus.
        partial_show: AtomicU64,
        /// Show attempts where the pill comes up but nothing proves the person
        /// can see it, the way a refused always-on-top leaves it.
        blind_show: AtomicU64,
        /// Hide attempts that return an error and leave the pill up.
        refuse_hide: Refusals,
        /// Focus restorations that return an error.
        refuse_focus: Refusals,
        /// Presentations the pill refuses to render.
        refuse_present: Refusals,
        /// States the tray refuses to take.
        refuse_tray: Refusals,
        /// Runs once as a presentation reaches the pill, so a test can move the
        /// companion on while an older presentation is still on the surface.
        at_present: Mutex<Option<Watcher>>,
    }

    impl Default for RecordedSurface {
        fn default() -> Self {
            Self {
                presentations: Mutex::default(),
                tray: Mutex::default(),
                copied: Mutex::default(),
                shown: AtomicU64::new(0),
                hidden: AtomicU64::new(0),
                restored: AtomicU64::new(0),
                on_screen: AtomicBool::new(false),
                focused: AtomicBool::new(false),
                window: Mutex::default(),
                panic_on_show: AtomicBool::new(false),
                refuse_show: Refusals::new("the pill did not come up"),
                partial_show: AtomicU64::new(0),
                blind_show: AtomicU64::new(0),
                refuse_hide: Refusals::new("the pill is still up"),
                refuse_focus: Refusals::new("the previous window would not take focus"),
                refuse_present: Refusals::new("the pill would not render"),
                refuse_tray: Refusals::new("the tray icon would not change"),
                at_present: Mutex::default(),
            }
        }
    }

    impl RecordedSurface {
        fn last(&self) -> PresentationPayload {
            self.presentations
                .lock()
                .unwrap()
                .last()
                .cloned()
                .expect("no presentation was published")
        }

        /// True while the pill window is up.
        fn on_screen(&self) -> bool {
            self.on_screen.load(Ordering::SeqCst)
        }

        /// True while the pill window holds the keyboard.
        fn focused(&self) -> bool {
            self.focused.load(Ordering::SeqCst)
        }

        /// Every window operation, in the order it arrived.
        fn window(&self) -> Vec<&'static str> {
            self.window.lock().unwrap().clone()
        }

        /// Every tray state applied, in order.
        fn tray_states(&self) -> Vec<String> {
            self.tray.lock().unwrap().clone()
        }
    }

    impl Surface for RecordedSurface {
        fn show_pill(&self) -> Result<Shown, String> {
            if self.panic_on_show.swap(false, Ordering::SeqCst) {
                panic!("the window connection is gone");
            }
            self.window.lock().unwrap().push("show");
            // A production adapter that cannot bring the pill up returns an
            // error and leaves the window exactly as it found it.
            self.refuse_show.attempt()?;
            self.on_screen.store(true, Ordering::SeqCst);
            self.shown.fetch_add(1, Ordering::Relaxed);
            if self.blind_show.load(Ordering::SeqCst) > 0 {
                self.blind_show.fetch_sub(1, Ordering::SeqCst);
                // The window is up, but it would not stay over whatever the
                // person is looking at, so nothing proves they can see it.
                return Ok(Shown::Doubtful("the pill would not stay on top".into()));
            }
            if self.partial_show.load(Ordering::SeqCst) > 0 {
                self.partial_show.fetch_sub(1, Ordering::SeqCst);
                return Ok(Shown::Seen("the pill could not take the keyboard".into()));
            }
            self.focused.store(true, Ordering::SeqCst);
            Ok(Shown::Ready)
        }
        fn show_pill_passive(&self) -> Result<(), String> {
            self.window.lock().unwrap().push("show-passive");
            self.refuse_show.attempt()?;
            self.on_screen.store(true, Ordering::SeqCst);
            self.shown.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        fn hide_pill(&self) -> Result<(), String> {
            self.window.lock().unwrap().push("hide");
            // The pill stays up when the hide fails, which is the whole reason
            // the runtime must not record it as down.
            self.refuse_hide.attempt()?;
            self.on_screen.store(false, Ordering::SeqCst);
            self.focused.store(false, Ordering::SeqCst);
            self.hidden.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        fn restore_focus(&self) -> Result<(), String> {
            self.window.lock().unwrap().push("restore");
            self.refuse_focus.attempt()?;
            self.focused.store(false, Ordering::SeqCst);
            self.restored.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        fn present(&self, payload: PresentationPayload) -> Result<(), String> {
            let watcher = self.at_present.lock().unwrap().take();
            if let Some(watcher) = watcher {
                watcher();
            }
            self.refuse_present.attempt()?;
            self.presentations.lock().unwrap().push(payload);
            Ok(())
        }
        fn tick(&self, _payload: TickPayload) {}
        fn tray(&self, state: &str, _detail: &str) -> Result<(), String> {
            self.refuse_tray.attempt()?;
            self.tray.lock().unwrap().push(state.to_string());
            Ok(())
        }
        fn copy(&self, text: String) {
            self.copied.lock().unwrap().push(text);
        }
    }

    /// Runs deferred work only when the test asks for it, and drops the
    /// animation loop, so every assertion is deterministic.
    #[derive(Default)]
    struct QueueExecutor {
        queued: Mutex<Vec<Box<dyn FnOnce() + Send + 'static>>>,
        delayed: Mutex<Vec<Box<dyn FnOnce() + Send + 'static>>>,
    }

    impl QueueExecutor {
        /// Runs the immediate work, leaving any timeout still pending.
        fn drain(&self) {
            loop {
                let tasks: Vec<_> = self.queued.lock().unwrap().drain(..).collect();
                if tasks.is_empty() {
                    return;
                }
                for task in tasks {
                    task();
                }
            }
        }

        /// Fires every pending timeout, then settles the work it caused.
        fn expire(&self) {
            let tasks: Vec<_> = self.delayed.lock().unwrap().drain(..).collect();
            for task in tasks {
                task();
            }
            self.drain();
        }
    }

    impl Executor for QueueExecutor {
        fn spawn(&self, task: Box<dyn FnOnce() + Send + 'static>) {
            self.queued.lock().unwrap().push(task);
        }
        fn spawn_after(&self, _delay: Duration, task: Box<dyn FnOnce() + Send + 'static>) {
            self.delayed.lock().unwrap().push(task);
        }
        fn spawn_presentation_loop(&self, _task: Box<dyn FnOnce() + Send + 'static>) {}
    }

    #[derive(Default)]
    struct MemoryStore {
        pending: Mutex<Option<Pending>>,
        clears: AtomicU64,
        /// When set, every save fails with this reason.
        refuse_save: Mutex<Option<String>>,
        /// When set, every clear fails.
        refuse_clear: Mutex<bool>,
        /// When set, every tombstone fails.
        refuse_tombstone: Mutex<bool>,
        tombstones: Mutex<Vec<String>>,
        /// When set, load reports a record it cannot read.
        corrupt: Mutex<bool>,
    }

    impl PendingStore for MemoryStore {
        fn load(&self) -> Result<Option<Pending>, PendingError> {
            if *self.corrupt.lock().unwrap() {
                return Err(PendingError::Corrupt);
            }
            Ok(self.pending.lock().unwrap().clone())
        }
        fn save(&self, pending: &Pending) -> Result<(), PendingError> {
            if let Some(reason) = self.refuse_save.lock().unwrap().clone() {
                return Err(PendingError::Io {
                    operation: "written",
                    reason,
                });
            }
            *self.pending.lock().unwrap() = Some(pending.clone());
            Ok(())
        }
        fn clear(&self) -> Result<(), PendingError> {
            self.clears.fetch_add(1, Ordering::Relaxed);
            if *self.refuse_clear.lock().unwrap() {
                return Err(PendingError::Io {
                    operation: "removed",
                    reason: "denied".into(),
                });
            }
            *self.pending.lock().unwrap() = None;
            Ok(())
        }
        fn tombstone(&self, id: &str) -> Result<(), PendingError> {
            if *self.refuse_tombstone.lock().unwrap() {
                return Err(PendingError::Io {
                    operation: "written",
                    reason: "denied".into(),
                });
            }
            self.tombstones.lock().unwrap().push(id.to_string());
            // A tombstone leaves nothing to restore.
            *self.pending.lock().unwrap() = None;
            Ok(())
        }
    }

    struct FakeCapture {
        discarded: Arc<AtomicBool>,
    }

    impl Capture for FakeCapture {
        fn elapsed(&self) -> Duration {
            Duration::ZERO
        }
        fn take_level(&self) -> f32 {
            0.0
        }
        fn finish(self: Box<Self>) -> Vec<u8> {
            b"RIFF".to_vec()
        }
        fn discard(self: Box<Self>) {
            self.discarded.store(true, Ordering::Relaxed);
        }
    }

    /// Work that runs at one point inside another port's call.
    type Watcher = Box<dyn Fn() + Send>;

    #[derive(Default)]
    struct FakeRecorder {
        failure: Mutex<Option<RecorderError>>,
        /// One sink per capture handed out, oldest first.
        sinks: Mutex<Vec<ErrorSink>>,
        /// Discard flags, one per capture handed out, oldest first.
        discarded: Mutex<Vec<Arc<AtomicBool>>>,
        /// When set, the stream fails inside `start`, before it returns.
        fail_during_start: Mutex<Option<String>>,
        /// Runs as the microphone opens, so a test can see what the rest of the
        /// companion looked like at that moment.
        at_start: Mutex<Option<Watcher>>,
    }

    impl FakeRecorder {
        fn failing(error: RecorderError) -> Self {
            Self {
                failure: Mutex::new(Some(error)),
                ..Self::default()
            }
        }

        /// Fires the stream error CPAL would deliver for one open capture.
        fn fail_stream(&self, index: usize, reason: &str) {
            let sink = self.sinks.lock().unwrap()[index].clone();
            sink(reason.to_string());
        }

        fn was_discarded(&self, index: usize) -> bool {
            self.discarded.lock().unwrap()[index].load(Ordering::Relaxed)
        }

        fn captures(&self) -> usize {
            self.sinks.lock().unwrap().len()
        }
    }

    impl Recorder for FakeRecorder {
        fn start(&self, on_error: ErrorSink) -> Result<Box<dyn Capture>, RecorderError> {
            if let Some(watcher) = self.at_start.lock().unwrap().as_ref() {
                watcher();
            }
            if let Some(error) = self.failure.lock().unwrap().take() {
                return Err(error);
            }
            let discarded = Arc::new(AtomicBool::new(false));
            self.sinks.lock().unwrap().push(Arc::clone(&on_error));
            self.discarded.lock().unwrap().push(Arc::clone(&discarded));
            // CPAL can report a dead stream before `start` returns.
            if let Some(reason) = self.fail_during_start.lock().unwrap().take() {
                on_error(reason);
            }
            Ok(Box::new(FakeCapture { discarded }))
        }
    }

    struct FakeTranscriber(Result<String, String>);

    impl Transcriber for FakeTranscriber {
        fn transcribe(&self, _wav: Vec<u8>) -> Result<String, String> {
            self.0.clone()
        }
    }

    /// Work that runs while one submission is on the wire.
    type Interleave = Box<dyn FnOnce(String) + Send>;

    #[derive(Default)]
    struct RecordingBackend {
        submissions: Mutex<Vec<(String, String)>>,
        refuse: Mutex<Option<String>>,
        /// Runs once, with the submission on the wire and the handoff not yet
        /// finished. That is exactly where a fast daemon answer arrives.
        during_submit: Mutex<Option<Interleave>>,
    }

    impl Backend for RecordingBackend {
        fn submit(&self, id: String, text: String, _force: bool) -> Result<(), String> {
            if let Some(reason) = self.refuse.lock().unwrap().clone() {
                return Err(reason);
            }
            self.submissions.lock().unwrap().push((id.clone(), text));
            let interleave = self.during_submit.lock().unwrap().take();
            if let Some(interleave) = interleave {
                interleave(id);
            }
            Ok(())
        }
    }

    struct Harness {
        app: Arc<App>,
        surface: Arc<RecordedSurface>,
        recorder: Arc<FakeRecorder>,
        store: Arc<MemoryStore>,
        executor: Arc<QueueExecutor>,
        backend: Arc<RecordingBackend>,
    }

    fn harness(recorder: FakeRecorder, transcript: Result<String, String>) -> Harness {
        harness_with(recorder, transcript, Arc::new(MemoryStore::default()))
    }

    fn harness_with(
        recorder: FakeRecorder,
        transcript: Result<String, String>,
        store: Arc<MemoryStore>,
    ) -> Harness {
        let surface = Arc::new(RecordedSurface::default());
        let recorder = Arc::new(recorder);
        let executor = Arc::new(QueueExecutor::default());
        let backend = Arc::new(RecordingBackend::default());
        let app = Arc::new(App::new(Ports {
            surface: Arc::clone(&surface) as Arc<dyn Surface>,
            recorder: Arc::clone(&recorder) as Arc<dyn Recorder>,
            pending: Arc::clone(&store) as Arc<dyn PendingStore>,
            transcriber: Arc::new(FakeTranscriber(transcript)),
            executor: Arc::clone(&executor) as Arc<dyn Executor>,
            prefix: "pill".into(),
            chat_command: None,
            restart_command: None,
            ack_timeout: Duration::ZERO,
        }));
        app.set_backend(Arc::clone(&backend) as Arc<dyn Backend>);
        app.set_connected(true);
        Harness {
            app,
            surface,
            recorder,
            store,
            executor,
            backend,
        }
    }

    #[test]
    fn a_microphone_that_never_starts_shows_the_error_and_stops_claiming_to_record() {
        let harness = harness(
            FakeRecorder::failing(RecorderError::NoInputDevice),
            Ok("unused".into()),
        );
        harness.app.handle(Event::Activate);
        // The pill opens optimistically; the failure arrives right after.
        assert_eq!(harness.surface.shown.load(Ordering::Relaxed), 1);
        harness.executor.drain();

        let presentation = harness.surface.last();
        assert_eq!(presentation.state, "error");
        assert!(!presentation.recording);
        assert_eq!(presentation.detail, "no microphone is available");
        assert_eq!(
            harness.surface.tray.lock().unwrap().last().unwrap(),
            "error"
        );
        assert!(harness.backend.submissions.lock().unwrap().is_empty());
    }

    #[test]
    fn a_capture_stream_that_dies_stops_the_recording_and_shows_a_local_error() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.app.handle(Event::Activate);
        assert_eq!(harness.surface.last().state, "listening");

        harness.recorder.fail_stream(0, "the device disappeared");
        harness.executor.drain();

        let presentation = harness.surface.last();
        assert_eq!(presentation.state, "error");
        assert!(!presentation.recording);
        assert!(presentation.detail.contains("the device disappeared"));
        assert!(
            harness.recorder.was_discarded(0),
            "the failed capture was not released"
        );
        assert!(harness.backend.submissions.lock().unwrap().is_empty());
    }

    #[test]
    fn a_stream_that_fails_inside_start_releases_the_capture_it_never_installed() {
        let recorder = FakeRecorder::default();
        *recorder.fail_during_start.lock().unwrap() = Some("the device disappeared".into());
        let harness = harness(recorder, Ok("unused".into()));

        harness.app.handle(Event::Activate);
        // The handle never reached the runtime, so only `start_recording` can
        // close it. A leaked capture would hold the microphone open forever.
        assert!(
            harness.recorder.was_discarded(0),
            "the capture that failed during start was leaked"
        );
        assert!(harness.app.recording.lock().unwrap().is_none());

        harness.executor.drain();
        let presentation = harness.surface.last();
        assert_eq!(presentation.state, "error");
        assert!(!presentation.recording);
    }

    #[test]
    fn a_stale_stream_error_cannot_kill_the_recording_that_replaced_it() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Escape);
        harness.app.handle(Event::Activate);
        harness.executor.drain();
        assert_eq!(harness.recorder.captures(), 2);
        assert_eq!(harness.surface.last().state, "listening");

        // The first capture's error arrives late, after it was retired.
        harness
            .recorder
            .fail_stream(0, "the first device disappeared");
        harness.executor.drain();

        assert_eq!(
            harness.surface.last().state,
            "listening",
            "a retired capture took down its successor"
        );
        assert!(!harness.recorder.was_discarded(1));
        assert!(harness.app.recording.lock().unwrap().is_some());
    }

    #[test]
    fn a_transcript_that_cannot_be_saved_is_never_submitted() {
        let harness = harness(FakeRecorder::default(), Ok("do not lose me".into()));
        *harness.store.refuse_save.lock().unwrap() = Some("the disk is full".into());
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Enter { text: None });
        harness.executor.drain();

        assert!(
            harness.backend.submissions.lock().unwrap().is_empty(),
            "text was submitted without a durable copy behind it"
        );
        let presentation = harness.surface.last();
        assert_eq!(presentation.state, "retained");
        assert_eq!(presentation.text, "do not lose me");
        assert!(presentation.detail.contains("the disk is full"));
        assert!(presentation.editable);

        // Once the store recovers, the same identifier is submitted.
        *harness.store.refuse_save.lock().unwrap() = None;
        harness.app.handle(Event::Enter { text: None });
        harness.executor.drain();
        assert_eq!(
            *harness.backend.submissions.lock().unwrap(),
            vec![("pill-1".to_string(), "do not lose me".to_string())]
        );
    }

    #[test]
    fn a_review_draft_survives_a_failed_save_and_says_so() {
        let harness = harness(FakeRecorder::default(), Ok("draft text".into()));
        *harness.store.refuse_save.lock().unwrap() = Some("the disk is full".into());
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Activate);
        harness.executor.drain();

        let presentation = harness.surface.last();
        assert_eq!(presentation.state, "review");
        assert_eq!(presentation.text, "draft text");
        assert!(presentation.detail.contains("the disk is full"));
        assert!(harness.backend.submissions.lock().unwrap().is_empty());
    }

    #[test]
    fn an_unreadable_store_is_reported_instead_of_being_read_as_empty() {
        let store = Arc::new(MemoryStore::default());
        *store.corrupt.lock().unwrap() = true;
        let harness = harness_with(FakeRecorder::default(), Ok("unused".into()), store);
        harness.app.start();

        let presentation = harness.surface.last();
        assert_eq!(presentation.state, "error");
        assert_eq!(presentation.detail, "the saved transcript is unreadable");
        assert_eq!(harness.surface.shown.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_removal_that_keeps_failing_does_not_reopen_the_finished_pill() {
        let harness = harness(FakeRecorder::default(), Ok("open the tasks widget".into()));
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Enter { text: None });
        harness.executor.drain();

        *harness.store.refuse_clear.lock().unwrap() = true;
        harness.app.handle(Event::Acknowledged("pill-1".into()));
        harness.executor.drain();

        // Retried, then left for the next start to recover and resend under the
        // same identifier, which the daemon suppresses. The resident pill
        // stays up, at rest.
        assert_eq!(harness.store.clears.load(Ordering::Relaxed), 2);
        assert_eq!(harness.surface.hidden.load(Ordering::Relaxed), 0);
        assert!(harness.surface.on_screen());
        assert_eq!(harness.surface.last().state, "idle");
    }

    #[test]
    fn an_accepted_transcript_is_persisted_before_it_is_submitted() {
        let harness = harness(FakeRecorder::default(), Ok("remember the milk".into()));
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Enter { text: None });
        harness.executor.drain();

        assert_eq!(
            *harness.store.pending.lock().unwrap(),
            Some(Pending {
                id: "pill-1".into(),
                text: "remember the milk".into(),
            })
        );
        assert_eq!(
            *harness.backend.submissions.lock().unwrap(),
            vec![("pill-1".to_string(), "remember the milk".to_string())]
        );
        // The keyboard comes back with the handoff, not with the answer: an
        // ordinary submission must not hold it for a whole turn. The window
        // itself stays up, passive, to report the turn it started.
        assert_eq!(harness.surface.window(), ["show", "restore"]);
        assert!(harness.surface.on_screen());
        assert!(!harness.surface.focused());

        // The acknowledgment retires the durable copy. The resident pill
        // stays up, resting, and focus, already restored at the handoff, is
        // not touched again.
        harness.app.handle(Event::Acknowledged("pill-1".into()));
        assert_eq!(*harness.store.pending.lock().unwrap(), None);
        assert_eq!(harness.surface.window(), ["show", "restore"]);
        assert!(harness.surface.on_screen());
        assert_eq!(harness.surface.last().state, "idle");
        assert_eq!(harness.surface.restored.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_restarted_companion_recovers_the_accepted_transcript_and_reuses_its_identifier() {
        let store = Arc::new(MemoryStore::default());
        let first = harness_with(
            FakeRecorder::default(),
            Ok("survives the crash".into()),
            Arc::clone(&store),
        );
        first.app.handle(Event::Activate);
        first.app.handle(Event::Enter { text: None });
        first.executor.drain();
        assert!(store.pending.lock().unwrap().is_some());
        // The process dies here: no acknowledgment, no explicit discard.

        let restarted = harness_with(
            FakeRecorder::default(),
            Ok("unused".into()),
            Arc::clone(&store),
        );
        restarted.app.start();

        let presentation = restarted.surface.last();
        assert_eq!(presentation.state, "uncertain");
        assert_eq!(presentation.text, "survives the crash");
        // The previous process may already have delivered it, so the recovered
        // text is frozen rather than editable.
        assert!(!presentation.editable);
        assert_eq!(restarted.surface.shown.load(Ordering::Relaxed), 1);

        // A recovered transcript may already have run, so the first Enter only
        // says so and the second is the person's own decision.
        restarted.app.handle(Event::Enter { text: None });
        restarted.executor.drain();
        assert!(
            restarted.backend.submissions.lock().unwrap().is_empty(),
            "a recovered transcript was resent without the person saying so"
        );
        restarted.app.handle(Event::Enter { text: None });
        restarted.executor.drain();
        assert_eq!(
            *restarted.backend.submissions.lock().unwrap(),
            vec![("pill-1".to_string(), "survives the crash".to_string())],
            "the recovered submission must keep its identifier"
        );
    }

    #[test]
    fn a_delivered_submission_with_no_acknowledgment_is_retried_under_one_identifier() {
        let harness = harness(FakeRecorder::default(), Ok("open the tasks widget".into()));
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Enter { text: None });
        harness.executor.drain();
        // The connection drops after the daemon accepted the text but before
        // the acknowledgment arrives, so the timeout is what fires.
        harness.executor.expire();

        let presentation = harness.surface.last();
        assert_eq!(presentation.state, "uncertain");
        assert!(
            !presentation.editable,
            "a possibly delivered transcript must not be editable"
        );
        assert_eq!(
            harness.surface.tray.lock().unwrap().last().unwrap(),
            "attention"
        );
        // The first Enter warns and sends nothing; only the second sends.
        harness.app.handle(Event::Enter {
            text: Some("something else entirely".into()),
        });
        harness.executor.drain();
        assert_eq!(harness.backend.submissions.lock().unwrap().len(), 1);
        harness.app.handle(Event::Enter {
            text: Some("something else entirely".into()),
        });
        harness.executor.drain();

        let submissions = harness.backend.submissions.lock().unwrap().clone();
        assert_eq!(submissions.len(), 2);
        assert_eq!(submissions[0].0, submissions[1].0);
        assert_eq!(
            submissions[0].1, submissions[1].1,
            "the retry must resend the accepted text, not an edit"
        );
        assert!(store_still_holds(&harness));
    }

    fn store_still_holds(harness: &Harness) -> bool {
        harness.store.pending.lock().unwrap().is_some()
    }

    #[test]
    fn a_daemon_refusal_keeps_the_words_editable_and_ordinarily_retriable() {
        let harness = harness(FakeRecorder::default(), Ok("open the tasks widget".into()));
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Enter { text: None });
        harness.executor.drain();
        assert_eq!(harness.surface.last().state, "sent");

        // What the daemon answers when its own preflight refused the send:
        // nothing left it, and it says which submission that was.
        harness.app.observe(DaemonEvent::Refused(
            "pill-1".into(),
            "submission pill-1 was not sent: the Scufris session is not ready".into(),
        ));
        harness.executor.drain();

        let presentation = harness.surface.last();
        assert_eq!(
            presentation.state, "retained",
            "a request the conversation never saw was frozen as if it might have run"
        );
        assert!(presentation.editable);

        // One Enter sends it, with no forced-send confirmation in the way.
        harness.app.handle(Event::Enter { text: None });
        harness.executor.drain();
        let submissions = harness.backend.submissions.lock().unwrap().clone();
        assert_eq!(submissions.len(), 2);
        assert_eq!(submissions[1].0, "pill-1");
    }

    /// The daemon can answer before the handoff has finished running. The
    /// answer is the newer decision, and both of its outcomes need the person,
    /// so the pill it reopens must not then be closed by the handoff that was
    /// already under way.
    #[test]
    fn an_answer_that_arrives_during_the_handoff_leaves_the_pill_on_screen() {
        let refused = || {
            DaemonEvent::Refused(
                "pill-1".into(),
                "submission pill-1 was not sent: the Scufris session is not ready".into(),
            )
        };
        let uncertain = || {
            DaemonEvent::Uncertain(
                "pill-1".into(),
                "The backend did not confirm delivery.".into(),
            )
        };
        for (answers, expected) in [
            (vec![refused()], "retained"),
            (vec![uncertain()], "uncertain"),
            // A refusal settles those words, so the uncertainty behind it
            // answers nothing and cannot freeze them.
            (vec![refused(), uncertain()], "retained"),
        ] {
            let harness = harness(FakeRecorder::default(), Ok("book the flight".into()));
            let runtime = Arc::clone(&harness.app);
            *harness.backend.during_submit.lock().unwrap() = Some(Box::new(move |_id| {
                // On the daemon link's own thread, and joined here, so the
                // interleaving is real and the test is still deterministic:
                // the answers are fully applied while the submitting thread
                // is stopped at the write.
                thread::spawn(move || {
                    for answer in answers {
                        runtime.observe(answer);
                    }
                })
                .join()
                .expect("the answer thread panicked");
            }));

            harness.app.handle(Event::Activate);
            harness.app.handle(Event::Enter { text: None });
            harness.executor.drain();

            assert_eq!(harness.surface.last().state, expected);
            assert!(
                harness.surface.on_screen(),
                "the {expected} transcript needs the person, and its pill was hidden"
            );
            assert_eq!(
                harness.surface.hidden.load(Ordering::Relaxed),
                0,
                "the handoff closed a pill that had already been reopened"
            );
            assert_eq!(
                harness.surface.restored.load(Ordering::Relaxed),
                0,
                "focus was given away from a pill that is still asking for it"
            );
        }
    }

    /// The privacy indicator has to be up before the microphone is, so the pill
    /// opens ahead of the actions the same change asks for.
    #[test]
    fn the_pill_opens_before_the_microphone_does() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        let surface = Arc::clone(&harness.surface);
        let up = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&up);
        *harness.recorder.at_start.lock().unwrap() = Some(Box::new(move || {
            seen.store(surface.on_screen(), Ordering::SeqCst)
        }));

        harness.app.handle(Event::Activate);
        assert!(
            up.load(Ordering::SeqCst),
            "the microphone opened before anything said so"
        );
    }

    /// A production window adapter reports a failure by returning one, not by
    /// unwinding. The pill is the recording privacy indicator, so a pill that
    /// did not come up is a microphone that does not open.
    #[test]
    fn a_pill_that_will_not_come_up_never_opens_the_microphone() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.surface.refuse_show.fail(1);

        harness.app.handle(Event::Activate);
        harness.executor.drain();

        assert_eq!(harness.surface.window(), ["show"]);
        assert!(
            !harness.surface.on_screen(),
            "the fake did not model a show that leaves the pill down"
        );
        assert!(
            harness.recorder.sinks.lock().unwrap().is_empty(),
            "the microphone opened behind a privacy indicator that never came up"
        );
        // The tray is the only thing left that can say what happened.
        assert_eq!(harness.surface.tray_states().last().unwrap(), "error");
        let presentation = harness.surface.last();
        assert_eq!(presentation.state, "error");
        assert_eq!(presentation.detail, "the pill did not come up");
        assert!(!presentation.recording);

        // Nothing retried by itself. The person's next activation does, and
        // this time both the pill and the microphone come up.
        harness.app.handle(Event::Activate);
        assert!(harness.surface.on_screen());
        assert_eq!(harness.surface.last().state, "listening");
        assert_eq!(harness.recorder.sinks.lock().unwrap().len(), 1);
    }

    /// A show can put the pill up and still not get everything it asked for.
    /// The person can see it, so what the pill is for works and the recording
    /// may start; the keyboard is elsewhere, so nothing records that it is.
    #[test]
    fn a_pill_that_comes_up_without_the_keyboard_is_asked_again() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.surface.partial_show.store(1, Ordering::SeqCst);

        harness.app.handle(Event::Activate);
        assert!(
            harness.surface.on_screen(),
            "the privacy indicator is not up"
        );
        assert!(!harness.surface.focused());
        assert_eq!(
            harness.recorder.sinks.lock().unwrap().len(),
            1,
            "the pill was seen, so the recording had the indicator it needs"
        );
        assert_eq!(harness.surface.window(), ["show"]);

        // Nobody can type into a pill that does not hold the keyboard, so no
        // key of the person's can ask for it again. The runtime asks itself.
        harness.executor.expire();
        assert_eq!(harness.surface.window(), ["show", "show"]);
        assert!(
            harness.surface.focused(),
            "the pill never asked for the keyboard again"
        );
    }

    /// A pill that would not go down is still up, and an always-on-top pill
    /// left over the desktop is the failure this must not record as success.
    ///
    /// Nobody outside the runtime can clear it. Focus has already gone back to
    /// the person's window, so the keys they press reach that window, not the
    /// pill. The runtime takes it down itself.
    #[test]
    fn a_pill_that_will_not_go_down_takes_itself_down() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.app.handle(Event::Activate);
        harness.surface.refuse_hide.fail(1);

        harness.app.handle(Event::Escape);
        assert!(
            harness.surface.on_screen(),
            "the fake did not model a hide that leaves the pill up"
        );
        assert_eq!(harness.surface.hidden.load(Ordering::Relaxed), 0);
        assert_eq!(harness.surface.window(), ["show", "restore", "hide"]);

        harness.executor.expire();
        assert!(
            !harness.surface.on_screen(),
            "the always-on-top pill was left over the desktop"
        );
        // Focus went back on the first attempt, before the hide failed, so
        // the repair has only the hide left to do.
        assert_eq!(
            harness.surface.window(),
            ["show", "restore", "hide", "hide"]
        );
    }

    /// Asking again is bounded. A window manager that has refused every time
    /// is refusing, and a runtime that asked forever would be a timer nobody
    /// can stop. What it must not do is stop asking for good.
    #[test]
    fn a_window_that_keeps_refusing_is_not_asked_forever() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.app.handle(Event::Activate);
        harness.surface.refuse_hide.fail(u64::MAX);

        harness.app.handle(Event::Escape);
        for _ in 0..REPAIR_ATTEMPTS + 2 {
            harness.executor.expire();
        }
        let hides = |harness: &Harness| {
            harness
                .surface
                .window()
                .iter()
                .filter(|operation| **operation == "hide")
                .count()
        };
        assert_eq!(
            hides(&harness),
            1 + REPAIR_ATTEMPTS,
            "the runtime did not stop asking"
        );

        // The next decision asks again, so a window that comes back is not
        // left with a pill nothing will ever take down.
        harness.surface.refuse_hide.fail(0);
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Escape);
        assert!(
            !harness.surface.on_screen(),
            "the runtime gave up on the window for good"
        );
    }

    /// The pill is the privacy indicator, so a presentation it would not take
    /// is not on screen, whatever the tray managed. The microphone stays shut,
    /// and the tray is still told, because it is the only thing left that can
    /// say anything at all.
    #[test]
    fn the_microphone_stays_shut_behind_a_pill_that_rendered_nothing() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        let before = harness.surface.presentations.lock().unwrap().len();
        harness.surface.refuse_present.fail(u64::MAX);

        harness.app.handle(Event::Activate);
        harness.executor.drain();
        assert_eq!(
            harness.surface.presentations.lock().unwrap().len(),
            before,
            "the fake did not model a pill that renders nothing"
        );
        assert_eq!(
            harness.recorder.captures(),
            0,
            "the microphone opened with nothing on screen saying so"
        );
        assert!(
            harness
                .surface
                .tray_states()
                .iter()
                .any(|state| state == "listening"),
            "the tray was left behind because the pill refused"
        );

        // The pill takes presentations again, and the next activation records.
        harness.surface.refuse_present.fail(0);
        harness.app.handle(Event::Activate);
        harness.executor.drain();
        assert_eq!(harness.surface.last().state, "listening");
        assert_eq!(harness.recorder.captures(), 1);
    }

    /// A pill the window manager will not keep on top may be behind whatever
    /// the person is reading. It is up, and it is rendering the recording
    /// state, and neither of those proves they can see it.
    #[test]
    fn a_pill_that_may_be_behind_another_window_stops_the_recording() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        // The pill comes up where it can be seen but without the keyboard, so
        // the runtime keeps asking for the window.
        harness.surface.partial_show.store(1, Ordering::SeqCst);
        harness.app.handle(Event::Activate);
        assert_eq!(
            harness.recorder.captures(),
            1,
            "the pill was seen, so the recording could start"
        );

        // The next attempt gets the window up and cannot keep it on top.
        harness.surface.blind_show.store(u64::MAX, Ordering::SeqCst);
        harness.executor.expire();

        assert!(
            harness.recorder.was_discarded(0),
            "the microphone stayed open behind a pill nothing proves is visible"
        );
        assert_eq!(harness.recorder.captures(), 1);
        let presentation = harness.surface.last();
        assert_eq!(presentation.state, "error");
        assert!(!presentation.recording);
    }

    /// A key pressed while another thread is on the surface is not settled by
    /// that thread's decision. Its own work waits for the surface to reach the
    /// state it needs, and runs on whichever thread got it there.
    #[test]
    fn a_key_pressed_while_the_surface_is_busy_waits_for_what_it_needs() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        let runtime = Arc::clone(&harness.app);
        let recorder = Arc::clone(&harness.recorder);
        let on_return = Arc::new(AtomicU64::new(u64::MAX));
        let recorded = Arc::clone(&on_return);
        *harness.surface.at_present.lock().unwrap() = Some(Box::new(move || {
            // On another thread, and joined here, so the interleaving is real
            // and the test is still deterministic: the person's key arrives
            // while the daemon's state change is still on the surface.
            let runtime = Arc::clone(&runtime);
            let recorder = Arc::clone(&recorder);
            let recorded = Arc::clone(&recorded);
            thread::spawn(move || {
                runtime.handle(Event::Activate);
                recorded.store(recorder.captures() as u64, Ordering::SeqCst);
            })
            .join()
            .expect("the interleaving thread panicked");
        }));

        harness
            .app
            .set_assistant(scufris_control::AssistantState::Working, "packing".into());

        assert_eq!(
            on_return.load(Ordering::SeqCst),
            0,
            "the microphone opened before any surface had shown the person it was open"
        );
        assert!(harness.surface.on_screen());
        assert_eq!(harness.surface.last().state, "listening");
        assert_eq!(
            harness.recorder.captures(),
            1,
            "the key press was left waiting for a proof that had already arrived"
        );
    }

    /// The pill and the tray are the only things the companion can say anything
    /// with, so a presentation that reached neither is tried again.
    #[test]
    fn a_presentation_that_reaches_nothing_is_tried_again() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        let before = harness.surface.presentations.lock().unwrap().len();

        // Two refusals, then the pill takes it: one change, three attempts.
        harness.surface.refuse_present.fail(2);
        harness.app.handle(Event::Activate);
        assert_eq!(
            harness.surface.presentations.lock().unwrap().len(),
            before + 1,
            "the presentation was never tried again"
        );
        assert_eq!(harness.surface.last().state, "listening");

        // Trying again is bounded. A surface refusing everything is left to the
        // next change rather than asked forever.
        harness.surface.refuse_present.fail(u64::MAX);
        harness.app.handle(Event::Enter { text: None });
        assert_eq!(
            u64::MAX
                - harness
                    .surface
                    .refuse_present
                    .remaining
                    .load(Ordering::SeqCst),
            RENDER_ATTEMPTS as u64,
            "the runtime did not stop asking"
        );
        assert_eq!(
            harness.surface.last().state,
            "listening",
            "a presentation nothing rendered was recorded as rendered"
        );

        // The next change publishes again, and reaches the pill.
        harness.surface.refuse_present.fail(0);
        harness.app.handle(Event::Escape);
        assert_eq!(harness.surface.last().state, "idle");
    }

    /// The tray can refuse a state. While that one is being tried again the
    /// companion can move on, and it is the newer presentation - not the one
    /// that failed - that the pill and the tray must end on.
    #[test]
    fn a_newer_presentation_overtakes_one_the_tray_refused() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.surface.refuse_tray.fail(1);
        let runtime = Arc::clone(&harness.app);
        *harness.surface.at_present.lock().unwrap() = Some(Box::new(move || {
            // On another thread, and joined here, so the interleaving is real
            // and the test is still deterministic: the companion has finished
            // with the pill while the tray is still refusing the state before.
            let runtime = Arc::clone(&runtime);
            thread::spawn(move || {
                runtime.set_assistant(scufris_control::AssistantState::Working, "packing".into());
                runtime.handle(Event::Escape);
            })
            .join()
            .expect("the interleaving thread panicked");
        }));

        harness.app.handle(Event::Activate);

        let states = harness.surface.tray_states();
        assert!(
            !states.iter().any(|state| state == "listening"),
            "the tray took a state the companion had already left"
        );
        assert_eq!(
            states.last().unwrap(),
            "working",
            "the tray did not end where the companion did"
        );
        let presentation = harness.surface.last();
        assert_eq!(presentation.state, "working");
        assert_eq!(presentation.detail, "packing");
        assert!(!harness.surface.on_screen());
    }

    /// A window operation that fails must not leave every later decision
    /// waiting for the thread it stopped in.
    #[test]
    fn a_window_operation_that_fails_does_not_freeze_the_pill() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.surface.panic_on_show.store(true, Ordering::SeqCst);

        let runtime = Arc::clone(&harness.app);
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            runtime.handle(Event::Activate)
        }));
        assert!(failed.is_err(), "the surface was expected to fail");
        assert!(!harness.surface.on_screen());

        // The runtime still owns the window, so the next activation opens it.
        harness.app.handle(Event::Escape);
        harness.app.handle(Event::Activate);
        assert!(
            harness.surface.on_screen(),
            "one failed window operation stopped every later one"
        );
        assert_eq!(harness.surface.last().state, "listening");
    }

    #[test]
    fn a_late_answer_for_a_retired_submission_leaves_the_new_one_alone() {
        let harness = harness(FakeRecorder::default(), Ok("open the tasks widget".into()));
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Enter { text: None });
        harness.executor.drain();
        harness
            .app
            .observe(DaemonEvent::Acknowledged("pill-1".into()));
        harness.executor.drain();

        // A second recording is under way when the first submission's slow
        // answer finally arrives.
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Enter { text: None });
        harness.executor.drain();
        assert_eq!(harness.surface.last().state, "sent");

        harness.app.observe(DaemonEvent::Uncertain(
            "pill-1".into(),
            "The backend did not confirm delivery.".into(),
        ));
        harness.executor.drain();
        assert_eq!(
            harness.surface.last().state,
            "sent",
            "an answer about a retired submission froze the one that replaced it"
        );

        harness
            .app
            .observe(DaemonEvent::Acknowledged("pill-2".into()));
        harness.executor.drain();
        assert_eq!(harness.surface.last().state, "idle");
    }

    #[test]
    fn an_unreachable_backend_keeps_the_transcript_on_disk() {
        let harness = harness(FakeRecorder::default(), Ok("keep me".into()));
        *harness.backend.refuse.lock().unwrap() = Some("The backend is unavailable.".into());
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Enter { text: None });
        harness.executor.drain();

        assert_eq!(harness.surface.last().state, "retained");
        assert_eq!(
            harness.store.pending.lock().unwrap().as_ref().unwrap().text,
            "keep me"
        );
        assert_eq!(harness.store.clears.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_discard_that_cannot_be_removed_is_tombstoned_so_it_cannot_come_back() {
        let store = Arc::new(MemoryStore::default());
        let first = harness_with(
            FakeRecorder::default(),
            Ok("forget I said that".into()),
            Arc::clone(&store),
        );
        first.app.handle(Event::Activate);
        first.app.handle(Event::Activate);
        first.executor.drain();
        assert_eq!(first.surface.last().state, "review");

        *store.refuse_clear.lock().unwrap() = true;
        first.app.handle(Event::Escape);
        first.executor.drain();

        assert_eq!(store.tombstones.lock().unwrap().len(), 1);
        assert_eq!(first.surface.hidden.load(Ordering::Relaxed), 1);

        // Discarded words must not reappear in the next process.
        let restarted = harness_with(
            FakeRecorder::default(),
            Ok("unused".into()),
            Arc::clone(&store),
        );
        restarted.app.start();
        assert_eq!(restarted.surface.last().state, "idle");
        assert_eq!(restarted.surface.shown.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_discard_that_can_neither_be_removed_nor_tombstoned_reopens_the_pill() {
        let harness = harness(FakeRecorder::default(), Ok("forget I said that".into()));
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Activate);
        harness.executor.drain();

        *harness.store.refuse_clear.lock().unwrap() = true;
        *harness.store.refuse_tombstone.lock().unwrap() = true;
        harness.app.handle(Event::Escape);
        harness.executor.drain();

        // Silently keeping words the user threw away would be worse than noise.
        let presentation = harness.surface.last();
        assert_eq!(presentation.state, "error");
        assert!(
            presentation.detail.contains("still on disk"),
            "{}",
            presentation.detail
        );
        // The discard never completed, so the pill it would have closed is
        // still up and now carries the reason.
        assert!(harness.surface.on_screen());
        assert_eq!(harness.surface.hidden.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_late_acknowledgment_retires_a_retained_transcript() {
        let harness = harness(FakeRecorder::default(), Ok("open the tasks widget".into()));
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Enter { text: None });
        harness.executor.drain();
        harness.executor.expire();
        assert_eq!(harness.surface.last().state, "uncertain");

        // The daemon confirms after the companion gave up waiting.
        harness.app.handle(Event::Acknowledged("pill-1".into()));
        harness.executor.drain();
        assert_eq!(harness.surface.last().state, "idle");
        assert_eq!(*harness.store.pending.lock().unwrap(), None);
        // Never hidden: the resident pill settles back to resting instead of
        // leaving the screen.
        assert_eq!(harness.surface.hidden.load(Ordering::Relaxed), 0);
        assert!(harness.surface.on_screen());
    }

    #[test]
    fn a_discarded_review_clears_the_durable_transcript() {
        let harness = harness(FakeRecorder::default(), Ok("never mind".into()));
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Activate);
        harness.executor.drain();
        assert_eq!(harness.surface.last().state, "review");
        assert!(harness.store.pending.lock().unwrap().is_some());

        harness.app.handle(Event::Escape);
        assert_eq!(*harness.store.pending.lock().unwrap(), None);
        assert_eq!(harness.store.clears.load(Ordering::Relaxed), 1);
        assert!(harness.store.tombstones.lock().unwrap().is_empty());
        assert_eq!(harness.surface.hidden.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_failed_transcription_persists_nothing() {
        let harness = harness(
            FakeRecorder::default(),
            Err("Speech recognition is unreachable.".into()),
        );
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Enter { text: None });
        harness.executor.drain();

        assert_eq!(harness.surface.last().state, "error");
        assert_eq!(*harness.store.pending.lock().unwrap(), None);
        assert!(harness.backend.submissions.lock().unwrap().is_empty());
    }

    #[test]
    fn identifier_prefixes_are_random_and_wire_safe() {
        let prefixes: std::collections::HashSet<String> =
            (0..64).map(|_| App::process_prefix()).collect();
        assert_eq!(prefixes.len(), 64, "prefixes repeated");
        for prefix in &prefixes {
            assert_eq!(prefix.len(), 32);
            assert!(scufris_control::is_submission_id(&format!("{prefix}-1")));
        }
    }

    #[test]
    fn the_status_line_matches_the_tray_tooltip() {
        assert_eq!(status_line("working", ""), "Scufris is working");
    }

    #[test]
    fn an_unconfigured_backend_restart_consumes_no_budget() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.app.restart_backend();
        assert!(harness.app.restarts.lock().unwrap().allow(0));
    }
}
