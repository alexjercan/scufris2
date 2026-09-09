//! Durable attachment bytes and the private local content API.

use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{
        DefaultBodyLimit, Path as AxumPath, Query, State,
        rejection::{BytesRejection, JsonRejection, QueryRejection},
    },
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use scufris_control::refusal;
use scufris_control::service::{
    AttachmentDescriptor, MAX_ATTACHMENT_BYTES, validate_attachment_descriptor,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

const MAX_OBJECTS: usize = 512;
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const UNREFERENCED_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
const REFERENCED_RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    descriptor: AttachmentDescriptor,
    created_at: u64,
    referenced: bool,
}

#[derive(Default)]
struct Index {
    records: HashMap<String, Record>,
    bytes: u64,
}

pub struct AttachmentStore {
    root: PathBuf,
    objects: PathBuf,
    metadata: PathBuf,
    index: Mutex<Index>,
}

impl AttachmentStore {
    pub fn open(root: PathBuf) -> Result<Arc<Self>, StoreError> {
        make_private_dir(&root)?;
        let objects = root.join("objects");
        let metadata = root.join("metadata");
        make_private_dir(&objects)?;
        make_private_dir(&metadata)?;
        let store = Arc::new(Self {
            root,
            objects,
            metadata,
            index: Mutex::new(Index::default()),
        });
        store.load()?;
        Ok(store)
    }

