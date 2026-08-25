//! Companion-local pill state machine.
//!
//! The machine owns every decision the accepted interaction describes and
//! produces the actions the host must run. It holds no window, audio, socket, or
//! file handle so the whole interaction is testable without a desktop session.

use scufris_control::AssistantState;

use crate::pending::Pending;

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
    /// The pill is closed.
    Hidden,
    /// The microphone is recording.
    Listening,
    /// Recording stopped and local transcription is running.
    Transcribing {
        /// What happens once the transcript arrives.
        intent: Intent,
    },
    /// An editable transcript is waiting for the user.
    Reviewing {
        /// Current transcript text.
        transcript: String,
        /// Identifier this transcript keeps until it is acknowledged.
        id: String,
        /// Why the durable copy is missing, empty when it is present.
        notice: String,
    },
    /// A transcript was accepted and is on its way to the daemon.
    Sent {
        /// Exact text handed to the daemon.
        transcript: String,
        /// Submission identifier awaiting its acknowledgment.
        id: String,
        /// What was already known about these words before this send. A send
        /// the daemon refuses says nothing about an earlier one that may
        /// already be in the conversation, so this is what the transcript
        /// falls back to rather than becoming editable again.
        prior: Delivery,
    },
    /// An accepted transcript that the daemon did not take.
    Retained {
        /// Transcript kept in the pill so it is never lost.
        transcript: String,
        /// Identifier reused by every retry, so a retry cannot duplicate.
        id: String,
        /// Why the submission did not land.
        reason: String,
        /// Whether the daemon might already hold this transcript.
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
            Phase::Hidden => "hidden",
            Phase::Listening => "listening",
            Phase::Transcribing { .. } => "transcribing",
            Phase::Reviewing { .. } => "reviewing",
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

/// What the user asked for when recording stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// Transcribe and submit without another confirmation.
    Send,
    /// Transcribe and open the editable review state.
    Review,
}

/// One thing that happened to the companion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The activation hotkey fired, or the tray asked for voice input.
    Activate,
    /// Escape was pressed in the pill.
    Escape,
    /// Enter was pressed in the pill, carrying any edited text.
    Enter {
        /// Text currently in the editable field, when the pill has one.
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
    /// The daemon acknowledged a submission.
    Acknowledged(String),
    /// The submission never reached the daemon, or the daemon refused it
    /// before any of its words could leave.
    SubmissionFailed {
        /// Identifier of the submission this answers.
        id: String,
        /// Why it was not sent.
        reason: String,
    },
    /// The submission left the companion and its outcome is unknown.
    SubmissionUncertain {
        /// Identifier of the submission this answers.
        id: String,
        /// What the daemon said about the uncertainty.
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
    /// Forget the durable transcript after the daemon acknowledged it.
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
    /// Submit one accepted transcript to the daemon.
    Submit {
        /// Submission identifier the acknowledgment must echo.
        id: String,
        /// Accepted transcript text.
        text: String,
        /// The person's own decision to send words that may already be in the
        /// conversation. Never set by anything but that decision.
        force: bool,
    },
    /// Put one transcript on the clipboard so it is not lost when the pill closes.
    CopyTranscript {
        /// Exact text to copy.
        text: String,
    },
}

/// Everything the pill renders, derived from the phase and the daemon state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presentation {
    /// Stable state name used by the pill and the tray.
    pub state: &'static str,
    /// Text the pill shows, empty when there is none.
    pub text: String,
    /// Short explanation of an error or a retained transcript.
    pub detail: String,
    /// Whether the pill offers an editable field.
    pub editable: bool,
    /// Whether the pill shows the recording indicator and duration.
    pub recording: bool,
}

