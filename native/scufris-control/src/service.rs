//! Control protocol version 3 between `scufris-service` and its clients.
//!
//! Version 2 faced the other way. There the popup Pi process was the server
//! and the companion was the client, so the protocol was named for a daemon
//! that was really an agent. Here the service is the server, `pi --mode rpc`
//! is one of its clients, and every surface is a client too. That inversion is
//! the whole design; this module is where it becomes a wire format. Version 2
//! itself is gone: nothing speaks it, and nothing converts to it.
//!
//! One socket, and the client says in its `hello` which kind it is. A
//! `frontend` is a surface: it submits text and is pushed the state, the
//! transcript, and the widget commands. An `agent` is the Pi process itself,
//! through its Scufris extensions: it says what it said and it opens widgets.
//! A `control` client is `scufris-ctl`: it asks one thing, reads the answer,
//! and goes away. By L1 there is at most one frontend and one agent at a time
//! and a second one replaces the first, but there may be any number of control
//! clients because that is just a person in a terminal.
//!
//! Widgets are the one thing the service does not decide. The agent asks and
//! the frontend answers, and the service is a relay that knows which role may
//! say what. It has to be in the middle because neither end knows where the
//! other one is, and because the agent is replaced under the frontend every
//! time a debug lease ends.
//!
//! Framing is the shared one: one LF-terminated JSON line each way, bounded by
//! [`MAX_MESSAGE_BYTES`](crate::MAX_MESSAGE_BYTES).

use std::{env, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ControlPathError, MessageError, in_runtime_dir, is_identifier, is_submission_text};

/// Wire protocol version accepted by the service and its clients.
pub const SERVICE_VERSION: u32 = 3;

/// Socket name below [`crate::SOCKET_DIRECTORY_NAME`].
///
/// Its own name rather than version 2's `daemon.sock`, which it does not
/// replace in place: a client left over from before finds nothing at the old
/// path and fails at connect rather than at hello.
pub const SERVICE_FILE_NAME: &str = "service.sock";

/// Maximum accepted size of one transcript entry, in UTF-8 bytes.
///
/// Smaller than a submission: the ring holds many of these and a frontend is
/// handed the whole ring when it connects.
pub const MAX_TRANSCRIPT_TEXT_BYTES: usize = 4 * 1024;

/// Maximum number of arguments in a debug command line.
pub const MAX_DEBUG_ARGS: usize = 16;

/// Maximum accepted size of one encoded widget payload, in UTF-8 bytes.
///
/// Well below the message cap: the same payload crosses the frontend's own
/// per-window channel, where small ordered messages are the contract.
pub const MAX_WIDGET_DATA_BYTES: usize = 8 * 1024;

/// Maximum number of widgets one catalog may announce.
pub const MAX_CATALOG_ENTRIES: usize = 64;

/// Maximum accepted length of one catalog name or description.
pub const MAX_CATALOG_TEXT_BYTES: usize = 512;

/// Returns the service socket path for the current user session.
pub fn service_socket_path() -> Result<PathBuf, ControlPathError> {
    in_runtime_dir(env::var_os("XDG_RUNTIME_DIR"), SERVICE_FILE_NAME)
}

/// The kind of client one connection is, declared in its `hello`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// A surface. Submits text, and is pushed state, transcript and widget
    /// commands.
    Frontend,
    /// The Pi process, through its Scufris extensions. Says what it said and
    /// asks for widgets.
    Agent,
    /// `scufris-ctl`. Asks one thing and reads the answer.
    Control,
}

impl Role {
    /// Returns the stable wire name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Frontend => "frontend",
            Self::Agent => "agent",
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

/// Where a surface lives once it is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Posture {
    /// Scufris opened it to show something. It ages and retires on its own.
    Exhibit,
    /// The person owns it. It stays until they close it.
    Instrument,
}

/// One installed widget, as the frontend announces it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// Widget identifier, equal to its directory name.
    pub id: String,
    /// Short display name shown in the window chrome.
    pub name: String,
    /// One line telling the model what the widget is for.
    pub description: String,
}