    pub fn put(
        &self,
        name: String,
        media_type: String,
        bytes: &[u8],
    ) -> Result<AttachmentDescriptor, StoreError> {
        if bytes.is_empty() {
            return Err(StoreError::Invalid("attachment bytes"));
        }
        if bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
            return Err(StoreError::TooLarge);
        }
        self.put_reader(name, media_type, bytes.len() as u64, bytes)
    }

    pub fn import(
        &self,
        path: &Path,
        media_type: String,
    ) -> Result<AttachmentDescriptor, StoreError> {
        if !path.is_absolute() {
            return Err(StoreError::Invalid("attachment path"));
        }
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(StoreError::Invalid("attachment path"));
        }
        if metadata.len() == 0 {
            return Err(StoreError::Invalid("attachment bytes"));
        }
        // `TooLarge` is the only error that reaches the picker and the model as
        // a size. Reported as `Invalid`, a 20 MB screen recording told Alex to
        // "choose a readable regular file", and the 16 MiB message that both
        // clients already carry was unreachable.
        if metadata.len() > MAX_ATTACHMENT_BYTES {
            return Err(StoreError::TooLarge);
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(StoreError::Invalid("attachment name"))?
            .to_owned();
        let mut file = File::open(path)?;
        let opened = file.metadata()?;
        if !opened.file_type().is_file()
            || opened.dev() != metadata.dev()
            || opened.ino() != metadata.ino()
            || opened.len() != metadata.len()
        {
            return Err(StoreError::Invalid("attachment path"));
        }
        self.put_reader(name, media_type, opened.len(), &mut file)
    }

    fn put_reader(
        &self,
        name: String,
        media_type: String,
        size: u64,
        mut reader: impl Read,
    ) -> Result<AttachmentDescriptor, StoreError> {
        let created_at = now()?;
        let descriptor = AttachmentDescriptor {
            id: new_id()?,
            name,
            media_type,
            size,
        };
        validate_attachment_descriptor(&descriptor)
            .map_err(|_| StoreError::Invalid("attachment descriptor"))?;

        let mut index = self.lock();
        let full = |index: &Index| {
            index.records.len() >= MAX_OBJECTS
                || index
                    .bytes
                    .checked_add(size)
                    .is_none_or(|total| total > MAX_TOTAL_BYTES)
        };
        // Expiry is worth its cost only when the quota is what would refuse
        // this put, and a store that is full of expired records has to be able
        // to clear itself without a restart.
        if full(&index) {
            self.expire(&mut index, created_at);
        }
        if full(&index) {
            return Err(StoreError::Quota);
        }
        let object = self.object_path(&descriptor.id);
        let object_temporary = self.objects.join(format!(".{}.tmp", descriptor.id));
        write_bounded(&object_temporary, &mut reader, size)?;
        fs::rename(&object_temporary, &object)?;

        let record = Record {
            descriptor: descriptor.clone(),
            created_at,
            referenced: false,
        };
        if let Err(error) = self.write_record(&record) {
            let _ = fs::remove_file(&object);
            return Err(error);
        }
        index.bytes += size;
        index.records.insert(descriptor.id.clone(), record);
        info!(
            attachment = descriptor.id,
            bytes = size,
            "attachment stored"
        );
        Ok(descriptor)
    }

    pub fn resolve(
        &self,
        ids: &[String],
        referenced: bool,
    ) -> Result<Vec<AttachmentDescriptor>, StoreError> {
        let mut index = self.lock();
        let mut records = Vec::with_capacity(ids.len());
        for id in ids {
            let record = index.records.get(id).ok_or(StoreError::NotFound)?;
            records.push(record.clone());
        }
        if referenced {
            for record in &mut records {
                if !record.referenced {
                    record.referenced = true;
                    self.write_record(record)?;
                    index
                        .records
                        .insert(record.descriptor.id.clone(), record.clone());
                }
            }
        }
        Ok(records
            .into_iter()
            .map(|record| record.descriptor)
            .collect())
    }

    /// The descriptor for a HEAD, checked against the object on disk.
    ///
    /// `resolve` answers from the index alone. A record whose object is gone or
    /// truncated answered HEAD with 200 and a full `Content-Length` and GET
    /// with 500, so a surface that probes before a ranged fetch was told the
    /// bytes were there and then failed on every range.
    pub fn stat(&self, id: &str) -> Result<AttachmentDescriptor, StoreError> {
        let descriptor = self
            .lock()
            .records
            .get(id)
            .map(|record| record.descriptor.clone())
            .ok_or(StoreError::NotFound)?;
        let object = fs::symlink_metadata(self.object_path(id))?;
        if !object.file_type().is_file() || object.len() != descriptor.size {
            return Err(StoreError::Corrupt);
        }
        Ok(descriptor)
    }

    pub fn read(&self, id: &str) -> Result<(AttachmentDescriptor, Vec<u8>), StoreError> {
        let descriptor = self
            .lock()
            .records
            .get(id)
            .map(|record| record.descriptor.clone())
            .ok_or(StoreError::NotFound)?;
        let bytes = fs::read(self.object_path(id))?;
        if bytes.len() as u64 != descriptor.size {
            return Err(StoreError::Corrupt);
        }
        Ok((descriptor, bytes))
    }

    /// Reads one metadata entry, or names why it cannot be trusted.
    fn read_record(&self, entry: &fs::DirEntry) -> Result<Record, &'static str> {
        if !entry.file_type().map_err(|_| "unreadable entry")?.is_file() {
            return Err("not a regular file");
        }
        let bytes = fs::read(entry.path()).map_err(|_| "unreadable record")?;
        let record: Record = serde_json::from_slice(&bytes).map_err(|_| "unreadable metadata")?;
        validate_attachment_descriptor(&record.descriptor).map_err(|_| "invalid descriptor")?;
        if entry.file_name() != format!("{}.json", record.descriptor.id).as_str() {
            return Err("misnamed record");
        }
        let object = fs::symlink_metadata(self.object_path(&record.descriptor.id))
            .map_err(|_| "missing object")?;
        if !object.file_type().is_file()
            || object.file_type().is_symlink()
            || object.len() != record.descriptor.size
        {
            return Err("object does not match its record");
        }
        Ok(record)
    }

    /// Removes both halves of one record.
    ///
    /// Metadata first, deliberately. An object with no record is swept at the
    /// next start, but a record with no object used to be fatal, so an
    /// interruption between the two deletions must leave the harmless half.
    fn discard(&self, id: &str) {
        let _ = fs::remove_file(self.metadata.join(format!("{id}.json")));
        let _ = fs::remove_file(self.object_path(id));
    }

    /// Drops every record past its retention.
    ///
    /// Retention used to be consulted only here, and only at startup, so a
    /// service that runs for weeks reached the quota and stayed there: every
    /// upload was refused until someone restarted it, and nothing said so.
    fn expire(&self, index: &mut Index, current: u64) {
        let expired: Vec<String> = index
            .records
            .values()
            .filter(|record| {
                let retention = if record.referenced {
                    REFERENCED_RETENTION
                } else {
                    UNREFERENCED_RETENTION
                };
                current.saturating_sub(record.created_at) > retention.as_secs()
            })
            .map(|record| record.descriptor.id.clone())
            .collect();
        for id in expired {
            let Some(record) = index.records.remove(&id) else {
                continue;
            };
            index.bytes = index.bytes.saturating_sub(record.descriptor.size);
            self.discard(&id);
            info!(attachment = id, "expired attachment removed");
        }
    }

    /// Reads the durable store into the index, dropping what it cannot trust.
    ///
    /// Every rejection here is a reason to drop one record, never a reason to
    /// refuse to start. `main` turns this error into a failed exit and the unit
    /// restarts on failure, so one orphaned metadata file used to take the
    /// conversation, the surfaces, and the control socket down for good, with
    /// `the attachment store would not open` as the only symptom.
    fn load(&self) -> Result<(), StoreError> {
        let current = now()?;
        let mut index = self.lock();
        let mut unindexed: Vec<String> = Vec::new();
        for entry in fs::read_dir(&self.metadata)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                let _ = fs::remove_file(&path);
                continue;
            }
            let record = match self.read_record(&entry) {
                Ok(record) => record,
                Err(reason) => {
                    warn!(record = %path.display(), reason, "attachment record dropped");
                    let _ = fs::remove_file(&path);
                    continue;
                }
            };
            let retention = if record.referenced {
                REFERENCED_RETENTION
            } else {
                UNREFERENCED_RETENTION
            };
            if current.saturating_sub(record.created_at) > retention.as_secs() {
                self.discard(&record.descriptor.id);
                continue;
            }
            if index.records.len() >= MAX_OBJECTS
                || index
                    .bytes
                    .checked_add(record.descriptor.size)
                    .is_none_or(|total| total > MAX_TOTAL_BYTES)
            {
                // A full store still opens. Refusing to start over an
                // attachment quota took the whole assistant down until the
                // oldest record aged out.
                unindexed.push(record.descriptor.id);
                continue;
            }
            index.bytes += record.descriptor.size;
            index.records.insert(record.descriptor.id.clone(), record);
        }
        if !unindexed.is_empty() {
            warn!(
                records = unindexed.len(),
                "the attachment store is full; these records were not indexed"
            );
        }
        for entry in fs::read_dir(&self.objects)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            // An object whose record exists but did not fit stays on disk, so
            // the quota does not silently delete what it declined to index.
            if index.records.contains_key(&name) || unindexed.contains(&name) {
                continue;
            }
            let _ = fs::remove_file(entry.path());
        }
        info!(root = %self.root.display(), attachments = index.records.len(), bytes = index.bytes, "attachment store opened");
        Ok(())
    }

    fn write_record(&self, record: &Record) -> Result<(), StoreError> {
        let final_path = self.metadata.join(format!("{}.json", record.descriptor.id));
        let temporary = self
            .metadata
            .join(format!(".{}.json.tmp", record.descriptor.id));
        let encoded = serde_json::to_vec(record)?;
        // `private_file` is `create_new`, so a temporary left behind by a
        // failed write made every later write for this id fail with EEXIST.
        // One full disk during `resolve` made an intact attachment permanently
        // unusable, reported as "the attachment is unavailable" every time.
        let result = private_file(&temporary).and_then(|mut file| {
            file.write_all(&encoded)?;
            file.sync_all()?;
            fs::rename(&temporary, &final_path)
        });
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        Ok(())
    }

    fn object_path(&self, id: &str) -> PathBuf {
        self.objects.join(id)
    }

    fn lock(&self) -> MutexGuard<'_, Index> {
        self.index.lock().unwrap_or_else(|held| held.into_inner())
    }
}

