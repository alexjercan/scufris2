//! Control protocol version 3 between `scufris-service` and its clients.
//!
//! Version 2 faces the other way. There the popup Pi process is the server and
//! the companion is the client, so the protocol is named for a daemon that is
//! really an agent. Here the service is the server, `pi --mode rpc` is one of
//! its clients, and every surface is a client too. That inversion is the whole
//! design; this module is where it becomes a wire format.
//!
//! One socket, and the client says in its `hello` which kind it is. A
//! `frontend` is a surface: it submits text and is pushed the state and the
//! transcript as they change. A `control` client is `scufris-ctl`: it asks one
//! thing, reads the answer, and goes away. By L1 there is at most one frontend
//! at a time and a second one replaces the first, but there may be any number
//! of control clients because that is just a person in a terminal.
//!
//! The `agent` role is not here yet. It arrives with the surface protocol in
//! the increment that turns `extensions/scufris/desktop/` into a client.
//!
//! Framing is version 2's: one LF-terminated JSON line each way, bounded by
//! [`MAX_MESSAGE_BYTES`](crate::MAX_MESSAGE_BYTES).

use std::{env, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ControlPathError, MessageError, in_runtime_dir, is_identifier, is_submission_text};

/// Wire protocol version accepted by the service and its clients.
pub const SERVICE_VERSION: u32 = 3;

/// Socket name below [`crate::SOCKET_DIRECTORY_NAME`].
///
/// Its own name rather than version 2's `daemon.sock`. The two servers stand
/// side by side until the switch, and a client that connects to the wrong one
/// should fail at connect rather than at hello.
pub const SERVICE_FILE_NAME: &str = "service.sock";

/// Maximum accepted size of one transcript entry, in UTF-8 bytes.
///
/// Smaller than a submission: the ring holds many of these and a frontend is
/// handed the whole ring when it connects.
pub const MAX_TRANSCRIPT_TEXT_BYTES: usize = 4 * 1024;

/// Maximum number of arguments in a debug command line.
pub const MAX_DEBUG_ARGS: usize = 16;

/// Returns the service socket path for the current user session.
pub fn service_socket_path() -> Result<PathBuf, ControlPathError> {
    in_runtime_dir(env::var_os("XDG_RUNTIME_DIR"), SERVICE_FILE_NAME)
}

/// The kind of client one connection is, declared in its `hello`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// A surface. Submits text, and is pushed state and transcript changes.
    Frontend,
    /// `scufris-ctl`. Asks one thing and reads the answer.
    Control,
}

impl Role {
    /// Returns the stable wire name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Frontend => "frontend",
            Self::Control => "control",
        }
    }
}

/// What the assistant is doing, as the service reports it.
///
/// Small on purpose. The frontend never parses a Pi event and never learns
/// Pi's vocabulary, so an event Pi adds tomorrow is a service change and
/// nothing else. Speaking and listening are not here: they are what the
/// frontend is doing, and it layers them on top of this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScufrisState {
    /// The agent is spawned and has not answered yet.
    Starting,
    /// The agent is up and no run is in progress.
    Idle,
    /// An agent run is in progress.
    Working,
    /// A debug lease is held, so the agent is a terminal somebody else owns.
    Detached,
    /// The agent could not be kept running.
    Error,
}

impl ScufrisState {
    /// Returns the stable wire name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Detached => "detached",
            Self::Error => "error",
        }
    }
}

/// Who said one line of the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Speaker {
    /// The person, or anything submitted on their behalf.
    User,
    /// The assistant.
    Assistant,
}

/// One line of the conversation, as the service keeps it.
///
/// Text only. Thinking, tool calls and tool results are in the session file
/// and reachable with `get_entries`; what a frontend needs on connect is the
/// last screenful of what was said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptEntry {
    /// Who said it.
    pub speaker: Speaker,
    /// What was said, truncated to [`MAX_TRANSCRIPT_TEXT_BYTES`].
    pub text: String,
}