/// What the agent asks the frontend to do with a widget.
///
/// Every variant carries the correlation identifier the answer echoes. The
/// agent may have several in flight, and an answer nobody can match is an
/// answer that can act on the wrong surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WidgetCommand {
    /// Open one widget from the announced catalog.
    Open {
        /// Correlation identifier the answer echoes.
        id: String,
        /// Widget to open.
        widget: String,
        /// Where the surface lives once it is open.
        posture: Posture,
        /// Widget-defined spawn payload, bounded by [`MAX_WIDGET_DATA_BYTES`].
        #[serde(default)]
        data: serde_json::Value,
    },
    /// Send new data to one open surface.
    Update {
        /// Correlation identifier the answer echoes.
        id: String,
        /// Surface to update.
        surface: String,
        /// Widget-defined payload, bounded by [`MAX_WIDGET_DATA_BYTES`].
        #[serde(default)]
        data: serde_json::Value,
    },
    /// Close one open surface.
    Close {
        /// Correlation identifier the answer echoes.
        id: String,
        /// Surface to close.
        surface: String,
    },
    /// Close every surface the runtime opened and leave the person's own.
    Clear {
        /// Correlation identifier the answer echoes.
        id: String,
    },
}

impl WidgetCommand {
    /// Returns the correlation identifier the answer must echo.
    pub fn id(&self) -> &str {
        match self {
            Self::Open { id, .. }
            | Self::Update { id, .. }
            | Self::Close { id, .. }
            | Self::Clear { id } => id,
        }
    }

    /// Returns the stable wire name used in logs.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Open { .. } => "open",
            Self::Update { .. } => "update",
            Self::Close { .. } => "close",
            Self::Clear { .. } => "clear",
        }
    }
}

/// What the frontend tells the agent about its widgets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WidgetReport {
    /// Answers [`WidgetCommand::Open`] with the surface that was created.
    Opened {
        /// Correlation identifier copied from the command.
        id: String,
        /// Surface the runtime created. It doubles as the window label.
        surface: String,
    },
    /// Answers every command that names no new surface. It was carried out.
    Done {
        /// Correlation identifier copied from the command.
        id: String,
    },
    /// Answers any command the runtime could not carry out.
    Failed {
        /// Correlation identifier copied from the command.
        id: String,
        /// Stable machine-readable reason, shaped like an identifier.
        code: String,
        /// Short human-readable explanation.
        #[serde(default)]
        detail: String,
    },
    /// One surface went away without the agent asking. The person closed it,
    /// or it aged off the shelf.
    Closed {
        /// Surface that is gone.
        surface: String,
    },
    /// Announces the widgets this frontend can open, so the agent can type its
    /// tools. Sent once per connection, and remembered by the service for an
    /// agent that connects later.
    Catalog {
        /// Every installed widget, ordered by identifier.
        widgets: Vec<CatalogEntry>,
    },
}

impl WidgetReport {
    /// Returns the stable wire name used in logs.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Opened { .. } => "opened",
            Self::Done { .. } => "done",
            Self::Failed { .. } => "failed",
            Self::Closed { .. } => "closed",
            Self::Catalog { .. } => "catalog",
        }
    }
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// One line the assistant said. Agents only. It goes on the transcript.
    ///
    /// The service cannot read this off the event stream: Scufris answers
    /// through a tool call rather than an assistant text block, and what it
    /// meant to say is the agent's to report.
    Said {
        /// What was said.
        text: String,
    },
    /// One line the assistant wants spoken. Agents only.
    ///
    /// Separate from `said` because they are different strings: the transcript
    /// holds the whole answer and speech holds a paragraph shaped for it. The
    /// agent decides what is worth saying aloud and the frontend, which owns
    /// the speaker, decides whether to say it.
    Speak {
        /// What to synthesise.
        text: String,
    },
    /// Asks the frontend for something on the screen. Agents only.
    Widget {
        /// What to do.
        command: WidgetCommand,
    },
    /// Asks the frontend to put the conversation window on screen, or to take
    /// it away. Agents only.
    ///
    /// Not a widget: it is the frontend's own window, it is built in rather
    /// than installed, and it carries no payload. What it shares with a widget
    /// is only that the agent is the one asking.
    ///
    /// `up` rather than a toggle. A toggle from a caller that cannot see the
    /// screen does one of two opposite things and cannot tell which, so the
    /// agent says what it wants and gets it.
    Conversation {
        /// Client-owned identifier the answer echoes.
        id: String,
        /// Whether the window should be up.
        up: bool,
    },
    /// Tells the agent what became of its widgets. Frontends only.
    Report {
        /// What happened.
        report: WidgetReport,
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
            Self::Said { .. } => "said",
            Self::Speak { .. } => "speak",
            Self::Widget { .. } => "widget",
            Self::Conversation { .. } => "conversation",
            Self::Report { .. } => "report",
        }
    }
}

