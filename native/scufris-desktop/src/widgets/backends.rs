//! The processes behind a live widget.
//!
//! A widget that shows a number that changes needs something producing the
//! number. That something is a backend: a program that writes one JSON line per
//! reading to its standard output, and reads one JSON line per action on its
//! standard input. The first line it is handed is the payload the open carried.
//!
//! Three rules are what make this safe to run on somebody's desktop all day.
//!
//! - **One process per question.** A backend is found by its identifier and the
//!   payload it was started with, so two widgets asking for the same numbers
//!   share one process and two asking for different ones do not. The last widget
//!   to stop reading is what stops the process.
//! - **Nothing is left behind.** Every backend is its own process group, its
//!   leader's identifier is recorded, and stopping one signals the group rather
//!   than the leader: a backend that started a child of its own does not leave
//!   it running after the panel is gone.
//! - **Nothing is streamed straight to a window.** Readings are coalesced,
//!   latest wins, and handed over on a fixed beat. A backend that writes faster
//!   than the screen refreshes is a backend whose extra lines are dropped rather
//!   than queued - WebKitGTK will hold every one of them if it is given the
//!   chance.
//!
//! A backend that goes quiet or dies says so. A frozen number that looks live is
//! the one outcome worse than an empty panel.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{BufRead, BufReader, Write},
    os::unix::process::CommandExt,
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::Duration,
};

use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use serde_json::Value;
use tracing::{debug, warn};

use crate::widgets::runtime::{Health, SurfaceId};

// The backends `build.rs` compiled into this binary. Generated rather than
// walked at startup, for the reason the widget table is: what ships is what was
// built.
include!(concat!(env!("OUT_DIR"), "/backends.rs"));

/// How many of a widget's own cadences of silence make a backend stale.
///
/// Three, because one missed reading is a busy machine and three in a row is
/// something wrong. The marker is a hint rather than a verdict: the process is
/// still running, and the next line clears it.
const SILENCE: u32 = 3;

/// How long a stopping backend has to go on its own terms.
const GOODBYE: Duration = Duration::from_secs(3);

/// How often the wait for a stopping backend looks again.
const GOODBYE_STEP: Duration = Duration::from_millis(100);

/// What one running backend is found by: which backend, and the payload it was
/// started with.
pub type Key = (String, String);

/// One backend as `build.rs` compiled it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backend<'a> {
    /// The directory name, which is the backend's identifier.
    pub id: &'a str,
    /// The program text.
    pub script: &'a str,
}

/// Returns one installed backend.
pub fn installed(id: &str) -> Option<Backend<'static>> {
    INSTALLED.iter().copied().find(|backend| backend.id == id)
}

/// Every installed backend's identifier.
pub fn names() -> Vec<&'static str> {
    INSTALLED.iter().map(|backend| backend.id).collect()
}

/// Everything one surface's subscription is made of.
#[derive(Debug, Clone, Copy)]
pub struct Order<'a> {
    /// Which backend.
    pub backend: Backend<'a>,
    /// The payload it is started with.
    pub spawn: &'a Value,
    /// How often the widget expects a reading.
    pub cadence: Duration,
    /// True while this widget is content to share a process with another one
    /// asking the same question.
    pub shared: bool,
}

impl Order<'_> {
    /// What the running process is found by.
    ///
    /// The backend and the payload, so two panels asking for the same numbers
    /// meet on one process. A widget that says it does not share puts its own
    /// surface in as well, which is what keeps two timers of the same length
    /// from becoming one timer counted twice.
    fn key(&self, surface: &str) -> Key {
        let question = canonical(self.spawn);
        let question = if self.shared {
            question
        } else {
            format!("{surface}\u{1f}{question}")
        };
        (self.backend.id.to_string(), question)
    }
}

/// One thing a backend has to say, on the beat the coalescer hands it over.
#[derive(Debug, Clone, PartialEq)]
pub enum News {
    /// The latest reading for one surface.
    Data {
        /// The surface reading this backend.
        surface: SurfaceId,
        /// What the backend wrote.
        data: Value,
    },
    /// A backend's health changed for one surface.
    Health {
        /// The surface reading this backend.
        surface: SurfaceId,
        /// What it now is.
        health: Health,
    },
}

