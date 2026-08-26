//! Control protocol version 2 between scufris-desktop and the Scufris daemon.
//!
//! The daemon is the popup Pi process. It serves one same-user Unix socket and
//! owns the authoritative conversation. The companion never writes session
//! files; it submits accepted transcripts as ordinary user messages and follows
//! the assistant state the daemon reports.
//!
//! Version 2 adds the widget commands. They are the first daemon-originated
//! requests: each carries a correlation `id` that the companion echoes in the
//! answer, so a caller can wait for its own command. The companion also reports
//! its widget catalog and unsolicited surface events.
//!
//! Every message is one LF-terminated JSON line bounded by
//! [`MAX_MESSAGE_BYTES`]. Both peers reject unknown message types and any
//! protocol version other than [`PROTOCOL_VERSION`].

use std::{
    env,
    io::{self, BufRead, Write},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Wire protocol version accepted by both peers.
pub const PROTOCOL_VERSION: u32 = 2;

/// Maximum encoded message size, including its LF terminator.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// Directory below `XDG_RUNTIME_DIR` that holds the daemon socket.
pub const SOCKET_DIRECTORY_NAME: &str = "scufris";

/// Socket name below [`SOCKET_DIRECTORY_NAME`].
pub const SOCKET_FILE_NAME: &str = "daemon.sock";

/// Maximum accepted length of one protocol identifier.
///
/// Submission, correlation, widget, and surface identifiers share one rule, so
/// a peer that can read one can read them all.
pub const MAX_IDENTIFIER_LENGTH: usize = 64;

/// Maximum accepted size of one encoded widget payload, in UTF-8 bytes.
///
/// Well below [`MAX_MESSAGE_BYTES`]: the same payload crosses the companion's
/// per-window channel, where small ordered messages are the contract.
pub const MAX_WIDGET_DATA_BYTES: usize = 8 * 1024;

/// Maximum accepted size of one submitted transcript, in UTF-8 bytes.
///
/// Bytes, not characters: the daemon measures the same way, so text either
/// side accepts is text both sides accept.
pub const MAX_SUBMISSION_TEXT_BYTES: usize = 8 * 1024;

/// Returns the daemon socket path for the current user session.
pub fn socket_path() -> Result<PathBuf, ControlPathError> {
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR").ok_or(ControlPathError::MissingRuntimeDir)?;
    if runtime_dir.is_empty() {
        return Err(ControlPathError::MissingRuntimeDir);
    }
    Ok(PathBuf::from(runtime_dir)
        .join(SOCKET_DIRECTORY_NAME)
        .join(SOCKET_FILE_NAME))
}

/// Failure to resolve the current user's daemon socket path.
#[derive(Debug, Error)]
pub enum ControlPathError {
    /// The process has no non-empty `XDG_RUNTIME_DIR`.
    #[error("XDG_RUNTIME_DIR is required")]
    MissingRuntimeDir,
}

/// Returns true for the default of an omitted boolean field.
fn is_false(value: &bool) -> bool {
    !*value
}

/// One versioned message sent by the companion to the daemon.
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
            v: PROTOCOL_VERSION,
            body,
        }
    }
}

/// Companion messages defined by protocol version 2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientBody {
    /// Opens the session and asks for the authoritative session identity.
    Hello,
    /// Submits one accepted transcript as a normal user message.
    Submit {
        /// Companion-owned identifier echoed by the matching acknowledgment.
        id: String,
        /// Accepted transcript text.
        text: String,
        /// The person's own decision to send words that may already be in the
        /// conversation. Absent from every ordinary submission, and never set
        /// by a timeout, a reconnect, or a restart.
        #[serde(default, skip_serializing_if = "is_false")]
        force: bool,
    },
    /// Liveness probe answered with [`DaemonBody::Pong`].
    Ping,
    /// Answers [`DaemonBody::WidgetOpen`] with the surface that was created.
    WidgetOpened {
        /// Correlation identifier copied from the command.
        id: String,
        /// Surface the runtime created. It doubles as the window label.
        surface: String,
    },
    /// Answers every widget command that names no new surface: update, close,
    /// and clear. The command was carried out.
    WidgetDone {
        /// Correlation identifier copied from the command.
        id: String,
    },
    /// Answers any widget command that the runtime could not carry out.
    WidgetFailed {
        /// Correlation identifier copied from the command.
        id: String,
        /// Stable machine-readable reason, shaped like an identifier.
        code: String,
        /// Short human-readable explanation.
        #[serde(default)]
        detail: String,
    },
    /// Reports a change to one surface that the daemon did not ask for.
    WidgetEvent {
        /// Surface the event is about.
        surface: String,
        /// What happened to it.
        event: SurfaceEvent,
    },
    /// Announces the widgets this companion can open. Sent once per connection,
    /// right after the daemon's welcome, so the daemon can type its tools.
    Catalog {
        /// Every installed widget, ordered by identifier.
        widgets: Vec<CatalogEntry>,
    },
}

