//! Framing and paths shared by every Scufris socket.
//!
//! Two protocols are built on this, and neither of them is here.
//! [`service`] is version 3, the one `scufris-service` serves and the
//! companion, the agent and `scufris-ctl` all speak. [`command`] is the tiny
//! one the companion serves for the person's own window manager, where one verb
//! is one connection.
//!
//! What is here is what they share: one LF-terminated JSON line per message,
//! bounded by [`MAX_MESSAGE_BYTES`], and one rule for what an identifier may
//! be, so a peer that can read one can read them all.
//!
//! Version 2 is gone. It was the arrangement where the popup Pi process served
//! the socket and the companion connected to it, and nothing speaks it any
//! more.

use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod command;
pub mod service;

/// Maximum encoded message size, including its LF terminator.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// Directory below `XDG_RUNTIME_DIR` that holds the Scufris sockets.
pub const SOCKET_DIRECTORY_NAME: &str = "scufris";

/// Maximum accepted length of one protocol identifier.
///
/// Correlation, widget, and surface identifiers share one rule, so a peer that
/// can read one can read them all.
pub const MAX_IDENTIFIER_LENGTH: usize = 64;

/// Maximum accepted size of one submitted transcript, in UTF-8 bytes.
///
/// Bytes, not characters: the service measures the same way, so text either
/// side accepts is text both sides accept.
pub const MAX_SUBMISSION_TEXT_BYTES: usize = 8 * 1024;

/// Returns one socket path below the session runtime directory.
///
/// The directory is taken rather than read, so the rule can be tested without
/// a test setting a variable every other test in the process can see.
pub(crate) fn in_runtime_dir(
    runtime_dir: Option<std::ffi::OsString>,
    name: &str,
) -> Result<PathBuf, ControlPathError> {
    let runtime_dir = runtime_dir.ok_or(ControlPathError::MissingRuntimeDir)?;
    if runtime_dir.is_empty() {
        return Err(ControlPathError::MissingRuntimeDir);
    }
    Ok(PathBuf::from(runtime_dir)
        .join(SOCKET_DIRECTORY_NAME)
        .join(name))
}

/// Failure to resolve one of the current user's socket paths.
#[derive(Debug, Error)]
pub enum ControlPathError {
    /// The process has no non-empty `XDG_RUNTIME_DIR`.
    #[error("XDG_RUNTIME_DIR is required")]
    MissingRuntimeDir,
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
    /// The underlying local transport failed.
    #[error("control transport failed: {0}")]
    Io(#[from] io::Error),
}

/// Returns true when the value is a safe bounded protocol identifier.
///
/// One rule for correlation, widget, and surface identifiers: a bounded ASCII
/// shape that is also safe as a window label and a file name.
pub fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_IDENTIFIER_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// Returns true when the transcript is a bounded single submission payload.
///
/// `str::len` is the UTF-8 byte length, which is the metric the service uses.
pub fn is_submission_text(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_SUBMISSION_TEXT_BYTES
        && !value.contains(['\r', '\0'])
}

/// Shortens text to a byte bound without splitting a character.
///
/// The one implementation. Both protocols bound their free text in UTF-8
/// bytes, and cutting at an arbitrary index inside a multi-byte character
/// produces a string that is not text at all.
pub fn truncate(text: &str, bound: usize) -> String {
    if text.len() <= bound {
        return text.to_string();
    }
    let mut end = bound;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// Reads one bounded LF-terminated line, without decoding it.
///
/// Split out from [`read_message`] so a reader can look at the version before
/// it commits to a body shape. A peer speaking another version should be told
/// which version it spoke, not that its message did not parse.
pub fn read_line(reader: &mut impl BufRead) -> Result<Vec<u8>, MessageError> {
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
    Ok(bytes)
}

/// Reads and decodes one bounded LF-terminated JSON message.
pub fn read_message<T: for<'de> Deserialize<'de>>(
    reader: &mut impl BufRead,
) -> Result<T, MessageError> {
    Ok(serde_json::from_slice(&read_line(reader)?)?)
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

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn framing_rejects_missing_terminators_and_oversized_lines() {
        assert!(matches!(
            read_line(&mut Cursor::new(Vec::new())),
            Err(MessageError::Empty)
        ));
        assert!(matches!(
            read_line(&mut Cursor::new(b"{}".to_vec())),
            Err(MessageError::MissingTerminator)
        ));
        // A trailing CR is not framing this protocol has. Tolerating it would
        // put the CR inside the JSON on one side and not on the other.
        assert!(matches!(
            read_line(&mut Cursor::new(b"{}\r\n".to_vec())),
            Err(MessageError::MissingTerminator)
        ));
        assert!(matches!(
            read_line(&mut Cursor::new(vec![b'x'; MAX_MESSAGE_BYTES + 1])),
            Err(MessageError::TooLarge)
        ));
        assert_eq!(
            read_line(&mut Cursor::new(b"{\"v\":3}\n".to_vec())).unwrap(),
            b"{\"v\":3}"
        );
    }

    #[test]
    fn writing_refuses_a_message_no_peer_would_read() {
        let mut written = Vec::new();
        write_message(&mut written, &serde_json::json!({ "v": 3 })).unwrap();
        assert_eq!(written, b"{\"v\":3}\n");

        let oversized = "x".repeat(MAX_MESSAGE_BYTES);
        assert!(matches!(
            write_message(&mut Vec::new(), &serde_json::json!({ "text": oversized })),
            Err(MessageError::TooLarge)
        ));
    }

    #[test]
    fn identifiers_and_submissions_are_bounded_the_same_way_on_both_sides() {
        assert!(is_identifier("pill-1"));
        assert!(is_identifier("scufris_widget.3"));
        assert!(!is_identifier(""));
        assert!(!is_identifier("pill 1"));
        assert!(!is_identifier("pill/1"));
        assert!(!is_identifier(&"x".repeat(MAX_IDENTIFIER_LENGTH + 1)));

        assert!(is_submission_text("what is on my calendar"));
        assert!(!is_submission_text("   "));
        // Measured in UTF-8 bytes, because that is what the far end measures.
        assert!(is_submission_text(
            &"é".repeat(MAX_SUBMISSION_TEXT_BYTES / 2)
        ));
        assert!(!is_submission_text(
            &"é".repeat(MAX_SUBMISSION_TEXT_BYTES / 2 + 1)
        ));
        assert!(!is_submission_text("carriage\rreturn"));
    }

    #[test]
    fn truncation_cuts_on_a_character_boundary_and_never_inside_one() {
        assert_eq!(truncate("short", 64), "short");
        assert_eq!(truncate("abcdef", 3), "abc");
        // Three bytes each. A cut at 4 lands inside the second one and has to
        // walk back to 3, not produce half a character.
        assert_eq!(truncate("日本語", 4), "日");
        assert_eq!(truncate("日本語", 3), "日");
        assert_eq!(truncate("日本語", 2), "");
        assert_eq!(truncate("", 8), "");
    }

    #[test]
    fn a_socket_path_needs_a_runtime_directory() {
        assert_eq!(
            in_runtime_dir(Some("/run/user/1000".into()), "service.sock").unwrap(),
            PathBuf::from("/run/user/1000/scufris/service.sock")
        );
        assert!(matches!(
            in_runtime_dir(None, "service.sock"),
            Err(ControlPathError::MissingRuntimeDir)
        ));
        assert!(matches!(
            in_runtime_dir(Some("".into()), "service.sock"),
            Err(ControlPathError::MissingRuntimeDir)
        ));
    }
}
