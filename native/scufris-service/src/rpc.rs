//! The half of Pi's RPC protocol the service uses.
//!
//! Documented in `docs/rpc.md` of the installed package: LF-terminated JSON
//! lines on the child's stdin and stdout. Commands go down, events come up,
//! and a command that carried an `id` is answered by a `response` carrying the
//! same one.
//!
//! Only what the service acts on is typed here. Everything else decodes to
//! [`Event::Other`] and is dropped, which is the point: Pi adds events, and an
//! event nobody reads must not be an error. The same rule runs the other way
//! for fields, so a `response` that grows a member does not stop parsing.
//!
//! One thing in here is not optional. In RPC mode an extension that opens a
//! dialog blocks on an answer from stdin. The terminal answers those today. A
//! service that does not would hang the agent on the first `confirm`, so
//! [`is_dialog`] names the methods that must be answered.

use scufris_control::service::{MAX_TRANSCRIPT_TEXT_BYTES, Speaker, TranscriptEntry};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How a message that arrives while a run is in progress is delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Streaming {
    /// After the current turn's tool calls and before the next model call.
    #[serde(rename = "steer")]
    Steer,
}

/// One command the service sends the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    /// Asks which session the agent is on and whether it is streaming. Sent
    /// once at startup: the answer is what the service knows the session by.
    GetState {
        /// Correlation identifier the response echoes.
        id: String,
    },
    /// Sends one user message.
    Prompt {
        /// Correlation identifier the response echoes.
        id: String,
        /// The text to say.
        message: String,
        /// Absent when the agent is idle, and [`Streaming::Steer`] when a run
        /// is already in progress. Pi refuses a prompt during a run that does
        /// not say how it wants to be queued.
        #[serde(rename = "streamingBehavior", skip_serializing_if = "Option::is_none")]
        streaming_behavior: Option<Streaming>,
    },
    /// Ends the current run.
    Abort {
        /// Correlation identifier the response echoes.
        id: String,
    },
}

/// The answer to one extension dialog request.
///
/// Always a cancellation for now. The extension sees the same thing it sees
/// when a person presses Escape, which every caller already handles, and the
/// agent is never left waiting on a terminal that is not there. Routing these
/// to a frontend arrives with the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DialogAnswer {
    /// Always `extension_ui_response`.
    #[serde(rename = "type")]
    pub kind: &'static str,
    /// Identifier copied from the request.
    pub id: String,
    /// Always true.
    pub cancelled: bool,
}

impl DialogAnswer {
    /// Cancels one dialog.
    pub fn cancel(id: String) -> Self {
        Self {
            kind: "extension_ui_response",
            id,
            cancelled: true,
        }
    }
}

/// One event the service reads from the agent.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    /// Answers one command by its identifier.
    #[serde(rename = "response")]
    Response {
        /// Identifier copied from the command, when it carried one.
        #[serde(default)]
        id: Option<String>,
        /// Whether the command was accepted.
        success: bool,
        /// Why it was not, when it was not.
        #[serde(default)]
        error: Option<String>,
        /// Command-specific payload.
        #[serde(default)]
        data: Value,
    },
    /// The agent began processing.
    #[serde(rename = "agent_start")]
    AgentStart,
    /// The run settled: no retry, compaction, or queued message remains.
    #[serde(rename = "agent_settled")]
    AgentSettled,
    /// One complete message. The transcript is built from these.
    #[serde(rename = "message_end")]
    MessageEnd {
        /// The message.
        message: Value,
    },
    /// An extension wants the user interface.
    #[serde(rename = "extension_ui_request")]
    ExtensionUiRequest {
        /// Identifier a dialog answer must echo.
        id: String,
        /// Which method was called.
        method: String,
        /// The text of a `notify`, when this is one.
        #[serde(default)]
        message: Option<String>,
    },
    /// An extension threw.
    #[serde(rename = "extension_error")]
    ExtensionError {
        /// What it threw.
        #[serde(default)]
        error: Option<String>,
    },
    /// Anything else Pi emits. Read and dropped.
    #[serde(other)]
    Other,
}