/// Stable refusal codes. A caller branches on these; `detail` is for a person.
pub mod refusal {
    /// There is no agent to send to.
    pub const AGENT_UNAVAILABLE: &str = "agent_unavailable";
    /// A debug lease is held, so the conversation belongs to a terminal.
    pub const DETACHED: &str = "detached";
    /// Somebody already holds the debug lease.
    pub const DEBUG_HELD: &str = "debug_held";
    /// The verb is not one this role may use.
    pub const WRONG_ROLE: &str = "wrong_role";
    /// The agent itself refused the command.
    pub const AGENT_REFUSED: &str = "agent_refused";
}

/// One versioned message sent by a client to the service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientMessage {
    /// Wire protocol version used to encode the message.
    pub v: u32,
    /// Typed message body.
    #[serde(flatten)]
    pub body: ClientBody,
}

impl ClientMessage {
    /// Creates a message carrying the current protocol version.
    pub fn new(body: ClientBody) -> Self {
        Self {
            v: SERVICE_VERSION,
            body,
        }
    }
}

/// Client messages defined by protocol version 3.
///
/// Every request carries an `id` and every answer echoes it, so a client that
/// pipelines two requests can tell the answers apart without counting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientBody {
    /// Opens the connection and declares what kind of client this is. It must
    /// be the first message, and there is never a second one.
    Hello {
        /// What this client is.
        role: Role,
    },
    /// Submits one line as a user message. A submission while the agent is
    /// working is delivered as a steer rather than refused.
    Submit {
        /// Client-owned identifier the answer echoes.
        id: String,
        /// The text to say.
        text: String,
    },
    /// Ends the current agent run.
    Abort {
        /// Client-owned identifier the answer echoes.
        id: String,
    },
    /// Asks for the current state once.
    GetState {
        /// Client-owned identifier the answer echoes.
        id: String,
    },
    /// Takes the agent away and asks for the command line that resumes its
    /// session in a terminal. The lease lasts exactly as long as this
    /// connection: when it closes, the service starts the agent again.
    Debug {
        /// Client-owned identifier the answer echoes.
        id: String,
    },
}

impl ClientBody {
    /// Returns the stable wire name used in logs.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "hello",
            Self::Submit { .. } => "submit",
            Self::Abort { .. } => "abort",
            Self::GetState { .. } => "get_state",
            Self::Debug { .. } => "debug",
        }
    }
}

/// One versioned message sent by the service to a client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceMessage {
    /// Wire protocol version used to encode the message.
    pub v: u32,
    /// Typed message body.
    #[serde(flatten)]
    pub body: ServiceBody,
}

impl ServiceMessage {
    /// Creates a message carrying the current protocol version.
    pub fn new(body: ServiceBody) -> Self {
        Self {
            v: SERVICE_VERSION,
            body,
        }
    }
}

/// Service messages defined by protocol version 3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServiceBody {
    /// Answers `hello`, echoing the role the connection was given.
    Welcome {
        /// The role this connection now has.
        role: Role,
    },
    /// The request was carried out.
    Ok {
        /// Identifier copied from the request.
        id: String,
    },
    /// The request was not carried out, and why.
    Refused {
        /// Identifier copied from the request.
        id: String,
        /// Stable machine-readable reason from [`refusal`].
        code: String,
        /// Short human-readable explanation.
        #[serde(default)]
        detail: String,
    },
    /// The current state. Carries the `id` when it answers `get_state`, and no
    /// `id` when the service pushes it to a frontend because it changed.
    State {
        /// Identifier copied from the request that asked, if one did.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// What the assistant is doing.
        state: ScufrisState,
        /// Short human-readable detail, empty when there is nothing to add.
        #[serde(default)]
        detail: String,
    },
    /// One more line of the conversation. Pushed to frontends only.
    Transcript {
        /// The line.
        entry: TranscriptEntry,
    },
    /// Answers `debug`. The agent is stopped and this connection holds the
    /// lease. Run `program` with `args` and the terminal has the session.
    Debug {
        /// Identifier copied from the request.
        id: String,
        /// Absolute path of the program to run.
        program: String,
        /// Its arguments, naming the session the service was using.
        args: Vec<String>,
    },
}

