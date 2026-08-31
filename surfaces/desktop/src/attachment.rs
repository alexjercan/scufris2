//! Local managed attachment import, download, open, and save operations.
//!
//! The service owns every byte. The desktop talks only to its private content
//! socket and keeps opaque IDs in composer state. Host paths never enter the
//! conversation protocol or logs.

use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

use reqwest::{StatusCode, blocking::Client, header};
use scufris_control::service::{
    AttachmentDescriptor, MAX_ATTACHMENT_BYTES, validate_attachment_descriptor,
};
use serde::Serialize;
#[cfg(test)]
use std::os::unix::fs::PermissionsExt;

const RESPONSE_BYTES: u64 = 16 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Serialize)]
struct ImportRequest<'a> {
    path: &'a Path,
    media_type: &'a str,
}

/// Client for the service-owned private attachment API.
pub struct AttachmentClient {
    client: Client,
}

impl AttachmentClient {
    pub fn new(content_socket: PathBuf) -> Result<Self, String> {
        let client = Client::builder()
            .unix_socket(content_socket)
            .timeout(TIMEOUT)
            .build()
            .map_err(|_| unavailable())?;
        Ok(Self { client })
    }

    /// Imports one selected host file without reading its bytes into the UI.
    pub fn import(&self, path: &Path) -> Result<AttachmentDescriptor, String> {
        let media_type = media_type(path);
        let body =
            serde_json::to_vec(&ImportRequest { path, media_type }).map_err(|_| unavailable())?;
        let response = self
            .client
            .post("http://localhost/attachments/import")
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .map_err(|_| unavailable())?;
        let status = response.status();
        let body = bounded(response, RESPONSE_BYTES)?;
        if status != StatusCode::OK {
            return Err(import_failure(status, &body));
        }
        let descriptor: AttachmentDescriptor =
            serde_json::from_slice(&body).map_err(|_| unavailable())?;
        validate_attachment_descriptor(&descriptor).map_err(|_| unavailable())?;
        Ok(descriptor)
    }

    /// Returns one bounded image or video for inline conversation presentation.
    pub fn presentation<'a>(
        &self,
        descriptor: &'a AttachmentDescriptor,
    ) -> Result<(&'a str, Vec<u8>), String> {
        let media_type = presentation_media_type(descriptor).ok_or_else(unavailable)?;
        Ok((media_type, self.download(descriptor)?))
    }

    /// Downloads canonical bytes and writes them to the selected destination.
    pub fn save(
        &self,
        descriptor: &AttachmentDescriptor,
        destination: &Path,
    ) -> Result<(), String> {
        let bytes = self.download(descriptor)?;
        atomic_write(destination, &bytes).map_err(|_| "The attachment could not be saved.".into())
    }

    fn download(&self, descriptor: &AttachmentDescriptor) -> Result<Vec<u8>, String> {
        validate_attachment_descriptor(descriptor).map_err(|_| unavailable())?;
        let response = self
            .client
            .get(format!("http://localhost/attachments/{}", descriptor.id))
            .send()
            .map_err(|_| unavailable())?;
        if response.status() != StatusCode::OK {
            return Err("The attachment is unavailable.".into());
        }
        if response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            != Some(descriptor.media_type.as_str())
        {
            return Err(unavailable());
        }
        let bytes = bounded(response, MAX_ATTACHMENT_BYTES + 1)?;
        if bytes.len() as u64 != descriptor.size || bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
            return Err(unavailable());
        }
        Ok(bytes)
    }
}

fn bounded(response: reqwest::blocking::Response, maximum: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    response
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() as u64 > maximum {
        return Err(unavailable());
    }
    Ok(bytes)
}