impl ClientBody {
    /// Returns the stable wire name used in logs.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Hello => "hello",
            Self::Submit { .. } => "submit",
            Self::Ping => "ping",
            Self::WidgetOpened { .. } => "widget_opened",
            Self::WidgetDone { .. } => "widget_done",
            Self::WidgetFailed { .. } => "widget_failed",
            Self::WidgetEvent { .. } => "widget_event",
            Self::Catalog { .. } => "catalog",
        }
    }
}

/// One installed widget, as the companion announces it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// Widget identifier, equal to its directory name.
    pub id: String,
    /// Short display name shown in the window chrome.
    pub name: String,
    /// One line telling the model what the widget is for.
    pub description: String,
}

/// A change to one surface that the daemon did not ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceEvent {
    /// The surface is gone. The daemon forgets it and never reopens it unasked.
    Closed,
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

/// One versioned message sent by the daemon to the companion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DaemonMessage {
    /// Wire protocol version used to encode the message.
    pub v: u32,
    /// Typed message body.
    #[serde(flatten)]
    pub body: DaemonBody,
}

impl DaemonMessage {
    /// Creates a message carrying the current protocol version.
    pub fn new(body: DaemonBody) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            body,
        }
    }
}

/// Daemon messages defined by protocol version 2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonBody {
    /// Answers [`ClientBody::Hello`] with the authoritative session identity.
    Welcome {
        /// Session identity owned by the daemon.
        session: String,
    },
    /// Confirms that one submitted transcript entered the conversation.
    Ack {
        /// Identifier copied from the matching submission.
        id: String,
    },
    /// Reports that one submission was dispatched once already and that the
    /// daemon cannot say whether it landed.
    Uncertain {
        /// Identifier copied from the matching submission.
        id: String,
        /// Short human-readable explanation.
        #[serde(default)]
        detail: String,
    },
    /// Reports that one submission never left the daemon, so the conversation
    /// never saw it and the companion may edit and retry those words.
    Refused {
        /// Identifier copied from the matching submission.
        id: String,
        /// Short human-readable explanation.
        #[serde(default)]
        detail: String,
    },
    /// Reports the current assistant state.
    State {
        /// Current assistant state.
        state: AssistantState,
        /// Short human-readable detail, empty when there is nothing to add.
        #[serde(default)]
        detail: String,
    },
    /// Answers [`ClientBody::Ping`].
    Pong,
    /// Asks the companion to open one widget. Answered with
    /// [`ClientBody::WidgetOpened`] or [`ClientBody::WidgetFailed`].
    WidgetOpen {
        /// Correlation identifier the answer echoes.
        id: String,
        /// Widget to open, from the announced catalog.
        widget: String,
        /// Where the surface lives once it is open.
        posture: Posture,
        /// Widget-defined spawn payload, bounded by [`MAX_WIDGET_DATA_BYTES`].
        #[serde(default)]
        data: serde_json::Value,
    },
    /// Sends new data to one open surface. Citing a surface is updating it.
    WidgetUpdate {
        /// Correlation identifier the answer echoes.
        id: String,
        /// Surface to update.
        surface: String,
        /// Widget-defined payload, bounded by [`MAX_WIDGET_DATA_BYTES`].
        #[serde(default)]
        data: serde_json::Value,
    },
    /// Closes one open surface.
    WidgetClose {
        /// Correlation identifier the answer echoes.
        id: String,
        /// Surface to close.
        surface: String,
    },
    /// Closes every surface the runtime owns and leaves the person's own
    /// widgets standing.
    WidgetClear {
        /// Correlation identifier the answer echoes.
        id: String,
    },
}