impl ServiceBody {
    /// Returns the stable wire name used in logs.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Welcome { .. } => "welcome",
            Self::Ok { .. } => "ok",
            Self::Refused { .. } => "refused",
            Self::State { .. } => "state",
            Self::Transcript { .. } => "transcript",
            Self::Debug { .. } => "debug",
        }
    }
}

/// Returns true when the entry is within the bound the ring is built for.
pub fn is_transcript_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TRANSCRIPT_TEXT_BYTES
}

/// Reads one bounded line and refuses it unless it is this protocol version.
///
/// The version is read before the body, so a peer that found the wrong socket
/// is told which version it spoke rather than that its message did not parse.
/// Version 2 has different message types, and a version check that ran after
/// the body would report every one of them as bad JSON.
fn versioned(reader: &mut impl std::io::BufRead) -> Result<Vec<u8>, MessageError> {
    #[derive(Deserialize)]
    struct Versioned {
        v: u32,
    }
    let line = crate::read_line(reader)?;
    let spoken: Versioned = serde_json::from_slice(&line)?;
    if spoken.v != SERVICE_VERSION {
        return Err(MessageError::UnsupportedVersion(spoken.v));
    }
    Ok(line)
}

/// Reads one client message and rejects unsupported versions and payloads.
pub fn read_client_message(
    reader: &mut impl std::io::BufRead,
) -> Result<ClientMessage, MessageError> {
    let message: ClientMessage = serde_json::from_slice(&versioned(reader)?)?;
    match &message.body {
        ClientBody::Submit { id, text } => {
            if !is_identifier(id) {
                return Err(MessageError::InvalidSubmission("id"));
            }
            if !is_submission_text(text) {
                return Err(MessageError::InvalidSubmission("text"));
            }
        }
        ClientBody::Abort { id } | ClientBody::GetState { id } | ClientBody::Debug { id } => {
            if !is_identifier(id) {
                return Err(MessageError::InvalidSubmission("id"));
            }
        }
        ClientBody::Hello { .. } => {}
    }
    Ok(message)
}