/// One versioned message sent by the service to a client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// One line the assistant wants spoken. Pushed to frontends only, and not
    /// kept: speech that arrived late is speech nobody asked for.
    Speak {
        /// What to synthesise.
        text: String,
    },
    /// One widget command from the agent. Pushed to the frontend.
    Widget {
        /// What to do.
        command: WidgetCommand,
    },
    /// The agent asked for the conversation window. Pushed to the frontend.
    ///
    /// It carries no identifier because the frontend answers nothing: the
    /// service answered the agent when it relayed this. See
    /// [`ClientBody::Conversation`].
    Conversation {
        /// Whether the window should be up.
        up: bool,
    },
    /// One widget answer or notice from the frontend. Pushed to the agent.
    Report {
        /// What happened.
        report: WidgetReport,
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
            Self::Speak { .. } => "speak",
            Self::Widget { .. } => "widget",
            Self::Conversation { .. } => "conversation",
            Self::Report { .. } => "report",
        }
    }
}

/// Returns true when the entry is within the bound the ring is built for.
pub fn is_transcript_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TRANSCRIPT_TEXT_BYTES
}

/// Returns true when the payload encodes within [`MAX_WIDGET_DATA_BYTES`].
pub fn is_widget_data(value: &serde_json::Value) -> bool {
    serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= MAX_WIDGET_DATA_BYTES)
}

/// Checks one widget command. The service relays these without reading them,
/// so this is the only place either end is protected from the other.
fn check_widget_command(command: &WidgetCommand) -> Result<(), MessageError> {
    if !is_identifier(command.id()) {
        return Err(MessageError::InvalidSubmission("id"));
    }
    match command {
        WidgetCommand::Open { widget, data, .. } => {
            if !is_identifier(widget) {
                return Err(MessageError::InvalidSubmission("widget"));
            }
            if !is_widget_data(data) {
                return Err(MessageError::InvalidSubmission("data"));
            }
        }
        WidgetCommand::Update { surface, data, .. } => {
            if !is_identifier(surface) {
                return Err(MessageError::InvalidSubmission("surface"));
            }
            if !is_widget_data(data) {
                return Err(MessageError::InvalidSubmission("data"));
            }
        }
        WidgetCommand::Close { surface, .. } => {
            if !is_identifier(surface) {
                return Err(MessageError::InvalidSubmission("surface"));
            }
        }
        WidgetCommand::Clear { .. } => {}
    }
    Ok(())
}