/// Answers whether one phase puts the pill on screen.
///
/// A transcript handed off for submission gives the desktop back at once: the
/// words are the daemon's now, and waiting for an acknowledgment would hold the
/// keyboard in an always-on-top pill for as long as the backend takes. The pill
/// comes back by itself if that submission is refused or its outcome turns out
/// to be unknown.
fn on_screen(phase: &Phase) -> bool {
    !matches!(phase, Phase::Hidden | Phase::Sent { .. })
}

/// The pill state machine.
#[derive(Debug)]
pub struct Companion {
    phase: Phase,
    assistant: AssistantState,
    assistant_detail: String,
    connected: bool,
    prefix: String,
    submissions: u64,
    /// Why the pill window is unusable, when the host could not put it where a
    /// phase needed it.
    ///
    /// Kept beside the phase rather than in it, because this is the state of
    /// having no window at all: nothing belongs on screen, nothing is left
    /// running, and the tray is the only thing that can still say so.
    blind: Option<String>,
}

impl Companion {
    /// Creates a closed pill with an unreachable daemon.
    ///
    /// `prefix` must be unique per process. Identifiers survive a restart, so a
    /// reused prefix could collide with an identifier the daemon already
    /// acknowledged and would then suppress a genuinely new submission.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            phase: Phase::Hidden,
            assistant: AssistantState::Idle,
            assistant_detail: String::new(),
            connected: false,
            prefix: prefix.into(),
            submissions: 0,
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