/// Reads one service message and rejects unsupported versions and payloads.
pub fn read_service_message(
    reader: &mut impl std::io::BufRead,
) -> Result<ServiceMessage, MessageError> {
    let message: ServiceMessage = serde_json::from_slice(&versioned(reader)?)?;
    match &message.body {
        ServiceBody::Ok { id } => {
            if !is_identifier(id) {
                return Err(MessageError::InvalidSubmission("id"));
            }
        }
        ServiceBody::Refused { id, code, .. } => {
            if !is_identifier(id) || !is_identifier(code) {
                return Err(MessageError::InvalidSubmission("id"));
            }
        }
        ServiceBody::State { id, .. } => {
            if id.as_deref().is_some_and(|id| !is_identifier(id)) {
                return Err(MessageError::InvalidSubmission("id"));
            }
        }
        ServiceBody::Transcript { entry } => {
            if !is_transcript_text(&entry.text) {
                return Err(MessageError::InvalidSubmission("text"));
            }
        }
        ServiceBody::Debug {
            id, program, args, ..
        } => {
            if !is_identifier(id) {
                return Err(MessageError::InvalidSubmission("id"));
            }
            // The client runs this. An absolute path is the difference between
            // running what the service is configured with and running whatever
            // the person's PATH happens to carry.
            if !std::path::Path::new(program).is_absolute() {
                return Err(MessageError::InvalidSubmission("program"));
            }
            if args.len() > MAX_DEBUG_ARGS {
                return Err(MessageError::InvalidSubmission("args"));
            }
        }
        ServiceBody::Welcome { .. } => {}
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use serde_json::json;

    use super::*;
    use crate::write_message;

    fn encoded(message: &impl Serialize) -> String {
        let mut bytes = Vec::new();
        write_message(&mut bytes, message).unwrap();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn a_client_says_what_kind_of_client_it_is_and_then_asks_for_things() {
        assert_eq!(
            encoded(&ClientMessage::new(ClientBody::Hello {
                role: Role::Control
            })),
            "{\"v\":3,\"type\":\"hello\",\"role\":\"control\"}\n"
        );
        let submit = ClientMessage::new(ClientBody::Submit {
            id: "c-1".into(),
            text: "what is on my calendar".into(),
        });
        assert_eq!(
            encoded(&submit),
            "{\"v\":3,\"type\":\"submit\",\"id\":\"c-1\",\"text\":\"what is on my calendar\"}\n"
        );
        assert_eq!(
            read_client_message(&mut Cursor::new(encoded(&submit))).unwrap(),
            submit
        );
        for body in [
            ClientBody::Abort { id: "c-2".into() },
            ClientBody::GetState { id: "c-3".into() },
            ClientBody::Debug { id: "c-4".into() },
        ] {
            let message = ClientMessage::new(body);
            assert_eq!(
                read_client_message(&mut Cursor::new(encoded(&message))).unwrap(),
                message
            );
        }
    }

    #[test]
    fn the_service_answers_with_the_identifier_it_was_asked_with() {
        assert_eq!(
            serde_json::to_value(ServiceMessage::new(ServiceBody::Ok { id: "c-1".into() }))
                .unwrap(),
            json!({ "v": 3, "type": "ok", "id": "c-1" })
        );
        assert_eq!(
            serde_json::to_value(ServiceMessage::new(ServiceBody::Refused {
                id: "c-1".into(),
                code: refusal::DETACHED.into(),
                detail: "a terminal has the session".into(),
            }))
            .unwrap(),
            json!({
                "v": 3,
                "type": "refused",
                "id": "c-1",
                "code": "detached",
                "detail": "a terminal has the session",
            })
        );
    }

    #[test]
    fn a_pushed_state_carries_no_identifier_and_an_answer_does() {
        // The same message type both ways. A frontend watching the state and a
        // ctl that asked for it once are reading the same thing, and only the
        // presence of the identifier says which of the two this is.
        assert_eq!(
            encoded(&ServiceMessage::new(ServiceBody::State {
                id: None,
                state: ScufrisState::Working,
                detail: String::new(),
            })),
            "{\"v\":3,\"type\":\"state\",\"state\":\"working\",\"detail\":\"\"}\n"
        );
        assert_eq!(
            encoded(&ServiceMessage::new(ServiceBody::State {
                id: Some("c-3".into()),
                state: ScufrisState::Detached,
                detail: String::new(),
            })),
            "{\"v\":3,\"type\":\"state\",\"id\":\"c-3\",\"state\":\"detached\",\"detail\":\"\"}\n"
        );
        let mut line = Cursor::new("{\"v\":3,\"type\":\"state\",\"state\":\"idle\"}\n");
        assert_eq!(
            read_service_message(&mut line).unwrap().body,
            ServiceBody::State {
                id: None,
                state: ScufrisState::Idle,
                detail: String::new(),
            }
        );
    }

    #[test]
    fn every_state_has_a_stable_name_and_reads_back_as_itself() {
        for state in [
            ScufrisState::Starting,
            ScufrisState::Idle,
            ScufrisState::Working,
            ScufrisState::Detached,
            ScufrisState::Error,
        ] {
            let line = format!(
                "{{\"v\":3,\"type\":\"state\",\"state\":\"{}\"}}\n",
                state.name()
            );
            assert_eq!(
                read_service_message(&mut Cursor::new(line)).unwrap().body,
                ServiceBody::State {
                    id: None,
                    state,
                    detail: String::new(),
                }
            );
        }
        // Speaking is not a state the service knows. It belongs to the process
        // that owns the speaker, and putting it here would put the service in
        // the audio path.
        let mut line = Cursor::new("{\"v\":3,\"type\":\"state\",\"state\":\"speaking\"}\n");
        assert!(read_service_message(&mut line).is_err());
    }

    #[test]
    fn a_debug_answer_names_an_absolute_program_and_a_bounded_command_line() {
        let debug = ServiceMessage::new(ServiceBody::Debug {
            id: "c-4".into(),
            program: "/nix/store/scufris/bin/scufris".into(),
            args: vec![
                "--session-dir".into(),
                "/home/a/.local/share/scufris/sessions".into(),
                "--session".into(),
                "/home/a/.local/share/scufris/sessions/one.jsonl".into(),
            ],
        });
        assert_eq!(
            read_service_message(&mut Cursor::new(encoded(&debug))).unwrap(),
            debug
        );
        let relative = encoded(&ServiceMessage::new(ServiceBody::Debug {
            id: "c-4".into(),
            program: "scufris".into(),
            args: Vec::new(),
        }));
        assert!(matches!(
            read_service_message(&mut Cursor::new(relative)),
            Err(MessageError::InvalidSubmission("program"))
        ));
        let many = encoded(&ServiceMessage::new(ServiceBody::Debug {
            id: "c-4".into(),
            program: "/bin/scufris".into(),
            args: vec!["-x".into(); MAX_DEBUG_ARGS + 1],
        }));
        assert!(matches!(
            read_service_message(&mut Cursor::new(many)),
            Err(MessageError::InvalidSubmission("args"))
        ));
    }

    #[test]
    fn transcript_entries_are_bounded_the_way_the_ring_is_built_for() {
        let entry = TranscriptEntry {
            speaker: Speaker::Assistant,
            text: "the harness is green".into(),
        };
        assert_eq!(
            encoded(&ServiceMessage::new(ServiceBody::Transcript {
                entry: entry.clone()
            })),
            "{\"v\":3,\"type\":\"transcript\",\"entry\":{\"speaker\":\"assistant\",\
             \"text\":\"the harness is green\"}}\n"
        );
        let oversized = encoded(&ServiceMessage::new(ServiceBody::Transcript {
            entry: TranscriptEntry {
                speaker: Speaker::User,
                text: "x".repeat(MAX_TRANSCRIPT_TEXT_BYTES + 1),
            },
        }));
        assert!(matches!(
            read_service_message(&mut Cursor::new(oversized)),
            Err(MessageError::InvalidSubmission("text"))
        ));
    }

    #[test]
    fn version_two_peers_are_refused_at_hello_in_both_directions() {
        // The two servers stand side by side until the switch. A companion
        // that finds the wrong socket must fail at once and say why.
        let mut line = Cursor::new("{\"v\":2,\"type\":\"hello\"}\n");
        assert!(matches!(
            read_client_message(&mut line),
            Err(MessageError::UnsupportedVersion(2))
        ));
        let mut line = Cursor::new("{\"v\":2,\"type\":\"pong\"}\n");
        assert!(matches!(
            read_service_message(&mut line),
            Err(MessageError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn requests_and_answers_are_identified() {
        let mut line =
            Cursor::new("{\"v\":3,\"type\":\"submit\",\"id\":\"c 1\",\"text\":\"hi\"}\n");
        assert!(matches!(
            read_client_message(&mut line),
            Err(MessageError::InvalidSubmission("id"))
        ));
        let mut line =
            Cursor::new("{\"v\":3,\"type\":\"submit\",\"id\":\"c-1\",\"text\":\"  \"}\n");
        assert!(matches!(
            read_client_message(&mut line),
            Err(MessageError::InvalidSubmission("text"))
        ));
        let mut line = Cursor::new("{\"v\":3,\"type\":\"abort\",\"id\":\"\"}\n");
        assert!(matches!(
            read_client_message(&mut line),
            Err(MessageError::InvalidSubmission("id"))
        ));
    }

    #[test]
    fn the_service_socket_is_not_the_version_two_socket() {
        let run = Some(std::ffi::OsString::from("/run/user/1000"));
        let service =
            in_runtime_dir(run.clone(), SERVICE_FILE_NAME).expect("the runtime directory is set");
        let daemon =
            in_runtime_dir(run, crate::SOCKET_FILE_NAME).expect("the runtime directory is set");
        assert_eq!(service.parent(), daemon.parent());
        assert_ne!(service, daemon);
        assert!(service.ends_with(SERVICE_FILE_NAME));
    }
}