fn write_bounded(path: &Path, reader: &mut impl Read, expected: u64) -> Result<(), StoreError> {
    let mut file = private_file(path)?;
    // Every failure removes the temporary. An id is fresh per put, so a leak
    // here cost disk rather than correctness, but it leaked on the one path
    // that fails most: a short read from the source file.
    let result =
        io::copy(&mut reader.take(MAX_ATTACHMENT_BYTES + 1), &mut file).and_then(|copied| {
            if copied != expected || copied == 0 || copied > MAX_ATTACHMENT_BYTES {
                return Ok(None);
            }
            file.sync_all().map(|()| Some(()))
        });
    drop(file);
    match result {
        Ok(Some(())) => Ok(()),
        Ok(None) => {
            let _ = fs::remove_file(path);
            Err(StoreError::Invalid("attachment bytes"))
        }
        Err(error) => {
            let _ = fs::remove_file(path);
            Err(error.into())
        }
    }
}

fn private_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

fn make_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

fn new_id() -> Result<String, StoreError> {
    let mut random = [0u8; 24];
    getrandom::fill(&mut random).map_err(|_| StoreError::Random)?;
    let mut id = String::with_capacity(4 + random.len() * 2);
    id.push_str("att_");
    for byte in random {
        use std::fmt::Write as _;
        write!(id, "{byte:02x}").expect("writing to a string cannot fail");
    }
    Ok(id)
}

