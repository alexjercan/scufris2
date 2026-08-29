//! Companion-local pill state machine.
//!
//! The machine owns every decision the accepted interaction describes and
//! produces the actions the host must run. It holds no window, audio, socket, or
//! file handle so the whole interaction is testable without a desktop session.

use std::collections::BTreeMap;

use scufris_control::service::{NoticeState, ScufrisState};

use crate::pending::Pending;

/// What the companion shows the assistant is doing.
///
/// Not the service's [`ScufrisState`], and one variant wider. The service
/// reports what the agent is doing; speaking is what the companion itself is
/// doing with the paragraph it was handed, and only the companion knows when
/// the speaker stops. Everything else is the service's word, taken as given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assistant {
    /// The agent is spawned and has not answered yet.
    Starting,
    /// The agent is up and no run is in progress.
    Idle,
    /// An agent run is in progress.
    Working,
    /// The companion is speaking an answer it was handed.
    Speaking,
    /// A debug lease is held, so the conversation is a terminal somebody else
    /// owns.
    Detached,
    /// The agent could not be kept running.
    Error,
}

impl Assistant {
    /// Short stable name used by the pill and the tray.
    pub fn name(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Speaking => "speaking",
            Self::Detached => "detached",
            Self::Error => "error",
        }
    }
}

impl From<ScufrisState> for Assistant {
    fn from(state: ScufrisState) -> Self {
        match state {
            ScufrisState::Starting => Self::Starting,
            ScufrisState::Idle => Self::Idle,
            ScufrisState::Working => Self::Working,
            ScufrisState::Detached => Self::Detached,
            ScufrisState::Error => Self::Error,
        }
    }
}

/// Reason shown for a transcript recovered from a previous process.
pub const RESTORED_REASON: &str = "Recovered after a restart. Whether it was sent is unknown.";

/// Reason shown once the person has asked to send an uncertain transcript.
pub const FORCE_WARNING: &str = "This may already be in the conversation. Sending it again could repeat what it did. \
     Press Enter again to send anyway, Ctrl+C to copy, Escape to discard.";

/// What an uncertain transcript offers instead of an ordinary retry.
pub const UNCERTAIN_CHOICES: &str = "Ctrl+C to copy, Escape to discard, Enter to send anyway.";

/// What the companion is doing with the microphone and the transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// No interaction is running. The pill rests on screen showing what the
    /// assistant is doing, unless the person has dismissed it.
    Resting,
    /// The microphone is recording.
    Listening,
    /// Recording stopped and local transcription is running.
    Transcribing,
    /// An editable transcript is waiting for the user in the textbox.
    Editing {
        /// Current transcript text.
        transcript: String,
        /// Identifier this transcript keeps until it is acknowledged.
        id: String,
        /// Why the durable copy is missing, empty when it is present.
        notice: String,
    },
    /// A transcript was accepted and is on its way to the service.
    Sent {
        /// Exact text handed to the service.
        transcript: String,
        /// Submission identifier awaiting its acknowledgment.
        id: String,
        /// What was already known about these words before this send. A send
        /// the service refuses says nothing about an earlier one that may
        /// already be in the conversation, so this is what the transcript
        /// falls back to rather than becoming editable again.
        prior: Delivery,
    },
    /// An accepted transcript that the service did not take.
    Retained {
        /// Transcript kept in the pill so it is never lost.
        transcript: String,
        /// Identifier reused by every retry, so an answer to any attempt is
        /// an answer to this transcript. It is a correlation handle and not a
        /// duplicate guard: nothing on the far side suppresses by it, and
        /// `warned` is what stops a resend the person did not ask for.
        id: String,
        /// Why the submission did not land.
        reason: String,
        /// Whether the service might already hold this transcript.
        delivery: Delivery,
        /// True once the person has asked to send an uncertain transcript and
        /// has been told what that risks. Only then may it be sent again.
        warned: bool,
    },
    /// Recording or local transcription failed and nothing was submitted.
    Failed {
        /// Why no transcript was produced.
        reason: String,
    },
}

impl Phase {
    /// Short stable name for logs. The transcript itself never appears in it.
    pub fn name(&self) -> &'static str {
        match self {
            Phase::Resting => "resting",
            Phase::Listening => "listening",
            Phase::Transcribing => "transcribing",
            Phase::Editing { .. } => "editing",
            Phase::Sent { .. } => "sent",
            Phase::Retained { .. } => "retained",
            Phase::Failed { .. } => "failed",
        }
    }
}

/// How much is known about a retained transcript's fate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// The submission never left the companion, so editing it is safe.
    Refused,
    /// The submission may already be in the conversation, and may not be.
    ///
    /// Nothing here can tell which, so the words are frozen and nothing resends
    /// them on its own: not a timeout, not a reconnection, not a restart, and
    /// not an ordinary Enter. A request that ran once can have written files,
    /// sent messages, or started work, and running it twice is not a smaller
    /// mistake than not running it at all. The person is told what is unknown
    /// and chooses.
    Uncertain,
}

/// One thing that happened to the companion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The activation hotkey fired, or the tray asked for voice input.
    Activate,
    /// Escape was pressed: in the textbox, or on the accelerator that answers
    /// a pill with no textbox under it.
    Escape,
    /// Escape, where there is a workspace on screen to go back to.
    ///
    /// The same verb as [`Event::Escape`] and the same thing thrown away. What
    /// differs is where it leaves the person: cancelling a take you did not
    /// mean to start should not also take down the panels you were reading. The
    /// host decides which of the two this is, because whether there is anything
    /// in the layer is not something the phases know.
    Cancel,
    /// The person asked for the workspace, without asking to be heard.
    ///
    /// The pill comes home and the layer comes up with it. The phase is not
    /// touched and the microphone is not opened: this is the door the companion
    /// did not have, and every other way onto the screen went through the
    /// microphone.
    Reveal,
    /// The person asked for the workspace to go away.
    ///
    /// Only from the phases that are holding nothing. A gesture that could put
    /// the pill away while there are words on screen is a gesture that can lose
    /// them, and this one is on the key that is pressed most.
    Dismiss,
    /// The stop key was pressed, or the tray was asked to stop Scufris.
    ///
    /// The one gesture here that reaches the conversation without saying
    /// anything to it. What it stops is the agent's run; cutting the speech is
    /// the host's, because the speaker never crossed the socket to begin with.
    Stop,
    /// Enter was pressed in the textbox, carrying what is in the field.
    Enter {
        /// Text currently in the field, when it is one the person may edit.
        text: Option<String>,
    },
    /// Local transcription produced text.
    Transcribed(String),
    /// Local transcription failed.
    TranscriptionFailed(String),
    /// The microphone could not start, or an open capture stream failed.
    RecordingFailed(String),
    /// The transcript could not be written to durable storage.
    PersistFailed(String),
    /// A transcript the user discarded could not be removed or tombstoned.
    DiscardFailed(String),
    /// The service acknowledged a submission.
    Acknowledged(String),
    /// The submission never reached the service, or the service refused it
    /// before any of its words could leave.
    SubmissionFailed {
        /// Identifier of the submission this answers.
        id: String,
        /// Why it was not sent.
        reason: String,
    },
    /// The submission left the companion and nothing came back in time.
    ///
    /// Raised by the companion's own timeout, never by the service: the
    /// service answers every submission one way or the other, so this is what
    /// a service that stopped answering looks like from here. The bytes left
    /// this process, so the request may already have run.
    SubmissionUncertain {
        /// Identifier of the submission this answers.
        id: String,
        /// Why its outcome is unknown.
        reason: String,
    },
    /// The transcript was asked to be copied to the clipboard.
    Copy,
}

/// One thing the host must do after a transition.
///
/// Showing and hiding the pill are not here. Where the window belongs is a
/// property of the phase, so the host reads it from the phase the transition
/// left behind rather than from a list it was handed: a list is a decision from
/// a moment that may have passed by the time it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Start microphone capture.
    StartRecording,
    /// Stop microphone capture and transcribe the audio.
    StopRecording,
    /// Stop microphone capture and throw the audio away.
    DiscardRecording,
    /// Abandon an in-flight transcription.
    CancelTranscription,
    /// Write the transcript to durable storage before anything can lose it.
    PersistPending {
        /// Identifier this transcript keeps until it is acknowledged.
        id: String,
        /// Transcript text to keep.
        text: String,
    },
    /// Forget the durable transcript after the service acknowledged it.
    ClearPending,
    /// Throw away a transcript the user explicitly discarded.
    ///
    /// Distinct from [`Action::ClearPending`] because discarded words must not
    /// come back: a removal that cannot happen leaves a tombstone instead, and
    /// a failure of both is reported rather than logged.
    DiscardPending {
        /// Identifier of the transcript being thrown away.
        id: String,
    },
    /// End the agent's current run.
    ///
    /// Carries an identifier for the same reason a submission does: the service
    /// answers every command it is given, and an answer that echoes nothing
    /// cannot be told from the answer to something else.
    Abort {
        /// Identifier the service's answer echoes.
        id: String,
    },
    /// Submit one accepted transcript to the service.
    Submit {
        /// Submission identifier the answer must echo.
        id: String,
        /// Accepted transcript text.
        text: String,
    },
    /// Put one transcript on the clipboard so it is not lost when the pill closes.
    CopyTranscript {
        /// Exact text to copy.
        text: String,
    },
}