/// One backend process.
struct Running {
    /// The leader's identifier, which is also its process group's. Recorded at
    /// the spawn so stopping it never has to look a process up by name.
    pid: u32,
    child: Child,
    /// The pipe its payload went down, held open afterwards. A backend reading
    /// its input reads the end of it as the word to stop, and dropping this is
    /// how that word is said.
    stdin: Option<ChildStdin>,
    /// Which surfaces are reading it. The last one to leave stops it.
    refs: BTreeSet<SurfaceId>,
    /// How long since it last wrote a line, counting only measured time.
    quiet: Duration,
    /// How often the widget says a line should arrive.
    cadence: Duration,
    /// What was last said about it.
    health: Health,
}

#[derive(Default)]
struct State {
    running: HashMap<Key, Running>,
    /// Which backend each subscribed surface reads from. Outlives the process:
    /// a backend that died is still the backend its panel restarts.
    readers: HashMap<SurfaceId, Key>,
    /// The latest reading per surface, waiting for the next beat. Sorted, so
    /// one beat hands its surfaces over in the same order every time.
    data: BTreeMap<SurfaceId, Value>,
    /// Health changes waiting for the next beat.
    health: Vec<(SurfaceId, Health)>,
}

/// How a backend process is started.
///
/// A field rather than a call, so the supervisor can be tested without the
/// interpreter the shipped backends happen to be written in.
type Launch = Box<dyn Fn(Backend<'_>) -> std::io::Result<Child> + Send + Sync>;

/// Every running backend, and who is reading it.
pub struct Backends {
    state: Arc<Mutex<State>>,
    launch: Launch,
}

impl Default for Backends {
    fn default() -> Self {
        Self::new()
    }
}

impl Backends {
    /// Returns a supervisor that starts the backends as they ship.
    pub fn new() -> Self {
        Self::with_launch(Box::new(python))
    }

    /// Returns a supervisor that starts backends some other way.
    pub fn with_launch(launch: Launch) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            launch,
        }
    }

    /// Starts reading one backend for one surface, sharing a process that is
    /// already answering the same question.
    pub fn subscribe(&self, surface: SurfaceId, order: Order<'_>) {
        let key = order.key(&surface);
        let mut held = self.state();
        held.readers.insert(surface.clone(), key.clone());
        if let Some(running) = held.running.get_mut(&key) {
            running.refs.insert(surface);
            return;
        }
        let refs = BTreeSet::from([surface]);
        self.raise(&mut held, key, refs, order);
    }

    /// Stops reading for one surface, and stops the process if it was the last.
    pub fn unsubscribe(&self, surface: &str) {
        let mut held = self.state();
        let Some(key) = held.readers.remove(surface) else {
            return;
        };
        held.data.remove(surface);
        let Some(running) = held.running.get_mut(&key) else {
            return;
        };
        running.refs.remove(surface);
        if running.refs.is_empty()
            && let Some(running) = held.running.remove(&key)
        {
            stop(running);
        }
    }

    /// Starts one backend over, for every surface that was reading it.
    ///
    /// The restart tick is on one panel, but the process behind it may be
    /// answering several. Bringing it back for the one that asked and leaving
    /// the others on a dead one would be the worst of both.
    pub fn restart(&self, surface: SurfaceId, order: Order<'_>) {
        let key = order.key(&surface);
        let mut held = self.state();
        if let Some(running) = held.running.remove(&key) {
            stop(running);
        }
        held.readers.insert(surface, key.clone());
        let refs: BTreeSet<SurfaceId> = held
            .readers
            .iter()
            .filter(|(_, reading)| *reading == &key)
            .map(|(id, _)| id.clone())
            .collect();
        self.raise(&mut held, key, refs, order);
    }

    /// Writes one action onto the backend a surface is reading.
    ///
    /// The mirror of a reading, and the same line format going the other way.
    /// A backend that owns something writable answers by reporting the new
    /// state, which fans out to every panel reading it - so an entry the person
    /// makes and one Scufris makes travel the same loop.
    pub fn send(&self, surface: &str, action: &Value) {
        let mut held = self.state();
        let Some(key) = held.readers.get(surface).cloned() else {
            return;
        };
        let Some(running) = held.running.get_mut(&key) else {
            return;
        };
        let Some(stdin) = running.stdin.as_mut() else {
            return;
        };
        if let Err(error) = writeln!(stdin, "{action}") {
            debug!(surface, "an action did not reach its backend: {error}");
        }
    }

    /// Stops every backend. The companion is going away.
    pub fn halt(&self) {
        let mut held = self.state();
        held.readers.clear();
        let running: Vec<Running> = held.running.drain().map(|(_, running)| running).collect();
        drop(held);
        for one in running {
            stop(one);
        }
    }

    /// Hands over everything the backends said since the last beat.
    ///
    /// `elapsed` is measured by the caller rather than read from a clock here,
    /// for the reason the aging sweep is: a machine that was asleep did not
    /// spend one beat per wake-up.
    pub fn drain(&self, elapsed: Duration) -> Vec<News> {
        let mut held = self.state();
        let mut news = Vec::new();
        let mut buried = Vec::new();
        for (key, running) in held.running.iter_mut() {
            if matches!(running.child.try_wait(), Ok(Some(_)) | Err(_)) {
                // A process that exited leaves a number that will never change
                // again. The panel says so rather than keeping it up as though
                // it were current.
                if running.health != Health::Dead {
                    for surface in &running.refs {
                        news.push(News::Health {
                            surface: surface.clone(),
                            health: Health::Dead,
                        });
                    }
                }
                buried.push(key.clone());
                continue;
            }
            running.quiet = running.quiet.saturating_add(elapsed);
            let overdue = running.cadence.saturating_mul(SILENCE);
            if running.health == Health::Fresh && running.quiet > overdue {
                running.health = Health::Stale;
                for surface in &running.refs {
                    news.push(News::Health {
                        surface: surface.clone(),
                        health: Health::Stale,
                    });
                }
            }
        }
        for key in buried {
            // The readers stay: a dead backend is still the backend its panels
            // restart, and the key is how the restart finds them all.
            held.running.remove(&key);
        }
        news.extend(
            held.health
                .drain(..)
                .map(|(surface, health)| News::Health { surface, health }),
        );
        let data = std::mem::take(&mut held.data);
        news.extend(
            data.into_iter()
                .map(|(surface, data)| News::Data { surface, data }),
        );
        news
    }

    fn raise(&self, held: &mut State, key: Key, refs: BTreeSet<SurfaceId>, order: Order<'_>) {
        let Order {
            backend,
            spawn,
            cadence,
            ..
        } = order;
        let mut child = match (self.launch)(backend) {
            Ok(child) => child,
            Err(error) => {
                warn!(
                    backend = backend.id,
                    "a widget backend would not start: {error}"
                );
                for surface in refs {
                    held.health.push((surface, Health::Dead));
                }
                return;
            }
        };
        let pid = child.id();
        let mut stdin = child.stdin.take();
        if let Some(pipe) = stdin.as_mut()
            && let Err(error) = writeln!(pipe, "{spawn}")
        {
            warn!(
                backend = backend.id,
                "a widget backend would not take its payload: {error}"
            );
        }
        if let Some(stdout) = child.stdout.take() {
            let state = Arc::clone(&self.state);
            let id = backend.id.to_string();
            let reading = key.clone();
            thread::spawn(move || read(&id, &reading, stdout, &state));
        }
        if let Some(stderr) = child.stderr.take() {
            let id = backend.id.to_string();
            thread::spawn(move || complain(&id, stderr));
        }
        held.running.insert(
            key,
            Running {
                pid,
                child,
                stdin,
                refs,
                quiet: Duration::ZERO,
                cadence,
                health: Health::Fresh,
            },
        );
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
}

