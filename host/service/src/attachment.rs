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
        if bytes.is_empty() || bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
            return Err(StoreError::Invalid("attachment bytes"));
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
        if metadata.len() == 0 || metadata.len() > MAX_ATTACHMENT_BYTES {
            return Err(StoreError::Invalid("attachment bytes"));
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
        if index.records.len() >= MAX_OBJECTS
            || index
                .bytes
                .checked_add(size)
                .is_none_or(|total| total > MAX_TOTAL_BYTES)
        {
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

    fn load(&self) -> Result<(), StoreError> {
        let current = now()?;
        let mut index = self.lock();
        for entry in fs::read_dir(&self.metadata)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err(StoreError::Corrupt);
            }
            if entry.file_name().to_string_lossy().starts_with('.') {
                fs::remove_file(entry.path())?;
                continue;
            }
            let record: Record = serde_json::from_slice(&fs::read(entry.path())?)?;
            validate_attachment_descriptor(&record.descriptor).map_err(|_| StoreError::Corrupt)?;
            let expected_name = format!("{}.json", record.descriptor.id);
            if entry.file_name() != expected_name.as_str() {
                return Err(StoreError::Corrupt);
            }
            let object = self.object_path(&record.descriptor.id);
            let object_metadata = fs::symlink_metadata(&object)?;
            if !object_metadata.file_type().is_file()
                || object_metadata.file_type().is_symlink()
                || object_metadata.len() != record.descriptor.size
            {
                return Err(StoreError::Corrupt);
            }
            let retention = if record.referenced {
                REFERENCED_RETENTION
            } else {
                UNREFERENCED_RETENTION
            };
            if current.saturating_sub(record.created_at) > retention.as_secs() {
                fs::remove_file(object)?;
                fs::remove_file(entry.path())?;
                continue;
            }
            if index.records.len() >= MAX_OBJECTS
                || index
                    .bytes
                    .checked_add(record.descriptor.size)
                    .is_none_or(|total| total > MAX_TOTAL_BYTES)
            {
                return Err(StoreError::Quota);
            }
            index.bytes += record.descriptor.size;
            index.records.insert(record.descriptor.id.clone(), record);
        }
        for entry in fs::read_dir(&self.objects)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err(StoreError::Corrupt);
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !index.records.contains_key(&name) {
                fs::remove_file(entry.path())?;
            }
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
        let mut file = private_file(&temporary)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
        fs::rename(temporary, final_path)?;
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
    let copied = io::copy(&mut reader.take(MAX_ATTACHMENT_BYTES + 1), &mut file)?;
    if copied != expected || copied == 0 || copied > MAX_ATTACHMENT_BYTES {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(StoreError::Invalid("attachment bytes"));
    }
    file.sync_all()?;
    Ok(())
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
                "invalid_attachment",
                "The attachment is invalid.",
            ),
            StoreError::TooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "attachment_too_large",
                "The attachment is too large.",
            ),
            StoreError::NotFound => (
                StatusCode::NOT_FOUND,
                "attachment_not_found",
                "The attachment is unavailable.",
            ),
            StoreError::Quota => (
                StatusCode::INSUFFICIENT_STORAGE,
                "attachment_quota",
                "Attachment storage is full.",
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "attachment_unavailable",
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
    let body = body.map_err(|_| ApiError(StoreError::TooLarge))?;
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
    let mut descriptors = store.resolve(&[id], false).map_err(ApiError)?;
    let descriptor = descriptors.remove(0);
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
                code: "invalid_range",
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
        assert!(matches!(
            store.import(&large, "application/pdf".into()),
            Err(StoreError::Invalid(_))
        ));
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
        assert_eq!(error["error"]["code"], "attachment_not_found");
        fs::remove_dir_all(root).unwrap();
    }
}
