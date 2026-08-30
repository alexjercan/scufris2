//! Local managed attachment import, download, open, and save operations.
//!
//! The service owns every byte. The desktop talks only to its private content
//! socket and keeps opaque IDs in composer state. Host paths never enter the
//! conversation protocol or logs.

use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use reqwest::{StatusCode, blocking::Client, header};
use scufris_control::service::{
    AttachmentDescriptor, MAX_ATTACHMENT_BYTES, validate_attachment_descriptor,
};
use serde::Serialize;

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
    cache: PathBuf,
}

impl AttachmentClient {
    pub fn new(content_socket: PathBuf) -> Result<Self, String> {
        let cache = content_socket
            .parent()
            .ok_or_else(unavailable)?
            .join("desktop-open");
        let client = Client::builder()
            .unix_socket(content_socket)
            .timeout(TIMEOUT)
            .build()
            .map_err(|_| unavailable())?;
        Ok(Self { client, cache })
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

    /// Downloads canonical bytes and writes them to the selected destination.
    pub fn save(
        &self,
        descriptor: &AttachmentDescriptor,
        destination: &Path,
    ) -> Result<(), String> {
        let bytes = self.download(descriptor)?;
        atomic_write(destination, &bytes).map_err(|_| "The attachment could not be saved.".into())
    }

    /// Downloads canonical bytes to a private cache file and opens its handler.
    pub fn open(&self, descriptor: &AttachmentDescriptor) -> Result<(), String> {
        if !safe_to_open(&descriptor.media_type) {
            return Err("Save this attachment before inspecting it.".into());
        }
        let bytes = self.download(descriptor)?;
        fs::create_dir_all(&self.cache).map_err(|_| open_failure())?;
        fs::set_permissions(&self.cache, fs::Permissions::from_mode(0o700))
            .map_err(|_| open_failure())?;
        let directory = self.cache.join(random_component()?);
        fs::create_dir(&directory).map_err(|_| open_failure())?;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(|_| open_failure())?;
        let path = directory.join(&descriptor.name);
        write_new(&path, &bytes).map_err(|_| open_failure())?;
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|_| open_failure())?;
        Ok(())
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

fn safe_to_open(media_type: &str) -> bool {
    media_type.starts_with("image/")
        || media_type.starts_with("text/")
        || media_type == "application/pdf"
        || media_type == "application/json"
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
        Some("md") => "text/markdown",
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("txt") => "text/plain",
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

fn open_failure() -> String {
    "The attachment could not be opened.".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_file_extensions_have_conservative_media_types() {
        assert_eq!(media_type(Path::new("IMAGE.JPEG")), "image/jpeg");
        assert_eq!(
            media_type(Path::new("archive.unknown")),
            "application/octet-stream"
        );
        assert_eq!(
            media_type(Path::new("no-extension")),
            "application/octet-stream"
        );
        assert!(safe_to_open("image/png"));
        assert!(safe_to_open("application/pdf"));
        assert!(!safe_to_open("application/x-executable"));
        assert!(!safe_to_open("application/octet-stream"));
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
