//! Warm shell windows, so opening a widget is a message rather than a build.
//!
//! Building a webview window and waiting for its page to load takes long enough
//! to be seen. A widget arrives in the middle of a sentence, so that wait would
//! land in the middle of the sentence too. The pool keeps two windows built,
//! loaded, and hidden; opening a widget takes one of them and sends it a single
//! message.
//!
//! A shell is used once. Its label is the surface identifier the service is
//! answered with, and a label handed out twice would let an update meant for a
//! widget that is gone land on whatever took its place. So a retired shell is
//! destroyed rather than re-adopted, and the pool builds its replacement in the
//! background, off the path a person is waiting on.

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, ipc::Channel};
use tracing::{debug, warn};

use crate::widgets::{
    runtime::{Health, Life},
    windows,
};

/// How many shells stay built and loaded.
///
/// Two: one for the widget that is about to open, and one for the widget that
/// follows it in the same answer. A third would be a window nothing has ever
/// needed at once.
pub const WARM_SHELLS: usize = 2;

/// How long taking a shell waits for one to finish loading.
///
/// Only ever spent when the pool is empty - at the very first widget of a
/// session, or after several opened at once. The agent waits five seconds for
/// its answer, so this leaves room for the answer to travel.
const WARM_WAIT: Duration = Duration::from_secs(3);

/// How long a shell that is still loading holds its place in the pool.
///
/// Nothing else ever gives that place back. A page that dies on its way up - a
/// module that throws, a webview that never paints - would otherwise be counted
/// as a shell forever, and two of them would leave the pool unable to build
/// anything again for the life of the process. Generous, because the cost of
/// waiting too long is one late window and the cost of not waiting long enough
/// is one extra.
const LOAD_PATIENCE: Duration = Duration::from_secs(10);

/// One message to one shell window.
///
/// The whole host-to-page contract. It travels on a `tauri::ipc::Channel`,
/// which keeps the order the messages were sent in - an update that overtook
/// the `become` carrying its widget would render into an empty page.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShellMsg {
    /// Load a widget into this shell and draw its chrome.
    Become {
        /// The surface identifier, which is also this window's label.
        surface: String,
        /// Widget to load.
        widget: String,
        /// Title the chrome prints.
        name: String,
        /// Widget-defined spawn payload.
        data: Value,
    },
    /// Hand new data to the widget already loaded.
    Update {
        /// Widget-defined payload.
        data: Value,
    },
    /// Change what the chrome says about this surface.
    Life {
        /// What the chrome now says.
        state: Life,
    },
    /// Change what the chrome says about the backend behind this surface.
    ///
    /// The frame carries it rather than the widget, for the reason the life
    /// state is carried there: a widget drawing its own "my numbers stopped"
    /// notice is a widget that has to be written to draw one, and the one that
    /// forgets is the one whose panel lies.
    Health {
        /// What it now is.
        state: Health,
    },
    /// Say that a tick the person used could not be carried out.
    ///
    /// A tick that silently does nothing reads as a tick that is broken. This
    /// is the chrome's only way to say otherwise, and the badge is where it
    /// says it.
    Refused {
        /// What the person reads.
        detail: String,
    },
    /// Unmount the widget. The window closes right behind this.
    Retire,
}

#[derive(Default)]
struct PoolState {
    /// Shells built, loaded, and holding no widget.
    idle: VecDeque<String>,
    /// The channel every built shell reported.
    channels: HashMap<String, Channel<ShellMsg>>,
    /// Shells built and still loading, with the moment each was built.
    ///
    /// Timed, because nothing else ever takes an entry out. A page that never
    /// loads is not distinguishable from one that is about to, so the pool
    /// stops counting on it after [`LOAD_PATIENCE`] rather than holding its
    /// place forever.
    loading: HashMap<String, Instant>,
    /// The last label minted. Labels only ever go up.
    minted: u32,
    /// What the display knows each shown shell by.
    ///
    /// One remembered name per label, rather than one for the pool: there are
    /// many widget windows, and a single name would answer for whichever of
    /// them was shown first. Filled at the show, because that is when a window
    /// has a name worth remembering, and read by the focus tracker - a shell is
    /// built unfocusable, so it is never somewhere to give the desktop back to.
    named: HashMap<String, Arc<AtomicU32>>,
}