/// Checks one widget report, the answer half of the same relay.
fn check_widget_report(report: &WidgetReport) -> Result<(), MessageError> {
    match report {
        WidgetReport::Opened { id, surface } => {
            if !is_identifier(id) {
                return Err(MessageError::InvalidSubmission("id"));
            }
            if !is_identifier(surface) {
                return Err(MessageError::InvalidSubmission("surface"));
            }
        }
        WidgetReport::Done { id } => {
            if !is_identifier(id) {
                return Err(MessageError::InvalidSubmission("id"));
            }
        }
        WidgetReport::Failed { id, code, .. } => {
            if !is_identifier(id) {
                return Err(MessageError::InvalidSubmission("id"));
            }
            if !is_identifier(code) {
                return Err(MessageError::InvalidSubmission("code"));
            }
        }
        WidgetReport::Closed { surface } => {
            if !is_identifier(surface) {
                return Err(MessageError::InvalidSubmission("surface"));
            }
        }
        WidgetReport::Catalog { widgets } => {
            if widgets.len() > MAX_CATALOG_ENTRIES {
                return Err(MessageError::InvalidSubmission("widgets"));
            }
            for widget in widgets {
                if !is_identifier(&widget.id) {
                    return Err(MessageError::InvalidSubmission("id"));
                }
                if widget.name.is_empty() || widget.name.len() > MAX_CATALOG_TEXT_BYTES {
                    return Err(MessageError::InvalidSubmission("name"));
                }
                if widget.description.len() > MAX_CATALOG_TEXT_BYTES {
                    return Err(MessageError::InvalidSubmission("description"));
                }
            }
        }
    }
    Ok(())
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
        ClientBody::Abort { id }
        | ClientBody::GetState { id }
        | ClientBody::Debug { id }
        | ClientBody::Conversation { id, .. } => {
            if !is_identifier(id) {
                return Err(MessageError::InvalidSubmission("id"));
            }
        }
        ClientBody::Said { text } | ClientBody::Speak { text } => {
            if !is_transcript_text(text) {
                return Err(MessageError::InvalidSubmission("text"));
            }
        }
        ClientBody::Widget { command } => check_widget_command(command)?,
        ClientBody::Report { report } => check_widget_report(report)?,
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
        ServiceBody::Speak { text } => {
            if !is_transcript_text(text) {
                return Err(MessageError::InvalidSubmission("text"));
            }
        }
        ServiceBody::Widget { command } => check_widget_command(command)?,
        ServiceBody::Report { report } => check_widget_report(report)?,
        ServiceBody::Welcome { .. } | ServiceBody::Conversation { .. } => {}
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
    fn the_agent_reports_what_it_said_and_what_it_wants_spoken_separately() {
        // Two strings, not one. The transcript holds the whole answer and
        // speech holds the paragraph shaped for a speaker, and the agent is
        // the only thing that knows which is which.
        let said = ClientMessage::new(ClientBody::Said {
            text: "the harness is green, 140 of 140".into(),
        });
        assert_eq!(
            encoded(&said),
            "{\"v\":3,\"type\":\"said\",\"text\":\"the harness is green, 140 of 140\"}\n"
        );
        assert_eq!(
            read_client_message(&mut Cursor::new(encoded(&said))).unwrap(),
            said
        );
        let speak = ServiceMessage::new(ServiceBody::Speak {
            text: "the harness is green".into(),
        });
        assert_eq!(
            read_service_message(&mut Cursor::new(encoded(&speak))).unwrap(),
            speak
        );
        let oversized = encoded(&ClientMessage::new(ClientBody::Speak {
            text: "x".repeat(MAX_TRANSCRIPT_TEXT_BYTES + 1),
        }));
        assert!(matches!(
            read_client_message(&mut Cursor::new(oversized)),
            Err(MessageError::InvalidSubmission("text"))
        ));
    }

    #[test]
    fn a_widget_command_is_relayed_whole_and_its_answer_carries_the_same_id() {
        // The service does not read these. It relays them because neither end
        // knows where the other one is, so the wire shape is the only check
        // either end gets.
        let command = WidgetCommand::Open {
            id: "w-1".into(),
            widget: "clock".into(),
            posture: Posture::Exhibit,
            data: json!({ "zone": "Europe/Bucharest" }),
        };
        assert_eq!(
            encoded(&ClientMessage::new(ClientBody::Widget {
                command: command.clone()
            })),
            "{\"v\":3,\"type\":\"widget\",\"command\":{\"type\":\"open\",\"id\":\"w-1\",\
             \"widget\":\"clock\",\"posture\":\"exhibit\",\
             \"data\":{\"zone\":\"Europe/Bucharest\"}}}\n"
        );
        let pushed = ServiceMessage::new(ServiceBody::Widget { command });
        assert_eq!(
            read_service_message(&mut Cursor::new(encoded(&pushed))).unwrap(),
            pushed
        );
        let opened = ClientMessage::new(ClientBody::Report {
            report: WidgetReport::Opened {
                id: "w-1".into(),
                surface: "clock-1".into(),
            },
        });
        assert_eq!(
            read_client_message(&mut Cursor::new(encoded(&opened))).unwrap(),
            opened
        );
        for report in [
            WidgetReport::Done { id: "w-2".into() },
            WidgetReport::Failed {
                id: "w-3".into(),
                code: "unknown_widget".into(),
                detail: "no widget named lamp".into(),
            },
            WidgetReport::Closed {
                surface: "clock-1".into(),
            },
        ] {
            let message = ServiceMessage::new(ServiceBody::Report { report });
            assert_eq!(
                read_service_message(&mut Cursor::new(encoded(&message))).unwrap(),
                message
            );
        }
    }

    #[test]
    fn widget_payloads_are_bounded_and_every_name_is_an_identifier() {
        let oversized = encoded(&ClientMessage::new(ClientBody::Widget {
            command: WidgetCommand::Update {
                id: "w-1".into(),
                surface: "clock-1".into(),
                data: json!({ "text": "x".repeat(MAX_WIDGET_DATA_BYTES) }),
            },
        }));
        assert!(matches!(
            read_client_message(&mut Cursor::new(oversized)),
            Err(MessageError::InvalidSubmission("data"))
        ));
        let named = encoded(&ClientMessage::new(ClientBody::Widget {
            command: WidgetCommand::Close {
                id: "w-1".into(),
                surface: "clock 1".into(),
            },
        }));
        assert!(matches!(
            read_client_message(&mut Cursor::new(named)),
            Err(MessageError::InvalidSubmission("surface"))
        ));
        let clear = ClientMessage::new(ClientBody::Widget {
            command: WidgetCommand::Clear { id: "w-9".into() },
        });
        assert_eq!(
            read_client_message(&mut Cursor::new(encoded(&clear))).unwrap(),
            clear
        );
    }

    #[test]
    fn the_catalog_is_bounded_because_it_types_the_agents_tools() {
        let catalog = ClientMessage::new(ClientBody::Report {
            report: WidgetReport::Catalog {
                widgets: vec![CatalogEntry {
                    id: "clock".into(),
                    name: "Clock".into(),
                    description: "Shows the time in one zone.".into(),
                }],
            },
        });
        assert_eq!(
            read_client_message(&mut Cursor::new(encoded(&catalog))).unwrap(),
            catalog
        );
        let many = encoded(&ClientMessage::new(ClientBody::Report {
            report: WidgetReport::Catalog {
                widgets: vec![
                    CatalogEntry {
                        id: "clock".into(),
                        name: "Clock".into(),
                        description: String::new(),
                    };
                    MAX_CATALOG_ENTRIES + 1
                ],
            },
        }));
        assert!(matches!(
            read_client_message(&mut Cursor::new(many)),
            Err(MessageError::InvalidSubmission("widgets"))
        ));
        let nameless = encoded(&ClientMessage::new(ClientBody::Report {
            report: WidgetReport::Catalog {
                widgets: vec![CatalogEntry {
                    id: "clock".into(),
                    name: String::new(),
                    description: String::new(),
                }],
            },
        }));
        assert!(matches!(
            read_client_message(&mut Cursor::new(nameless)),
            Err(MessageError::InvalidSubmission("name"))
        ));
    }

    #[test]
    fn every_role_has_a_stable_name_and_reads_back_as_itself() {
        for role in [Role::Frontend, Role::Control, Role::Agent] {
            let line = format!(
                "{{\"v\":3,\"type\":\"hello\",\"role\":\"{}\"}}\n",
                role.name()
            );
            assert_eq!(
                read_client_message(&mut Cursor::new(line)).unwrap().body,
                ClientBody::Hello { role }
            );
        }
    }

    #[test]
    fn the_service_socket_is_one_named_path_in_the_session_runtime_directory() {
        let run = Some(std::ffi::OsString::from("/run/user/1000"));
        assert_eq!(
            in_runtime_dir(run, SERVICE_FILE_NAME).expect("the runtime directory is set"),
            std::path::Path::new("/run/user/1000/scufris/service.sock")
        );
        assert!(in_runtime_dir(None, SERVICE_FILE_NAME).is_err());
    }
}