fn import_failure(status: StatusCode, body: &[u8]) -> String {
    let code = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.pointer("/error/code")?.as_str().map(str::to_owned));
    match (status, code.as_deref()) {
        (StatusCode::PAYLOAD_TOO_LARGE, _) | (_, Some("attachment_too_large")) => {
            "The attachment is larger than 16 MiB.".into()
        }
        (StatusCode::UNPROCESSABLE_ENTITY, _) | (_, Some("invalid_attachment")) => {
            "Choose a readable regular file with a valid name.".into()
        }
        (StatusCode::INSUFFICIENT_STORAGE, _) | (_, Some("attachment_quota")) => {
            "Attachment storage is full.".into()
        }
        _ => unavailable(),
    }
}

fn inline_media(media_type: &str) -> bool {
    (media_type.starts_with("image/") && media_type != "image/svg+xml")
        || media_type.starts_with("video/")
}

fn presentation_media_type(descriptor: &AttachmentDescriptor) -> Option<&str> {
    if inline_media(&descriptor.media_type) {
        return Some(&descriptor.media_type);
    }
    if descriptor.media_type != "application/octet-stream" {
        return None;
    }
    let inferred = media_type(Path::new(&descriptor.name));
    inline_media(inferred).then_some(inferred)
}

fn media_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("avif") => "image/avif",
        Some("bmp") => "image/bmp",
        Some("csv") => "text/csv",
        Some("gif") => "image/gif",
        Some("heic") => "image/heic",
        Some("html") => "text/html",
        Some("jpeg" | "jpg") => "image/jpeg",
        Some("json") => "application/json",
        Some("m4v") => "video/x-m4v",
        Some("md") => "text/markdown",
        Some("mkv") => "video/x-matroska",
        Some("mov") => "video/quicktime",
        Some("mp4") => "video/mp4",
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("txt") => "text/plain",
        Some("webm") => "video/webm",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

fn random_component() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| unavailable())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent"))?;
    let temporary = parent.join(format!(
        ".scufris-{}.tmp",
        random_component().map_err(std::io::Error::other)?
    ));
    let result = write_new(&temporary, bytes).and_then(|()| fs::rename(&temporary, path));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn unavailable() -> String {
    "Attachment storage is unavailable.".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_file_extensions_have_conservative_media_types() {
        assert_eq!(media_type(Path::new("IMAGE.JPEG")), "image/jpeg");
        assert_eq!(media_type(Path::new("clip.MP4")), "video/mp4");
        assert_eq!(media_type(Path::new("clip.mov")), "video/quicktime");
        assert_eq!(
            media_type(Path::new("archive.unknown")),
            "application/octet-stream"
        );
        assert_eq!(
            media_type(Path::new("no-extension")),
            "application/octet-stream"
        );
        assert!(inline_media("image/png"));
        assert!(inline_media("video/mp4"));
        assert!(!inline_media("image/svg+xml"));
        let mut descriptor = AttachmentDescriptor {
            id: "a".repeat(48),
            name: "answer.mp4".into(),
            media_type: "application/octet-stream".into(),
            size: 10,
        };
        assert_eq!(presentation_media_type(&descriptor), Some("video/mp4"));
        descriptor.name = "answer.bin".into();
        assert_eq!(presentation_media_type(&descriptor), None);
    }

    #[test]
    fn import_failures_do_not_expose_service_bodies() {
        assert_eq!(
            import_failure(
                StatusCode::UNPROCESSABLE_ENTITY,
                br#"{"error":{"code":"invalid_attachment","message":"private"}}"#,
            ),
            "Choose a readable regular file with a valid name."
        );
        assert_eq!(
            import_failure(StatusCode::INTERNAL_SERVER_ERROR, b"private"),
            "Attachment storage is unavailable."
        );
    }

    #[test]
    fn writes_are_private_and_atomic() {
        let root = std::env::temp_dir().join(format!(
            "scufris-desktop-save-{}",
            random_component().unwrap()
        ));
        fs::create_dir(&root).unwrap();
        let destination = root.join("answer.txt");
        atomic_write(&destination, b"answer").unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"answer");
        assert_eq!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
