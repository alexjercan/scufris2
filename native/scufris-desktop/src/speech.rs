//! The speaker the companion owns.
//!
//! The agent decides what is worth saying aloud and sends the paragraph; this
//! is what turns it into sound. The split is deliberate: the prose rules that
//! make a paragraph safe to say are the agent's, and the speaker belongs to
//! whoever is sitting in front of the machine. A headless agent has no speaker
//! to run, and a companion with no synthesiser configured may refuse what it is
//! handed without the conversation noticing.
//!
//! One utterance at a time. A new paragraph cuts the one being spoken, and so
//! does the microphone opening: a person who has started talking is not waiting
//! for the rest of the sentence.

use std::{
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
};

use tracing::{debug, warn};

/// Longest paragraph handed to the synthesiser, in UTF-8 bytes.
///
/// The helper measures the same way and refuses anything longer, so this is
/// where a long answer is shortened rather than dropped.
pub const MAX_SPOKEN_BYTES: usize = 1_000;

/// Told whether the companion is speaking, on every change.
pub type Listener = Box<dyn Fn(bool) + Send + Sync>;

/// Runs the synthesiser and owns whatever it is playing.
pub struct Speaker {
    command: Option<PathBuf>,
    /// Whether the person has asked for silence.
    ///
    /// Here rather than in the agent because it is a property of the speaker,
    /// and the speaker is this. A muted companion still receives every
    /// paragraph and still knows the answer; it just does not play it, so
    /// unmuting takes effect on the next answer with nothing to restore.
    muted: AtomicBool,
    /// The child currently making sound, if any.
    playing: Mutex<Option<Child>>,
    /// Which utterance the child belongs to. A reaper for an utterance that was
    /// already cut must not report silence over the one that cut it.
    utterance: AtomicU64,
    /// What the listener was last told, so it hears each change once.
    speaking: AtomicBool,
    listener: OnceLock<Listener>,
}

impl Speaker {
    /// Creates a speaker that runs `command`, or one that stays silent.
    ///
    /// No command is a deployment without a synthesiser, not a fault. Speech is
    /// the one thing Scufris does that the person can simply not want.
    pub fn new(command: Option<PathBuf>) -> Arc<Self> {
        if command.is_none() {
            debug!("no synthesiser is configured; nothing will be spoken");
        }
        Arc::new(Self {
            command,
            muted: AtomicBool::new(false),
            playing: Mutex::new(None),
            utterance: AtomicU64::new(0),
            speaking: AtomicBool::new(false),
            listener: OnceLock::new(),
        })
    }

    /// Says who to tell when the companion starts and stops speaking.
    ///
    /// Separate from [`Speaker::new`] because the listener is the pill runtime
    /// and the runtime is built holding this speaker: one of the two has to
    /// exist first, and it is this one.
    pub fn attach(&self, listener: impl Fn(bool) + Send + Sync + 'static) {
        let _ = self.listener.set(Box::new(listener));
    }

    /// Asks for silence, or takes the ask back. Returns whether it is muted now.
    ///
    /// Muting cuts what is being spoken. Waiting for the sentence to end is not
    /// what anybody means by mute.
    pub fn mute(&self, muted: bool) -> bool {
        self.muted.store(muted, Ordering::Release);
        if muted {
            self.hush();
        }
        muted
    }

    /// Whether the person has asked for silence.
    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::Acquire)
    }

    /// Speaks one paragraph, cutting whatever is being spoken now.
    pub fn say(self: &Arc<Self>, text: String) {
        if self.muted() {
            debug!("a paragraph was dropped: the speaker is muted");
            return;
        }
        let Some(command) = self.command.clone() else {
            debug!("a paragraph was dropped: no synthesiser is configured");
            return;
        };
        let text = bounded(text);
        if text.is_empty() {
            return;
        }
        // Claimed before the old child is cut, so the reaper for the utterance
        // being replaced can see that it is no longer the current one.
        let utterance = self.utterance.fetch_add(1, Ordering::SeqCst) + 1;

        let mut child = match Command::new(&command)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                warn!("the synthesiser would not start: {error}");
                return;
            }
        };
        // Well under a pipe buffer, so this cannot block on a child that has
        // not started reading yet. Dropping the handle closes the pipe, which
        // is what tells the helper the paragraph is complete.
        if let Some(mut stdin) = child.stdin.take()
            && let Err(error) = stdin.write_all(text.as_bytes())
        {
            debug!("the synthesiser closed its input early: {error}");
        }
        self.replace(Some(child));
        self.tell(true);

        let speaker = Arc::clone(self);
        thread::spawn(move || speaker.reap(utterance));
    }

    /// Cuts whatever is being spoken. Silence is not an error.
    pub fn hush(&self) {
        // Bumped first, so the reaper for the utterance being cut knows it is
        // no longer the one anybody is listening to.
        self.utterance.fetch_add(1, Ordering::SeqCst);
        if self.replace(None) {
            debug!("speech was cut short");
            self.tell(false);
        }
    }

    /// Waits for one utterance to finish and reports the silence it leaves.
    fn reap(self: Arc<Self>, utterance: u64) {
        loop {
            let finished = {
                let mut playing = self
                    .playing
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                match playing.as_mut() {
                    // Somebody else's utterance is playing, so this one is over
                    // and its silence has already been spoken for.
                    None => return,
                    Some(child) => match child.try_wait() {
                        Ok(Some(_)) => {
                            playing.take();
                            true
                        }
                        Ok(None) => false,
                        Err(error) => {
                            warn!("the synthesiser could not be waited on: {error}");
                            playing.take();
                            true
                        }
                    },
                }
            };
            if self.utterance.load(Ordering::SeqCst) != utterance {
                return;
            }
            if finished {
                self.tell(false);
                return;
            }
            thread::sleep(POLL);
        }
    }

    /// Installs the child that is playing now and stops the one that was.
    ///
    /// Returns true when there was something to stop.
    fn replace(&self, child: Option<Child>) -> bool {
        let mut playing = self
            .playing
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let previous = playing.take();
        *playing = child;
        let Some(mut previous) = previous else {
            return false;
        };
        // The helper stops its own children on a signal, so this takes the
        // synthesiser and the playback down with it rather than leaving a
        // sound running that nothing owns.
        let _ = previous.kill();
        let _ = previous.wait();
        true
    }

    /// Reports a change, and only a change: a second paragraph in a row is one
    /// utterance replacing another, not the companion falling silent between.
    fn tell(&self, speaking: bool) {
        if self.speaking.swap(speaking, Ordering::SeqCst) == speaking {
            return;
        }
        if let Some(listener) = self.listener.get() {
            listener(speaking);
        }
    }
}