impl Drop for Backends {
    fn drop(&mut self) {
        self.halt();
    }
}

/// Reads one backend's lines until it stops writing them.
fn read(id: &str, key: &Key, stdout: ChildStdout, state: &Arc<Mutex<State>>) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { return };
        if line.trim().is_empty() {
            continue;
        }
        let data: Value = match serde_json::from_str(&line) {
            Ok(data) => data,
            Err(error) => {
                // One bad line is not a reason to take the panel down. The
                // backend is still running and the next line may be fine.
                warn!(
                    backend = id,
                    "a widget backend wrote a line that is not JSON: {error}"
                );
                continue;
            }
        };
        let mut held = state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(running) = held.running.get_mut(key) else {
            return;
        };
        running.quiet = Duration::ZERO;
        let revived = running.health != Health::Fresh;
        running.health = Health::Fresh;
        let refs: Vec<SurfaceId> = running.refs.iter().cloned().collect();
        for surface in refs {
            if revived {
                held.health.push((surface.clone(), Health::Fresh));
            }
            // Latest wins. A backend that writes faster than the screen
            // refreshes loses the readings nobody would have seen anyway.
            held.data.insert(surface, data.clone());
        }
    }
}

/// Puts one backend's complaints in the journal under its own name.
fn complain(id: &str, stderr: ChildStderr) {
    for line in BufReader::new(stderr).lines() {
        let Ok(line) = line else { return };
        if !line.trim().is_empty() {
            warn!(backend = id, "{line}");
        }
    }
}