fn now() -> Result<u64, StoreError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::Clock)?
        .as_secs())
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("invalid {0}")]
    Invalid(&'static str),
    #[error("attachment not found")]
    NotFound,
    #[error("attachment exceeds its byte bound")]
    TooLarge,
    #[error("attachment body did not arrive")]
    Incomplete,
    #[error("attachment quota exceeded")]
    Quota,
    #[error("attachment store is corrupt")]
    Corrupt,
    #[error("system clock is invalid")]
    Clock,
    #[error("secure random generation failed")]
    Random,
    #[error("attachment I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("attachment metadata failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UploadQuery {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportRequest {
    path: PathBuf,
    media_type: String,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: &'static str,
    message: &'static str,
}

struct ApiError(StoreError);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self.0 {
            StoreError::Invalid(_) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                refusal::INVALID_ATTACHMENT,
                "The attachment is invalid.",
            ),
            StoreError::TooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                refusal::ATTACHMENT_TOO_LARGE,
                "The attachment is too large.",
            ),
            StoreError::Incomplete => (
                StatusCode::BAD_REQUEST,
                refusal::ATTACHMENT_INCOMPLETE,
                "The attachment did not finish uploading.",
            ),
            StoreError::NotFound => (
                StatusCode::NOT_FOUND,
                refusal::ATTACHMENT_NOT_FOUND,
                "The attachment is unavailable.",
            ),
            StoreError::Quota => (
                StatusCode::INSUFFICIENT_STORAGE,
                refusal::ATTACHMENT_QUOTA,
                "Attachment storage is full.",
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                refusal::ATTACHMENT_UNAVAILABLE,
                "Attachment storage is unavailable.",
            ),
        };
        (
            status,
            Json(ErrorEnvelope {
                error: ErrorBody { code, message },
            }),
        )
            .into_response()
    }
}

pub fn router(store: Arc<AttachmentStore>) -> Router {
    Router::new()
        .route("/attachments", post(upload))
        .route("/attachments/import", post(import_attachment))
        .route("/attachments/{id}", get(download).head(head))
        .layer(DefaultBodyLimit::max(MAX_ATTACHMENT_BYTES as usize))
        .with_state(store)
}