/// How often a running utterance is checked for having finished.
const POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// Trims one paragraph to what the synthesiser accepts, on a character
/// boundary, and strips what it refuses outright.
fn bounded(text: String) -> String {
    let text: String = text
        .chars()
        .filter(|character| !character.is_control() && *character != '\u{7f}')
        .collect();
    let text = text.trim();
    if text.len() <= MAX_SPOKEN_BYTES {
        return text.to_string();
    }
    let mut end = MAX_SPOKEN_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paragraph_is_trimmed_rather_than_dropped_and_never_carries_control_bytes() {
        assert_eq!(bounded("  it is raining  ".into()), "it is raining");
        // Newlines are what the helper reads as the end of the paragraph, so a
        // paragraph that carries one would be cut in half by the wire rather
        // than by this.
        assert_eq!(bounded("two\nlines".into()), "twolines");

        let long = bounded("é".repeat(MAX_SPOKEN_BYTES).to_string());
        assert!(long.len() <= MAX_SPOKEN_BYTES);
        assert!(
            long.chars().all(|character| character == 'é'),
            "a multi-byte character was cut in half"
        );
        assert!(!long.is_empty());
    }

    #[test]
    fn a_companion_with_no_synthesiser_stays_silent_and_says_nothing_is_wrong() {
        let speaker = Speaker::new(None);
        let heard = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&heard);
        speaker.attach(move |speaking| {
            recorded
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(speaking)
        });
        speaker.say("it is raining".into());
        speaker.hush();
        assert!(
            heard
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_empty(),
            "a companion with no speaker reported itself speaking"
        );
    }

    #[test]
    fn one_utterance_at_a_time_and_the_last_word_is_the_current_one() {
        let script = std::env::temp_dir().join(format!("scufris-speak-{}", std::process::id()));
        std::fs::write(&script, "#!/bin/sh\ncat >/dev/null\nsleep 5\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let speaker = Speaker::new(Some(script.clone()));
        let heard = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&heard);
        speaker.attach(move |speaking| {
            recorded
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(speaking)
        });

        speaker.say("the first thing".into());
        speaker.say("the second thing".into());
        speaker.hush();
        // The cut utterance's reaper must not report silence over a later one,
        // and the last thing anybody hears about is the silence that is real.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if heard
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .last()
                == Some(&false)
            {
                break;
            }
            thread::sleep(POLL);
        }
        let heard = heard.lock().unwrap_or_else(|error| error.into_inner());
        assert_eq!(
            heard.as_slice(),
            &[true, false],
            "the companion did not settle into silence: {heard:?}"
        );
        std::fs::remove_file(&script).unwrap();
    }

    #[test]
    fn a_muted_speaker_cuts_what_is_playing_and_takes_nothing_new() {
        let script = std::env::temp_dir().join(format!("scufris-mute-{}", std::process::id()));
        std::fs::write(&script, "#!/bin/sh\ncat >/dev/null\nsleep 5\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let speaker = Speaker::new(Some(script.clone()));
        let heard = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&heard);
        speaker.attach(move |speaking| {
            recorded
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(speaking)
        });

        assert!(!speaker.muted());
        speaker.say("the first thing".into());
        // Muting is not "after this sentence". It stops the sound.
        assert!(speaker.mute(true));
        speaker.say("the muted thing".into());
        let heard = heard.lock().unwrap_or_else(|error| error.into_inner());
        assert_eq!(
            heard.as_slice(),
            &[true, false],
            "muting did not cut what was playing: {heard:?}"
        );
        drop(heard);

        // Unmuting restores nothing. The next answer is what is heard.
        assert!(!speaker.mute(false));
        assert!(!speaker.muted());
        std::fs::remove_file(&script).unwrap();
    }
}