/// Stops one backend, and everything it started.
///
/// Closing its input is the polite word: a backend reading lines sees the end
/// of them. The group is signalled after it, and signalled again with no way to
/// refuse once its three seconds are up. The second signal goes out before the
/// leader is reaped rather than after, because a reaped identifier is one the
/// kernel is free to hand to somebody else.
fn stop(mut running: Running) {
    drop(running.stdin.take());
    let pid = running.pid;
    group(pid, Signal::SIGTERM);
    thread::spawn(move || {
        let mut waited = Duration::ZERO;
        while waited < GOODBYE {
            match running.child.try_wait() {
                Ok(None) => {
                    thread::sleep(GOODBYE_STEP);
                    waited = waited.saturating_add(GOODBYE_STEP);
                }
                _ => break,
            }
        }
        group(pid, Signal::SIGKILL);
        let _ = running.child.wait();
    });
}

/// Signals one process group by its leader's recorded identifier.
///
/// The group rather than the leader, and by the identifier the spawn recorded
/// rather than by anything matched on a name. The leader was put in a group of
/// its own, so this reaches the backend and every child it started, and reaches
/// nothing else.
fn group(pid: u32, signal: Signal) {
    let Ok(leader) = i32::try_from(pid) else {
        return;
    };
    if let Err(error) = killpg(Pid::from_raw(leader), signal) {
        debug!(pid, %signal, "a widget backend was already gone: {error}");
    }
}

/// Starts one backend the way the shipped ones are written.
fn python(backend: Backend<'_>) -> std::io::Result<Child> {
    let mut command = Command::new("python3");
    command
        .arg("-c")
        .arg(backend.script)
        // Named, so the backend is recognizable in a process listing rather
        // than being one more anonymous interpreter.
        .arg(backend.id)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    command.spawn()
}