/// The warm shells.
pub struct Pool {
    app: AppHandle,
    /// What this run of the companion stamps into every label it mints.
    ///
    /// The counter alone starts at one with the process, so a companion that
    /// restarts hands out the identifiers the last one already gave the service.
    /// An update written for a panel that is gone would then land on whatever
    /// took its place, which is the one thing this module promises cannot
    /// happen.
    run: String,
    state: Mutex<PoolState>,
    loaded: Condvar,
}

impl Pool {
    /// Returns an empty pool. Nothing is built until [`Pool::warm`].
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            run: stamp(),
            state: Mutex::new(PoolState::default()),
            loaded: Condvar::new(),
        }
    }

    /// Builds shells until the pool is full again.
    ///
    /// Every window is built hidden and reports itself when its page loads, so
    /// this returns before any of them is usable. That is the point: the wait
    /// belongs here, where nobody is watching, rather than at the open.
    pub fn warm(&self) {
        let now = Instant::now();
        let wanted: Vec<String> = {
            let mut state = self.lock();
            state.loading.retain(|label, built| {
                let loading = now.duration_since(*built) < LOAD_PATIENCE;
                if !loading {
                    warn!(label, "a widget shell never finished loading");
                }
                loading
            });
            let short = WARM_SHELLS.saturating_sub(state.idle.len() + state.loading.len());
            (0..short)
                .map(|_| {
                    state.minted += 1;
                    let label = format!("{}{}-{}", windows::LABEL_PREFIX, self.run, state.minted);
                    state.loading.insert(label.clone(), now);
                    label
                })
                .collect()
        };
        for label in wanted {
            if let Err(error) = windows::build(&self.app, &label) {
                warn!("a warm widget shell could not be built: {error}");
                self.lock().loading.remove(&label);
            }
        }
    }

    /// Records that one shell's page has loaded and is listening.
    ///
    /// A shell the pool gave up waiting for is still welcome. What it is not is
    /// counted twice: a page that reports itself again - a reload, a second
    /// load handler - has a label that is already in the queue or already
    /// holding a widget, and queuing it again would hand one window to two
    /// surfaces.
    pub fn ready(&self, label: String, channel: Channel<ShellMsg>) {
        let mut state = self.lock();
        let waited_for = state.loading.remove(&label).is_some();
        let first = waited_for && !state.channels.contains_key(&label);
        // Kept either way. A page that loaded again has a live channel, and the
        // one it replaces is dead.
        state.channels.insert(label.clone(), channel);
        if !first {
            debug!(label, "a widget shell reported itself more than once");
            return;
        }
        state.idle.push_back(label);
        drop(state);
        self.loaded.notify_all();
    }

    /// Returns what the display knows one shell by, remembered for this label.
    pub fn named(&self, label: &str) -> Arc<AtomicU32> {
        Arc::clone(self.lock().named.entry(label.to_string()).or_default())
    }

    /// Returns every shell window the display has named.
    ///
    /// For the focus tracker, which must refuse them alongside the pill and the
    /// transcript box. A widget shell is built unfocusable and stays that way,
    /// so a capture that recorded one would hand the person's keys to the one
    /// kind of window on this desktop that is certain to refuse them.
    pub fn shown(&self) -> Vec<u32> {
        self.lock()
            .named
            .values()
            .map(|known| known.load(Ordering::SeqCst))
            .filter(|id| *id != 0)
            .collect()
    }

    /// Takes one warm shell and answers with its label, or with nothing when
    /// none arrives.
    ///
    /// The label is the whole handle: it is the window's, it becomes the
    /// surface's, and every later message is addressed by it. Blocks only when
    /// the pool has run dry, which is the case the pool exists to make rare. A
    /// caller that gets nothing has to say so: a widget that never opened is
    /// better reported than waited out.
    pub fn take(&self) -> Option<String> {
        let deadline = Instant::now() + WARM_WAIT;
        let mut state = self.lock();
        loop {
            if let Some(label) = state.idle.pop_front() {
                if !state.channels.contains_key(&label) {
                    // A shell with no channel is not a shell. Drop it and look
                    // at the next one rather than handing back a window that
                    // cannot be told anything.
                    continue;
                }
                drop(state);
                // The replacement is started now rather than at the next open.
                // A pool that only refills once it has run dry makes the third
                // widget of a session pay the build the pool exists to remove.
                self.warm();
                return Some(label);
            }
            drop(state);
            self.warm();
            state = self.lock();
            if !state.idle.is_empty() {
                continue;
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            let (waited, outcome) = self
                .loaded
                .wait_timeout(state, left)
                .unwrap_or_else(|error| error.into_inner());
            state = waited;
            if outcome.timed_out() && state.idle.is_empty() {
                return None;
            }
        }
    }

    /// Sends one message to a shell that is already holding a widget.
    pub fn send(&self, label: &str, message: ShellMsg) {
        let channel = self.lock().channels.get(label).cloned();
        let Some(channel) = channel else {
            warn!(label, "a widget shell was told something it cannot hear");
            return;
        };
        if let Err(error) = channel.send(message) {
            warn!(label, "a widget shell would not take a message: {error}");
        }
    }

    /// Unmounts one shell, closes its window, and starts its replacement.
    ///
    /// The label goes with it. It was the surface identifier the service was
    /// answered with, and reusing it would let a late update land on a widget
    /// that has nothing to do with the one it was written for.
    pub fn discard(&self, label: &str) {
        self.send(label, ShellMsg::Retire);
        let mut state = self.lock();
        state.channels.remove(label);
        state.named.remove(label);
        state.idle.retain(|idle| idle != label);
        drop(state);
        if let Some(window) = self.app.get_webview_window(label)
            && let Err(error) = windows::close(&window)
        {
            warn!(label, "{error}");
        }
        self.warm();
    }

    fn lock(&self) -> MutexGuard<'_, PoolState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
}