/// Everything the surfaces render, derived from the phase and the service
/// state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presentation {
    /// Stable state name used by the pill and the tray.
    pub state: &'static str,
    /// Text the textbox shows, empty when there is none.
    pub text: String,
    /// Short explanation of an error or a retained transcript.
    pub detail: String,
    /// Whether the textbox field may be edited.
    pub editable: bool,
    /// Whether the pill shows the recording indicator and duration.
    pub recording: bool,
}

/// Where the companion's windows belong for the current phase.
///
/// The pill never holds the keyboard: it is an indicator, and taking keys away
/// from whatever the person is typing in to show them one is a trade nothing
/// here is worth. The textbox does hold it, and only while there are words in
/// front of the person to read, correct, and send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Posture {
    /// The pill is up and the textbox stands over it with the keyboard: a
    /// phase whose words the person answers.
    Editing,
    /// The pill is up and the phase waits for it. The microphone is open or
    /// about to be, or something went wrong that the person has to read.
    /// Nothing holds the keyboard.
    Watched,
    /// The pill is up, reporting, and nothing waits on it: the handoff of a
    /// submission, and the turn it started, watched until the assistant
    /// settles.
    Passive,
    /// Off screen.
    Off,
}

impl Posture {
    /// Answers whether the phase may not run until its windows are on screen.
    ///
    /// The microphone must never open behind a privacy indicator that is not
    /// there, and words nobody can see are words nobody can correct. A phase
    /// the person only watches has neither problem and is not held up.
    pub fn waits(self) -> bool {
        matches!(self, Self::Editing | Self::Watched)
    }

    /// Answers whether the pill belongs on screen.
    pub fn on_screen(self) -> bool {
        self != Self::Off
    }

    /// Answers whether the textbox belongs on screen with the keyboard.
    pub fn textbox(self) -> bool {
        self == Self::Editing
    }
}

/// The pill state machine.
#[derive(Debug)]
pub struct Companion {
    phase: Phase,
    assistant: Assistant,
    assistant_detail: String,
    /// Ambient notices last replayed by the service, keyed by their owner.
    notices: BTreeMap<String, (NoticeState, String)>,
    /// True while the companion is speaking an answer. Its own doing, and not
    /// the service's word, so it sits beside the reported state rather than
    /// replacing it: what the agent is doing is still what it was doing.
    speaking: bool,
    connected: bool,
    prefix: String,
    /// How many commands this companion has given the service.
    commands: u64,
    /// True while the person has the pill closed. The pill is a resident HUD:
    /// once an activation brings it up it stays up, resting between
    /// interactions, and only the person's own Escape puts it away. Nothing
    /// the assistant does ever takes the pill down or brings it back.
    dismissed: bool,
    /// Why the pill window is unusable, when the host could not put it where a
    /// phase needed it.
    ///
    /// Kept beside the phase rather than in it, because this is the state of
    /// having no window at all: nothing belongs on screen, nothing is left
    /// running, and the tray is the only thing that can still say so.
    blind: Option<String>,
}