    /// Returns the assistant state last reported by the daemon.
    pub fn assistant(&self) -> AssistantState {
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
            Phase::Reviewing { .. } | Phase::Sent { .. } | Phase::Retained { .. }
        )
    }

    /// Answers whether the pill belongs on screen right now.
    ///
    /// The host reads this after every change instead of being handed show and
    /// hide actions. Two changes can be in flight at once - the person's key
    /// and the daemon's answer - and the one that ran last is the one whose
    /// window the person must end up looking at.
    pub fn on_screen(&self) -> bool {
        self.blind.is_none() && on_screen(&self.phase)
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
            Phase::Transcribing { .. } => vec![Action::CancelTranscription],
            _ => Vec::new(),
        };
        self.phase = Phase::Hidden;
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
        if !matches!(self.phase, Phase::Hidden) {
            return;
        }
        self.phase = Phase::Failed { reason };
    }

    /// Reopens the pill on a transcript recovered from a previous process.
    ///
    /// The recovered identifier is kept, so resending cannot duplicate a
    /// request the daemon already accepted.
    pub fn restore(&mut self, pending: Pending) {
        if !matches!(self.phase, Phase::Hidden) {
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

    /// Records whether the daemon connection is currently open.
    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
        if !connected {
            self.assistant = AssistantState::Idle;
            self.assistant_detail.clear();
        }
    }

    /// Returns true while the daemon connection is open.
    pub fn connected(&self) -> bool {
        self.connected
    }

    /// Records the assistant state the daemon reported.
    pub fn set_assistant(&mut self, state: AssistantState, detail: String) {
        self.assistant = state;
        self.assistant_detail = detail;
    }

    /// Applies one event and returns the actions the host must run.
    pub fn apply(&mut self, event: Event) -> Vec<Action> {
        // The runtime stopped trying the window when it could not open one.
        // The person asking for the pill again is what tries it again: nothing
        // else here knows whether the display has come back.
        if matches!(event, Event::Activate) {
            self.blind = None;
        }
        self.transition(event)
    }

    fn transition(&mut self, event: Event) -> Vec<Action> {
        match (&self.phase, event) {
            (Phase::Hidden, Event::Activate) => {
                self.phase = Phase::Listening;
                vec![Action::StartRecording]
            }
            (Phase::Failed { .. }, Event::Activate) => {
                self.phase = Phase::Listening;
                vec![Action::StartRecording]
            }
            (Phase::Listening, Event::Activate) => {
                self.phase = Phase::Transcribing {
                    intent: Intent::Review,
                };
                vec![Action::StopRecording]
            }
            (Phase::Listening, Event::Enter { .. }) => {
                self.phase = Phase::Transcribing {
                    intent: Intent::Send,
                };
                vec![Action::StopRecording]
            }
            (Phase::Listening, Event::Escape) => self.close(vec![Action::DiscardRecording]),
            (Phase::Transcribing { .. }, Event::Escape) => {
                self.close(vec![Action::CancelTranscription])
            }
            // The microphone can fail before capture starts and while it runs.
            // Either way the pill must stop claiming to record.
            (Phase::Listening, Event::RecordingFailed(reason)) => {
                self.phase = Phase::Failed { reason };
                vec![Action::DiscardRecording]
            }
            (Phase::Transcribing { .. }, Event::RecordingFailed(reason)) => {
                self.phase = Phase::Failed { reason };
                vec![Action::CancelTranscription]
            }
            (Phase::Transcribing { intent }, Event::Transcribed(transcript)) => {
                let intent = *intent;
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
                match intent {
                    Intent::Send => {
                        let mut actions = vec![persist];
                        actions.extend(self.submit(transcript, id, false));
                        actions
                    }
                    Intent::Review => {
                        self.phase = Phase::Reviewing {
                            transcript,
                            id,
                            notice: String::new(),
                        };
                        vec![persist]
                    }
                }
            }
            (Phase::Transcribing { .. }, Event::TranscriptionFailed(reason)) => {
                self.phase = Phase::Failed { reason };
                Vec::new()
            }
            (
                Phase::Reviewing { transcript, id, .. }
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
                Phase::Reviewing { transcript, .. } | Phase::Retained { transcript, .. },
                Event::Copy,
            ) => vec![Action::CopyTranscript {
                text: transcript.clone(),
            }],
            (Phase::Reviewing { id, .. } | Phase::Retained { id, .. }, Event::Escape) => {
                let id = id.clone();
                self.close(vec![Action::DiscardPending { id }])
            }
            (Phase::Failed { .. }, Event::Escape | Event::Enter { .. }) => self.close(Vec::new()),
            // The pill has already closed by the time this arrives, so it is
            // reopened: silently keeping discarded words would be worse.
            (Phase::Hidden, Event::DiscardFailed(reason)) => self.report_failure(reason),
            (
                Phase::Sent { id, .. } | Phase::Retained { id, .. },
                Event::Acknowledged(acknowledged),
            ) if *id == acknowledged => self.close(vec![Action::ClearPending]),
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
            (Phase::Reviewing { transcript, id, .. }, Event::PersistFailed(notice)) => {
                self.phase = Phase::Reviewing {
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
            _ => Vec::new(),
        }
    }

    fn next_id(&mut self) -> String {
        self.submissions += 1;
        format!("{}-{}", self.prefix, self.submissions)
    }

    fn submit(&mut self, transcript: String, id: String, force: bool) -> Vec<Action> {
        self.phase = Phase::Sent {
            transcript: transcript.clone(),
            id: id.clone(),
            // A forced send is the only send of words that may already be in
            // the conversation, so it is the only one with anything to fall
            // back to.
            prior: if force {
                Delivery::Uncertain
            } else {
                Delivery::Refused
            },
        };
        vec![Action::Submit {
            id,
            text: transcript,
            force,
        }]
    }

    fn close(&mut self, actions: Vec<Action>) -> Vec<Action> {
        self.phase = Phase::Hidden;
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
            Phase::Hidden => Presentation {
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
            Phase::Transcribing { .. } => Presentation {
                state: "transcribing",
                text: String::new(),
                detail: String::new(),
                editable: false,
                recording: false,
            },
            Phase::Reviewing {
                transcript, notice, ..
            } => Presentation {
                state: "review",
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

    /// Returns the tray state name for the current companion and daemon state.
    pub fn tray_state(&self) -> &'static str {
        if self.blind.is_some() {
            return "error";
        }
        match self.phase {
            Phase::Listening => "listening",
            Phase::Transcribing { .. } => "transcribing",
            Phase::Failed { .. } => "error",
            // An accepted transcript the daemon has not taken needs the user,
            // whatever the daemon itself is doing.
            Phase::Retained { .. } => "attention",
            _ => self.resting_state(),
        }
    }

    fn resting_state(&self) -> &'static str {
        if !self.connected {
            return "disconnected";
        }
        match self.assistant {
            AssistantState::Idle => "idle",
            AssistantState::Working => "working",
            AssistantState::Speaking => "speaking",
            AssistantState::Attention => "attention",
            AssistantState::Error => "error",
        }
    }

    fn resting_detail(&self) -> String {
        if !self.connected {
            return "The Scufris backend is unavailable.".into();
        }
        self.assistant_detail.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_names_are_stable_and_free_of_transcripts() {
        assert_eq!(Phase::Hidden.name(), "hidden");
        assert_eq!(Phase::Listening.name(), "listening");
        assert_eq!(
            Phase::Reviewing {
                transcript: "secret words".into(),
                id: "pill-1".into(),
                notice: String::new(),
            }
            .name(),
            "reviewing"
        );
        let mut companion = Companion::new("pill");
        assert_eq!(companion.phase_name(), "hidden");
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
            force: false,
        }
    }

    fn forced(id: &str, text: &str) -> Action {
        Action::Submit {
            id: id.into(),
            text: text.into(),
            force: true,
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

    #[test]
    fn enter_while_recording_transcribes_and_submits_without_confirmation() {
        let mut companion = opened();
        assert_eq!(
            companion.apply(Event::Enter { text: None }),
            vec![Action::StopRecording]
        );
        assert_eq!(
            companion.apply(Event::Transcribed("  open the tasks widget \n".into())),
            vec![
                persist("pill-1", "open the tasks widget"),
                submit("pill-1", "open the tasks widget"),
            ]
        );
        assert!(!companion.on_screen(), "the desktop was not given back");
        assert_eq!(companion.presentation().text, "open the tasks widget");
        // The desktop is already back; the acknowledgment only retires the
        // durable copy.
        assert_eq!(
            companion.apply(Event::Acknowledged("pill-1".into())),
            vec![Action::ClearPending]
        );
        assert_eq!(companion.phase(), &Phase::Hidden);
    }

    #[test]
    fn a_second_activation_while_recording_opens_the_editable_review() {
        let mut companion = opened();
        assert_eq!(
            companion.apply(Event::Activate),
            vec![Action::StopRecording]
        );
        assert_eq!(
            companion.apply(Event::Transcribed("draft text".into())),
            vec![persist("pill-1", "draft text")]
        );
        let presentation = companion.presentation();
        assert_eq!(presentation.state, "review");
        assert!(presentation.editable);
        assert_eq!(presentation.text, "draft text");
        assert_eq!(
            companion.apply(Event::Enter {
                text: Some("corrected text".into())
            }),
            vec![
                persist("pill-1", "corrected text"),
                submit("pill-1", "corrected text"),
            ]
        );
        assert!(!companion.on_screen());
    }

    #[test]
    fn escape_discards_recording_and_review_and_closes_the_pill() {
        let mut companion = opened();
        assert_eq!(
            companion.apply(Event::Escape),
            vec![Action::DiscardRecording]
        );
        assert!(!companion.on_screen(), "the pill stayed up after a cancel");

        let mut companion = opened();
        companion.apply(Event::Activate);
        companion.apply(Event::Transcribed("draft".into()));
        // Discarding a reviewed transcript is explicit, so the durable copy goes
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
        companion.apply(Event::Enter { text: None });
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
        companion.apply(Event::Enter { text: None });
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
        let mut companion = opened();
        companion.apply(Event::Enter { text: None });
        // Handing the words to the daemon is where the pill's work ends. It
        // must not hold the keyboard until an acknowledgment that can take a
        // whole turn to arrive, or never arrive at all.
        assert_eq!(
            companion.apply(Event::Transcribed("open the tasks widget".into())),
            vec![
                persist("pill-1", "open the tasks widget"),
                submit("pill-1", "open the tasks widget"),
            ]
        );
        assert!(!companion.on_screen());

        // Nothing reopens it while the submission is simply outstanding.
        assert_eq!(companion.apply(Event::Copy), Vec::new());
        assert!(!companion.on_screen());
        // And the acknowledgment has no window left to close.
        assert_eq!(
            companion.apply(Event::Acknowledged("pill-1".into())),
            vec![Action::ClearPending]
        );
    }

    #[test]
    fn a_handed_off_transcript_brings_the_pill_back_when_it_goes_wrong() {
        for (event, expected) in [
            (refused("pill-1", "The backend is unavailable."), "retained"),
            (
                uncertain("pill-1", "The backend did not confirm delivery."),
                "uncertain",
            ),
        ] {
            let mut companion = opened();
            companion.apply(Event::Enter { text: None });
            companion.apply(Event::Transcribed("book the flight".into()));
            assert!(!companion.on_screen());
            assert_eq!(companion.apply(event), Vec::new());
            assert!(
                companion.on_screen(),
                "the pill did not come back for {expected}"
            );
            assert_eq!(companion.presentation().state, expected);
            assert_eq!(companion.presentation().text, "book the flight");
        }
    }

    #[test]
    fn an_undeliverable_transcript_is_retained_and_resubmittable() {
        let mut companion = opened();
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Transcribed("remember the milk".into()));
        // The pill closed when the words were handed off, so a refusal has to
        // open it again to say so.
        assert_eq!(
            companion.apply(refused("pill-1", "The backend is unavailable.")),
            Vec::new()
        );
        assert!(companion.on_screen());
        let presentation = companion.presentation();
        assert_eq!(presentation.state, "retained");
        assert_eq!(presentation.text, "remember the milk");
        // The send was refused outright, so the daemon never saw it and the
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
        assert!(!companion.on_screen());
    }

    /// The pill is the recording privacy indicator and the only place a
    /// transcript can be read or corrected. A phase that cannot have one has
    /// nothing left to offer, so it stops rather than carrying on unseen.
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
        let mut companion = opened();
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Transcribed("remember the milk".into()));
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
        companion.apply(Event::Enter { text: None });
        assert_eq!(
            companion.abandon("the pill did not come up".into()),
            vec![Action::CancelTranscription]
        );
        assert!(!companion.on_screen());
    }

    #[test]
    fn an_uncertain_transcript_is_never_resent_without_the_person_saying_so() {
        let mut companion = opened();
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Transcribed("book the flight".into()));
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
                forced("pill-1", "book the flight"),
            ],
            "the forced send must carry the accepted words, never an edit"
        );
        assert!(!companion.on_screen());
    }

    #[test]
    fn an_answer_for_another_submission_never_settles_the_current_one() {
        let mut companion = opened();
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Transcribed("book the flight".into()));
        // The first submission is answered late, after the person recorded
        // again: concurrent dispatch and forced sends make that ordinary.
        companion.apply(Event::Acknowledged("pill-1".into()));
        companion.apply(Event::Activate);
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Transcribed("read my mail".into()));
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
        let mut companion = opened();
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Transcribed("remember the milk".into()));
        // The daemon refused before anything left it, and said which
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
        assert!(!companion.on_screen());
    }

    #[test]
    fn a_refusal_of_a_forced_send_does_not_forget_the_earlier_uncertainty() {
        let mut companion = opened();
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Transcribed("book the flight".into()));
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
                forced("pill-1", "book the flight"),
            ]
        );
        assert!(!companion.on_screen());
    }

    #[test]
    fn a_failed_durable_save_stops_the_submission_and_keeps_the_text() {
        let mut companion = opened();
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Transcribed("do not lose me".into()));
        // The runtime stops before Submit, so nothing was sent, and the phase
        // it lands in is one the person has to see.
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
    fn a_failed_durable_save_during_review_is_visible_without_losing_the_draft() {
        let mut companion = opened();
        companion.apply(Event::Activate);
        companion.apply(Event::Transcribed("draft text".into()));
        companion.apply(Event::PersistFailed("the disk is full".into()));

        let presentation = companion.presentation();
        assert_eq!(presentation.state, "review");
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
        let mut companion = opened();
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Transcribed("open the tasks widget".into()));
        companion.apply(uncertain("pill-1", "no confirmation"));
        assert_eq!(companion.presentation().state, "uncertain");
        // The daemon confirms after the companion gave up waiting.
        assert_eq!(
            companion.apply(Event::Acknowledged("pill-1".into())),
            vec![Action::ClearPending]
        );
        assert!(!companion.on_screen());
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
    fn a_transcript_keeps_one_identifier_across_review_edits_and_retries() {
        let mut companion = opened();
        companion.apply(Event::Activate);
        companion.apply(Event::Transcribed("first draft".into()));
        companion.apply(Event::Enter {
            text: Some("second draft".into()),
        });
        // A refused send never reached the daemon, so a further edit is safe.
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
        companion.apply(Event::Enter { text: None });
        assert_eq!(
            companion.apply(Event::Transcribed("a new request".into())),
            vec![
                persist("pill-2", "a new request"),
                submit("pill-2", "a new request"),
            ]
        );
    }

    #[test]
    fn activation_never_discards_an_unsent_transcript() {
        let mut companion = opened();
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Transcribed("keep me".into()));
        companion.apply(refused("pill-1", "offline"));
        assert_eq!(companion.apply(Event::Activate), Vec::new());
        assert!(companion.holds_unsent_transcript());
    }

    #[test]
    fn a_restored_transcript_reopens_the_pill_and_keeps_its_identifier() {
        let mut companion = Companion::new("later");
        companion.set_connected(true);
        companion.restore(Pending {
            id: "pill-7".into(),
            text: "survived the crash".into(),
        });
        assert!(companion.on_screen());
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
                forced("pill-7", "survived the crash"),
            ]
        );
        // A new recording in this process cannot reuse the recovered identifier.
        companion.apply(Event::Acknowledged("pill-7".into()));
        companion.apply(Event::Activate);
        companion.apply(Event::Enter { text: None });
        assert_eq!(
            companion.apply(Event::Transcribed("fresh".into())),
            vec![persist("later-1", "fresh"), submit("later-1", "fresh")]
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
        let mut companion = opened();
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Transcribed("first".into()));
        companion.apply(Event::Acknowledged("pill-1".into()));
        companion.apply(Event::Activate);
        companion.apply(Event::Enter { text: None });
        companion.apply(Event::Transcribed("second".into()));
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
        companion.set_assistant(AssistantState::Working, String::new());
        assert_eq!(companion.tray_state(), "working");
        companion.set_assistant(AssistantState::Attention, "job blocked".into());
        assert_eq!(companion.tray_state(), "attention");
        companion.set_assistant(AssistantState::Speaking, String::new());
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
            "The Scufris backend is unavailable."
        );
    }

    #[test]
    fn an_empty_transcript_is_an_error_rather_than_an_empty_submission() {
        let mut companion = opened();
        companion.apply(Event::Enter { text: None });
        assert_eq!(
            companion.apply(Event::Transcribed("   ".into())),
            Vec::new()
        );
        assert_eq!(companion.presentation().state, "error");
    }

    #[test]
    fn escape_during_transcription_cancels_the_pending_transcript() {
        let mut companion = opened();
        companion.apply(Event::Enter { text: None });
        assert_eq!(
            companion.apply(Event::Escape),
            vec![Action::CancelTranscription]
        );
        assert!(!companion.on_screen());
    }
}