/// The spawn payload as one string, with object keys in a fixed order.
///
/// The other half of the key a running backend is found by. Two widgets that
/// ask the same question share one process, and asking it with the fields
/// written in another order is still the same question.
fn canonical(value: &Value) -> String {
    match value {
        Value::Object(fields) => {
            let mut sorted: Vec<(&String, &Value)> = fields.iter().collect();
            sorted.sort_by(|left, right| left.0.cmp(right.0));
            let body: Vec<String> = sorted
                .into_iter()
                .map(|(name, field)| {
                    format!("{}:{}", Value::String(name.clone()), canonical(field))
                })
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Value::Array(items) => {
            let body: Vec<String> = items.iter().map(canonical).collect();
            format!("[{}]", body.join(","))
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CADENCE: Duration = Duration::from_millis(100);

    /// Starts a backend whose "script" is a shell program, so the supervisor
    /// can be exercised without depending on what interpreters a test machine
    /// happens to carry.
    fn shell() -> Launch {
        Box::new(|backend: Backend<'_>| {
            let mut command = Command::new("/bin/sh");
            command
                .arg("-c")
                .arg(backend.script)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            command.process_group(0);
            command.spawn()
        })
    }

    /// Waits for one drain to say something, or gives up.
    ///
    /// Drained with no time on it, so waiting for a process to write its first
    /// line does not itself age the process into silence.
    fn hear(backends: &Backends) -> Vec<News> {
        for _ in 0..100 {
            let news = backends.drain(Duration::ZERO);
            if !news.is_empty() {
                return news;
            }
            thread::sleep(Duration::from_millis(20));
        }
        Vec::new()
    }

    fn source(script: &str) -> Backend<'_> {
        Backend {
            id: "probe",
            script,
        }
    }

    /// One subscription, with sharing allowed and the usual cadence.
    fn order<'a>(backend: Backend<'a>, spawn: &'a Value) -> Order<'a> {
        Order {
            backend,
            spawn,
            cadence: CADENCE,
            shared: true,
        }
    }

    #[test]
    fn a_line_a_backend_writes_reaches_the_surface_reading_it() {
        let backends = Backends::with_launch(shell());
        backends.subscribe(
            "widget-1".into(),
            order(source("echo '{\"load\":3}'; sleep 30"), &Value::Null),
        );
        assert_eq!(
            hear(&backends),
            vec![News::Data {
                surface: "widget-1".into(),
                data: serde_json::json!({ "load": 3 }),
            }]
        );
    }

    #[test]
    fn two_widgets_asking_the_same_question_share_one_process() {
        // The whole point of keying on the payload. Two CPU graphs on screen
        // are two panels reading one sampler, not two samplers.
        let backends = Backends::with_launch(shell());
        let script = source("echo '{\"load\":1}'; sleep 30");
        let spawn = serde_json::json!({ "every": 1, "what": "cpu" });
        backends.subscribe("widget-1".into(), order(script, &spawn));
        // The same question, written the other way round.
        let reordered = serde_json::json!({ "what": "cpu", "every": 1 });
        backends.subscribe("widget-2".into(), order(script, &reordered));
        assert_eq!(backends.state().running.len(), 1);
        // And both panels are handed the reading.
        let news = hear(&backends);
        assert!(news.contains(&News::Data {
            surface: "widget-1".into(),
            data: serde_json::json!({ "load": 1 }),
        }));
        assert!(news.contains(&News::Data {
            surface: "widget-2".into(),
            data: serde_json::json!({ "load": 1 }),
        }));
    }

    #[test]
    fn a_different_question_gets_a_process_of_its_own() {
        let backends = Backends::with_launch(shell());
        let script = source("sleep 30");
        backends.subscribe(
            "widget-1".into(),
            order(script, &serde_json::json!({ "what": "cpu" })),
        );
        backends.subscribe(
            "widget-2".into(),
            order(script, &serde_json::json!({ "what": "memory" })),
        );
        assert_eq!(backends.state().running.len(), 2);
    }

    #[test]
    fn the_last_reader_leaving_is_what_stops_the_process() {
        let backends = Backends::with_launch(shell());
        let script = source("sleep 30");
        backends.subscribe("widget-1".into(), order(script, &Value::Null));
        backends.subscribe("widget-2".into(), order(script, &Value::Null));
        backends.unsubscribe("widget-1");
        assert_eq!(
            backends.state().running.len(),
            1,
            "a panel closing took the other panel's numbers with it"
        );
        backends.unsubscribe("widget-2");
        assert!(backends.state().running.is_empty());
    }

    #[test]
    fn only_the_latest_reading_survives_the_wait_for_the_next_beat() {
        // A backend that writes faster than the screen refreshes is the
        // documented way to make a webview hold gigabytes. What it writes in
        // between beats is dropped rather than queued.
        let backends = Backends::with_launch(shell());
        backends.subscribe(
            "widget-1".into(),
            order(
                source("echo '{\"n\":1}'; echo '{\"n\":2}'; echo '{\"n\":3}'; sleep 30"),
                &Value::Null,
            ),
        );
        thread::sleep(Duration::from_millis(200));
        assert_eq!(
            hear(&backends),
            vec![News::Data {
                surface: "widget-1".into(),
                data: serde_json::json!({ "n": 3 }),
            }],
            "a beat handed over more than the latest reading"
        );
    }

    #[test]
    fn a_backend_that_exits_says_so_rather_than_leaving_its_last_number_up() {
        let backends = Backends::with_launch(shell());
        backends.subscribe("widget-1".into(), order(source("exit 1"), &Value::Null));
        let mut seen = Vec::new();
        for _ in 0..100 {
            seen.extend(backends.drain(Duration::ZERO));
            if seen.contains(&News::Health {
                surface: "widget-1".into(),
                health: Health::Dead,
            }) {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("a dead backend never reported itself: {seen:?}");
    }

    #[test]
    fn a_backend_that_goes_quiet_is_marked_before_it_is_buried() {
        // Still running, just not writing. The panel says the number is old,
        // which is a different thing from saying the panel is broken.
        let backends = Backends::with_launch(shell());
        backends.subscribe("widget-1".into(), order(source("sleep 30"), &Value::Null));
        // Three cadences of measured silence, handed over in one beat.
        let news = backends.drain(CADENCE * SILENCE + Duration::from_millis(1));
        assert_eq!(
            news,
            vec![News::Health {
                surface: "widget-1".into(),
                health: Health::Stale,
            }]
        );
        // And it is said once, not on every beat that follows.
        assert!(backends.drain(CADENCE * SILENCE).is_empty());
    }

    #[test]
    fn a_backend_that_will_not_start_is_a_dead_panel_rather_than_a_blank_one() {
        let backends = Backends::with_launch(Box::new(|_| {
            Err(std::io::Error::from(std::io::ErrorKind::NotFound))
        }));
        backends.subscribe("widget-1".into(), order(source("whatever"), &Value::Null));
        assert_eq!(
            backends.drain(Duration::ZERO),
            vec![News::Health {
                surface: "widget-1".into(),
                health: Health::Dead,
            }]
        );
    }

    #[test]
    fn restarting_brings_back_every_panel_that_was_reading_it() {
        // The tick is on one panel and the process is shared. A restart that
        // only revived the panel that asked would leave the others watching a
        // number that has stopped.
        let backends = Backends::with_launch(shell());
        let script = source("echo '{\"n\":1}'; sleep 30");
        backends.subscribe("widget-1".into(), order(script, &Value::Null));
        backends.subscribe("widget-2".into(), order(script, &Value::Null));
        let first = backends.state().running.values().next().map(|one| one.pid);
        backends.restart("widget-1".into(), order(script, &Value::Null));
        let state = backends.state();
        assert_eq!(state.running.len(), 1);
        let running = state.running.values().next().expect("one backend runs");
        assert_ne!(Some(running.pid), first, "the same process came back");
        assert_eq!(
            running.refs,
            BTreeSet::from(["widget-1".to_string(), "widget-2".to_string()]),
        );
    }

    #[test]
    fn a_reading_after_a_silence_clears_the_marker() {
        let backends = Backends::with_launch(shell());
        backends.subscribe(
            "widget-1".into(),
            order(
                source("sleep 0.4; echo '{\"n\":1}'; sleep 30"),
                &Value::Null,
            ),
        );
        let quiet = backends.drain(CADENCE * SILENCE + Duration::from_millis(1));
        assert_eq!(
            quiet,
            vec![News::Health {
                surface: "widget-1".into(),
                health: Health::Stale,
            }]
        );
        let mut seen = Vec::new();
        for _ in 0..100 {
            seen.extend(backends.drain(Duration::ZERO));
            if seen.contains(&News::Health {
                surface: "widget-1".into(),
                health: Health::Fresh,
            }) {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("a backend that started writing again never said so: {seen:?}");
    }

    #[test]
    fn a_widget_that_says_it_does_not_share_gets_a_process_of_its_own() {
        // Two timers of the same length are two timers, not one counted twice.
        let backends = Backends::with_launch(shell());
        let alone = |backend| Order {
            backend,
            spawn: &Value::Null,
            cadence: CADENCE,
            shared: false,
        };
        backends.subscribe("widget-1".into(), alone(source("sleep 30")));
        backends.subscribe("widget-2".into(), alone(source("sleep 30")));
        assert_eq!(backends.state().running.len(), 2);
    }

    #[test]
    fn an_action_a_widget_sends_reaches_its_backend() {
        // The mirror of a reading. A backend that owns something writable
        // answers by reporting the new state, so an entry the person makes and
        // one Scufris makes travel the same loop.
        let backends = Backends::with_launch(shell());
        backends.subscribe(
            "widget-1".into(),
            order(
                source("while read -r line; do echo \"$line\"; done"),
                &Value::Null,
            ),
        );
        backends.send("widget-1", &serde_json::json!({ "add": "milk" }));
        assert_eq!(
            hear(&backends),
            vec![News::Data {
                surface: "widget-1".into(),
                data: serde_json::json!({ "add": "milk" }),
            }]
        );
    }

    #[test]
    fn an_action_for_a_panel_with_no_backend_goes_nowhere_rather_than_anywhere() {
        let backends = Backends::with_launch(shell());
        backends.send("widget-9", &serde_json::json!({ "add": "milk" }));
        assert!(backends.drain(Duration::ZERO).is_empty());
    }

    #[test]
    fn the_key_does_not_depend_on_how_the_payload_was_written() {
        assert_eq!(
            canonical(&serde_json::json!({ "b": 1, "a": [2, { "d": 3, "c": 4 }] })),
            canonical(&serde_json::json!({ "a": [2, { "c": 4, "d": 3 }], "b": 1 })),
        );
        assert_ne!(
            canonical(&serde_json::json!({ "a": 1 })),
            canonical(&serde_json::json!({ "a": "1" })),
        );
    }

    #[test]
    fn every_backend_shipped_with_the_companion_is_named() {
        // The generated table is what `build.rs` walked. A widget's manifest
        // names one of these, and the catalog refuses a name that is not here.
        assert!(!names().is_empty(), "no backend is shipped");
        assert!(
            installed("system").is_some(),
            "the system backend is missing"
        );
        assert!(installed("nothing-like-this").is_none());
    }
}