/// Assistant states the daemon reports.
///
/// Listening and transcribing are companion-local; the daemon never sees audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantState {
    /// No agent run, speech, or unattended work.
    Idle,
    /// An agent run is in progress.
    Working,
    /// Spoken output is playing.
    Speaking,
    /// Something needs the user before work continues.
    Attention,
    /// The daemon could not complete the last requested operation.
    Error,
}

impl AssistantState {
    /// Returns the stable wire name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Speaking => "speaking",
            Self::Attention => "attention",
            Self::Error => "error",
        }
    }
}

/// Failure to read, validate, or write one control message.
#[derive(Debug, Error)]
pub enum MessageError {
    /// The peer closed the connection before sending data.
    #[error("control message is empty")]
    Empty,
    /// The encoded message exceeded [`MAX_MESSAGE_BYTES`].
    #[error("control message exceeds {MAX_MESSAGE_BYTES} bytes")]
    TooLarge,
    /// The peer did not terminate its message with exactly one LF byte.
    #[error("control message must end with LF")]
    MissingTerminator,
    /// The bounded line was not valid JSON for a known message type.
    #[error("control message is not valid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    /// The peer requested a protocol version this build does not implement.
    #[error("unsupported control protocol version {0}")]
    UnsupportedVersion(u32),
    /// A submission field was outside its accepted bounds.
    #[error("invalid submission: {0}")]
    InvalidSubmission(&'static str),
    /// A widget field was outside its accepted bounds.
    #[error("invalid widget message: {0}")]
    InvalidWidget(&'static str),
    /// The underlying local transport failed.
    #[error("control transport failed: {0}")]
    Io(#[from] io::Error),
}

/// Returns true when the value is a safe bounded protocol identifier.
///
/// One rule for submission, correlation, widget, and surface identifiers: a
/// bounded ASCII shape that is also safe as a window label and a file name.
pub fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_IDENTIFIER_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// Returns true when the payload encodes within [`MAX_WIDGET_DATA_BYTES`].
pub fn is_widget_data(value: &serde_json::Value) -> bool {
    serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= MAX_WIDGET_DATA_BYTES)
}

/// Returns true when the transcript is a bounded single submission payload.
///
/// `str::len` is the UTF-8 byte length, which is the metric the daemon uses.
pub fn is_submission_text(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_SUBMISSION_TEXT_BYTES
        && !value.contains(['\r', '\0'])
}

/// Reads and decodes one bounded LF-terminated JSON message.
pub fn read_message<T: for<'de> Deserialize<'de>>(
    reader: &mut impl BufRead,
) -> Result<T, MessageError> {
    let mut bytes = Vec::new();
    let mut limited = std::io::Read::take(reader, (MAX_MESSAGE_BYTES + 1) as u64);
    limited.read_until(b'\n', &mut bytes)?;
    if bytes.is_empty() {
        return Err(MessageError::Empty);
    }
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(MessageError::TooLarge);
    }
    if bytes.pop() != Some(b'\n') {
        return Err(MessageError::MissingTerminator);
    }
    if bytes.last() == Some(&b'\r') {
        return Err(MessageError::MissingTerminator);
    }
    Ok(serde_json::from_slice(&bytes)?)
}

/// Encodes and writes one bounded LF-terminated JSON message.
pub fn write_message<T: Serialize>(
    writer: &mut impl Write,
    message: &T,
) -> Result<(), MessageError> {
    let bytes = serde_json::to_vec(message)?;
    if bytes.len() + 1 > MAX_MESSAGE_BYTES {
        return Err(MessageError::TooLarge);
    }
    writer.write_all(&bytes)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

/// Returns an error unless every named identifier is well formed.
fn check_identifiers(fields: &[(&'static str, &str)]) -> Result<(), MessageError> {
    for (field, value) in fields {
        if !is_identifier(value) {
            return Err(MessageError::InvalidWidget(field));
        }
    }
    Ok(())
}

/// Reads one companion message and rejects unsupported versions and payloads.
pub fn read_client_message(reader: &mut impl BufRead) -> Result<ClientMessage, MessageError> {
    let message: ClientMessage = read_message(reader)?;
    if message.v != PROTOCOL_VERSION {
        return Err(MessageError::UnsupportedVersion(message.v));
    }
    match &message.body {
        ClientBody::Submit { id, text, .. } => {
            if !is_identifier(id) {
                return Err(MessageError::InvalidSubmission("id"));
            }
            if !is_submission_text(text) {
                return Err(MessageError::InvalidSubmission("text"));
            }
        }
        ClientBody::WidgetOpened { id, surface } => {
            check_identifiers(&[("id", id), ("surface", surface)])?;
        }
        ClientBody::WidgetDone { id } => check_identifiers(&[("id", id)])?,
        ClientBody::WidgetFailed { id, code, .. } => {
            check_identifiers(&[("id", id), ("code", code)])?;
        }
        ClientBody::WidgetEvent { surface, .. } => {
            check_identifiers(&[("surface", surface)])?;
        }
        ClientBody::Catalog { widgets } => {
            for widget in widgets {
                check_identifiers(&[("widget", &widget.id)])?;
            }
        }
        ClientBody::Hello | ClientBody::Ping => {}
    }
    Ok(message)
}

/// Reads one daemon message and rejects unsupported versions and payloads.
pub fn read_daemon_message(reader: &mut impl BufRead) -> Result<DaemonMessage, MessageError> {
    let message: DaemonMessage = read_message(reader)?;
    if message.v != PROTOCOL_VERSION {
        return Err(MessageError::UnsupportedVersion(message.v));
    }
    match &message.body {
        DaemonBody::WidgetOpen {
            id, widget, data, ..
        } => {
            check_identifiers(&[("id", id), ("widget", widget)])?;
            if !is_widget_data(data) {
                return Err(MessageError::InvalidWidget("data"));
            }
        }
        DaemonBody::WidgetUpdate { id, surface, data } => {
            check_identifiers(&[("id", id), ("surface", surface)])?;
            if !is_widget_data(data) {
                return Err(MessageError::InvalidWidget("data"));
            }
        }
        DaemonBody::WidgetClose { id, surface } => {
            check_identifiers(&[("id", id), ("surface", surface)])?;
        }
        DaemonBody::WidgetClear { id } => check_identifiers(&[("id", id)])?,
        DaemonBody::Welcome { .. }
        | DaemonBody::Ack { .. }
        | DaemonBody::Uncertain { .. }
        | DaemonBody::Refused { .. }
        | DaemonBody::State { .. }
        | DaemonBody::Pong => {}
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use serde_json::json;

    use super::*;

    fn encoded(message: &impl Serialize) -> String {
        let mut bytes = Vec::new();
        write_message(&mut bytes, message).unwrap();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn companion_messages_round_trip_as_one_json_line() {
        let submit = ClientMessage::new(ClientBody::Submit {
            id: "pill-1".into(),
            text: "open the tasks widget".into(),
            force: false,
        });
        // An ordinary submission carries no `force`: the wire says nothing
        // where nothing was decided.
        assert_eq!(
            encoded(&submit),
            "{\"v\":2,\"type\":\"submit\",\"id\":\"pill-1\",\"text\":\"open the tasks widget\"}\n"
        );
        let forced = ClientMessage::new(ClientBody::Submit {
            id: "pill-1".into(),
            text: "open the tasks widget".into(),
            force: true,
        });
        assert_eq!(
            encoded(&forced),
            "{\"v\":2,\"type\":\"submit\",\"id\":\"pill-1\",\"text\":\"open the tasks widget\",\"force\":true}\n"
        );
        assert_eq!(
            read_client_message(&mut Cursor::new(encoded(&forced))).unwrap(),
            forced
        );
        assert_eq!(
            read_client_message(&mut Cursor::new(encoded(&submit))).unwrap(),
            submit
        );
        assert_eq!(
            encoded(&ClientMessage::new(ClientBody::Hello)),
            "{\"v\":2,\"type\":\"hello\"}\n"
        );
        assert_eq!(
            encoded(&ClientMessage::new(ClientBody::Ping)),
            "{\"v\":2,\"type\":\"ping\"}\n"
        );
    }

    #[test]
    fn daemon_messages_use_the_documented_shapes() {
        assert_eq!(
            serde_json::to_value(DaemonMessage::new(DaemonBody::Welcome {
                session: "popup-1".into()
            }))
            .unwrap(),
            json!({ "v": 2, "type": "welcome", "session": "popup-1" })
        );
        assert_eq!(
            serde_json::to_value(DaemonMessage::new(DaemonBody::Ack {
                id: "pill-1".into()
            }))
            .unwrap(),
            json!({ "v": 2, "type": "ack", "id": "pill-1" })
        );
        assert_eq!(
            serde_json::to_value(DaemonMessage::new(DaemonBody::Refused {
                id: "pill-1".into(),
                detail: "the Scufris session is not ready".into(),
            }))
            .unwrap(),
            json!({
                "v": 2,
                "type": "refused",
                "id": "pill-1",
                "detail": "the Scufris session is not ready",
            })
        );
        assert_eq!(
            serde_json::to_value(DaemonMessage::new(DaemonBody::State {
                state: AssistantState::Attention,
                detail: "job 1 is blocked".into(),
            }))
            .unwrap(),
            json!({ "v": 2, "type": "state", "state": "attention", "detail": "job 1 is blocked" })
        );
        assert_eq!(
            serde_json::to_value(DaemonMessage::new(DaemonBody::Pong)).unwrap(),
            json!({ "v": 2, "type": "pong" })
        );
    }

    #[test]
    fn state_messages_accept_an_absent_detail() {
        let mut line = Cursor::new("{\"v\":2,\"type\":\"state\",\"state\":\"idle\"}\n");
        assert_eq!(
            read_daemon_message(&mut line).unwrap().body,
            DaemonBody::State {
                state: AssistantState::Idle,
                detail: String::new(),
            }
        );
    }

    #[test]
    fn unknown_message_types_are_rejected() {
        let mut line = Cursor::new("{\"v\":2,\"type\":\"mirror\",\"entries\":[]}\n");
        assert!(matches!(
            read_client_message(&mut line),
            Err(MessageError::InvalidJson(_))
        ));
        let mut line = Cursor::new("{\"v\":2,\"type\":\"transcript\"}\n");
        assert!(matches!(
            read_daemon_message(&mut line),
            Err(MessageError::InvalidJson(_))
        ));
    }

    #[test]
    fn other_protocol_versions_are_rejected() {
        // A version 1 peer is refused at hello. Nothing in the tree speaks it
        // any more, and half a protocol is worse than no connection.
        let mut line = Cursor::new("{\"v\":1,\"type\":\"hello\"}\n");
        assert!(matches!(
            read_client_message(&mut line),
            Err(MessageError::UnsupportedVersion(1))
        ));
        let mut line = Cursor::new("{\"v\":3,\"type\":\"ping\"}\n");
        assert!(matches!(
            read_client_message(&mut line),
            Err(MessageError::UnsupportedVersion(3))
        ));
        let mut line = Cursor::new("{\"v\":0,\"type\":\"pong\"}\n");
        assert!(matches!(
            read_daemon_message(&mut line),
            Err(MessageError::UnsupportedVersion(0))
        ));
    }

    #[test]
    fn widget_commands_and_answers_use_the_documented_shapes() {
        assert_eq!(
            encoded(&DaemonMessage::new(DaemonBody::WidgetOpen {
                id: "w-1".into(),
                widget: "note".into(),
                posture: Posture::Exhibit,
                data: json!({ "text": "the harness is green" }),
            })),
            "{\"v\":2,\"type\":\"widget_open\",\"id\":\"w-1\",\"widget\":\"note\",\
             \"posture\":\"exhibit\",\"data\":{\"text\":\"the harness is green\"}}\n"
        );
        assert_eq!(
            encoded(&DaemonMessage::new(DaemonBody::WidgetClear {
                id: "w-5".into()
            })),
            "{\"v\":2,\"type\":\"widget_clear\",\"id\":\"w-5\"}\n"
        );
        assert_eq!(
            encoded(&ClientMessage::new(ClientBody::WidgetOpened {
                id: "w-1".into(),
                surface: "widget-3".into(),
            })),
            "{\"v\":2,\"type\":\"widget_opened\",\"id\":\"w-1\",\"surface\":\"widget-3\"}\n"
        );
        assert_eq!(
            encoded(&ClientMessage::new(ClientBody::WidgetDone {
                id: "w-4".into()
            })),
            "{\"v\":2,\"type\":\"widget_done\",\"id\":\"w-4\"}\n"
        );
        assert_eq!(
            encoded(&ClientMessage::new(ClientBody::WidgetEvent {
                surface: "widget-3".into(),
                event: SurfaceEvent::Closed,
            })),
            "{\"v\":2,\"type\":\"widget_event\",\"surface\":\"widget-3\",\"event\":\"closed\"}\n"
        );
        assert_eq!(
            encoded(&ClientMessage::new(ClientBody::Catalog {
                widgets: vec![CatalogEntry {
                    id: "note".into(),
                    name: "Note".into(),
                    description: "A short note beside the pill.".into(),
                }],
            })),
            "{\"v\":2,\"type\":\"catalog\",\"widgets\":[{\"id\":\"note\",\"name\":\"Note\",\
             \"description\":\"A short note beside the pill.\"}]}\n"
        );
        // An omitted payload is the empty payload, so a command that carries
        // nothing does not have to say so.
        let mut line = Cursor::new(
            "{\"v\":2,\"type\":\"widget_update\",\"id\":\"w-2\",\"surface\":\"widget-3\"}\n",
        );
        assert_eq!(
            read_daemon_message(&mut line).unwrap().body,
            DaemonBody::WidgetUpdate {
                id: "w-2".into(),
                surface: "widget-3".into(),
                data: serde_json::Value::Null,
            }
        );
    }

    #[test]
    fn widget_messages_are_bounded_and_identified() {
        let mut line = Cursor::new(
            "{\"v\":2,\"type\":\"widget_close\",\"id\":\"w-1\",\"surface\":\"widget 3\"}\n",
        );
        assert!(matches!(
            read_daemon_message(&mut line),
            Err(MessageError::InvalidWidget("surface"))
        ));
        let mut line =
            Cursor::new("{\"v\":2,\"type\":\"widget_opened\",\"id\":\"w 1\",\"surface\":\"s\"}\n");
        assert!(matches!(
            read_client_message(&mut line),
            Err(MessageError::InvalidWidget("id"))
        ));
        let mut line = Cursor::new(
            "{\"v\":2,\"type\":\"catalog\",\"widgets\":[{\"id\":\"a b\",\"name\":\"A\",\"description\":\"x\"}]}\n",
        );
        assert!(matches!(
            read_client_message(&mut line),
            Err(MessageError::InvalidWidget("widget"))
        ));

        // The payload cap is smaller than the line cap on purpose: the same
        // bytes cross the companion's per-window channel afterwards.
        let oversized = DaemonMessage::new(DaemonBody::WidgetUpdate {
            id: "w-1".into(),
            surface: "widget-3".into(),
            data: json!({ "text": "x".repeat(MAX_WIDGET_DATA_BYTES) }),
        });
        let mut line = Cursor::new(encoded(&oversized));
        assert!(matches!(
            read_daemon_message(&mut line),
            Err(MessageError::InvalidWidget("data"))
        ));
        assert!(is_widget_data(&json!({ "text": "x".repeat(1024) })));
    }

    #[test]
    fn submissions_are_bounded_and_identified() {
        let mut line =
            Cursor::new("{\"v\":2,\"type\":\"submit\",\"id\":\"a b\",\"text\":\"hi\"}\n");
        assert!(matches!(
            read_client_message(&mut line),
            Err(MessageError::InvalidSubmission("id"))
        ));
        let mut line =
            Cursor::new("{\"v\":2,\"type\":\"submit\",\"id\":\"pill-1\",\"text\":\"  \"}\n");
        assert!(matches!(
            read_client_message(&mut line),
            Err(MessageError::InvalidSubmission("text"))
        ));
        assert!(is_identifier("pill-1.2_3"));
        assert!(!is_identifier(&"x".repeat(MAX_IDENTIFIER_LENGTH + 1)));
        assert!(!is_submission_text(
            &"x".repeat(MAX_SUBMISSION_TEXT_BYTES + 1)
        ));
    }

    #[test]
    fn the_transcript_bound_is_utf8_bytes_on_both_sides() {
        // The daemon measures the same way. Counting characters here would
        // accept text the daemon refuses, and counting UTF-16 units there
        // would accept text this side cannot read back.
        let cjk = "\u{4f60}\u{597d}";
        assert_eq!(cjk.chars().count(), 2);
        assert_eq!(cjk.len(), 6);
        let astral = "\u{1f600}";
        assert_eq!(astral.chars().count(), 1);
        assert_eq!(astral.len(), 4);

        // Exactly at the bound, in bytes.
        let filled = cjk.repeat(MAX_SUBMISSION_TEXT_BYTES / 6);
        assert_eq!(filled.len(), MAX_SUBMISSION_TEXT_BYTES - 2);
        assert!(is_submission_text(&filled));
        assert!(!is_submission_text(&format!("{filled}{cjk}")));
        assert!(is_submission_text(
            &astral.repeat(MAX_SUBMISSION_TEXT_BYTES / 4)
        ));
        assert!(!is_submission_text(
            &astral.repeat(MAX_SUBMISSION_TEXT_BYTES / 4 + 1)
        ));
    }

    #[test]
    fn framing_rejects_missing_terminators_and_oversized_lines() {
        assert!(matches!(
            read_message::<ClientMessage>(&mut Cursor::new(b"{\"v\":2,\"type\":\"ping\"}")),
            Err(MessageError::MissingTerminator)
        ));
        assert!(matches!(
            read_message::<ClientMessage>(&mut Cursor::new(b"{\"v\":2,\"type\":\"ping\"}\r\n")),
            Err(MessageError::MissingTerminator)
        ));
        assert!(matches!(
            read_message::<ClientMessage>(&mut Cursor::new(vec![b'x'; MAX_MESSAGE_BYTES + 1])),
            Err(MessageError::TooLarge)
        ));
        assert!(matches!(
            read_message::<ClientMessage>(&mut Cursor::new(Vec::new())),
            Err(MessageError::Empty)
        ));
    }

    #[test]
    fn shared_wire_fixtures_decode_the_same_way_on_both_sides() {
        // The daemon implements this protocol separately in TypeScript. Both
        // suites read these exact lines so the two implementations cannot drift.
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../../control-protocol-v2.json")).unwrap();
        let lines = |group: &str, side: &str| -> Vec<String> {
            fixtures[group][side]
                .as_array()
                .unwrap()
                .iter()
                .map(|line| line.as_str().unwrap().to_string())
                .collect()
        };

        for line in lines("canonical", "companion") {
            let message = read_client_message(&mut Cursor::new(format!("{line}\n"))).unwrap();
            assert_eq!(encoded(&message), format!("{line}\n"), "{line}");
        }
        for line in lines("canonical", "daemon") {
            let message = read_daemon_message(&mut Cursor::new(format!("{line}\n"))).unwrap();
            assert_eq!(encoded(&message), format!("{line}\n"), "{line}");
        }
        for line in lines("tolerated", "companion") {
            read_client_message(&mut Cursor::new(format!("{line}\n"))).expect(&line);
        }
        for line in lines("tolerated", "daemon") {
            read_daemon_message(&mut Cursor::new(format!("{line}\n"))).expect(&line);
        }
        for line in lines("rejected", "companion") {
            assert!(
                read_client_message(&mut Cursor::new(format!("{line}\n"))).is_err(),
                "accepted {line}"
            );
        }
        for line in lines("rejected", "daemon") {
            assert!(
                read_daemon_message(&mut Cursor::new(format!("{line}\n"))).is_err(),
                "accepted {line}"
            );
        }
    }

    #[test]
    fn socket_path_requires_a_runtime_directory() {
        assert_eq!(
            PathBuf::from("/run/user/1000")
                .join(SOCKET_DIRECTORY_NAME)
                .join(SOCKET_FILE_NAME),
            PathBuf::from("/run/user/1000/scufris/daemon.sock")
        );
    }
}
