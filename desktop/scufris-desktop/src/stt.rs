//! Local transcription against a whisper-server-compatible HTTP endpoint.
//!
//! The companion owns the audio and the request. Nothing is submitted when
//! transcription fails, so every failure path returns a short message the pill
//! shows instead of a transcript.

use std::time::{Duration, Instant};

use serde::Deserialize;
use thiserror::Error;

/// Multipart boundary used for every transcription request.
const BOUNDARY: &str = "scufris-desktop-boundary";

/// Longest a single transcription may take.
pub const TRANSCRIPTION_TIMEOUT: Duration = Duration::from_secs(60);

/// Longest transcript the companion accepts from the endpoint.
pub const MAX_TRANSCRIPT_BYTES: usize = 8 * 1024;

/// Failure to turn a recording into text.
#[derive(Debug, Error)]
pub enum TranscriptionError {
    /// The endpoint could not be reached or did not answer in time.
    #[error("Speech recognition is unreachable.")]
    Unreachable,
    /// The endpoint answered with a failure status.
    #[error("Speech recognition failed with status {0}.")]
    Status(u16),
    /// The endpoint answered with something other than a bounded transcript.
    #[error("Speech recognition returned no usable text.")]
    Unusable,
}

/// Returns the multipart content type header value for transcription requests.
pub fn content_type() -> String {
    format!("multipart/form-data; boundary={BOUNDARY}")
}

/// Builds the multipart body carrying one WAV recording.
pub fn multipart_body(wav: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(wav.len() + 512);
    let mut field = |name: &str, value: &str| {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    };
    field("response_format", "json");
    field("temperature", "0.0");
    body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"recording.wav\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: audio/wav\r\n\r\n");
    body.extend_from_slice(wav);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

#[derive(Debug, Deserialize)]
struct TranscriptPayload {
    text: Option<String>,
}

/// Extracts the transcript from one endpoint response body.
pub fn parse_transcript(body: &str) -> Result<String, TranscriptionError> {
    let payload: TranscriptPayload =
        serde_json::from_str(body).map_err(|_| TranscriptionError::Unusable)?;
    let text = payload.text.ok_or(TranscriptionError::Unusable)?;
    let text = text.trim();
    if text.is_empty() || text.len() > MAX_TRANSCRIPT_BYTES {
        return Err(TranscriptionError::Unusable);
    }
    Ok(text.to_string())
}

/// Transcribes one WAV recording through the configured endpoint.
///
/// Only sizes and timing reach the log; the transcript itself never does.
pub fn transcribe(endpoint: &str, wav: &[u8]) -> Result<String, TranscriptionError> {
    let started = Instant::now();
    let outcome = request(endpoint, wav);
    let elapsed_ms = started.elapsed().as_millis() as u64;
    match &outcome {
        Ok(text) => tracing::debug!(
            wav_bytes = wav.len(),
            elapsed_ms,
            transcript_bytes = text.len(),
            "transcribed"
        ),
        Err(error) => {
            tracing::error!(
                wav_bytes = wav.len(),
                elapsed_ms,
                "transcription failed: {error}"
            )
        }
    }
    outcome
}

fn request(endpoint: &str, wav: &[u8]) -> Result<String, TranscriptionError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(TRANSCRIPTION_TIMEOUT)
        .build()
        .map_err(|_| TranscriptionError::Unreachable)?;
    let response = client
        .post(endpoint)
        .header(reqwest::header::CONTENT_TYPE, content_type())
        .body(multipart_body(wav))
        .send()
        .map_err(|_| TranscriptionError::Unreachable)?;
    let status = response.status();
    if !status.is_success() {
        return Err(TranscriptionError::Status(status.as_u16()));
    }
    let body = response.text().map_err(|_| TranscriptionError::Unusable)?;
    parse_transcript(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_carries_the_recording_and_the_requested_format() {
        let body = multipart_body(b"RIFFfake");
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("name=\"response_format\"\r\n\r\njson"));
        assert!(text.contains("filename=\"recording.wav\""));
        assert!(text.contains("Content-Type: audio/wav"));
        assert!(text.contains("RIFFfake"));
        assert!(text.ends_with("--scufris-desktop-boundary--\r\n"));
        assert_eq!(
            content_type(),
            "multipart/form-data; boundary=scufris-desktop-boundary"
        );
    }

    #[test]
    fn a_transcript_is_trimmed_and_bounded() {
        assert_eq!(
            parse_transcript("{\"text\":\"  open the widget \\n\"}").unwrap(),
            "open the widget"
        );
        assert!(matches!(
            parse_transcript("{\"text\":\"   \"}"),
            Err(TranscriptionError::Unusable)
        ));
        assert!(matches!(
            parse_transcript("{}"),
            Err(TranscriptionError::Unusable)
        ));
        assert!(matches!(
            parse_transcript("not json"),
            Err(TranscriptionError::Unusable)
        ));
        let oversized = format!("{{\"text\":\"{}\"}}", "x".repeat(MAX_TRANSCRIPT_BYTES + 1));
        assert!(matches!(
            parse_transcript(&oversized),
            Err(TranscriptionError::Unusable)
        ));
    }

    #[test]
    fn failures_read_as_short_user_facing_sentences() {
        assert_eq!(
            TranscriptionError::Unreachable.to_string(),
            "Speech recognition is unreachable."
        );
        assert_eq!(
            TranscriptionError::Status(503).to_string(),
            "Speech recognition failed with status 503."
        );
    }
}