/// Returns true for the extension methods that block until they are answered.
///
/// The fire-and-forget half (`notify`, `setStatus`, `setWidget`, `setTitle`,
/// `set_editor_text`) must not be answered: the agent is not waiting, and a
/// response to a request nobody made is a line Pi has to discard.
pub fn is_dialog(method: &str) -> bool {
    matches!(method, "select" | "confirm" | "input" | "editor")
}

/// What a `get_state` response says about the session.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionState {
    /// Absolute path of the session file, when Pi named one.
    pub file: Option<String>,
    /// True while a run is in progress.
    pub streaming: bool,
}

impl SessionState {
    /// Reads the fields the service uses out of a `get_state` payload.
    pub fn from_data(data: &Value) -> Self {
        Self {
            file: data
                .get("sessionFile")
                .and_then(Value::as_str)
                .filter(|file| std::path::Path::new(file).is_absolute())
                .map(str::to_string),
            streaming: data
                .get("isStreaming")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

/// Reads one line of the conversation out of a complete message.
///
/// Text only, and only from the two roles a person would recognise as the
/// conversation. Tool calls, tool results and thinking are in the session file
/// and reachable with `get_entries`; what a frontend needs when it connects is
/// the last screenful of what was said.
pub fn transcript_entry(message: &Value) -> Option<TranscriptEntry> {
    let speaker = match message.get("role").and_then(Value::as_str)? {
        "user" => Speaker::User,
        "assistant" => Speaker::Assistant,
        _ => return None,
    };
    let text = text_of(message.get("content")?);
    let text = text.trim();
    if text.is_empty() {
        // An assistant turn that only called tools said nothing.
        return None;
    }
    Some(TranscriptEntry {
        speaker,
        text: scufris_control::truncate(text, MAX_TRANSCRIPT_TEXT_BYTES),
    })
}

/// Joins the text blocks of one message's content.
fn text_of(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn event(line: &str) -> Event {
        serde_json::from_str(line).expect("the event decodes")
    }

    #[test]
    fn commands_use_the_shapes_the_documentation_gives() {
        assert_eq!(
            serde_json::to_string(&Command::GetState { id: "boot".into() }).unwrap(),
            r#"{"type":"get_state","id":"boot"}"#
        );
        assert_eq!(
            serde_json::to_string(&Command::Prompt {
                id: "p-1".into(),
                message: "hello".into(),
                streaming_behavior: None,
            })
            .unwrap(),
            r#"{"type":"prompt","id":"p-1","message":"hello"}"#
        );
        // The queueing rule is a camel-cased field of Pi's, not one of ours.
        assert_eq!(
            serde_json::to_string(&Command::Prompt {
                id: "p-2".into(),
                message: "actually, stop".into(),
                streaming_behavior: Some(Streaming::Steer),
            })
            .unwrap(),
            r#"{"type":"prompt","id":"p-2","message":"actually, stop","streamingBehavior":"steer"}"#
        );
        assert_eq!(
            serde_json::to_string(&Command::Abort { id: "a-1".into() }).unwrap(),
            r#"{"type":"abort","id":"a-1"}"#
        );
        assert_eq!(
            serde_json::to_string(&DialogAnswer::cancel("uuid-2".into())).unwrap(),
            r#"{"type":"extension_ui_response","id":"uuid-2","cancelled":true}"#
        );
    }

    #[test]
    fn the_events_the_service_acts_on_decode_and_the_rest_do_not_fail() {
        assert_eq!(event(r#"{"type":"agent_start"}"#), Event::AgentStart);
        assert_eq!(event(r#"{"type":"agent_settled"}"#), Event::AgentSettled);
        // Unknown events and unknown fields are both ordinary. Pi adds events,
        // and an event nobody reads must not take the connection down.
        assert_eq!(
            event(r#"{"type":"queue_update","pending":3}"#),
            Event::Other
        );
        assert_eq!(
            event(r#"{"type":"agent_end","messages":[],"willRetry":false}"#),
            Event::Other
        );
        assert_eq!(
            event(r#"{"type":"response","id":"p-1","command":"prompt","success":true}"#),
            Event::Response {
                id: Some("p-1".into()),
                success: true,
                error: None,
                data: Value::Null,
            }
        );
        assert_eq!(
            event(r#"{"type":"response","success":false,"error":"no"}"#),
            Event::Response {
                id: None,
                success: false,
                error: Some("no".into()),
                data: Value::Null,
            }
        );
        assert_eq!(
            event(r#"{"type":"extension_error","error":"boom"}"#),
            Event::ExtensionError {
                error: Some("boom".into()),
            }
        );
    }

    #[test]
    fn a_dialog_is_answered_and_a_notification_is_not() {
        // The half that blocks. Answering these is the difference between an
        // extension dialog and a hung agent.
        for method in ["select", "confirm", "input", "editor"] {
            assert!(is_dialog(method), "{method}");
        }
        for method in [
            "notify",
            "setStatus",
            "setWidget",
            "setTitle",
            "set_editor_text",
        ] {
            assert!(!is_dialog(method), "{method}");
        }
        assert_eq!(
            event(
                r#"{"type":"extension_ui_request","id":"u-5","method":"notify","message":"done","notifyType":"info"}"#
            ),
            Event::ExtensionUiRequest {
                id: "u-5".into(),
                method: "notify".into(),
                message: Some("done".into()),
            }
        );
        assert_eq!(
            event(
                r#"{"type":"extension_ui_request","id":"u-2","method":"confirm","title":"Clear?","timeout":5000}"#
            ),
            Event::ExtensionUiRequest {
                id: "u-2".into(),
                method: "confirm".into(),
                message: None,
            }
        );
    }

    #[test]
    fn the_session_the_agent_is_on_comes_out_of_its_first_answer() {
        let state = SessionState::from_data(&json!({
            "sessionFile": "/home/a/.local/share/scufris/sessions/one.jsonl",
            "sessionId": "abc",
            "isStreaming": true,
        }));
        assert_eq!(
            state,
            SessionState {
                file: Some("/home/a/.local/share/scufris/sessions/one.jsonl".into()),
                streaming: true,
            }
        );
        // A relative path is not a session the ctl could hand to a terminal
        // that starts somewhere else.
        assert_eq!(
            SessionState::from_data(&json!({ "sessionFile": "one.jsonl" })),
            SessionState::default()
        );
        assert_eq!(
            SessionState::from_data(&Value::Null),
            SessionState::default()
        );
    }

    #[test]
    fn the_transcript_takes_what_was_said_and_leaves_the_rest() {
        assert_eq!(
            transcript_entry(&json!({ "role": "user", "content": "what is on my calendar" })),
            Some(TranscriptEntry {
                speaker: Speaker::User,
                text: "what is on my calendar".into(),
            })
        );
        assert_eq!(
            transcript_entry(&json!({
                "role": "assistant",
                "content": [
                    { "type": "thinking", "thinking": "they want the calendar" },
                    { "type": "text", "text": "Two things today." },
                    { "type": "toolCall", "id": "c1", "name": "bash", "arguments": {} },
                ],
            })),
            Some(TranscriptEntry {
                speaker: Speaker::Assistant,
                text: "Two things today.".into(),
            })
        );
        // A turn that only called tools said nothing, and a tool result is not
        // part of the conversation.
        assert_eq!(
            transcript_entry(&json!({
                "role": "assistant",
                "content": [{ "type": "toolCall", "id": "c1", "name": "bash", "arguments": {} }],
            })),
            None
        );
        assert_eq!(
            transcript_entry(&json!({ "role": "toolResult", "content": [] })),
            None
        );
    }

    #[test]
    fn a_long_line_is_cut_on_a_character_and_not_in_the_middle_of_one() {
        let long = "\u{4f60}".repeat(MAX_TRANSCRIPT_TEXT_BYTES);
        let entry =
            transcript_entry(&json!({ "role": "user", "content": long })).expect("it is an entry");
        assert!(entry.text.len() <= MAX_TRANSCRIPT_TEXT_BYTES);
        // Cut on a boundary: the bound is not a multiple of three, so a naive
        // slice would panic and a naive byte copy would produce invalid UTF-8.
        assert_eq!(entry.text.len() % 3, 0);
        assert!(entry.text.chars().all(|glyph| glyph == '\u{4f60}'));
    }
}
