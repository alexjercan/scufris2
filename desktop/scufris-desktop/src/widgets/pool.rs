//! Warm shell windows, so opening a widget is a message rather than a build.
//!
//! Building a webview window and waiting for its page to load takes long enough
//! to be seen. A widget arrives in the middle of a sentence, so that wait would
//! land in the middle of the sentence too. The pool keeps two windows built,
//! loaded, and hidden; opening a widget takes one of them and sends it a single
//! message.
//!
//! A shell is used once. Its label is the surface identifier the daemon is
//! answered with, and a label handed out twice would let an update meant for a
//! widget that is gone land on whatever took its place. So a retired shell is
//! destroyed rather than re-adopted, and the pool builds its replacement in the
//! background, off the path a person is waiting on.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, ipc::Channel};
use tracing::warn;

use crate::widgets::{runtime::Life, windows};

/// How many shells stay built and loaded.
///
/// Two: one for the widget that is about to open, and one for the widget that
/// follows it in the same answer. A third would be a window nothing has ever
/// needed at once.
pub const WARM_SHELLS: usize = 2;

/// How long taking a shell waits for one to finish loading.
///
/// Only ever spent when the pool is empty - at the very first widget of a
/// session, or after several opened at once. The daemon waits five seconds for
/// its answer, so this leaves room for the answer to travel.
const WARM_WAIT: Duration = Duration::from_secs(3);

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
    /// Shells built and still loading.
    loading: usize,
    /// The last label minted. Labels only ever go up.
    minted: u32,
}

/// The warm shells.
pub struct Pool {
    app: AppHandle,
    state: Mutex<PoolState>,
    loaded: Condvar,
}

impl Pool {
    /// Returns an empty pool. Nothing is built until [`Pool::warm`].
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
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
        let wanted: Vec<String> = {
            let mut state = self.lock();
            let short = WARM_SHELLS.saturating_sub(state.idle.len() + state.loading);
            (0..short)
                .map(|_| {
                    state.minted += 1;
                    state.loading += 1;
                    format!("{}{}", windows::LABEL_PREFIX, state.minted)
                })
                .collect()
        };
        for label in wanted {
            if let Err(error) = windows::build(&self.app, &label) {
                warn!("a warm widget shell could not be built: {error}");
                let mut state = self.lock();
                state.loading = state.loading.saturating_sub(1);
            }
        }
    }

    /// Records that one shell's page has loaded and is listening.
    pub fn ready(&self, label: String, channel: Channel<ShellMsg>) {
        let mut state = self.lock();
        state.loading = state.loading.saturating_sub(1);
        state.channels.insert(label.clone(), channel);
        state.idle.push_back(label);
        drop(state);
        self.loaded.notify_all();
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
    /// The label goes with it. It was the surface identifier the daemon was
    /// answered with, and reusing it would let a late update land on a widget
    /// that has nothing to do with the one it was written for.
    pub fn discard(&self, label: &str) {
        self.send(label, ShellMsg::Retire);
        let mut state = self.lock();
        state.channels.remove(label);
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

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