async fn upload(
    State(store): State<Arc<AttachmentStore>>,
    query: Result<Query<UploadQuery>, QueryRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<AttachmentDescriptor>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError(StoreError::Invalid("upload query")))?;
    // 413 only for a length limit. Anything else while buffering is a body
    // that never arrived, and reporting that as "too large" made Alex shrink a
    // photo whose upload his phone had simply dropped.
    let body = body.map_err(|rejection| {
        ApiError(if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            StoreError::TooLarge
        } else {
            StoreError::Incomplete
        })
    })?;
    let media_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError(StoreError::Invalid("media type")))?
        .to_owned();
    let descriptor = store.put(query.name, media_type, &body).map_err(ApiError)?;
    Ok(Json(descriptor))
}

async fn import_attachment(
    State(store): State<Arc<AttachmentStore>>,
    request: Result<Json<ImportRequest>, JsonRejection>,
) -> Result<Json<AttachmentDescriptor>, ApiError> {
    let Json(request) = request.map_err(|_| ApiError(StoreError::Invalid("import request")))?;
    let descriptor = store
        .import(&request.path, request.media_type)
        .map_err(ApiError)?;
    Ok(Json(descriptor))
}

async fn download(
    State(store): State<Arc<AttachmentStore>>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let (descriptor, bytes) = store.read(&id).map_err(ApiError)?;
    let range = match requested_range(&headers, descriptor.size) {
        Ok(range) => range,
        Err(()) => return range_not_satisfiable(descriptor.size),
    };
    match range {
        Some((start, end)) => {
            let selected = bytes[start as usize..=end as usize].to_vec();
            response(
                &descriptor,
                StatusCode::PARTIAL_CONTENT,
                selected.len() as u64,
                Some((start, end)),
                Body::from(selected),
            )
        }
        None => response(
            &descriptor,
            StatusCode::OK,
            descriptor.size,
            None,
            Body::from(bytes),
        ),
    }
}

async fn head(
    State(store): State<Arc<AttachmentStore>>,
    AxumPath(id): AxumPath<String>,
) -> Result<Response, ApiError> {
    let descriptor = store.stat(&id).map_err(ApiError)?;
    response(
        &descriptor,
        StatusCode::OK,
        descriptor.size,
        None,
        Body::empty(),
    )
}

fn requested_range(headers: &HeaderMap, size: u64) -> Result<Option<(u64, u64)>, ()> {
    let values = headers.get_all(header::RANGE);
    let mut values = values.iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(());
    }
    let value = value.to_str().map_err(|_| ())?;
    let range = value.strip_prefix("bytes=").ok_or(())?;
    if range.is_empty() || range.contains(',') {
        return Err(());
    }
    let (start, end) = range.split_once('-').ok_or(())?;
    if start.is_empty() {
        let suffix = end.parse::<u64>().map_err(|_| ())?;
        if suffix == 0 {
            return Err(());
        }
        return Ok(Some((size.saturating_sub(suffix), size - 1)));
    }
    let start = start.parse::<u64>().map_err(|_| ())?;
    if start >= size {
        return Err(());
    }
    let end = if end.is_empty() {
        size - 1
    } else {
        end.parse::<u64>().map_err(|_| ())?.min(size - 1)
    };
    if end < start {
        return Err(());
    }
    Ok(Some((start, end)))
}

fn range_not_satisfiable(size: u64) -> Result<Response, ApiError> {
    let mut response = (
        StatusCode::RANGE_NOT_SATISFIABLE,
        Json(ErrorEnvelope {
            error: ErrorBody {
                code: refusal::INVALID_RANGE,
                message: "The requested attachment range is unavailable.",
            },
        }),
    )
        .into_response();
    response.headers_mut().insert(
        header::CONTENT_RANGE,
        HeaderValue::from_str(&format!("bytes */{size}"))
            .map_err(|_| ApiError(StoreError::Corrupt))?,
    );
    response
        .headers_mut()
        .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    Ok(response)
}

