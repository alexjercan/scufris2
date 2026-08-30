//! Runtime glue between the pill state machine and the outside world.
//!
//! Every decision lives in [`crate::state::Companion`]. This module runs the
//! actions the machine returns and keeps the window and the pill on the phase
//! those actions left behind, which is not the same thing: the person's key and
//! the service's answer arrive on different threads, and the phase from the
//! change that ran last is the one they must both end up looking at. Each
//! outside effect is a port, so the failure paths that matter - a microphone
//! that never starts, a capture stream that dies, a submission the service
//! never
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
    link::LinkEvent,
    pending::{Pending, PendingStore},
    state::{Action, Assistant, Companion, Event, Posture},
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

/// How long between two looks at the keyboard while a phase needs it.
///
/// Only ever while the textbox is holding the person's keys, which is a turn
/// and nothing longer, and it costs two questions to the display. Short enough
/// that a keyboard lost to something that came and went is back before the
/// person has finished pressing the key that did nothing.
const WATCH_INTERVAL: Duration = Duration::from_millis(400);

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
    /// Transcript the textbox shows.
    pub text: String,
    /// Short explanation of an error or a retained transcript.
    pub detail: String,
    /// Whether the textbox field may be edited.
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

/// What a request to put a window on screen achieved.
///
/// Asking is not achieving, and the difference decides whether the microphone
/// may open: the pill is the recording privacy indicator, so the runtime is
/// told what the window is doing rather than what it was told to do.
///
/// Not achieving is not the same as failing, which is what [`Shown::Unsure`] is
/// for. A window request is carried out somewhere else and later, so most of
/// the time an answer read straight after one describes the moment before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    /// The window is up, on top, and holds the keyboard: everything asked for.
    /// Only the textbox ever answers this. The pill never takes the keyboard.
    Ready,
    /// The window is up and on top, so the person can see it. `Some` when the
    /// keyboard is provably somewhere else, which is worth saying out loud, and
    /// `None` when nothing could say where it went, which is not.
    Seen(Option<String>),
    /// The window is up, but nothing proved the person can see it: it may be
    /// behind whatever they are looking at. Nothing that rests on being seen
    /// may rest on this.
    Doubtful(String),
    /// The window was asked to come up and nothing could say whether it did.
    /// Not a failure and not an achievement: the phase carries on, nothing that
    /// needs the person to see the pill may run, and the next decision asks
    /// again.
    Unsure(String),
}

/// What a request to take the pill off screen achieved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hidden {
    /// The pill is down.
    Down,
    /// The pill was asked to go down and nothing could say whether it did.
    Unsure(String),
}