/// Returns a token that is this run of the companion and no other.
///
/// Seconds since the epoch, in hex. Short enough to read in a log, and ordered,
/// so the newer of two labels is the one that sorts later. Two runs a second
/// apart is the resolution: a companion cannot be stopped and started again
/// inside one, and a clock that will not answer at all gives every run the same
/// token, which is where this started.
fn stamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    format!("{seconds:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_carries_the_run_that_minted_it() {
        // The counter restarts at one with the process. A service that outlives
        // the companion still holds the identifiers the last run handed out,
        // and an update written for a panel that is gone must not find a new
        // one wearing its name.
        let label = format!("{}{}-{}", windows::LABEL_PREFIX, stamp(), 1);
        assert!(label.starts_with(windows::LABEL_PREFIX));
        assert!(crate::widgets::is_shell(&label));
        assert_ne!(label, format!("{}1", windows::LABEL_PREFIX));
    }

    #[test]
    fn a_shell_message_names_its_kind_the_way_the_page_reads_it() {
        // shell.ts switches on `kind`. A rename on this side is a page that
        // silently ignores every message it is sent.
        let arrival = ShellMsg::Become {
            surface: "widget-1".into(),
            widget: "note".into(),
            name: "Note".into(),
            data: serde_json::json!({ "text": "hello" }),
        };
        assert_eq!(
            serde_json::to_value(&arrival).expect("the message serializes"),
            serde_json::json!({
                "kind": "become",
                "surface": "widget-1",
                "widget": "note",
                "name": "Note",
                "data": { "text": "hello" },
            })
        );
        assert_eq!(
            serde_json::to_value(ShellMsg::Retire).expect("the message serializes"),
            serde_json::json!({ "kind": "retire" })
        );
        assert_eq!(
            serde_json::to_value(ShellMsg::Life {
                state: Life::Pinned
            })
            .expect("the message serializes"),
            serde_json::json!({ "kind": "life", "state": "pinned" })
        );
        assert_eq!(
            serde_json::to_value(ShellMsg::Life { state: Life::Dim })
                .expect("the message serializes"),
            serde_json::json!({ "kind": "life", "state": "dim" })
        );
        assert_eq!(
            serde_json::to_value(ShellMsg::Refused {
                detail: "every slot is taken".into()
            })
            .expect("the message serializes"),
            serde_json::json!({ "kind": "refused", "detail": "every slot is taken" })
        );
        assert_eq!(
            serde_json::to_value(ShellMsg::Health {
                state: Health::Dead
            })
            .expect("the message serializes"),
            serde_json::json!({ "kind": "health", "state": "dead" })
        );
        assert_eq!(
            serde_json::to_value(ShellMsg::Health {
                state: Health::Stale
            })
            .expect("the message serializes"),
            serde_json::json!({ "kind": "health", "state": "stale" })
        );
    }
}