impl Companion {
    /// Creates a closed pill with an unreachable service.
    ///
    /// `prefix` must be unique per process. Identifiers survive a restart, and
    /// they are how an answer is matched to the submission that asked, so a
    /// reused prefix could have one submission take another's answer.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            phase: Phase::Resting,
            assistant: Assistant::Idle,
            assistant_detail: String::new(),
            notices: BTreeMap::new(),
            speaking: false,
            connected: false,
            prefix: prefix.into(),
            commands: 0,
            dismissed: true,
            blind: None,
        }
    }

    /// Returns the current phase.
    #[cfg(test)]
    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    /// Short stable name of the current phase, for logs.
    pub fn phase_name(&self) -> &'static str {
        self.phase.name()
    }

    /// Returns the assistant state last reported by the service.
    pub fn assistant(&self) -> Assistant {
        self.assistant
    }

    /// Returns true while this exact submission is still unacknowledged.
    ///
    /// Nothing in the runtime asks: every answer names the submission it
    /// answers, and the transitions above apply it only to that one.
    #[cfg(test)]
    pub fn awaiting(&self, submission: &str) -> bool {
        matches!(&self.phase, Phase::Sent { id, .. } if id == submission)
    }

    /// Returns true while an accepted transcript is still unsent.
    #[cfg(test)]
    pub fn holds_unsent_transcript(&self) -> bool {
        matches!(
            self.phase,
            Phase::Editing { .. } | Phase::Sent { .. } | Phase::Retained { .. }
        )
    }

    /// Answers where the companion's windows belong right now.
    ///
    /// The host reads this after every change instead of being handed show and
    /// hide actions. Two changes can be in flight at once - the person's key
    /// and the service's answer - and the one that ran last is the one whose
    /// windows the person must end up looking at.
    ///
    /// A transcript handed off for submission gives the keyboard back at once:
    /// the words are the service's now, and waiting for an acknowledgment would
    /// hold the keyboard in an always-on-top textbox for as long as the backend
    /// takes. The pill stays, passively, and goes on reporting what the
    /// assistant does - sent, working, speaking, then resting - because it is
    /// a resident HUD: only the person's Escape puts it away, and only their
    /// activation brings it back.
    pub fn posture(&self) -> Posture {
        if self.blind.is_some() {
            return Posture::Off;
        }
        match &self.phase {
            // The two passive phases: on screen, reporting, holding nothing.
            // Dismissal is theirs alone, because they are the only ones the
            // person can put away without also cancelling something.
            Phase::Resting | Phase::Sent { .. } => {
                if self.dismissed {
                    Posture::Off
                } else {
                    Posture::Passive
                }
            }
            // The phases with words in them. The textbox is where they are
            // read, and it is the one window here that takes the keyboard.
            Phase::Editing { .. } | Phase::Retained { .. } => Posture::Editing,
            // The microphone, the transcription, and whatever went wrong with
            // either: nothing to type into, and nothing that may run before
            // the person can see it.
            Phase::Listening | Phase::Transcribing | Phase::Failed { .. } => Posture::Watched,
        }
    }

    /// Answers whether the pill belongs on screen, in any posture.
    #[cfg(test)]
    pub fn on_screen(&self) -> bool {
        self.posture().on_screen()
    }

    /// Returns true while the host has no usable pill window.
    #[cfg(test)]
    pub fn blinded(&self) -> bool {
        self.blind.is_some()
    }

    /// Gives up an interaction whose pill could not be put on screen.
    ///
    /// Every phase that needs the person needs the window, so a phase that
    /// cannot have one has nothing left to offer them: the microphone must
    /// never run behind a privacy indicator that is not there, and words nobody
    /// can see are words nobody can correct. An accepted transcript is already
    /// on disk and comes back on the next start, so nothing is thrown away
    /// here. The tray says what happened, and the person's next activation is
    /// what tries the window again.
    pub fn abandon(&mut self, reason: String) -> Vec<Action> {
        let actions = match self.phase {
            Phase::Listening => vec![Action::DiscardRecording],
            Phase::Transcribing => vec![Action::CancelTranscription],
            _ => Vec::new(),
        };
        self.phase = Phase::Resting;
        self.blind = Some(reason);
        actions
    }

    /// Reports a failure that has no transcript behind it any more.
    fn report_failure(&mut self, reason: String) -> Vec<Action> {
        self.phase = Phase::Failed { reason };
        Vec::new()
    }

    /// Reports a durable-storage failure that has no transcript behind it.
    pub fn report_store_failure(&mut self, reason: String) {
        if !matches!(self.phase, Phase::Resting) {
            return;
        }
        self.phase = Phase::Failed { reason };
    }

    /// Reopens the pill on a transcript recovered from a previous process.
    ///
    /// The recovered identifier is kept, so an answer to the resend answers
    /// the transcript the previous process was holding. What keeps it out of
    /// the conversation twice is `Delivery::Uncertain` below, not the
    /// identifier: nothing suppresses by identifier any more.
    pub fn restore(&mut self, pending: Pending) {
        if !matches!(self.phase, Phase::Resting) {
            return;
        }
        self.phase = Phase::Retained {
            transcript: pending.text,
            id: pending.id,
            reason: format!("{RESTORED_REASON} {UNCERTAIN_CHOICES}"),
            // Whether the previous process delivered this is unknowable here,
            // so it is treated as possibly delivered: it cannot be edited, and
            // nothing sends it again without the person saying so.
            delivery: Delivery::Uncertain,
            warned: false,
        };
    }

    /// Records whether the service connection is currently open.
    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
        if connected {
            // Welcome begins a new replay. Anything absent from the service's
            // following notice set was cleared while this frontend was away.
            self.notices.clear();
        } else {
            self.assistant = Assistant::Idle;
            self.assistant_detail.clear();
        }
    }

    /// Raises, replaces, or clears one identified ambient notice.
    pub fn set_notice(&mut self, id: String, state: NoticeState, detail: String) {
        if state == NoticeState::Clear {
            self.notices.remove(&id);
        } else {
            self.notices.insert(id, (state, detail));
        }
    }

    /// Returns true while the service connection is open.
    pub fn connected(&self) -> bool {
        self.connected
    }

    /// Records the assistant state the service reported.
    pub fn set_assistant(&mut self, state: Assistant, detail: String) {
        self.assistant = state;
        self.assistant_detail = detail;
    }

    /// Records whether the companion is speaking an answer.
    pub fn set_speaking(&mut self, speaking: bool) {
        self.speaking = speaking;
    }

    /// Returns the assistant state the companion is showing, speech included.
    pub fn shown_assistant(&self) -> Assistant {
        if self.speaking {
            Assistant::Speaking
        } else {
            self.assistant
        }
    }

    /// Applies one event and returns the actions the host must run.
    pub fn apply(&mut self, event: Event) -> Vec<Action> {
        // The runtime stopped trying the window when it could not open one.
        // The person asking for the pill again is what tries it again: nothing
        // else here knows whether the display has come back. Both of the
        // gestures that ask for the pill bring a dismissed one home.
        if matches!(event, Event::Activate | Event::Reveal) {
            self.blind = None;
            self.dismissed = false;
        }
        self.transition(event)
    }

    fn transition(&mut self, event: Event) -> Vec<Action> {
        // `Cancel` is `Escape` that leaves the workspace standing. The phases
        // answer one verb and throw the same thing away; all that differs is
        // whether the pill goes down with it, so it travels as an argument to
        // `close` rather than as a second row for every phase.
        let keep = matches!(event, Event::Cancel);
        let event = if keep { Event::Escape } else { event };
        match (&self.phase, event) {
            (Phase::Resting, Event::Activate) => {
                self.phase = Phase::Listening;
                vec![Action::StartRecording]
            }
            (Phase::Failed { .. }, Event::Activate) => {
                self.phase = Phase::Listening;
                vec![Action::StartRecording]
            }
            // The activation key always listens, so a second press is what
            // stops the microphone. There is no second thing it could mean:
            // every transcript goes to the textbox, and the textbox is what
            // sends.
            (Phase::Listening, Event::Activate) => {
                self.phase = Phase::Transcribing;
                vec![Action::StopRecording]
            }
            (Phase::Listening, Event::Escape) => self.close(keep, vec![Action::DiscardRecording]),
            (Phase::Transcribing, Event::Escape) => {
                self.close(keep, vec![Action::CancelTranscription])
            }
            // The microphone can fail before capture starts and while it runs.
            // Either way the pill must stop claiming to record.
            (Phase::Listening, Event::RecordingFailed(reason)) => {
                self.phase = Phase::Failed { reason };
                vec![Action::DiscardRecording]
            }
            (Phase::Transcribing, Event::RecordingFailed(reason)) => {
                self.phase = Phase::Failed { reason };
                vec![Action::CancelTranscription]
            }
            (Phase::Transcribing, Event::Transcribed(transcript)) => {
                let transcript = transcript.trim().to_string();
                if transcript.is_empty() {
                    self.phase = Phase::Failed {
                        reason: "Nothing was transcribed.".into(),
                    };
                    return Vec::new();
                }
                let id = self.next_id();
                let persist = Action::PersistPending {
                    id: id.clone(),
                    text: transcript.clone(),
                };
                self.phase = Phase::Editing {
                    transcript,
                    id,
                    notice: String::new(),
                };
                vec![persist]
            }
            (Phase::Transcribing, Event::TranscriptionFailed(reason)) => {
                self.phase = Phase::Failed { reason };
                Vec::new()
            }
            (
                Phase::Editing { transcript, id, .. }
                | Phase::Retained {
                    transcript,
                    id,
                    delivery: Delivery::Refused,
                    ..
                },
                Event::Enter { text },
            ) => {
                let id = id.clone();
                let accepted = text.unwrap_or_else(|| transcript.clone());
                let accepted = accepted.trim().to_string();
                if accepted.is_empty() {
                    return Vec::new();
                }
                let mut actions = vec![Action::PersistPending {
                    id: id.clone(),
                    text: accepted.clone(),
                }];
                actions.extend(self.submit(accepted, id, false));
                actions
            }
            // This may already be in the conversation, and may not be. Sending
            // it again could repeat whatever it did, so the first Enter only
            // says so; the second is the person's own decision and the only
            // thing that ever sends it.
            (
                Phase::Retained {
                    transcript,
                    id,
                    delivery: Delivery::Uncertain,
                    warned,
                    ..
                },
                Event::Enter { .. },
            ) => {
                if !*warned {
                    self.phase = Phase::Retained {
                        transcript: transcript.clone(),
                        id: id.clone(),
                        reason: FORCE_WARNING.to_string(),
                        delivery: Delivery::Uncertain,
                        warned: true,
                    };
                    return Vec::new();
                }
                let id = id.clone();
                let accepted = transcript.clone();
                let mut actions = vec![Action::PersistPending {
                    id: id.clone(),
                    text: accepted.clone(),
                }];
                actions.extend(self.submit(accepted, id, true));
                actions
            }
            (
                Phase::Editing { transcript, .. } | Phase::Retained { transcript, .. },
                Event::Copy,
            ) => vec![Action::CopyTranscript {
                text: transcript.clone(),
            }],
            (Phase::Editing { id, .. } | Phase::Retained { id, .. }, Event::Escape) => {
                let id = id.clone();
                self.close(keep, vec![Action::DiscardPending { id }])
            }
            // Nothing to answer and nothing to correct, so Escape is the only
            // key a failure has: it puts the pill away.
            (Phase::Failed { .. }, Event::Escape) => self.close(keep, Vec::new()),
            // The two phases the pill is on screen without the keyboard in.
            // Escape there is the person putting the pill away, and it had no
            // road down until the verb could arrive without focus: the pill
            // takes no keys while it is passive, so the only way down was to
            // open the microphone with the hotkey and then cancel it.
            //
            // The phase is left alone. A send in flight is a turn the pill is
            // watching, and losing it to put the window away would lose the
            // acknowledgment the words are waiting on.
            //
            // A cancel is not a dismissal here. There is nothing to cancel in
            // either phase, and answering it with a dismissal would make the
            // Escape ladder circular: the take goes and the pill stays, and the
            // next press has to be the one that ends it.
            (Phase::Resting | Phase::Sent { .. }, Event::Escape) => {
                if !keep {
                    self.dismissed = true;
                }
                Vec::new()
            }
            // Asked for on its own. The pill is already home by the time this
            // runs - `apply` brings it - so there is nothing left to do but
            // leave the phase exactly as it was found.
            (_, Event::Reveal) => Vec::new(),
            // Only the phases holding nothing. Everything else is words on
            // screen or a microphone that is open, and this arrives from the
            // key that is pressed most: a tap that could lose a take is a tap
            // that eventually will.
            (Phase::Resting | Phase::Sent { .. }, Event::Dismiss) => {
                self.dismissed = true;
                Vec::new()
            }
            // The interaction has already ended by the time this arrives, so
            // it is reported: silently keeping discarded words would be worse.
            (Phase::Resting, Event::DiscardFailed(reason)) => self.report_failure(reason),
            (
                Phase::Sent { id, .. } | Phase::Retained { id, .. },
                Event::Acknowledged(acknowledged),
            ) if *id == acknowledged => self.settle(vec![Action::ClearPending]),
            (
                Phase::Sent {
                    transcript,
                    id,
                    prior,
                },
                Event::PersistFailed(reason),
            ) => {
                // The runtime stops before Submit when the save fails, so this
                // send never happened. Whether an earlier one did is what
                // `prior` remembers.
                self.phase = Phase::Retained {
                    transcript: transcript.clone(),
                    id: id.clone(),
                    reason,
                    delivery: *prior,
                    warned: false,
                };
                Vec::new()
            }
            (Phase::Editing { transcript, id, .. }, Event::PersistFailed(notice)) => {
                self.phase = Phase::Editing {
                    transcript: transcript.clone(),
                    id: id.clone(),
                    notice,
                };
                Vec::new()
            }
            (
                Phase::Retained {
                    transcript,
                    id,
                    delivery,
                    warned,
                    ..
                },
                Event::PersistFailed(reason),
            ) => {
                self.phase = Phase::Retained {
                    transcript: transcript.clone(),
                    id: id.clone(),
                    reason,
                    delivery: *delivery,
                    warned: *warned,
                };
                Vec::new()
            }
            // Answered to the submission it names. A companion holds one
            // transcript at a time and may already have started another, so an
            // answer that arrives late must not settle words it is not about.
            (
                Phase::Sent {
                    transcript,
                    id,
                    prior,
                },
                Event::SubmissionFailed { id: failed, reason },
            ) if *id == failed => {
                self.phase = Phase::Retained {
                    transcript: transcript.clone(),
                    id: id.clone(),
                    // Nothing left the companion this time. An earlier send
                    // that may already be in the conversation still did.
                    reason: match prior {
                        Delivery::Refused => reason,
                        Delivery::Uncertain => format!("{reason} {UNCERTAIN_CHOICES}"),
                    },
                    delivery: *prior,
                    warned: false,
                };
                Vec::new()
            }
            (
                Phase::Sent { transcript, id, .. },
                Event::SubmissionUncertain {
                    id: uncertain,
                    reason,
                },
            ) if *id == uncertain => {
                self.phase = Phase::Retained {
                    transcript: transcript.clone(),
                    id: id.clone(),
                    reason: format!("{reason} {UNCERTAIN_CHOICES}"),
                    delivery: Delivery::Uncertain,
                    // The person has not been asked yet. The first Enter asks.
                    warned: false,
                };
                Vec::new()
            }
            // Stop belongs to the assistant rather than to the phase, so it
            // reads the one and leaves the other exactly where it was: a
            // transcript being edited while a run is stopped is still a
            // transcript, and nothing here has been said to the conversation.
            //
            // Only a run can be stopped. A settled agent has nothing to end,
            // and pressing the key then is not a mistake to report: whatever
            // the person meant to stop, the speaker, or a run that finished
            // while they were reaching for the key, has stopped.
            (_, Event::Stop) => {
                if self.assistant != Assistant::Working {
                    return Vec::new();
                }
                vec![Action::Abort { id: self.next_id() }]
            }
            _ => Vec::new(),
        }
    }

    /// The next identifier for anything this companion asks the service for.
    ///
    /// One counter for every command rather than one per kind, so no two live
    /// identifiers can be equal and an answer is never taken for the answer to
    /// something else.
    fn next_id(&mut self) -> String {
        self.commands += 1;
        format!("{}-{}", self.prefix, self.commands)
    }

    /// Sends one transcript and records what was already known about it.
    ///
    /// `forced` is the person's own decision to send words that may already be
    /// in the conversation. It never reaches the wire: the service takes every
    /// submission at face value, so the confirmation above is the whole of what
    /// stands between an uncertain transcript and a second run. What it does
    /// here is set what this send falls back to, because a refusal now says
    /// nothing about the send that may already have landed.
    fn submit(&mut self, transcript: String, id: String, forced: bool) -> Vec<Action> {
        self.phase = Phase::Sent {
            transcript: transcript.clone(),
            id: id.clone(),
            prior: if forced {
                Delivery::Uncertain
            } else {
                Delivery::Refused
            },
        };
        vec![Action::Submit {
            id,
            text: transcript,
        }]
    }

    /// Ends an interaction on the person's say-so, and says whether the pill
    /// stays.
    ///
    /// `keep` is a workspace to go back to. Without one the pill goes away,
    /// which is the road off the screen every other transition lacks:
    /// everything the machine finishes on its own settles into resting instead.
    /// With one it stays, because the panels the person was reading are not
    /// what they cancelled.
    fn close(&mut self, keep: bool, actions: Vec<Action>) -> Vec<Action> {
        self.phase = Phase::Resting;
        if !keep {
            self.dismissed = true;
        }
        actions
    }

    /// Ends an interaction without touching where the pill lives.
    fn settle(&mut self, actions: Vec<Action>) -> Vec<Action> {
        self.phase = Phase::Resting;
        actions
    }

    /// Returns what the pill must render right now.
    pub fn presentation(&self) -> Presentation {
        if let Some(reason) = &self.blind {
            // Nothing is on screen to read this, and that is the point: the
            // tray renders the same presentation and is what is left.
            return Presentation {
                state: "error",
                text: String::new(),
                detail: reason.clone(),
                editable: false,
                recording: false,
            };
        }
        match &self.phase {
            Phase::Resting => Presentation {
                state: self.resting_state(),
                text: String::new(),
                detail: self.resting_detail(),
                editable: false,
                recording: false,
            },
            Phase::Listening => Presentation {
                state: "listening",
                text: String::new(),
                detail: String::new(),
                editable: false,
                recording: true,
            },
            Phase::Transcribing => Presentation {
                state: "transcribing",
                text: String::new(),
                detail: String::new(),
                editable: false,
                recording: false,
            },
            Phase::Editing {
                transcript, notice, ..
            } => Presentation {
                state: "editing",
                text: transcript.clone(),
                detail: notice.clone(),
                editable: true,
                recording: false,
            },
            Phase::Sent { transcript, .. } => Presentation {
                state: "sent",
                text: transcript.clone(),
                detail: String::new(),
                editable: false,
                recording: false,
            },
            Phase::Retained {
                transcript,
                reason,
                delivery,
                ..
            } => Presentation {
                // Two different things, and the pill must not call them the
                // same: one was never sent, and one may already have run.
                state: match delivery {
                    Delivery::Refused => "retained",
                    Delivery::Uncertain => "uncertain",
                },
                text: transcript.clone(),
                detail: reason.clone(),
                editable: *delivery == Delivery::Refused,
                recording: false,
            },
            Phase::Failed { reason } => Presentation {
                state: "error",
                text: String::new(),
                detail: reason.clone(),
                editable: false,
                recording: false,
            },
        }
    }

    /// Returns the tray state and its own detail for the current companion.
    pub fn tray_presentation(&self) -> (&'static str, String) {
        if let Some(reason) = &self.blind {
            return ("error", reason.clone());
        }
        match &self.phase {
            Phase::Listening => return ("listening", String::new()),
            Phase::Transcribing => return ("transcribing", String::new()),
            Phase::Failed { reason } => return ("error", reason.clone()),
            // An accepted transcript the service has not taken needs the user,
            // whatever the service itself is doing.
            Phase::Retained { reason, .. } => return ("attention", reason.clone()),
            _ => {}
        }
        let notice = self
            .notices
            .values()
            .find(|(state, _)| *state == NoticeState::Error)
            .or_else(|| self.notices.values().next());
        if let Some((state, detail)) = notice {
            let state = match state {
                NoticeState::Error => "error",
                NoticeState::Attention => "attention",
                NoticeState::Clear => unreachable!("clear notices are not retained"),
            };
            return (state, detail.clone());
        }
        (self.resting_state(), self.resting_detail())
    }

    /// Returns the tray state name for the current companion and service state.
    #[cfg(test)]
    pub fn tray_state(&self) -> &'static str {
        self.tray_presentation().0
    }

    fn resting_state(&self) -> &'static str {
        if !self.connected {
            return "disconnected";
        }
        self.shown_assistant().name()
    }

    fn resting_detail(&self) -> String {
        if !self.connected {
            return "The Scufris service is unavailable.".into();
        }
        self.assistant_detail.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_names_are_stable_and_free_of_transcripts() {
        assert_eq!(Phase::Resting.name(), "resting");
        assert_eq!(Phase::Listening.name(), "listening");
        assert_eq!(
            Phase::Editing {
                transcript: "secret words".into(),
                id: "pill-1".into(),
                notice: String::new(),
            }
            .name(),
            "editing"
        );
        let mut companion = Companion::new("pill");
        assert_eq!(companion.phase_name(), "resting");
        companion.set_connected(true);
        companion.apply(Event::Activate);
        assert_eq!(companion.phase_name(), "listening");
    }

    fn opened() -> Companion {
        let mut companion = Companion::new("pill");
        companion.set_connected(true);
        assert_eq!(
            companion.apply(Event::Activate),
            vec![Action::StartRecording]
        );
        assert!(companion.on_screen());
        companion
    }

    /// A companion whose recording is transcribed and waiting in the textbox.
    ///
    /// Two presses to get here, and that is the whole interaction now: the
    /// activation key opens the microphone and the same key closes it, and
    /// what was said lands in a window the person reads before Enter sends it.
    fn drafted(words: &str) -> Companion {
        let mut companion = opened();
        assert_eq!(
            companion.apply(Event::Activate),
            vec![Action::StopRecording]
        );
        assert_eq!(
            companion.apply(Event::Transcribed(words.into())),
            vec![persist("pill-1", words)]
        );
        companion
    }

    /// A companion whose submission was just acknowledged: resting, up.
    fn handed_off() -> Companion {
        let mut companion = drafted("open the tasks widget");
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Acknowledged("pill-1".into()));
        assert_eq!(companion.phase(), &Phase::Resting);
        companion
    }

    /// The pill is an indicator and nothing else. Every phase that wants the
    /// keyboard wants it for words, and words live in the textbox.
    #[test]
    fn only_the_phases_with_words_in_them_ask_for_the_keyboard() {
        let mut companion = opened();
        assert_eq!(companion.posture(), Posture::Watched);
        assert!(!companion.posture().textbox());
        companion.apply(Event::Activate);
        assert_eq!(companion.posture(), Posture::Watched);
        companion.apply(Event::Transcribed("draft".into()));
        assert_eq!(companion.posture(), Posture::Editing);
        assert!(companion.posture().textbox());
        // Handed over: the words are the service's, so the keyboard is the
        // person's again while the pill goes on reporting.
        companion.apply(Event::Enter { text: None });
        assert_eq!(companion.posture(), Posture::Passive);
        assert!(!companion.posture().textbox());
        assert!(!companion.posture().waits());
        // And a refusal brings the words back, so it brings the textbox back.
        companion.apply(refused("pill-1", "offline"));
        assert_eq!(companion.posture(), Posture::Editing);
    }

    #[test]
    fn the_pill_rests_on_screen_through_and_after_a_turn() {
        let mut companion = handed_off();
        // The service picks the turn up: the pill reports it, passively.
        companion.set_assistant(Assistant::Working, String::new());
        assert_eq!(companion.posture(), Posture::Passive);
        assert_eq!(companion.presentation().state, "working");
        companion.set_assistant(Assistant::Speaking, String::new());
        assert_eq!(companion.posture(), Posture::Passive);
        assert_eq!(companion.presentation().state, "speaking");
        // The pill is a resident HUD: an idle assistant is something to show,
        // not a reason to leave.
        companion.set_assistant(Assistant::Idle, String::new());
        assert_eq!(companion.posture(), Posture::Passive);
        assert_eq!(companion.presentation().state, "idle");
        // A turn started somewhere else is shown like any other.
        companion.set_assistant(Assistant::Working, String::new());
        assert_eq!(companion.posture(), Posture::Passive);
        assert_eq!(companion.presentation().state, "working");
    }

    /// Stop belongs to the assistant, not to the phase. It ends the run and
    /// leaves whatever the person was in the middle of exactly where it was.
    #[test]
    fn stopping_a_run_ends_it_and_leaves_the_words_alone() {
        let mut companion = drafted("open the tasks widget");
        companion.set_assistant(Assistant::Working, String::new());
        assert_eq!(
            companion.apply(Event::Stop),
            vec![Action::Abort {
                id: "pill-2".into()
            }],
            "the run was not stopped"
        );
        // An identifier of its own, so the service's answer to the stop can
        // never be read as the answer to the words still on screen.
        assert_eq!(
            companion.phase(),
            &Phase::Editing {
                transcript: "open the tasks widget".into(),
                id: "pill-1".into(),
                notice: String::new(),
            }
        );
        assert_eq!(companion.posture(), Posture::Editing);
    }

    /// Nothing to stop is not a mistake worth reporting. Whatever the person
    /// meant to stop - the speaker, or a run that settled while they reached
    /// for the key - has stopped.
    #[test]
    fn stopping_a_settled_assistant_asks_the_service_for_nothing() {
        let mut companion = handed_off();
        for state in [
            Assistant::Starting,
            Assistant::Idle,
            Assistant::Detached,
            Assistant::Error,
        ] {
            companion.set_assistant(state, String::new());
            assert_eq!(
                companion.apply(Event::Stop),
                Vec::new(),
                "{state:?} was taken for a run"
            );
        }
        assert!(companion.on_screen(), "stopping put the pill away");
    }

    #[test]
    fn assistant_activity_never_raises_a_dismissed_pill() {
        let mut companion = Companion::new("pill");
        companion.set_connected(true);
        companion.set_assistant(Assistant::Working, String::new());
        assert_eq!(companion.posture(), Posture::Off);
        companion.set_assistant(Assistant::Error, "the agent stopped".into());
        assert_eq!(companion.posture(), Posture::Off);
    }

    #[test]
    fn only_the_persons_escape_dismisses_the_resting_pill() {
        let mut companion = handed_off();
        companion.set_assistant(Assistant::Working, String::new());
        assert_eq!(companion.posture(), Posture::Passive);
        // The socket closing is something the resident pill reports, not a
        // reason for it to leave.
        companion.set_connected(false);
        assert_eq!(companion.posture(), Posture::Passive);
        assert_eq!(companion.presentation().state, "disconnected");
        companion.set_connected(true);
        companion.set_assistant(Assistant::Working, String::new());
        assert_eq!(companion.posture(), Posture::Passive);

        // Escape is the one road off the screen, and the next activation is
        // the one road back.
        companion.apply(Event::Activate);
        assert_eq!(companion.posture(), Posture::Watched);
        companion.apply(Event::Escape);
        assert_eq!(companion.posture(), Posture::Off);
        companion.set_assistant(Assistant::Speaking, String::new());
        assert_eq!(
            companion.posture(),
            Posture::Off,
            "a dismissed pill stays dismissed whatever the assistant does"
        );
        companion.apply(Event::Activate);
        assert_eq!(companion.posture(), Posture::Watched);
    }

    fn persist(id: &str, text: &str) -> Action {
        Action::PersistPending {
            id: id.into(),
            text: text.into(),
        }
    }

    fn submit(id: &str, text: &str) -> Action {
        Action::Submit {
            id: id.into(),
            text: text.into(),
        }
    }

    fn refused(id: &str, reason: &str) -> Event {
        Event::SubmissionFailed {
            id: id.into(),
            reason: reason.into(),
        }
    }

    fn uncertain(id: &str, reason: &str) -> Event {
        Event::SubmissionUncertain {
            id: id.into(),
            reason: reason.into(),
        }
    }

    /// Every voice submission passes through the textbox. There is no key that
    /// sends a transcript nobody has read: the second activation stops the
    /// microphone, the words arrive in a window, and Enter is what sends them.
    #[test]
    fn a_transcript_is_read_in_the_textbox_before_anything_is_sent() {
        let mut companion = opened();
        assert_eq!(
            companion.apply(Event::Activate),
            vec![Action::StopRecording]
        );
        assert_eq!(
            companion.apply(Event::Transcribed("  open the tasks widget \n".into())),
            vec![persist("pill-1", "open the tasks widget")],
            "the words were sent without the person seeing them"
        );
        let presentation = companion.presentation();
        assert_eq!(presentation.state, "editing");
        assert!(presentation.editable);
        assert_eq!(presentation.text, "open the tasks widget");
        assert_eq!(companion.posture(), Posture::Editing);

        // The person corrects them and sends.
        assert_eq!(
            companion.apply(Event::Enter {
                text: Some("open the tasks panel".into())
            }),
            vec![
                persist("pill-1", "open the tasks panel"),
                submit("pill-1", "open the tasks panel"),
            ]
        );
        assert_eq!(
            companion.posture(),
            Posture::Passive,
            "the keyboard was not given back"
        );
        // The desktop is already back; the acknowledgment only retires the
        // durable copy.
        assert_eq!(
            companion.apply(Event::Acknowledged("pill-1".into())),
            vec![Action::ClearPending]
        );
        assert_eq!(companion.phase(), &Phase::Resting);
    }

    #[test]
    fn escape_discards_the_recording_and_the_draft_and_closes_the_pill() {
        let mut companion = opened();
        assert_eq!(
            companion.apply(Event::Escape),
            vec![Action::DiscardRecording]
        );
        assert!(!companion.on_screen(), "the pill stayed up after a cancel");

        let mut companion = drafted("draft");
        // Discarding a drafted transcript is explicit, so the durable copy goes
        // with it under the stronger discard rules.
        assert_eq!(
            companion.apply(Event::Escape),
            vec![Action::DiscardPending {
                id: "pill-1".into()
            }]
        );
        assert!(!companion.on_screen());
        assert!(!companion.holds_unsent_transcript());
    }

    #[test]
    fn failed_transcription_submits_nothing_and_reports_the_reason() {
        let mut companion = opened();
        companion.apply(Event::Activate);
        assert_eq!(
            companion.apply(Event::TranscriptionFailed("Whisper is unreachable.".into())),
            Vec::new()
        );
        let presentation = companion.presentation();
        assert_eq!(presentation.state, "error");
        assert_eq!(presentation.detail, "Whisper is unreachable.");
        assert!(!companion.holds_unsent_transcript());
        assert_eq!(
            companion.apply(Event::Activate),
            vec![Action::StartRecording]
        );
    }

    #[test]
    fn a_microphone_that_never_starts_leaves_no_recording_indicator() {
        let mut companion = opened();
        assert_eq!(companion.presentation().state, "listening");
        assert_eq!(
            companion.apply(Event::RecordingFailed("no microphone is available".into())),
            vec![Action::DiscardRecording]
        );
        let presentation = companion.presentation();
        assert_eq!(presentation.state, "error");
        assert!(!presentation.recording);
        assert_eq!(presentation.detail, "no microphone is available");
        assert_eq!(companion.tray_state(), "error");
    }

    #[test]
    fn a_capture_stream_that_fails_mid_recording_reports_the_same_way() {
        let mut companion = opened();
        companion.apply(Event::Activate);
        assert_eq!(companion.presentation().state, "transcribing");
        assert_eq!(
            companion.apply(Event::RecordingFailed("microphone capture failed".into())),
            vec![Action::CancelTranscription]
        );
        assert_eq!(companion.presentation().state, "error");
        assert!(!companion.holds_unsent_transcript());
    }

    #[test]
    fn the_desktop_comes_back_when_the_words_are_handed_off() {
        let mut companion = drafted("open the tasks widget");
        // Handing the words to the service is where the keyboard comes back. It
        // must not be held until an acknowledgment that can take a whole turn
        // to arrive, or never arrive at all. The pill itself stays, passive,
        // to report the turn it started.
        assert_eq!(
            companion.apply(Event::Enter { text: None }),
            vec![
                persist("pill-1", "open the tasks widget"),
                submit("pill-1", "open the tasks widget"),
            ]
        );
        assert_eq!(companion.posture(), Posture::Passive);

        // Nothing gives it the keyboard while the submission is outstanding.
        assert_eq!(companion.apply(Event::Copy), Vec::new());
        assert_eq!(companion.posture(), Posture::Passive);
        // The acknowledgment retires the durable copy. The pill stays, at
        // rest, until the person dismisses it.
        assert_eq!(
            companion.apply(Event::Acknowledged("pill-1".into())),
            vec![Action::ClearPending]
        );
        assert_eq!(companion.posture(), Posture::Passive);
    }

    #[test]
    fn a_handed_off_transcript_brings_the_textbox_back_when_it_goes_wrong() {
        for (event, expected) in [
            (refused("pill-1", "The backend is unavailable."), "retained"),
            (
                uncertain("pill-1", "The backend did not confirm delivery."),
                "uncertain",
            ),
        ] {
            let mut companion = drafted("book the flight");
            companion.apply(Event::Enter { text: None });
            assert_eq!(companion.posture(), Posture::Passive);
            assert_eq!(companion.apply(event), Vec::new());
            assert_eq!(
                companion.posture(),
                Posture::Editing,
                "the textbox did not take the keyboard back for {expected}"
            );
            assert_eq!(companion.presentation().state, expected);
            assert_eq!(companion.presentation().text, "book the flight");
        }
    }

    #[test]
    fn an_undeliverable_transcript_is_retained_and_resubmittable() {
        let mut companion = drafted("remember the milk");
        companion.apply(Event::Enter { text: None });
        // The pill went passive at the handoff, so a refusal has to bring the
        // textbox back to say so.
        assert_eq!(
            companion.apply(refused("pill-1", "The backend is unavailable.")),
            Vec::new()
        );
        assert!(companion.on_screen());
        let presentation = companion.presentation();
        assert_eq!(presentation.state, "retained");
        assert_eq!(presentation.text, "remember the milk");
        // The send was refused outright, so the service never saw it and the
        // text is still safe to change.
        assert!(presentation.editable);
        assert!(companion.holds_unsent_transcript());
        assert_eq!(
            companion.apply(Event::Enter { text: None }),
            vec![
                persist("pill-1", "remember the milk"),
                submit("pill-1", "remember the milk"),
            ]
        );
        assert_eq!(companion.posture(), Posture::Passive);
    }

    /// The pill is the recording privacy indicator and the textbox is the only
    /// place a transcript can be read or corrected. A phase that cannot have a
    /// window has nothing left to offer, so it stops rather than carrying on
    /// unseen.
    #[test]
    fn a_pill_that_cannot_be_opened_stops_the_recording_and_says_so_on_the_tray() {
        let mut companion = opened();
        assert_eq!(
            companion.abandon("the pill did not come up".into()),
            vec![Action::DiscardRecording]
        );
        assert!(!companion.on_screen());
        assert!(companion.blinded());
        // The tray is the only thing left that can say anything at all.
        assert_eq!(companion.tray_state(), "error");
        let presentation = companion.presentation();
        assert_eq!(presentation.state, "error");
        assert_eq!(presentation.detail, "the pill did not come up");
        assert!(!presentation.recording);

        // Nothing here tries the window again by itself. The person asking for
        // the pill is what does, and it starts from the beginning.
        assert_eq!(
            companion.apply(Event::Activate),
            vec![Action::StartRecording]
        );
        assert!(!companion.blinded());
        assert!(companion.on_screen());
        assert_eq!(companion.presentation().state, "listening");
    }

    #[test]
    fn abandoning_an_accepted_transcript_never_throws_it_away() {
        let mut companion = drafted("remember the milk");
        companion.apply(Event::Enter { text: None });
        companion.apply(refused("pill-1", "The backend is unavailable."));
        assert!(companion.on_screen());

        // The transcript reached durable storage before anything was sent, so
        // giving up the window leaves it exactly where the next start finds it.
        assert_eq!(
            companion.abandon("the pill did not come up".into()),
            Vec::new()
        );
        assert!(!companion.holds_unsent_transcript());
        assert!(!companion.on_screen());
        assert_eq!(companion.tray_state(), "error");
    }

    #[test]
    fn abandoning_during_transcription_cancels_it() {
        let mut companion = opened();
        companion.apply(Event::Activate);
        assert_eq!(
            companion.abandon("the pill did not come up".into()),
            vec![Action::CancelTranscription]
        );
        assert!(!companion.on_screen());
    }

    #[test]
    fn an_uncertain_transcript_is_never_resent_without_the_person_saying_so() {
        let mut companion = drafted("book the flight");
        companion.apply(Event::Enter { text: None });
        // The submission left the companion and was never acknowledged, so it
        // may already be in the conversation and may already have run.
        companion.apply(uncertain("pill-1", "The backend did not confirm delivery."));

        let presentation = companion.presentation();
        assert_eq!(presentation.state, "uncertain");
        assert!(
            !presentation.editable,
            "an uncertain transcript must not be editable"
        );
        assert!(
            presentation.detail.contains("Escape to discard"),
            "the choices must be on offer: {}",
            presentation.detail
        );

        // The ordinary key says what is at stake and sends nothing at all.
        assert_eq!(
            companion.apply(Event::Enter {
                text: Some("cancel the flight".into())
            }),
            Vec::new(),
            "an ordinary Enter resent a request that may already have run"
        );
        assert!(
            companion
                .presentation()
                .detail
                .contains("could repeat what it did"),
            "the person was not told what sending again risks"
        );

        // Copying is always safe, and never sends anything.
        assert_eq!(
            companion.apply(Event::Copy),
            vec![Action::CopyTranscript {
                text: "book the flight".into()
            }]
        );

        // The second Enter is the person's own decision, and carries it.
        assert_eq!(
            companion.apply(Event::Enter {
                text: Some("cancel the flight".into())
            }),
            vec![
                persist("pill-1", "book the flight"),
                submit("pill-1", "book the flight"),
            ],
            "the forced send must carry the accepted words, never an edit"
        );
        assert_eq!(companion.posture(), Posture::Passive);
    }

    #[test]
    fn an_answer_for_another_submission_never_settles_the_current_one() {
        let mut companion = drafted("book the flight");
        companion.apply(Event::Enter { text: None });
        // The first submission is answered late, after the person recorded
        // again: concurrent dispatch and forced sends make that ordinary.
        companion.apply(Event::Acknowledged("pill-1".into()));
        companion.apply(Event::Activate);
        companion.apply(Event::Activate);
        companion.apply(Event::Transcribed("read my mail".into()));
        companion.apply(Event::Enter { text: None });
        assert!(companion.awaiting("pill-2"));

        assert_eq!(
            companion.apply(uncertain("pill-1", "The backend did not confirm delivery.")),
            Vec::new()
        );
        assert_eq!(
            companion.presentation().state,
            "sent",
            "a stale uncertainty froze the transcript that replaced it"
        );
        assert!(companion.awaiting("pill-2"));

        assert_eq!(companion.apply(refused("pill-1", "offline")), Vec::new());
        assert_eq!(
            companion.presentation().state,
            "sent",
            "a stale refusal reopened the transcript that replaced it"
        );

        // Its own answer still lands.
        companion.apply(uncertain("pill-2", "The backend did not confirm delivery."));
        assert_eq!(companion.presentation().state, "uncertain");
    }

    #[test]
    fn a_refused_send_says_so_and_keeps_the_words_editable() {
        let mut companion = drafted("remember the milk");
        companion.apply(Event::Enter { text: None });
        // The service refused before anything left it, and said which
        // submission it was refusing.
        assert_eq!(
            companion.apply(refused(
                "pill-1",
                "submission pill-1 was not sent: no session"
            )),
            Vec::new()
        );
        assert!(companion.on_screen());

        let presentation = companion.presentation();
        assert_eq!(presentation.state, "retained");
        assert!(
            presentation.editable,
            "words the conversation never saw must stay editable"
        );
        assert!(!presentation.detail.contains("could repeat"));
        // And one Enter sends them, with no warning to acknowledge first.
        assert_eq!(
            companion.apply(Event::Enter { text: None }),
            vec![
                persist("pill-1", "remember the milk"),
                submit("pill-1", "remember the milk"),
            ]
        );
        assert_eq!(companion.posture(), Posture::Passive);
    }

    #[test]
    fn a_refusal_of_a_forced_send_does_not_forget_the_earlier_uncertainty() {
        let mut companion = drafted("book the flight");
        companion.apply(Event::Enter { text: None });
        companion.apply(uncertain("pill-1", "no confirmation"));
        // The person decided to send it anyway, and this attempt was refused.
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Enter { text: None });
        assert_eq!(companion.presentation().state, "sent");
        companion.apply(refused(
            "pill-1",
            "submission pill-1 was not sent: no session",
        ));

        let presentation = companion.presentation();
        assert_eq!(
            presentation.state, "uncertain",
            "a refused retry made words that may already have run editable again"
        );
        assert!(!presentation.editable);
        assert!(presentation.detail.contains("Escape to discard"));
        // The warning is owed again before anything is sent again.
        assert_eq!(companion.apply(Event::Enter { text: None }), Vec::new());
        assert_eq!(
            companion.apply(Event::Enter { text: None }),
            vec![
                persist("pill-1", "book the flight"),
                submit("pill-1", "book the flight"),
            ]
        );
        assert_eq!(companion.posture(), Posture::Passive);
    }

    #[test]
    fn a_failed_durable_save_stops_the_submission_and_keeps_the_text() {
        let mut companion = drafted("do not lose me");
        companion.apply(Event::Enter { text: None });
        // The runtime stops before Submit when the save fails, so this send
        // never happened, and the phase it lands in is one the person sees.
        assert_eq!(
            companion.apply(Event::PersistFailed("the disk is full".into())),
            Vec::new()
        );
        assert!(companion.on_screen());

        let presentation = companion.presentation();
        assert_eq!(presentation.state, "retained");
        assert_eq!(presentation.text, "do not lose me");
        assert_eq!(presentation.detail, "the disk is full");
        assert!(
            presentation.editable,
            "nothing was delivered, so the text stays editable"
        );
        assert!(companion.holds_unsent_transcript());
    }

    #[test]
    fn a_failed_durable_save_while_editing_is_visible_without_losing_the_draft() {
        let mut companion = drafted("draft text");
        companion.apply(Event::PersistFailed("the disk is full".into()));

        let presentation = companion.presentation();
        assert_eq!(presentation.state, "editing");
        assert_eq!(presentation.text, "draft text");
        assert_eq!(presentation.detail, "the disk is full");
        assert!(presentation.editable);
    }

    #[test]
    fn a_discard_that_cannot_happen_reopens_the_pill_on_the_reason() {
        let mut companion = Companion::new("pill");
        companion.set_connected(true);
        assert_eq!(
            companion.apply(Event::DiscardFailed(
                "The discarded transcript is still on disk.".into()
            )),
            Vec::new()
        );
        assert!(companion.on_screen());
        let presentation = companion.presentation();
        assert_eq!(presentation.state, "error");
        assert_eq!(
            presentation.detail,
            "The discarded transcript is still on disk."
        );
        assert_eq!(presentation.text, "", "discarded words must not be shown");
    }

    #[test]
    fn a_late_acknowledgment_retires_a_retained_transcript() {
        let mut companion = drafted("open the tasks widget");
        companion.apply(Event::Enter { text: None });
        companion.apply(uncertain("pill-1", "no confirmation"));
        assert_eq!(companion.presentation().state, "uncertain");
        // The service confirms after the companion gave up waiting. The
        // transcript is retired; the pill settles back to resting on screen.
        assert_eq!(
            companion.apply(Event::Acknowledged("pill-1".into())),
            vec![Action::ClearPending]
        );
        assert_eq!(companion.posture(), Posture::Passive);
        assert!(!companion.holds_unsent_transcript());
    }

    #[test]
    fn a_startup_storage_failure_is_visible_rather_than_silent() {
        let mut companion = Companion::new("pill");
        companion.set_connected(true);
        companion.report_store_failure("the saved transcript is unreadable".into());
        assert!(companion.on_screen());
        let presentation = companion.presentation();
        assert_eq!(presentation.state, "error");
        assert_eq!(presentation.detail, "the saved transcript is unreadable");
        assert_eq!(companion.tray_state(), "error");
    }

    #[test]
    fn a_transcript_keeps_one_identifier_across_edits_and_retries() {
        let mut companion = drafted("first draft");
        companion.apply(Event::Enter {
            text: Some("second draft".into()),
        });
        // A refused send never reached the service, so a further edit is safe.
        companion.apply(refused("pill-1", "offline"));
        assert_eq!(
            companion.apply(Event::Enter {
                text: Some("third draft".into())
            }),
            vec![
                persist("pill-1", "third draft"),
                submit("pill-1", "third draft"),
            ]
        );
        companion.apply(Event::Acknowledged("pill-1".into()));

        // Only a genuinely new recording takes a new identifier.
        companion.apply(Event::Activate);
        companion.apply(Event::Activate);
        assert_eq!(
            companion.apply(Event::Transcribed("a new request".into())),
            vec![persist("pill-2", "a new request")]
        );
    }

    #[test]
    fn activation_never_discards_an_unsent_transcript() {
        let mut companion = drafted("keep me");
        companion.apply(Event::Enter { text: None });
        companion.apply(refused("pill-1", "offline"));
        assert_eq!(companion.apply(Event::Activate), Vec::new());
        assert!(companion.holds_unsent_transcript());
    }

    #[test]
    fn a_restored_transcript_reopens_the_textbox_and_keeps_its_identifier() {
        let mut companion = Companion::new("later");
        companion.set_connected(true);
        companion.restore(Pending {
            id: "pill-7".into(),
            text: "survived the crash".into(),
        });
        assert_eq!(companion.posture(), Posture::Editing);
        let presentation = companion.presentation();
        assert_eq!(presentation.state, "uncertain");
        assert_eq!(presentation.text, "survived the crash");
        assert!(presentation.detail.starts_with(RESTORED_REASON));
        // The previous process may have delivered it, so it cannot be edited,
        // and nothing sends it again until the person says to.
        assert!(!presentation.editable);
        assert_eq!(companion.apply(Event::Enter { text: None }), Vec::new());
        assert_eq!(
            companion.apply(Event::Enter { text: None }),
            vec![
                persist("pill-7", "survived the crash"),
                submit("pill-7", "survived the crash"),
            ]
        );
        // A new recording in this process cannot reuse the recovered identifier.
        companion.apply(Event::Acknowledged("pill-7".into()));
        companion.apply(Event::Activate);
        companion.apply(Event::Activate);
        assert_eq!(
            companion.apply(Event::Transcribed("fresh".into())),
            vec![persist("later-1", "fresh")]
        );
    }

    #[test]
    fn restoring_never_overwrites_live_pill_state() {
        let mut companion = opened();
        companion.restore(Pending {
            id: "pill-7".into(),
            text: "stale".into(),
        });
        assert_eq!(companion.presentation().state, "listening");
    }

    #[test]
    fn a_stale_acknowledgment_does_not_close_a_newer_submission() {
        let mut companion = drafted("first");
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Acknowledged("pill-1".into()));
        companion.apply(Event::Activate);
        companion.apply(Event::Activate);
        companion.apply(Event::Transcribed("second".into()));
        companion.apply(Event::Enter { text: None });
        assert!(companion.awaiting("pill-2"));
        assert!(!companion.awaiting("pill-1"));
        assert_eq!(
            companion.apply(Event::Acknowledged("pill-1".into())),
            Vec::new()
        );
        assert!(companion.holds_unsent_transcript());
        assert_eq!(
            companion.apply(Event::Acknowledged("pill-2".into())),
            vec![Action::ClearPending]
        );
    }

    #[test]
    fn tray_state_distinguishes_recording_work_attention_and_backend_failure() {
        let mut companion = Companion::new("pill");
        assert_eq!(companion.tray_state(), "disconnected");
        companion.set_connected(true);
        assert_eq!(companion.tray_state(), "idle");
        companion.set_assistant(Assistant::Working, String::new());
        assert_eq!(companion.tray_state(), "working");
        companion.set_assistant(Assistant::Detached, "a terminal has the session".into());
        assert_eq!(companion.tray_state(), "detached");
        companion.set_assistant(Assistant::Speaking, String::new());
        assert_eq!(companion.tray_state(), "speaking");
        companion.apply(Event::Activate);
        assert_eq!(companion.tray_state(), "listening");
        companion.apply(Event::Activate);
        assert_eq!(companion.tray_state(), "transcribing");
        companion.apply(Event::Escape);
        companion.set_connected(false);
        assert_eq!(companion.tray_state(), "disconnected");
        assert_eq!(
            companion.presentation().detail,
            "The Scufris service is unavailable."
        );
    }

    #[test]
    fn ambient_notices_override_the_assistant_and_clear_independently() {
        let mut companion = Companion::new("pill");
        companion.set_connected(true);
        companion.set_assistant(Assistant::Working, "packing".into());
        companion.set_notice(
            "job-one".into(),
            NoticeState::Attention,
            "Job job-one is blocked".into(),
        );
        assert_eq!(
            companion.tray_presentation(),
            ("attention", "Job job-one is blocked".into())
        );

        companion.set_notice(
            "job-two".into(),
            NoticeState::Error,
            "Job job-two failed".into(),
        );
        assert_eq!(
            companion.tray_presentation(),
            ("error", "Job job-two failed".into())
        );
        companion.set_notice("job-two".into(), NoticeState::Clear, String::new());
        assert_eq!(companion.tray_state(), "attention");
        companion.set_notice("job-one".into(), NoticeState::Clear, String::new());
        assert_eq!(companion.tray_presentation(), ("working", "packing".into()));

        // Welcome starts a complete replay, so a notice cleared while this
        // frontend was away cannot survive by being absent from the replay.
        companion.set_notice(
            "stale".into(),
            NoticeState::Attention,
            "stale detail".into(),
        );
        companion.set_connected(false);
        companion.set_connected(true);
        assert_eq!(companion.tray_state(), "idle");
    }

    #[test]
    fn an_empty_transcript_is_an_error_rather_than_an_empty_submission() {
        let mut companion = opened();
        companion.apply(Event::Activate);
        assert_eq!(
            companion.apply(Event::Transcribed("   ".into())),
            Vec::new()
        );
        assert_eq!(companion.presentation().state, "error");
    }

    #[test]
    fn escape_puts_a_resting_pill_away_and_an_activation_brings_it_back() {
        let mut companion = handed_off();
        assert_eq!(companion.posture(), Posture::Passive);
        // Nothing to cancel, so nothing to do but go down. The pill holds no
        // keys, so this Escape is the accelerator arriving from outside it.
        assert_eq!(companion.apply(Event::Escape), Vec::new());
        assert_eq!(companion.posture(), Posture::Off);
        assert_eq!(companion.phase(), &Phase::Resting);
        companion.apply(Event::Activate);
        assert_eq!(companion.posture(), Posture::Watched);
    }

    #[test]
    fn escape_while_a_submission_is_in_flight_hides_the_pill_and_keeps_waiting() {
        let mut companion = drafted("open the tasks widget");
        companion.apply(Event::Enter { text: None });
        let Phase::Sent { id, .. } = companion.phase().clone() else {
            panic!("the transcript was handed over: {:?}", companion.phase());
        };
        assert_eq!(companion.apply(Event::Escape), Vec::new());
        assert_eq!(companion.posture(), Posture::Off);
        // The words are still waiting on their acknowledgment, and it still
        // lands. Putting the window away is not abandoning the submission.
        assert_eq!(
            companion.apply(Event::Acknowledged(id)),
            vec![Action::ClearPending]
        );
        assert_eq!(companion.phase(), &Phase::Resting);
    }

    #[test]
    fn escape_during_transcription_cancels_the_pending_transcript() {
        let mut companion = opened();
        companion.apply(Event::Activate);
        assert_eq!(
            companion.apply(Event::Escape),
            vec![Action::CancelTranscription]
        );
        assert!(!companion.on_screen());
    }

    #[test]
    fn reveal_brings_a_dismissed_pill_back_without_opening_the_microphone() {
        let mut companion = handed_off();
        companion.apply(Event::Escape);
        assert_eq!(companion.posture(), Posture::Off);
        assert_eq!(companion.apply(Event::Reveal), Vec::new());
        assert_eq!(companion.posture(), Posture::Passive);
        assert_eq!(companion.phase(), &Phase::Resting);
    }

    #[test]
    fn dismiss_puts_a_resting_pill_away_and_leaves_a_working_one_alone() {
        let mut companion = handed_off();
        assert_eq!(companion.posture(), Posture::Passive);
        assert_eq!(companion.apply(Event::Dismiss), Vec::new());
        assert_eq!(companion.posture(), Posture::Off);

        // A phase holding the person's words answers neither gesture. A tap
        // that threw away a draft would be a tap nobody dares make.
        let mut companion = drafted("open the tasks widget");
        assert_eq!(companion.apply(Event::Dismiss), Vec::new());
        assert_eq!(companion.posture(), Posture::Editing);
        assert_eq!(companion.apply(Event::Reveal), Vec::new());
        assert_eq!(companion.posture(), Posture::Editing);
    }

    #[test]
    fn cancel_ends_the_take_and_leaves_the_pill_standing() {
        let mut companion = opened();
        assert_eq!(
            companion.apply(Event::Cancel),
            vec![Action::DiscardRecording]
        );
        assert_eq!(companion.phase(), &Phase::Resting);
        assert_eq!(
            companion.posture(),
            Posture::Passive,
            "the take is gone and the workspace it was cancelled over is not"
        );

        // And the same throw-away out of the textbox.
        let mut companion = drafted("open the tasks widget");
        assert_eq!(
            companion.apply(Event::Cancel),
            vec![Action::DiscardPending {
                id: "pill-1".into()
            }]
        );
        assert_eq!(companion.posture(), Posture::Passive);
    }

    #[test]
    fn the_escape_ladder_ends_at_a_dismissal() {
        let mut companion = opened();
        // The host answers a real Escape with a cancel while there is a
        // workspace to go back to, and with an Escape once there is not.
        assert_eq!(
            companion.apply(Event::Cancel),
            vec![Action::DiscardRecording]
        );
        assert_eq!(companion.posture(), Posture::Passive);
        assert_eq!(companion.apply(Event::Escape), Vec::new());
        assert_eq!(companion.posture(), Posture::Off);
    }
}