/// The two windows and the tray.
///
/// Every operation whose outcome a later decision depends on reports whether it
/// happened. A window that did not come up, a pill that is still on screen after
/// a hide, a presentation that never reached the pill: each is something the
/// runtime must not record as done, because the record is what stops it being
/// tried again.
pub trait Surface: Send + Sync {
    /// Shows the pill without ever touching the keyboard, and says what that
    /// achieved. Never [`Shown::Ready`]: the pill is an indicator.
    fn show_pill(&self) -> Result<Shown, String>;
    /// Hides the pill, and says what that achieved.
    fn hide_pill(&self) -> Result<Hidden, String>;
    /// Answers whether the layer is holding anything worth staying on screen
    /// for.
    ///
    /// What separates cancelling a take from putting the companion away. With
    /// panels on the layer the two are different intentions and Escape means
    /// only the first; with an empty layer they are the same intention, and
    /// asking for it twice is a step the person did not need.
    fn holding(&self) -> bool;
    /// Records the active window, then puts the textbox over the pill with the
    /// keyboard, and says what that achieved.
    fn show_textbox(&self) -> Result<Shown, String>;
    /// Takes the textbox down and gives the keyboard back to the window it
    /// covered.
    ///
    /// Asked for whether or not the record says the box is up, so a box that
    /// came up without being written down is still taken away. The adapter is
    /// what makes that idempotent: focus is given back only by a box that was
    /// really there, because restoring it from a box that never rose would take
    /// the keyboard off the person's own window.
    fn hide_textbox(&self) -> Result<(), String>;
    /// Answers whether the textbox holds the keyboard at this moment.
    ///
    /// What a show achieved was true when it was written down, and the person
    /// can click their own window straight afterwards. A phase that needs the
    /// keys asks the display rather than reading that record.
    fn textbox_has_keyboard(&self) -> bool;
    /// Answers whether the keyboard is on nothing that can use it.
    ///
    /// The display says so, or nothing does: a keyboard no client was given, or
    /// one on a window of the companion's own that refuses every key. Both are
    /// keys landing nowhere, and neither can be taken from anybody, which is
    /// what separates this from a window the person moved to themselves.
    fn nobody_has_the_keyboard(&self) -> bool;
    /// Renders one presentation in the pill and the textbox.
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

/// Where the one key outside a window is held while the pill is up.
///
/// Every ordinary key belongs to the textbox, which is focused and reads them
/// itself. What is left is the gap the textbox is not up for: between the
/// hotkey and the words arriving, nothing holds the keyboard, so the key that
/// stops a listen has to be an accelerator the display grabs. It is safe for as
/// long as the pill is on screen and is given straight back when it is not, so
/// the runtime says whether the pill is up and leaves the arrangement here.
pub trait Keys: Send + Sync {
    /// Says whether the pill is on screen, and only when that changes.
    ///
    /// Nothing is reported back. A key that could not be arranged is not a
    /// phase that failed: the tray, the hotkey, and `scufris-ctl` all still
    /// reach the same runtime.
    fn stand(&self, on_screen: bool);
}

/// Local speech-to-text.
pub trait Transcriber: Send + Sync {
    /// Turns one WAV recording into text, or explains why it could not.
    fn transcribe(&self, wav: Vec<u8>) -> Result<String, String>;
}

/// The client end of the service protocol.
pub trait Backend: Send + Sync {
    /// Submits one accepted transcript.
    fn submit(&self, id: String, text: String) -> Result<(), String>;
    /// Ends the agent's current run.
    fn abort(&self, id: String) -> Result<(), String>;
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
    /// Detail for the tray tooltip and status item, independent of the pill.
    tray_detail: String,
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

/// What the last window operations proved about the companion's windows.
///
/// One ladder for both windows, because the textbox only ever stands over a
/// pill that is up: there is no rung where the box holds the keys and the
/// indicator under it is not there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    /// The pill is down.
    Off,
    /// Nothing is known. The window was asked for something and nothing could
    /// say whether it happened, so every posture counts as not reached and the
    /// repair chain asks again from a thread that can wait for the answer.
    Unknown,
    /// The pill is up, but nothing proved the person can see it.
    Doubtful,
    /// The pill is up and on top, so the person can see it.
    Seen,
    /// The pill is up and the textbox stands over it holding the keyboard.
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
    /// Where the pill's keys are held while the pill is up.
    pub keys: Arc<dyn Keys>,
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
    /// Executable that opens the conversation in a terminal, when one is
    /// configured.
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
    /// Identifies the submission attempt the runtime is waiting on.
    ///
    /// The identifier cannot do it. A retry reuses the identifier of the
    /// attempt it retries - that is what makes it a retry - so the
    /// acknowledgement timers of the two attempts are indistinguishable to
    /// `Event::SubmissionUncertain`, which matches on the identifier alone.
    /// The first attempt's timer would then settle the second attempt: submit
    /// at t=0, refused at t=2, retry at t=3, and at t=15 a twelve-second-old
    /// live submission is frozen as uncertain with a forced-send warning it
    /// has not earned. Same shape as [`App::capture_generation`], same reason.
    submissions: AtomicU64,
    /// Counts the changes made to the companion, so the surfaces can tell a
    /// newer decision from an older one.
    decisions: AtomicU64,
    /// Keeps the windows, the pill, and the tray on the newest decision.
    surface: Ordered<Surfaced>,
    /// What the window operations proved the windows are doing, as opposed to
    /// what they were asked to do. Only a change is worth an operation: raising
    /// a textbox that already holds the keyboard would take focus back from
    /// whatever the person moved to.
    screen: Mutex<Screen>,
    /// True while the presentation the pill last took says the microphone is
    /// open. The pill is the privacy indicator: the tray says the same thing,
    /// but a tray icon can be folded away into an overflow menu, so nothing
    /// rests the microphone on it.
    drawn_recording: AtomicBool,
    /// True while a chain of window repairs is already under way, so a window
    /// that keeps falling short does not collect one chain per decision.
    repairing: AtomicBool,
    /// True while the keyboard is being watched, so a phase that needs it does
    /// not collect one watch per decision.
    watching: AtomicBool,
    /// Whether the pill was on screen when the cancel key was last arranged.
    /// Arranging it costs a round trip to the display, so only a change is
    /// worth one.
    keys_stand: Mutex<bool>,
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
            submissions: AtomicU64::new(0),
            decisions: AtomicU64::new(0),
            surface: Ordered::default(),
            // The pill window is built hidden.
            screen: Mutex::new(Screen::Off),
            drawn_recording: AtomicBool::new(false),
            repairing: AtomicBool::new(false),
            watching: AtomicBool::new(false),
            keys_stand: Mutex::new(false),
        }
    }

    /// Returns a per-process identifier prefix.
    ///
    /// Submission identifiers outlive the process that made them, and they are
    /// how an answer is matched to the request that asked: the service echoes
    /// the identifier and nothing else names the submission. Two live
    /// submissions sharing one would take each other's answers. A process
    /// identifier and a clock are not enough - identifiers are reused and
    /// clocks move backwards - so the prefix is drawn from the operating
    /// system's randomness.
    ///
    /// It is not a duplicate guard. Protocol v2 suppressed by identifier and
    /// the inversion deleted that; the service keys its own pending table by
    /// its own correlation and never looks the client's identifier up. What
    /// stops a transcript reaching the conversation twice is that every resend
    /// goes through [`crate::state::Delivery::Uncertain`], which is not
    /// editable and takes two Enters past a warning. The person is the guard.
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
    ///
    /// Called only once the event loop is running, and from a thread that is
    /// not the one running it. The recovered words go into a phase the person
    /// has to see, so this waits for the pill to be put on screen before
    /// deciding anything about them - and the loop is what puts it there. Run
    /// before the loop, this decides about a window that has not been given the
    /// chance to exist, which is how the recovered transcript came to be
    /// dropped on every start.
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

    /// Installs the service connection.
    pub fn set_backend(&self, backend: Arc<dyn Backend>) {
        *self
            .backend
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(backend);
    }

    /// Records whether the service connection is open.
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
                info!("service connected");
            } else {
                info!("service disconnected");
            }
        }
        // The phase is untouched, so the window has nothing to catch up to.
        self.publish();
    }

    /// Applies one thing the service link observed.
    ///
    /// Every answer about a submission carries the identifier it answers, and
    /// the state machine applies it only to that submission: a slow answer for
    /// one transcript must not settle the transcript that replaced it.
    pub fn observe(self: &Arc<Self>, event: LinkEvent) {
        match event {
            LinkEvent::ReplayStarted => {}
            LinkEvent::Ready => self.set_connected(true),
            LinkEvent::Disconnected => self.set_connected(false),
            LinkEvent::HandshakeFailed => self.set_connection_failure(crate::link::UPDATE_TOGETHER),
            LinkEvent::State(state, detail) => self.set_assistant(state.into(), detail),
            LinkEvent::Accepted(id) => {
                debug!(id = %id, "submission accepted");
                self.handle(Event::Acknowledged(id))
            }
            LinkEvent::Refused(id, detail) => {
                debug!(id = %id, detail = %detail, "submission refused");
                self.handle(Event::SubmissionFailed { id, reason: detail })
            }
            LinkEvent::Message { .. } => {}
        }
    }

    /// Records whether the companion is speaking an answer.
    ///
    /// Its own doing, not the service's word: the paragraph arrived over the
    /// link, but only the companion knows when the speaker stops.
    pub fn set_speaking(self: &Arc<Self>, speaking: bool) {
        {
            let mut companion = self
                .companion
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            companion.set_speaking(speaking);
        }
        self.publish();
    }

    /// Returns the assistant state the companion is showing, speech included.
    pub fn shown_assistant(&self) -> Assistant {
        self.companion
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .shown_assistant()
    }

    /// Records what the companion shows the assistant is doing.
    pub fn set_assistant(self: &Arc<Self>, state: Assistant, detail: String) {
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

    fn set_connection_failure(self: &Arc<Self>, detail: &str) {
        self.companion
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .set_connection_failure(detail.to_string());
        self.publish();
    }

    /// Stops any live recording. The accepted transcript stays on disk.
    pub fn shutdown(&self) {
        self.tick.fetch_add(1, Ordering::Relaxed);
        // Before anything else that can fail. A grab is the display's state,
        // not the companion's, and the display drops it with the connection
        // that holds it, so this is only what makes that orderly.
        self.set_keys(false);
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
        let event = self.escapes_to(event);
        let (actions, decision) = self.decide(|companion| companion.apply(event));
        if !decision.surfaced.posture.waits() {
            // A phase that is leaving, or one that is only reporting, stays on
            // the surfaces until its actions are done, so the pill the person
            // is looking at is the one they finished with.
            self.carry_out(actions, Some(decision));
            return;
        }
        // A phase the person has to see is on the surfaces before any of its
        // own actions run, and it is on them in fact: this waits for what the
        // windows and the pill actually did, on whichever thread did it.
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

    /// Decides which Escape this is: the one that ends the take, or the one
    /// that ends the take and the companion with it.
    ///
    /// Only in the phases that are holding something to cancel. In the passive
    /// ones Escape is already the dismissal and has nothing else it could mean,
    /// and answering those with a cancel would make the ladder circular - the
    /// take would go, the pill would stay, and the next press would find itself
    /// in the same place.
    ///
    /// The phases cannot decide this themselves. Whether there is a workspace
    /// to go back to is a fact about the widget layer, and the layer is the
    /// host's.
    fn escapes_to(&self, event: Event) -> Event {
        if event != Event::Escape {
            return event;
        }
        if !matches!(self.posture(), Posture::Watched | Posture::Editing) {
            return event;
        }
        if self.ports.surface.holding() {
            Event::Cancel
        } else {
            event
        }
    }

    /// Brings the workspace up, or puts it away.
    ///
    /// One gesture, because the person has one thing in mind: the pill and its
    /// panels, on screen or not. Which of the two this is follows from where
    /// the windows are, and a phase that is holding words answers neither -
    /// [`Event::Dismiss`] refuses those, and they are already on screen so
    /// there is nothing for [`Event::Reveal`] to do.
    pub fn workspace(self: &Arc<Self>) {
        let event = if self.posture() == Posture::Off {
            Event::Reveal
        } else {
            Event::Dismiss
        };
        self.handle(event);
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
        let (tray, tray_detail) = companion.tray_presentation();
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
                tray,
                tray_detail,
            },
            version: self.decisions.fetch_add(1, Ordering::SeqCst) + 1,
        }
    }

    /// Puts the surfaces where one decision wants them, then runs `follow`.
    ///
    /// The person's key and the service's answer change the companion from two
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

    /// Puts the windows where one decision wants them and renders that decision.
    ///
    /// Returns a failure only for the windows: a pill that is not there is a
    /// phase with nowhere to happen, while a surface that refused to render is
    /// one of two, and the other may well have taken it.
    fn put(self: &Arc<Self>, surfaced: &Surfaced) -> Result<(), String> {
        let placed = match surfaced.posture {
            Posture::Editing => self.raise(),
            // Two postures, one arrangement: the pill up and the box away. What
            // separates them is what may run behind it, which the caller has
            // already decided by waiting or not waiting for this.
            Posture::Watched | Posture::Passive => self.settle(),
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
        // The cancel key is safe for as long as the pill is up and belongs to
        // the desktop the moment it is not.
        self.set_keys(surfaced.posture.on_screen());
        // A phase that holds the person's keys keeps them for as long as it is
        // on screen, and nothing outside the runtime can say when they are
        // gone: a person whose keys reach nothing has no key left to ask with.
        if surfaced.posture.textbox() {
            self.watch();
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
                    .tray(surfaced.tray, &surfaced.tray_detail)
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

    /// Puts the pill up and the textbox over it with the keyboard, and records
    /// only what that achieved.
    ///
    /// The pill first, always: it is the indicator, and the box is placed above
    /// wherever it landed. A pill that would not come up stops the raise, so a
    /// box never stands over nothing.
    ///
    /// A record that the box has the keyboard is worth skipping the work for
    /// only while it is still true. The person can click their own window
    /// between one phase and the next, and every phase that reaches here is one
    /// whose keys - Enter, Escape - have nowhere else to go. So the display is
    /// asked, and a box that has lost the keyboard is raised again with a whole
    /// repair chain behind it rather than left on a record written earlier.
    fn raise(&self) -> Result<(), String> {
        self.stand()?;
        if self.screen() == Screen::Ready {
            if self.ports.surface.textbox_has_keyboard() {
                return Ok(());
            }
            // Whatever the record said, the keyboard is not on the box.
            self.set_screen(Screen::Seen);
        }
        match self.ports.surface.show_textbox() {
            Ok(Shown::Ready) => {
                // Ready is about both windows. The box has the keys, but a pill
                // nothing has proved is up leaves the raise short of it, and the
                // repair chain asks for the pill again.
                if self.screen().visible() {
                    self.set_screen(Screen::Ready);
                }
                Ok(())
            }
            // Up without the keys. The person can read the words and cannot
            // answer them, so nothing is recorded as reached and the repair
            // chain asks again.
            Ok(Shown::Seen(trouble)) => {
                if let Some(trouble) = trouble {
                    warn!("{trouble}");
                }
                Ok(())
            }
            // The box is up and may be behind what the person is looking at.
            // The pill under it is unchanged, so its record is left alone.
            Ok(Shown::Doubtful(trouble)) => {
                warn!("{trouble}");
                Ok(())
            }
            Ok(Shown::Unsure(reason)) => {
                debug!("{reason}");
                Ok(())
            }
            // Nothing is recorded. What the windows were doing before is still
            // the best thing known about them, and the next decision is free to
            // try again.
            Err(reason) => Err(reason),
        }
    }

    /// Puts the pill up, and records only what that achieved.
    ///
    /// A pill nothing can see is put up again: the two ways it gets there -
    /// a placement that failed, an always-on-top the window manager refused -
    /// are both retried by asking for the same show, and the microphone rests
    /// on the answer.
    fn stand(&self) -> Result<(), String> {
        if self.screen().visible() {
            return Ok(());
        }
        match self.ports.surface.show_pill() {
            Ok(shown) => {
                self.record(shown);
                Ok(())
            }
            Err(reason) => Err(reason),
        }
    }

    /// Writes down what one pill show proved, and says out loud only what
    /// failed.
    ///
    /// A window operation that came back with an error failed, and is worth a
    /// warning. A window that has simply not got there yet is not: the runtime
    /// asks again, and says so itself if it runs out of asking. An operator
    /// reading this log has to be able to take every warning in it at face
    /// value, or none of them are worth anything.
    fn record(&self, shown: Shown) {
        self.set_screen(match shown {
            // The pill never asks for the keyboard, so nothing it answers with
            // reaches here. Written down all the same rather than left to a
            // panic: an indicator is not worth stopping the process for.
            Shown::Ready => Screen::Ready,
            // The person can see the pill, which is what the privacy indicator
            // needs.
            Shown::Seen(trouble) => {
                if let Some(trouble) = trouble {
                    warn!("{trouble}");
                }
                Screen::Seen
            }
            // Up, but perhaps behind what the person is looking at. The phase
            // still has a window, so it goes on; what it does not have is
            // anything to rest a privacy indicator on.
            Shown::Doubtful(trouble) => {
                warn!("{trouble}");
                Screen::Doubtful
            }
            Shown::Unsure(reason) => {
                debug!("{reason}");
                Screen::Unknown
            }
        });
    }

    /// Leaves the pill on screen with the textbox away, and records only what
    /// that achieved.
    ///
    /// Two phases live here. The microphone's, where the pill has to be seen
    /// before anything opens, and the handoff, where the desktop is the
    /// person's again while the window stays to report the turn. Neither has
    /// anything to type into, so the box goes, and it goes whatever the record
    /// says: a box that came up without being written down is still an
    /// always-on-top window over a phase that is finished.
    fn settle(&self) -> Result<(), String> {
        self.ports.surface.hide_textbox()?;
        if self.screen() == Screen::Ready {
            self.set_screen(Screen::Seen);
        }
        self.stand()
    }

    /// Takes both windows off screen, and records only what that achieved.
    fn lower(&self) -> Result<(), String> {
        if self.screen() == Screen::Off {
            return Ok(());
        }
        // The box first: it stands over the pill and it is the one holding the
        // keyboard, so this is what gives the person's window its keys back.
        // The pill is going away either way, so a box that will not go is said
        // out loud rather than allowed to stop the hide.
        if let Err(reason) = self.ports.surface.hide_textbox() {
            warn!("{reason}");
        }
        match self.ports.surface.hide_pill() {
            Ok(Hidden::Down) => {
                self.set_screen(Screen::Off);
                Ok(())
            }
            Ok(Hidden::Unsure(reason)) => {
                // Asked, and nothing could say it happened. Writing it down as
                // down is exactly what would stop it ever being taken down
                // again, so nothing is claimed and the repair asks again.
                debug!("{reason}");
                self.set_screen(Screen::Unknown);
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

    /// Answers whether the windows are not where the phase needs them.
    fn falls_short(&self, posture: Posture) -> bool {
        match posture {
            // The box up with the keyboard, over a pill that is up.
            Posture::Editing => self.screen() != Screen::Ready,
            // The pill somewhere the person can read it: the microphone rests
            // on exactly that, and so does a failure they have to see.
            Posture::Watched => !self.screen().visible(),
            // Up and reporting is enough. What must not be true is the box
            // still holding the keys the person went back to typing with.
            Posture::Passive => !matches!(self.screen(), Screen::Seen | Screen::Doubtful),
            Posture::Off => self.screen() != Screen::Off,
        }
    }

    /// Asks again for a window that did not reach the state its phase needs.
    ///
    /// Nobody else can. A pill that would not go down is over the desktop and
    /// has no keys of its own, so it cannot even be sent an Escape; a textbox
    /// that would not take the keyboard cannot be typed into. So the runtime
    /// asks again itself, on the newest phase rather than the one that fell
    /// short, and it stops asking: a window manager that has refused three
    /// times is refusing, and the next decision asks again anyway.
    ///
    /// Running out of asking is the moment the shortfall is said out loud, and
    /// the only one. Up to then the runtime is still doing something about it,
    /// and a window that arrives one attempt late would have left a warning
    /// behind for a failure that never happened.
    fn repair(self: &Arc<Self>, left: usize) {
        if left == 0 {
            warn!("{}", self.shortfall());
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

    /// Watches the keyboard for as long as a phase needs it.
    ///
    /// The repair chain covers a box that never got there. This covers a box
    /// that had the keyboard and lost it with nothing to show for it: a window
    /// that maps, takes the keyboard and goes away leaves the window manager
    /// handing focus to the pill, which refuses it, and the keys land on
    /// nothing at all. No decision follows, because a decision needs a key, and
    /// the person's keys are exactly what has gone. So the runtime looks for
    /// itself.
    fn watch(self: &Arc<Self>) {
        if self.watching.swap(true, Ordering::SeqCst) {
            return;
        }
        self.look_again();
    }

    /// Arranges the cancel key, and only when the pill comes or goes.
    ///
    /// A grab is a round trip to the display, so a phase that follows another
    /// on-screen phase pays for nothing. The lock is held across the
    /// arrangement so two threads changing the posture at once cannot leave the
    /// key arranged for the older of the two.
    fn set_keys(&self, on_screen: bool) {
        let mut stood = self
            .keys_stand
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if *stood == on_screen {
            return;
        }
        *stood = on_screen;
        self.ports.keys.stand(on_screen);
    }

    fn look_again(self: &Arc<Self>) {
        let runtime = Arc::clone(self);
        self.ports
            .executor
            .spawn_after(WATCH_INTERVAL, Box::new(move || runtime.look()));
    }

    /// Takes the keyboard back when it is on nothing that can use it.
    ///
    /// Only then. A window the person moved to has the keyboard because they
    /// put it there, and a textbox that fought them for it would be worse than
    /// the hole this closes; the next decision raises the box again, which is
    /// the behaviour that already exists for that case. A keyboard nobody holds
    /// is taken from nobody.
    fn look(self: &Arc<Self>) {
        if !self.posture().textbox() {
            // The phase let the keyboard go, so there is nothing left to watch.
            self.watching.store(false, Ordering::SeqCst);
            // A phase that started needing the keyboard while this tick was
            // deciding to stop found the watch still armed and left it here.
            if self.posture().textbox() {
                self.watch();
            }
            return;
        }
        // A box that has not got there yet belongs to the repair chain, which
        // is already asking for it. Two of them asking would be two raises for
        // one shortfall.
        if self.screen() == Screen::Ready
            && !self.ports.surface.textbox_has_keyboard()
            && self.ports.surface.nobody_has_the_keyboard()
        {
            info!("the keyboard was left on nothing, so the textbox takes it back");
            self.show(self.snapshot(), Box::new(|_| {}));
        }
        self.look_again();
    }

    /// Says what the window never reached, in the words the person's log uses.
    ///
    /// Read at the moment the runtime stops asking, from the newest phase and
    /// the newest record rather than from the attempt that failed: what matters
    /// is where the window has been left, not which try left it there.
    fn shortfall(&self) -> String {
        match (self.posture(), self.screen()) {
            (Posture::Off, _) => "the pill is still up".into(),
            (Posture::Editing, Screen::Seen | Screen::Doubtful) => {
                "the textbox did not take the keyboard".into()
            }
            (Posture::Watched, Screen::Doubtful) => "the pill would not stay on top".into(),
            (Posture::Passive, Screen::Ready) => "the textbox would not go away".into(),
            _ => "the pill did not come up".into(),
        }
    }

    /// Where the newest phase needs the companion's windows to be.
    pub fn posture(&self) -> Posture {
        self.companion
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .posture()
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
            Action::Abort { id } => self.abort(id),
            Action::Submit { id, text } => self.submit(id, text),
            Action::CopyTranscript { text } => self.ports.surface.copy(text),
        }
        Ok(())
    }

    /// Removes the durable transcript, retrying once.
    ///
    /// A removal that keeps failing is logged rather than reopening the pill the
    /// user just finished with. The record left behind is recovered on the next
    /// start under its original identifier, and the identifier is not what
    /// keeps it out of the conversation twice - nothing suppresses by it any
    /// more. [`crate::state::Companion::restore`] is what does: a recovered
    /// record comes back as [`crate::state::Delivery::Uncertain`], which is not
    /// editable and needs two Enters past an explicit warning, so it is resent
    /// only if the person says to.
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
                        // The same thing a second press of the activation key
                        // does: stop the microphone and put the words in the
                        // textbox. Nothing is sent by a timer.
                        runtime.handle(Event::Activate);
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

    fn submit(self: &Arc<Self>, id: String, text: String) {
        // Claimed before anything can go out, so a later attempt has already
        // retired this one's timer whichever of them the executor runs first.
        let generation = self.submissions.fetch_add(1, Ordering::SeqCst) + 1;
        let backend = self
            .backend
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let Some(backend) = backend else {
            self.fail_submission(id, "The Scufris service is unavailable.".into());
            return;
        };
        if let Err(reason) = backend.submit(id.clone(), text) {
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
                // This attempt's, or nobody's. A retry reuses the identifier,
                // so the identifier cannot tell the two timers apart and the
                // older one would settle the newer attempt.
                if runtime.submissions.load(Ordering::SeqCst) != generation {
                    return;
                }
                runtime.handle(Event::SubmissionUncertain {
                    id,
                    reason: "The backend did not confirm delivery.".into(),
                });
            }),
        );
    }

    /// Asks the service to end the run.
    ///
    /// Nothing waits for the answer and nothing is retained. A stop that did not
    /// land leaves the pill saying the assistant is working, which is the truth
    /// and is the whole of what the person needs to know; a stop that did land
    /// is followed by the state going idle on its own. There is nothing to keep
    /// and nothing to send twice, so a failure is a line in the log.
    fn abort(&self, id: String) {
        let backend = self
            .backend
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let Some(backend) = backend else {
            warn!("nothing was stopped: the Scufris service is unavailable");
            return;
        };
        if let Err(reason) = backend.abort(id) {
            warn!("nothing was stopped: {reason}");
        }
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

    /// Opens the conversation in a terminal through the configured hook.
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
        /// True while the textbox is up.
        box_up: AtomicBool,
        /// True while the textbox holds the keyboard.
        focused: AtomicBool,
        /// True while the keyboard is on nothing that can use it, the way a
        /// window that took it and went away leaves the display.
        stranded: AtomicBool,
        /// Window operations in the order the surface received them, whether
        /// they worked or not.
        window: Mutex<Vec<&'static str>>,
        /// When set, the next attempt to show the pill fails the way a broken
        /// window connection would.
        panic_on_show: AtomicBool,
        /// Show attempts that return an error without the pill coming up.
        refuse_show: Refusals,
        /// Show attempts where the pill comes up but nothing proves the person
        /// can see it, the way a refused always-on-top leaves it.
        blind_show: AtomicU64,
        /// Show attempts where the window is asked and nothing can say what
        /// happened, the way a display that has not carried the request out
        /// leaves it.
        unsure_show: AtomicU64,
        /// Hide attempts nothing can speak for, the same way.
        unsure_hide: AtomicU64,
        /// Hide attempts that return an error and leave the pill up.
        refuse_hide: Refusals,
        /// Raises that return an error without the textbox coming up.
        refuse_raise: Refusals,
        /// Raises where the textbox comes up but does not take the keyboard.
        partial_raise: AtomicU64,
        /// Raises nothing can speak for.
        unsure_raise: AtomicU64,
        /// Attempts to take the textbox down that return an error, the way a
        /// window that will not go or a focus that will not go back does.
        refuse_focus: Refusals,
        /// Presentations the pill refuses to render.
        refuse_present: Refusals,
        /// States the tray refuses to take.
        refuse_tray: Refusals,
        /// Runs once as a presentation reaches the pill, so a test can move the
        /// companion on while an older presentation is still on the surface.
        at_present: Mutex<Option<Watcher>>,
        /// Whether the layer is holding a panel, which is what decides between
        /// the two Escapes.
        holding: AtomicBool,
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
                box_up: AtomicBool::new(false),
                focused: AtomicBool::new(false),
                stranded: AtomicBool::new(false),
                window: Mutex::default(),
                panic_on_show: AtomicBool::new(false),
                refuse_show: Refusals::new("the pill did not come up"),
                blind_show: AtomicU64::new(0),
                unsure_show: AtomicU64::new(0),
                unsure_hide: AtomicU64::new(0),
                refuse_hide: Refusals::new("the pill is still up"),
                refuse_raise: Refusals::new("the textbox did not come up"),
                partial_raise: AtomicU64::new(0),
                unsure_raise: AtomicU64::new(0),
                refuse_focus: Refusals::new("the previous window would not take focus"),
                refuse_present: Refusals::new("the pill would not render"),
                refuse_tray: Refusals::new("the tray icon would not change"),
                at_present: Mutex::default(),
                holding: AtomicBool::new(false),
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

        /// Puts a panel on the layer, or takes the last one off.
        fn hold(&self, holding: bool) {
            self.holding.store(holding, Ordering::SeqCst);
        }

        /// True while the textbox holds the keyboard.
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
        fn holding(&self) -> bool {
            self.holding.load(Ordering::SeqCst)
        }

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
            if self.unsure_show.load(Ordering::SeqCst) > 0 {
                self.unsure_show.fetch_sub(1, Ordering::SeqCst);
                // The request reached the window and nothing has carried it out
                // yet, which is what every show looks like before the event
                // loop has run.
                return Ok(Shown::Unsure(
                    "nothing could say whether the pill came up".into(),
                ));
            }
            if self.blind_show.load(Ordering::SeqCst) > 0 {
                self.blind_show.fetch_sub(1, Ordering::SeqCst);
                // The window is up, but it would not stay over whatever the
                // person is looking at, so nothing proves they can see it.
                return Ok(Shown::Doubtful("the pill would not stay on top".into()));
            }
            // Never Ready. This window refuses the keyboard on every show.
            Ok(Shown::Seen(None))
        }
        fn hide_pill(&self) -> Result<Hidden, String> {
            self.window.lock().unwrap().push("hide");
            // The pill stays up when the hide fails, which is the whole reason
            // the runtime must not record it as down.
            self.refuse_hide.attempt()?;
            self.on_screen.store(false, Ordering::SeqCst);
            self.hidden.fetch_add(1, Ordering::Relaxed);
            if self.unsure_hide.load(Ordering::SeqCst) > 0 {
                self.unsure_hide.fetch_sub(1, Ordering::SeqCst);
                return Ok(Hidden::Unsure(
                    "nothing could say whether the pill went down".into(),
                ));
            }
            Ok(Hidden::Down)
        }
        fn show_textbox(&self) -> Result<Shown, String> {
            self.window.lock().unwrap().push("raise");
            self.refuse_raise.attempt()?;
            self.box_up.store(true, Ordering::SeqCst);
            if self.unsure_raise.load(Ordering::SeqCst) > 0 {
                self.unsure_raise.fetch_sub(1, Ordering::SeqCst);
                return Ok(Shown::Unsure(
                    "nothing could say whether the textbox came up".into(),
                ));
            }
            if self.partial_raise.load(Ordering::SeqCst) > 0 {
                self.partial_raise.fetch_sub(1, Ordering::SeqCst);
                return Ok(Shown::Seen(Some(
                    "the textbox could not take the keyboard".into(),
                )));
            }
            self.focused.store(true, Ordering::SeqCst);
            // The box has the keys, so they are on something again.
            self.stranded.store(false, Ordering::SeqCst);
            Ok(Shown::Ready)
        }
        fn hide_textbox(&self) -> Result<(), String> {
            // A box that is already down is left alone, the way the adapter
            // leaves it: the keyboard is not given back to a window that never
            // lost it.
            if !self.box_up.load(Ordering::SeqCst) {
                return Ok(());
            }
            self.window.lock().unwrap().push("drop");
            self.refuse_focus.attempt()?;
            self.box_up.store(false, Ordering::SeqCst);
            self.focused.store(false, Ordering::SeqCst);
            self.restored.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        fn textbox_has_keyboard(&self) -> bool {
            self.focused.load(Ordering::SeqCst)
        }
        fn nobody_has_the_keyboard(&self) -> bool {
            self.stranded.load(Ordering::SeqCst)
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

        /// How many timeouts are still waiting to be fired.
        fn pending_delays(&self) -> usize {
            self.delayed.lock().unwrap().len()
        }

        /// Fires the oldest pending timeout only, then settles its work.
        ///
        /// The delay is discarded here, so two timeouts that were set fifteen
        /// seconds apart in real time are indistinguishable to [`Self::expire`]:
        /// it runs both, and whichever acts first decides. When what is being
        /// tested is which of two timers may act, they have to be fired one at
        /// a time.
        fn expire_oldest(&self) {
            let mut delayed = self.delayed.lock().unwrap();
            if delayed.is_empty() {
                return;
            }
            let task = delayed.remove(0);
            drop(delayed);
            task();
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

    /// Every arrangement of the cancel key, in the order it was asked for.
    #[derive(Default)]
    struct RecordedKeys(Mutex<Vec<bool>>);

    impl Keys for RecordedKeys {
        fn stand(&self, on_screen: bool) {
            self.0.lock().unwrap().push(on_screen);
        }
    }

    impl RecordedKeys {
        fn changes(&self) -> Vec<bool> {
            self.0.lock().unwrap().clone()
        }
    }

    /// Work that runs while one submission is on the wire.
    type Interleave = Box<dyn FnOnce(String) + Send>;

    #[derive(Default)]
    struct RecordingBackend {
        submissions: Mutex<Vec<(String, String)>>,
        aborts: Mutex<Vec<String>>,
        refuse: Mutex<Option<String>>,
        /// Runs once, with the submission on the wire and the handoff not yet
        /// finished. That is exactly where a fast service answer arrives.
        during_submit: Mutex<Option<Interleave>>,
    }

    impl Backend for RecordingBackend {
        fn submit(&self, id: String, text: String) -> Result<(), String> {
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

        fn abort(&self, id: String) -> Result<(), String> {
            if let Some(reason) = self.refuse.lock().unwrap().clone() {
                return Err(reason);
            }
            self.aborts.lock().unwrap().push(id);
            Ok(())
        }
    }

    struct Harness {
        app: Arc<App>,
        surface: Arc<RecordedSurface>,
        keys: Arc<RecordedKeys>,
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
        let keys = Arc::new(RecordedKeys::default());
        let recorder = Arc::new(recorder);
        let executor = Arc::new(QueueExecutor::default());
        let backend = Arc::new(RecordingBackend::default());
        let app = Arc::new(App::new(Ports {
            surface: Arc::clone(&surface) as Arc<dyn Surface>,
            keys: Arc::clone(&keys) as Arc<dyn Keys>,
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
            keys,
            recorder,
            store,
            executor,
            backend,
        }
    }

    /// Records one take and leaves the words in the textbox, which is what the
    /// two presses of the one key do: the first opens the microphone, the
    /// second closes it and puts what was said on screen.
    fn take(harness: &Harness) {
        harness.app.handle(Event::Activate);
        harness.app.handle(Event::Activate);
        harness.executor.drain();
    }

    /// Records one take and sends it unchanged: the whole of an ordinary run.
    fn say(harness: &Harness) {
        take(harness);
        harness.app.handle(Event::Enter { text: None });
        harness.executor.drain();
    }

    /// The stop key is the one gesture on the pill that reaches the
    /// conversation without saying anything to it.
    #[test]
    fn the_stop_key_ends_the_run_and_leaves_the_pill_reporting() {
        let harness = harness(FakeRecorder::default(), Ok("open the tasks widget".into()));
        say(&harness);
        harness.app.observe(LinkEvent::Accepted("pill-1".into()));
        harness.executor.drain();
        harness
            .app
            .set_assistant(Assistant::Working, "packing".into());
        harness.executor.drain();

        harness.app.handle(Event::Stop);
        harness.executor.drain();
        assert_eq!(
            *harness.backend.aborts.lock().unwrap(),
            vec!["pill-2".to_string()],
            "the run was not stopped"
        );
        // The service is what says the run ended, and it has not said so yet.
        // A pill that reported the stop itself would be reporting a hope.
        assert_eq!(harness.surface.last().state, "working");
        assert!(harness.surface.on_screen());
        assert!(!harness.surface.focused());
    }

    /// The key is grabbed for as long as the pill is up, so it is pressed far
    /// more often than there is a run to end.
    #[test]
    fn the_stop_key_says_nothing_to_a_settled_assistant() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.app.handle(Event::Stop);
        harness.executor.drain();
        assert!(harness.backend.aborts.lock().unwrap().is_empty());
        assert!(
            !harness.surface.on_screen(),
            "the stop key raised the pill it is not grabbed for"
        );
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
        say(&harness);

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
    fn a_transcript_survives_a_failed_save_and_says_so() {
        let harness = harness(FakeRecorder::default(), Ok("draft text".into()));
        *harness.store.refuse_save.lock().unwrap() = Some("the disk is full".into());
        take(&harness);

        let presentation = harness.surface.last();
        assert_eq!(presentation.state, "editing");
        assert_eq!(presentation.text, "draft text");
        assert!(presentation.detail.contains("the disk is full"));
        assert!(
            presentation.editable,
            "the words are still the person's to correct"
        );
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
        say(&harness);

        *harness.store.refuse_clear.lock().unwrap() = true;
        harness.app.handle(Event::Acknowledged("pill-1".into()));
        harness.executor.drain();

        // Retried, then left for the next start to recover. It comes back as
        // uncertain and is resent only if the person says to, which is what
        // makes leaving it safe: nothing suppresses by identifier any more.
        // The resident pill stays up, at rest.
        assert_eq!(harness.store.clears.load(Ordering::Relaxed), 2);
        assert_eq!(harness.surface.hidden.load(Ordering::Relaxed), 0);
        assert!(harness.surface.on_screen());
        assert_eq!(harness.surface.last().state, "idle");
    }

    #[test]
    fn an_accepted_transcript_is_persisted_before_it_is_submitted() {
        let harness = harness(FakeRecorder::default(), Ok("remember the milk".into()));
        say(&harness);

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
        // The textbox goes with the handoff, not with the answer, and the
        // keyboard goes back with it: an ordinary submission must not hold the
        // person's keys for a whole turn. The pill stays up, passive, to report
        // the turn it started.
        assert_eq!(harness.surface.window(), ["show", "raise", "drop"]);
        assert!(harness.surface.on_screen());
        assert!(!harness.surface.focused());

        // The acknowledgment retires the durable copy. The resident pill
        // stays up, resting, and focus, already restored at the handoff, is
        // not touched again.
        harness.app.handle(Event::Acknowledged("pill-1".into()));
        assert_eq!(*harness.store.pending.lock().unwrap(), None);
        assert_eq!(harness.surface.window(), ["show", "raise", "drop"]);
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
        say(&first);
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

    /// The startup show is the one show nothing has had a chance to carry out:
    /// the pill is asked for as the companion starts, and the display is asked
    /// about it in the same breath. A runtime that reads that as "the pill did
    /// not come up" abandons the phase, and the phase it abandons is the one
    /// holding words the person cannot get back - which is what happened on
    /// every start with a transcript to recover.
    #[test]
    fn a_restore_nothing_can_confirm_keeps_the_words_on_screen() {
        let store = Arc::new(MemoryStore::default());
        *store.pending.lock().unwrap() = Some(Pending {
            id: "pill-1".into(),
            text: "survives the restart".into(),
        });

        // A pill that genuinely refused to come up is what abandoning is for:
        // words nobody can see are words nobody can correct. This is the answer
        // the startup show used to give, from a window that had simply not been
        // put up yet.
        let refused = harness_with(
            FakeRecorder::default(),
            Ok("unused".into()),
            Arc::clone(&store),
        );
        refused.surface.refuse_show.fail(u64::MAX);
        refused.app.start();
        assert_eq!(refused.surface.last().state, "error");

        let harness = harness_with(
            FakeRecorder::default(),
            Ok("unused".into()),
            Arc::clone(&store),
        );
        harness
            .surface
            .unsure_show
            .store(u64::MAX, Ordering::SeqCst);

        harness.app.start();

        let presentation = harness.surface.last();
        assert_eq!(
            presentation.state, "uncertain",
            "the recovered transcript was abandoned by an unproved show"
        );
        assert_eq!(presentation.text, "survives the restart");
        assert_eq!(
            *store.pending.lock().unwrap(),
            Some(Pending {
                id: "pill-1".into(),
                text: "survives the restart".into(),
            }),
        );
        // Nothing was proved, so nothing is recorded as reached, and the
        // runtime asks again on its own.
        assert!(harness.app.falls_short(Posture::Editing));
    }

    /// Being asked is not being on screen. The microphone rests on the person
    /// being able to read that it is open, and an unproved show proves nothing,
    /// so it stays shut until a show that does.
    #[test]
    fn a_show_nothing_can_confirm_never_opens_the_microphone() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.surface.unsure_show.store(1, Ordering::SeqCst);

        harness.app.handle(Event::Activate);
        harness.executor.drain();
        assert_eq!(
            harness.recorder.captures(),
            0,
            "the microphone opened behind a pill nothing could see"
        );
        let presentation = harness.surface.last();
        assert_eq!(presentation.state, "error");
        assert!(!presentation.recording);
    }

    /// A hide nothing can speak for is not a hide that failed and not a hide
    /// that happened. Writing it down as down is what would stop the pill ever
    /// being taken down again, so the runtime asks once more instead.
    #[test]
    fn a_hide_nothing_can_confirm_is_asked_again_rather_than_written_down() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.app.handle(Event::Activate);
        harness.surface.unsure_hide.store(1, Ordering::SeqCst);

        harness.app.handle(Event::Escape);
        assert_eq!(harness.surface.window(), ["show", "hide"]);

        harness.executor.expire();
        assert_eq!(
            harness.surface.window(),
            ["show", "hide", "hide"],
            "an unproved hide was recorded as done"
        );
        // The second hide was proved, so the chain has nothing left to ask for.
        harness.executor.expire();
        assert_eq!(harness.surface.window(), ["show", "hide", "hide"]);
    }

    #[test]
    fn a_delivered_submission_with_no_acknowledgment_is_retried_under_one_identifier() {
        let harness = harness(FakeRecorder::default(), Ok("open the tasks widget".into()));
        say(&harness);
        // The connection drops after the service accepted the text but before
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
    fn a_service_refusal_keeps_the_words_editable_and_ordinarily_retriable() {
        let harness = harness(FakeRecorder::default(), Ok("open the tasks widget".into()));
        say(&harness);
        assert_eq!(harness.surface.last().state, "sent");

        // What the service answers when it refused the send before any of the
        // words could leave: it says which submission that was.
        harness.app.observe(LinkEvent::Refused(
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

        // M2. Both attempts left an acknowledgement timer behind, and the retry
        // reused the identifier - that is what makes it a retry - so the first
        // attempt's timer matches the second attempt's phase, and the guard at
        // `state.rs` is the identifier alone. Unguarded, the older timer
        // freezes a live submission as uncertain with a forced-send warning it
        // has not earned. In real time it is worse than it reads here: the
        // first timer fires fifteen seconds after the first attempt, which is
        // twelve seconds into the second one, and a retry issued fourteen
        // seconds after the original would get a one-second deadline.
        assert_eq!(harness.surface.last().state, "sent");
        // The live attempt's timer is the last one queued, so nothing may
        // settle this submission before every earlier timer has run. Fired one
        // at a time, because `expire` discards the delay and runs both, and
        // then whichever acts first decides.
        let waiting = harness.executor.pending_delays();
        for fired in 1..waiting {
            harness.executor.expire_oldest();
            assert_eq!(
                harness.surface.last().state,
                "sent",
                "timer {fired} of {waiting} settled a submission that is still running"
            );
        }

        // The guard retires older attempts and nothing else. The live
        // attempt's own timer still has to fire, or a service that stopped
        // answering would leave the pill saying `sent` for good.
        harness.executor.expire();
        assert_eq!(harness.surface.last().state, "uncertain");
    }

    /// An answer can arrive before the handoff has finished running. The
    /// answer is the newer decision, and both of its outcomes need the person,
    /// so the pill it reopens must not then be closed by the handoff that was
    /// already under way.
    ///
    /// The two outcomes reach the runtime by two different roads. A refusal is
    /// the service's word, on the link's thread; an uncertainty is the
    /// companion's own timeout, and nothing on the wire ever says it.
    #[test]
    fn an_answer_that_arrives_during_the_handoff_leaves_the_pill_on_screen() {
        #[derive(Clone, Copy)]
        enum Answer {
            Refused,
            Uncertain,
        }
        for (answers, expected) in [
            (vec![Answer::Refused], "retained"),
            (vec![Answer::Uncertain], "uncertain"),
            // A refusal settles those words, so the uncertainty behind it
            // answers nothing and cannot freeze them.
            (vec![Answer::Refused, Answer::Uncertain], "retained"),
        ] {
            let harness = harness(FakeRecorder::default(), Ok("book the flight".into()));
            let runtime = Arc::clone(&harness.app);
            *harness.backend.during_submit.lock().unwrap() = Some(Box::new(move |_id| {
                // On a thread of its own, and joined here, so the interleaving
                // is real and the test is still deterministic: the answers are
                // fully applied while the submitting thread is stopped at the
                // write.
                thread::spawn(move || {
                    for answer in answers {
                        match answer {
                            Answer::Refused => runtime.observe(LinkEvent::Refused(
                                "pill-1".into(),
                                "submission pill-1 was not sent: the Scufris session is not ready"
                                    .into(),
                            )),
                            Answer::Uncertain => runtime.handle(Event::SubmissionUncertain {
                                id: "pill-1".into(),
                                reason: "The service did not confirm delivery.".into(),
                            }),
                        }
                    }
                })
                .join()
                .expect("the answer thread panicked");
            }));

            say(&harness);

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

    /// A raise can put the textbox up and still not get everything it asked
    /// for. The words are on screen, so the person can read them; the keyboard
    /// is elsewhere, so they cannot answer them and nothing records that they
    /// can.
    #[test]
    fn a_textbox_that_comes_up_without_the_keyboard_is_asked_again() {
        let harness = harness(FakeRecorder::default(), Ok("recovered".into()));
        harness.surface.partial_raise.store(1, Ordering::SeqCst);

        take(&harness);
        assert!(
            harness.surface.on_screen(),
            "the privacy indicator is not up"
        );
        assert!(!harness.surface.focused());
        assert_eq!(harness.surface.window(), ["show", "raise"]);

        // Nobody can type into a box that does not hold the keyboard, so no key
        // of the person's can ask for it again. The runtime asks itself.
        harness.executor.expire();
        assert_eq!(harness.surface.window(), ["show", "raise", "raise"]);
        assert!(
            harness.surface.focused(),
            "the textbox never asked for the keyboard again"
        );
    }

    /// A pill that would not go down is still up, and an always-on-top pill
    /// left over the desktop is the failure this must not record as success.
    ///
    /// Nobody outside the runtime can clear it. The pill has no keys of its
    /// own, so it cannot even be sent an Escape. The runtime takes it down
    /// itself.
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
        assert_eq!(harness.surface.window(), ["show", "hide"]);

        harness.executor.expire();
        assert!(
            !harness.surface.on_screen(),
            "the always-on-top pill was left over the desktop"
        );
        assert_eq!(harness.surface.window(), ["show", "hide", "hide"]);
    }

    /// What the runtime says when it stops asking is the one line about a
    /// window that anybody should have to act on, so it has to name what was
    /// actually left behind rather than what one attempt saw.
    #[test]
    fn the_shortfall_names_what_the_window_never_reached() {
        // A finished turn keeps the pill and gives the keyboard back, so a box
        // still standing over it is what this one has to name.
        let sent = harness(FakeRecorder::default(), Ok("recovered".into()));
        say(&sent);
        sent.app.set_screen(Screen::Ready);
        assert_eq!(sent.app.shortfall(), "the textbox would not go away");

        let harness = harness(FakeRecorder::default(), Ok("recovered".into()));
        harness.app.handle(Event::Activate);

        harness.app.set_screen(Screen::Doubtful);
        assert_eq!(harness.app.shortfall(), "the pill would not stay on top");
        harness.app.set_screen(Screen::Unknown);
        assert_eq!(harness.app.shortfall(), "the pill did not come up");

        // The take ends in the textbox, which is where the keys are.
        harness.app.handle(Event::Activate);
        harness.executor.drain();
        harness.app.set_screen(Screen::Seen);
        assert_eq!(
            harness.app.shortfall(),
            "the textbox did not take the keyboard"
        );

        harness.app.handle(Event::Escape);
        harness.app.set_screen(Screen::Seen);
        assert_eq!(harness.app.shortfall(), "the pill is still up");
    }

    /// A phase whose keys have nowhere else to go asks for the keyboard again.
    ///
    /// The textbox is where that matters: the words are on screen, Enter sends
    /// them and Escape throws them away, and a runtime that read an older
    /// record instead of asking would leave the person pressing both into
    /// whatever window took the keyboard in the meantime.
    #[test]
    fn a_phase_that_needs_the_keys_asks_again_when_the_textbox_has_lost_them() {
        let harness = harness(FakeRecorder::default(), Ok("recovered".into()));
        take(&harness);
        assert_eq!(harness.app.screen(), Screen::Ready);

        // The person clicked their own window. Nothing told the runtime, which
        // is exactly why the record is not the thing to trust.
        harness.surface.focused.store(false, Ordering::SeqCst);
        let before = harness.surface.window().len();

        harness
            .app
            .handle(Event::PersistFailed("the disk is full".into()));

        assert!(
            harness.surface.window()[before..].contains(&"raise"),
            "the words kept a keyboard the textbox no longer had"
        );
        assert_eq!(harness.app.screen(), Screen::Ready);
    }

    /// A window that takes the keyboard and goes away leaves the keys on
    /// nothing at all: the window manager hands focus to the transcript box,
    /// which refuses it by contract, and the display is left with no client
    /// holding anything.
    ///
    /// Nobody outside the runtime can report this. The person is looking at
    /// their own words with Enter and Escape on the table, and every key they
    /// press reaches nothing, so there is no key left to ask for the keyboard
    /// back with. The runtime looks for itself.
    #[test]
    fn a_keyboard_left_on_nothing_is_taken_back_without_a_key() {
        let harness = harness(FakeRecorder::default(), Ok("recovered".into()));
        take(&harness);
        assert_eq!(harness.app.screen(), Screen::Ready);
        assert_eq!(harness.surface.last().state, "editing");

        // Something mapped over the textbox, took the keyboard, and went.
        harness.surface.focused.store(false, Ordering::SeqCst);
        harness.surface.stranded.store(true, Ordering::SeqCst);
        let before = harness.surface.window().len();

        harness.executor.expire();

        assert!(
            harness.surface.window()[before..].contains(&"raise"),
            "the person was left pressing keys into nothing"
        );
        assert!(
            harness.surface.focused(),
            "the textbox never got the keys back"
        );
        assert_eq!(harness.app.screen(), Screen::Ready);
        // Still on the same words: taking the keyboard back is not a phase.
        assert_eq!(harness.surface.last().state, "editing");
    }

    /// The other half of the same rule. A window the person moved to has the
    /// keyboard because they put it there, and a textbox that took it back
    /// would make the desktop unusable for as long as a turn is up. Only a
    /// keyboard nobody holds is taken, because that one is taken from nobody.
    #[test]
    fn a_keyboard_another_window_took_is_left_where_the_person_put_it() {
        let harness = harness(FakeRecorder::default(), Ok("recovered".into()));
        take(&harness);

        // The person clicked their own window. It holds the keyboard, and it
        // can use it.
        harness.surface.focused.store(false, Ordering::SeqCst);
        let before = harness.surface.window().len();

        harness.executor.expire();

        assert_eq!(
            harness.surface.window()[before..],
            [] as [&'static str; 0],
            "the textbox fought the person for the keyboard"
        );
    }

    /// The watch belongs to the phase that needs the keys. A phase that has
    /// given the desktop back must not have the companion looking over its
    /// shoulder, and must not leave a watch running that nothing will ever
    /// stop.
    #[test]
    fn the_watch_stops_when_the_phase_stops_needing_the_keyboard() {
        let harness = harness(FakeRecorder::default(), Ok("recovered".into()));
        take(&harness);
        assert!(harness.app.watching.load(Ordering::SeqCst));

        harness.app.handle(Event::Escape);
        assert_eq!(harness.app.posture(), Posture::Off);
        // The tick that finds the phase finished is the one that stops.
        harness.executor.expire();
        assert!(!harness.app.watching.load(Ordering::SeqCst));

        // A companion whose windows are down is not one to take the keyboard
        // back to, however lost the keys are.
        harness.surface.stranded.store(true, Ordering::SeqCst);
        let before = harness.surface.window().len();
        harness.executor.expire();
        assert_eq!(harness.surface.window().len(), before);

        // The next phase that needs the keys watches them again.
        take(&harness);
        assert!(harness.app.watching.load(Ordering::SeqCst));
    }

    /// The cancel key is grabbed for as long as the pill is up. One arrangement
    /// per arrival: a phase that follows another on-screen phase changes
    /// nothing, because each arrangement costs a round trip to the display.
    #[test]
    fn the_cancel_key_is_arranged_once_for_each_time_the_pill_comes_or_goes() {
        let harness = harness(FakeRecorder::default(), Ok("recovered".into()));
        harness.app.handle(Event::Activate);
        assert_eq!(harness.keys.changes(), [true]);

        // Recording to the textbox is one on-screen phase after another.
        harness.app.handle(Event::Activate);
        harness.executor.drain();
        assert_eq!(
            harness.keys.changes(),
            [true],
            "the key was grabbed twice for one pill"
        );

        harness.app.handle(Event::Escape);
        harness.executor.drain();
        assert_eq!(harness.app.posture(), Posture::Off);
        assert_eq!(harness.keys.changes(), [true, false]);
    }

    /// A pill the person only watches is something they glance at while they
    /// type, and it is told apart from a pill that is gone. The desktop is
    /// theirs again; the pill is still on screen, so the key that puts it away
    /// stays grabbed.
    #[test]
    fn a_pill_the_person_only_watches_is_told_from_one_that_is_gone() {
        let harness = harness(FakeRecorder::default(), Ok("open the tasks widget".into()));
        say(&harness);

        assert_eq!(harness.surface.last().state, "sent");
        assert_eq!(harness.app.posture(), Posture::Passive);
        assert_eq!(
            harness.keys.changes(),
            [true],
            "the cancel key was given back while the pill was still up"
        );
    }

    /// A grabbed accelerator is the display's state and not the companion's, so
    /// a companion that goes away holding one leaves the person's cancel key
    /// reaching nothing at all.
    #[test]
    fn a_companion_that_stops_does_not_take_the_persons_escape_key_with_it() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.app.handle(Event::Activate);
        assert_eq!(harness.keys.changes(), [true]);

        harness.app.shutdown();

        assert_eq!(harness.keys.changes(), [true, false]);
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
        harness.app.handle(Event::Activate);
        assert_eq!(
            harness.recorder.captures(),
            1,
            "the pill was seen, so the recording could start"
        );

        // The pill lost its place, and every show from here leaves it where
        // nothing proves the person can see it. The next thing the companion
        // does is what finds it there.
        harness.app.set_screen(Screen::Unknown);
        harness.surface.blind_show.store(u64::MAX, Ordering::SeqCst);
        harness
            .app
            .set_assistant(Assistant::Working, "packing".into());
        harness.executor.drain();

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
            // while the service's state change is still on the surface.
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
            .set_assistant(Assistant::Working, "packing".into());

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
        harness.app.handle(Event::Activate);
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
                runtime.set_assistant(Assistant::Working, "packing".into());
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
        say(&harness);
        harness.app.observe(LinkEvent::Accepted("pill-1".into()));
        harness.executor.drain();

        // A second recording is under way when the first submission's slow
        // answer finally arrives.
        say(&harness);
        assert_eq!(harness.surface.last().state, "sent");

        harness.app.observe(LinkEvent::Refused(
            "pill-1".into(),
            "the Scufris session is not ready".into(),
        ));
        harness.executor.drain();
        assert_eq!(
            harness.surface.last().state,
            "sent",
            "an answer about a retired submission reopened the one that replaced it"
        );

        harness.app.observe(LinkEvent::Accepted("pill-2".into()));
        harness.executor.drain();
        assert_eq!(harness.surface.last().state, "idle");
    }

    #[test]
    fn an_unreachable_backend_keeps_the_transcript_on_disk() {
        let harness = harness(FakeRecorder::default(), Ok("keep me".into()));
        *harness.backend.refuse.lock().unwrap() = Some("The backend is unavailable.".into());
        say(&harness);

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
        take(&first);
        assert_eq!(first.surface.last().state, "editing");

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
        take(&harness);

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
        say(&harness);
        harness.executor.expire();
        assert_eq!(harness.surface.last().state, "uncertain");

        // The service confirms after the companion gave up waiting.
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
    fn a_discarded_transcript_clears_the_durable_copy() {
        let harness = harness(FakeRecorder::default(), Ok("never mind".into()));
        take(&harness);
        assert_eq!(harness.surface.last().state, "editing");
        assert!(harness.store.pending.lock().unwrap().is_some());

        harness.app.handle(Event::Escape);
        assert_eq!(*harness.store.pending.lock().unwrap(), None);
        assert_eq!(harness.store.clears.load(Ordering::Relaxed), 1);
        assert!(harness.store.tombstones.lock().unwrap().is_empty());
        assert_eq!(harness.surface.hidden.load(Ordering::Relaxed), 1);
    }

    /// The live sequence that locked the keyboard up, at the level the phases
    /// can see it: listen, read the words, send, let the turn run out, then
    /// listen and read again.
    ///
    /// The second take is where the person lost the keyboard, and nothing here
    /// reproduces it. This walk passes before the fix as well as after: the
    /// runtime presents the second take exactly like the first, asks for the
    /// keyboard again, and is told the box holds it. That is the finding. The
    /// defect is one layer down, in what the textbox tells the window manager
    /// when it is mapped a second time, and `textbox::raise` carries the
    /// regression test for it. What this walk protects is that no later change
    /// to the phases quietly makes a second turn present less than the first.
    #[test]
    fn a_second_turn_presents_exactly_like_the_first() {
        let harness = harness(FakeRecorder::default(), Ok("open the tasks widget".into()));

        // Turn one: the activation hotkey, some words, the hotkey again.
        take(&harness);
        let first = harness.surface.last();
        assert_eq!(first.state, "editing");
        assert!(first.editable);
        assert!(harness.surface.focused());

        // Enter sends, the service takes it, and the assistant runs the turn out.
        harness.app.handle(Event::Enter {
            text: Some("open the tasks widget".into()),
        });
        harness.executor.drain();
        assert_eq!(harness.surface.last().state, "sent");
        harness.app.observe(LinkEvent::Accepted("pill-1".into()));
        harness.executor.drain();
        harness.app.set_assistant(Assistant::Working, String::new());
        harness.executor.drain();
        assert_eq!(harness.surface.last().state, "working");
        // Speech is the companion's own, and it shows over whatever the
        // service is reporting underneath.
        harness.app.set_speaking(true);
        harness.executor.drain();
        assert_eq!(harness.surface.last().state, "speaking");
        harness.app.set_assistant(Assistant::Idle, String::new());
        harness.executor.drain();
        assert_eq!(
            harness.surface.last().state,
            "speaking",
            "an idle agent was shown while the companion was still talking"
        );
        harness.app.set_speaking(false);
        harness.executor.drain();
        assert_eq!(harness.surface.last().state, "idle");
        // The pill is resident: it stayed up and gave the keyboard back.
        assert!(harness.surface.on_screen());
        assert!(!harness.surface.focused());

        // Turn two: the same two keys, from idle.
        harness.app.handle(Event::Activate);
        assert_eq!(harness.surface.last().state, "listening");
        harness.app.handle(Event::Activate);
        harness.executor.drain();

        let second = harness.surface.last();
        assert_eq!(second.state, first.state);
        assert_eq!(second.editable, first.editable);
        assert_eq!(second.text, first.text);
        assert!(
            harness.surface.focused(),
            "the second take left the keyboard somewhere else"
        );
    }

    #[test]
    fn a_failed_transcription_persists_nothing() {
        let harness = harness(
            FakeRecorder::default(),
            Err("Speech recognition is unreachable.".into()),
        );
        say(&harness);

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
            assert!(scufris_control::is_identifier(&format!("{prefix}-1")));
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

    /// The gesture the person has in mind is one thing: the workspace, on
    /// screen or not. Which event that is follows from where the windows are.
    #[test]
    fn the_workspace_gesture_brings_the_pill_up_and_puts_it_back_down() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        assert!(!harness.surface.on_screen());

        harness.app.workspace();
        harness.executor.drain();
        assert!(harness.surface.on_screen());
        assert_eq!(
            harness.recorder.captures(),
            0,
            "the workspace gesture opened the microphone"
        );
        assert!(!harness.surface.last().recording);

        harness.app.workspace();
        harness.executor.drain();
        assert!(!harness.surface.on_screen());
    }

    /// A gesture that threw away words on screen would be one nobody dares
    /// make, so a phase holding any answers neither half of it.
    #[test]
    fn the_workspace_gesture_leaves_a_take_and_a_draft_alone() {
        let harness = harness(FakeRecorder::default(), Ok("open the tasks widget".into()));
        harness.app.handle(Event::Activate);
        harness.executor.drain();
        harness.app.workspace();
        harness.executor.drain();
        assert!(harness.surface.last().recording, "the take was thrown away");
        assert!(harness.surface.on_screen());

        take(&harness);
        assert!(harness.surface.focused());
        harness.app.workspace();
        harness.executor.drain();
        assert!(harness.surface.focused(), "the draft was thrown away");
    }

    /// Escape out of a take is a cancel while there is a workspace behind it to
    /// go back to, and the whole dismissal once there is not. The layer is the
    /// host's, so this is the host's decision and not the phase's.
    #[test]
    fn escape_keeps_the_pill_only_while_the_layer_is_holding_something() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.surface.hold(true);
        harness.app.handle(Event::Activate);
        harness.executor.drain();
        harness.app.handle(Event::Escape);
        harness.executor.drain();
        assert!(harness.recorder.was_discarded(0), "the take was kept");
        assert!(
            harness.surface.on_screen(),
            "the panels the take was cancelled over went with it"
        );

        // The ladder's second rung. Nothing is left to cancel, so this Escape
        // is the dismissal whatever the layer is holding.
        harness.app.handle(Event::Escape);
        harness.executor.drain();
        assert!(!harness.surface.on_screen());
    }

    #[test]
    fn escape_takes_an_empty_workspace_down_with_the_take() {
        let harness = harness(FakeRecorder::default(), Ok("unused".into()));
        harness.app.handle(Event::Activate);
        harness.executor.drain();
        harness.app.handle(Event::Escape);
        harness.executor.drain();
        assert!(harness.recorder.was_discarded(0), "the take was kept");
        assert!(
            !harness.surface.on_screen(),
            "a pill over a bare desktop was left standing"
        );
    }
}