fn response(
    descriptor: &AttachmentDescriptor,
    status: StatusCode,
    content_length: u64,
    range: Option<(u64, u64)>,
    body: Body,
) -> Result<Response, ApiError> {
    let media_type =
        HeaderValue::from_str(&descriptor.media_type).map_err(|_| ApiError(StoreError::Corrupt))?;
    let mut response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, media_type)
        .header(header::CONTENT_LENGTH, content_length)
        .header(header::ACCEPT_RANGES, "bytes")
        .header("x-content-type-options", "nosniff");
    if let Some((start, end)) = range {
        response = response.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{}", descriptor.size),
        );
    }
    response
        .body(body)
        .map_err(|_| ApiError(StoreError::Corrupt))
}

pub fn serve(
    listener: std::os::unix::net::UnixListener,
    store: Arc<AttachmentStore>,
) -> io::Result<thread::JoinHandle<()>> {
    listener.set_nonblocking(true)?;
    thread::Builder::new()
        .name("scufris-content-api".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    warn!(%error, "the content API runtime would not start");
                    return;
                }
            };
            runtime.block_on(async move {
                let listener = match tokio::net::UnixListener::from_std(listener) {
                    Ok(listener) => listener,
                    Err(error) => {
                        warn!(%error, "the content API listener would not start");
                        return;
                    }
                };
                if let Err(error) = axum::serve(listener, router(store)).await {
                    warn!(%error, "the content API stopped");
                }
            });
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, http::Request};
    use tower::ServiceExt;

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("scufris-attachment-{name}-{}", std::process::id()))
    }

    #[test]
    fn store_is_private_durable_and_resolves_only_owned_ids() {
        let root = root("durable");
        let _ = fs::remove_dir_all(&root);
        let store = AttachmentStore::open(root.clone()).unwrap();
        let descriptor = store
            .put("diagram.png".into(), "image/png".into(), b"image bytes")
            .unwrap();
        assert_eq!(
            fs::metadata(store.object_path(&descriptor.id))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let resolved = store
            .resolve(std::slice::from_ref(&descriptor.id), true)
            .unwrap();
        assert_eq!(resolved.as_slice(), std::slice::from_ref(&descriptor));
        drop(store);
        let reopened = AttachmentStore::open(root.clone()).unwrap();
        assert_eq!(reopened.read(&descriptor.id).unwrap().1, b"image bytes");
        assert!(matches!(
            reopened.resolve(&["att_missing".into()], false),
            Err(StoreError::NotFound)
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reopening_expires_old_unreferenced_content_and_cleans_orphans() {
        let root = root("expiry");
        let _ = fs::remove_dir_all(&root);
        let store = AttachmentStore::open(root.clone()).unwrap();
        let descriptor = store
            .put("old.png".into(), "image/png".into(), b"old")
            .unwrap();
        {
            let mut index = store.lock();
            let record = index.records.get_mut(&descriptor.id).unwrap();
            record.created_at = now().unwrap() - UNREFERENCED_RETENTION.as_secs() - 1;
            store.write_record(record).unwrap();
        }
        fs::write(store.objects.join("att_orphan"), b"orphan").unwrap();
        drop(store);

        let reopened = AttachmentStore::open(root.clone()).unwrap();
        assert!(matches!(
            reopened.read(&descriptor.id),
            Err(StoreError::NotFound)
        ));
        assert!(!reopened.objects.join("att_orphan").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn import_rejects_relative_symlink_directory_and_oversized_paths() {
        let root = root("import");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.pdf");
        fs::write(&source, b"pdf").unwrap();
        let link = root.join("link.pdf");
        std::os::unix::fs::symlink(&source, &link).unwrap();
        let store = AttachmentStore::open(root.join("store")).unwrap();
        assert!(store.import(&source, "application/pdf".into()).is_ok());
        for path in [Path::new("relative.pdf"), link.as_path(), root.as_path()] {
            assert!(matches!(
                store.import(path, "application/pdf".into()),
                Err(StoreError::Invalid(_))
            ));
        }
        let large = root.join("large.pdf");
        File::create(&large)
            .unwrap()
            .set_len(MAX_ATTACHMENT_BYTES + 1)
            .unwrap();
        // Size is the one rejection both clients can explain, and reporting it
        // as `Invalid` told Alex to choose a readable regular file instead.
        assert!(matches!(
            store.import(&large, "application/pdf".into()),
            Err(StoreError::TooLarge)
        ));
        assert!(matches!(
            store.put("empty.pdf".into(), "application/pdf".into(), b""),
            Err(StoreError::Invalid(_))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn one_inconsistent_record_costs_that_record_and_not_the_service() {
        // `main` turns an `open` failure into a failed exit that the unit
        // restarts, so a store that refused to open took the conversation, the
        // surfaces, and the control socket down permanently.
        let root = root("quarantine");
        let _ = fs::remove_dir_all(&root);
        let store = AttachmentStore::open(root.clone()).unwrap();
        let kept = store
            .put("kept.png".into(), "image/png".into(), b"kept")
            .unwrap();
        let orphan = store
            .put("orphan.png".into(), "image/png".into(), b"orphan")
            .unwrap();
        let truncated = store
            .put("short.png".into(), "image/png".into(), b"truncated")
            .unwrap();
        // Exactly what an interrupted expiry pass leaves behind.
        fs::remove_file(store.object_path(&orphan.id)).unwrap();
        fs::write(store.object_path(&truncated.id), b"cut").unwrap();
        fs::write(store.metadata.join("att_garbage.json"), b"not json").unwrap();
        fs::write(store.metadata.join("att_misnamed.json"), b"{}").unwrap();
        drop(store);

        let reopened = AttachmentStore::open(root.clone()).unwrap();
        assert_eq!(reopened.read(&kept.id).unwrap().1, b"kept");
        for lost in [&orphan.id, &truncated.id] {
            assert!(matches!(
                reopened.resolve(std::slice::from_ref(lost), false),
                Err(StoreError::NotFound)
            ));
            assert!(!reopened.metadata.join(format!("{lost}.json")).exists());
        }
        assert!(!reopened.metadata.join("att_garbage.json").exists());
        assert_eq!(reopened.lock().records.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_full_store_expires_rather_than_refusing_every_upload() {
        // Retention used to run only at startup, so a service that runs for
        // weeks reached the quota and stayed there until someone restarted it.
        let root = root("quota");
        let _ = fs::remove_dir_all(&root);
        let store = AttachmentStore::open(root.clone()).unwrap();
        let stale = store
            .put("stale.png".into(), "image/png".into(), b"stale")
            .unwrap();
        {
            let mut index = store.lock();
            let record = index.records.get_mut(&stale.id).unwrap();
            record.created_at = now().unwrap() - UNREFERENCED_RETENTION.as_secs() - 1;
            store.write_record(record).unwrap();
            // Stand in for a store at MAX_OBJECTS without writing 512 files.
            index.bytes = MAX_TOTAL_BYTES;
        }
        let fresh = store
            .put("fresh.png".into(), "image/png".into(), b"fresh")
            .unwrap();
        assert_eq!(store.read(&fresh.id).unwrap().1, b"fresh");
        assert!(matches!(
            store.resolve(std::slice::from_ref(&stale.id), false),
            Err(StoreError::NotFound)
        ));
        assert!(!store.object_path(&stale.id).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_failed_record_write_does_not_wedge_that_attachment() {
        // `private_file` is `create_new`, so a leftover temporary made every
        // later write for the id fail with EEXIST and the attachment read as
        // permanently unavailable.
        let root = root("temporary");
        let _ = fs::remove_dir_all(&root);
        let store = AttachmentStore::open(root.clone()).unwrap();
        let descriptor = store
            .put("note.txt".into(), "text/plain".into(), b"note")
            .unwrap();
        let temporary = store.metadata.join(format!(".{}.json.tmp", descriptor.id));
        fs::write(&temporary, b"leftover").unwrap();
        assert!(matches!(
            store.resolve(std::slice::from_ref(&descriptor.id), true),
            Err(StoreError::Io(_))
        ));
        assert!(!temporary.exists());
        // The retry succeeds because the failure cleaned up after itself.
        assert!(
            store
                .resolve(std::slice::from_ref(&descriptor.id), true)
                .is_ok()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn head_and_get_agree_about_a_missing_object() {
        let root = root("stat");
        let _ = fs::remove_dir_all(&root);
        let store = AttachmentStore::open(root.clone()).unwrap();
        let descriptor = store
            .put("clip.mp4".into(), "video/mp4".into(), b"clip")
            .unwrap();
        assert_eq!(store.stat(&descriptor.id).unwrap(), descriptor);
        fs::write(store.object_path(&descriptor.id), b"c").unwrap();
        assert!(matches!(
            store.stat(&descriptor.id),
            Err(StoreError::Corrupt)
        ));
        assert!(matches!(
            store.read(&descriptor.id),
            Err(StoreError::Corrupt)
        ));
        fs::remove_file(store.object_path(&descriptor.id)).unwrap();
        assert!(store.stat(&descriptor.id).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn private_api_uploads_imports_and_downloads_without_paths_in_descriptors() {
        let root = root("api");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("picked.pdf");
        fs::write(&source, b"pdf bytes").unwrap();
        let store = AttachmentStore::open(root.join("store")).unwrap();

        let upload = router(Arc::clone(&store))
            .oneshot(
                Request::post("/attachments?name=diagram.png")
                    .header(header::CONTENT_TYPE, "image/png")
                    .body(Body::from("png bytes"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(upload.status(), StatusCode::OK);
        let descriptor: AttachmentDescriptor =
            serde_json::from_slice(&to_bytes(upload.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(descriptor.name, "diagram.png");

        let import = router(Arc::clone(&store))
            .oneshot(
                Request::post("/attachments/import")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({"path": source, "media_type": "application/pdf"})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(import.status(), StatusCode::OK);

        let download = router(Arc::clone(&store))
            .oneshot(
                Request::get(format!("/attachments/{}", descriptor.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(download.status(), StatusCode::OK);
        assert_eq!(download.headers()[header::ACCEPT_RANGES], "bytes");
        assert_eq!(
            to_bytes(download.into_body(), 1024).await.unwrap(),
            "png bytes"
        );

        let partial = router(Arc::clone(&store))
            .oneshot(
                Request::get(format!("/attachments/{}", descriptor.id))
                    .header(header::RANGE, "bytes=4-7")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(partial.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(partial.headers()[header::CONTENT_RANGE], "bytes 4-7/9");
        assert_eq!(to_bytes(partial.into_body(), 1024).await.unwrap(), "byte");

        let suffix = router(Arc::clone(&store))
            .oneshot(
                Request::get(format!("/attachments/{}", descriptor.id))
                    .header(header::RANGE, "bytes=-3")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(to_bytes(suffix.into_body(), 1024).await.unwrap(), "tes");

        let invalid_range = router(Arc::clone(&store))
            .oneshot(
                Request::get(format!("/attachments/{}", descriptor.id))
                    .header(header::RANGE, "bytes=99-")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid_range.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(invalid_range.headers()[header::CONTENT_RANGE], "bytes */9");

        let head = router(Arc::clone(&store))
            .oneshot(
                Request::head(format!("/attachments/{}", descriptor.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(head.status(), StatusCode::OK);
        assert_eq!(head.headers()[header::CONTENT_LENGTH], "9");
        assert!(to_bytes(head.into_body(), 1024).await.unwrap().is_empty());

        let missing = router(store)
            .oneshot(
                Request::get("/attachments/att_missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let error: serde_json::Value =
            serde_json::from_slice(&to_bytes(missing.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(error["error"]["code"], refusal::ATTACHMENT_NOT_FOUND);
        fs::remove_dir_all(root).unwrap();
    }
}
