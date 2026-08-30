//! Authenticated loopback API for remote Scufris surfaces.

use std::{
    fs,
    io::{self, Cursor},
    net::{IpAddr, SocketAddr},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State, WebSocketUpgrade, ws},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use clap::Parser;
use futures_util::StreamExt;
use reqwest::{Client, Url, multipart};
use scufris_control::{
    MAX_MESSAGE_BYTES, MessageError,
    service::{SurfaceRequest, read_surface_request, read_surface_response, surface_socket_path},
};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, UnixStream},
};
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

const FAILED: u8 = 1;
const DEFAULT_LISTEN: &str = "127.0.0.1:10440";
const DEFAULT_AI_TOOLS_API: &str = "http://127.0.0.1:10300";
const MIN_TOKEN_BYTES: usize = 32;
const MAX_TOKEN_BYTES: usize = 256;
const MAX_AUDIO_BYTES: usize = 2 * 1024 * 1024;
const MAX_AUDIO_MILLISECONDS: u64 = 60_000;
const MAX_TRANSCRIPT_BYTES: usize = 8 * 1024;
const MAX_UPSTREAM_RESPONSE_BYTES: usize = 16 * 1024;
const TRANSCRIPTION_TIMEOUT: Duration = Duration::from_secs(130);
const TOKEN_VARIABLE: &str = "SCUFRIS_GATEWAY_TOKEN_FILE";
const LISTEN_VARIABLE: &str = "SCUFRIS_GATEWAY_LISTEN";
const AI_TOOLS_API_VARIABLE: &str = "SCUFRIS_GATEWAY_AI_TOOLS_API";
static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Parser)]
#[command(
    name = "scufris-surface-gateway",
    version,
    about = "Serve authenticated remote Scufris surface and audio APIs"
)]
struct Options {
    /// Loopback address to accept from a local TLS or tailnet proxy.
    #[arg(long, env = LISTEN_VARIABLE, default_value = DEFAULT_LISTEN)]
    listen: SocketAddr,

    /// Absolute mode-0600 file containing the bearer token.
    #[arg(long, env = TOKEN_VARIABLE, value_name = "PATH")]
    token_file: PathBuf,

    /// Loopback ai-tools-api base URL used for transcription.
    #[arg(
        long,
        env = AI_TOOLS_API_VARIABLE,
        default_value = DEFAULT_AI_TOOLS_API,
        value_name = "URL"
    )]
    ai_tools_api: Url,
}

#[derive(Clone)]
struct GatewayState {
    authorization: Arc<str>,
    surface_socket: Arc<Path>,
    ai_tools_api: Url,
    client: Client,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    service: &'static str,
    version: u8,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TranscriptResponse {
    text: String,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    error: ApiErrorBody,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    code: &'static str,
    message: &'static str,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
}

impl ApiError {
    const fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorEnvelope {
                error: ApiErrorBody {
                    code: self.code,
                    message: self.message,
                },
            }),
        )
            .into_response()
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(error) = run().await {
        eprintln!("scufris-surface-gateway: {error}");
        ExitCode::from(FAILED)
    } else {
        ExitCode::SUCCESS
    }
}

async fn run() -> Result<(), GatewayError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init()
        .map_err(|error| {
            GatewayError::Configuration(format!("logging initialization failed: {error}"))
        })?;
    let options = Options::parse();
    ensure_loopback(options.listen.ip())?;
    ensure_loopback_url(&options.ai_tools_api)?;
    let token = read_token(&options.token_file)?;
    let surface_socket =
        surface_socket_path().map_err(|error| GatewayError::Configuration(error.to_string()))?;
    let client = Client::builder().timeout(TRANSCRIPTION_TIMEOUT).build()?;
    let state = GatewayState {
        authorization: format!("Bearer {token}").into(),
        surface_socket: surface_socket.into(),
        ai_tools_api: options.ai_tools_api,
        client,
    };
    let listener = TcpListener::bind(options.listen).await?;
    info!(listen = %options.listen, "the remote surface API is listening");
    axum::serve(listener, router(state)).await?;
    Ok(())
}

fn router(state: GatewayState) -> Router {
    Router::new()
        .route("/", get(surface_websocket))
        .route("/surface", get(surface_websocket))
        .route("/health", get(health))
        .route("/audio/transcription", post(transcribe))
        .layer(DefaultBodyLimit::max(MAX_AUDIO_BYTES))
        .with_state(state)
}

async fn health(
    State(state): State<GatewayState>,
    headers: HeaderMap,
) -> Result<Json<HealthResponse>, ApiError> {
    authorize(&headers, &state)?;
    Ok(Json(HealthResponse {
        service: "scufris-surface-gateway",
        version: 1,
    }))
}

async fn surface_websocket(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    authorize(&headers, &state)?;
    let connection = NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed);
    Ok(upgrade
        .max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| async move {
            info!(connection, "remote surface authenticated");
            if let Err(error) = serve_surface(socket, &state, connection).await {
                warn!(connection, %error, "remote surface disconnected");
            }
        }))
}

async fn serve_surface(
    mut websocket: ws::WebSocket,
    state: &GatewayState,
    _connection: u64,
) -> Result<(), GatewayError> {
    let local = UnixStream::connect(state.surface_socket.as_ref()).await?;
    let (local_reader, mut local_writer) = local.into_split();
    let mut local_reader = BufReader::new(local_reader);
    let mut local_frame = Vec::with_capacity(MAX_MESSAGE_BYTES);

    loop {
        tokio::select! {
            remote = websocket.recv() => {
                match remote {
                    Some(Ok(ws::Message::Text(text))) => {
                        let request = match decode_request(text.as_str()) {
                            Ok(request) => request,
                            Err(error) => {
                                close(&mut websocket, 1008, "invalid surface message").await;
                                return Err(error.into());
                            }
                        };
                        let mut encoded = serde_json::to_vec(&request)?;
                        encoded.push(b'\n');
                        local_writer.write_all(&encoded).await?;
                    }
                    Some(Ok(ws::Message::Ping(payload))) => {
                        websocket.send(ws::Message::Pong(payload)).await?;
                    }
                    Some(Ok(ws::Message::Pong(_))) => {}
                    Some(Ok(ws::Message::Close(_))) | None => return Ok(()),
                    Some(Ok(ws::Message::Binary(_))) => {
                        close(&mut websocket, 1003, "text frames required").await;
                        return Ok(());
                    }
                    Some(Err(error)) => return Err(error.into()),
                }
            }
            read = local_reader.read_until(b'\n', &mut local_frame) => {
                let read = read?;
                if read == 0 {
                    close(&mut websocket, 1001, "service unavailable").await;
                    return Ok(());
                }
                if local_frame.len() > MAX_MESSAGE_BYTES {
                    close(&mut websocket, 1001, "service unavailable").await;
                    return Err(MessageError::TooLarge.into());
                }
                let response = read_surface_response(&mut Cursor::new(&local_frame))?;
                local_frame.clear();
                let text = serde_json::to_string(&response)?;
                websocket.send(ws::Message::Text(text.into())).await?;
            }
        }
    }
}

async fn close(websocket: &mut ws::WebSocket, code: u16, reason: &'static str) {
    let _ = websocket
        .send(ws::Message::Close(Some(ws::CloseFrame {
            code,
            reason: reason.into(),
        })))
        .await;
}

async fn transcribe(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    body: Result<Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<TranscriptResponse>, ApiError> {
    authorize(&headers, &state)?;
    let content_type = headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if content_type.split(';').next() != Some("audio/wav") {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "invalid_content_type",
            "Send a mono PCM WAV recording.",
        ));
    }
    let audio = body.map_err(|_| {
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "audio_too_large",
            "The recording is too large.",
        )
    })?;
    validate_wav(&audio)?;

    let part = multipart::Part::bytes(audio.to_vec())
        .file_name("dictation.wav")
        .mime_str("audio/wav")
        .map_err(|_| upstream_failure())?;
    let form = multipart::Form::new()
        .part("file", part)
        .text("model", "whisper-1")
        .text("response_format", "json");
    let endpoint = state
        .ai_tools_api
        .join("v1/audio/transcriptions")
        .map_err(|_| upstream_failure())?;
    let response = state
        .client
        .post(endpoint)
        .multipart(form)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                ApiError::new(
                    StatusCode::GATEWAY_TIMEOUT,
                    "transcription_timeout",
                    "Transcription timed out.",
                )
            } else {
                upstream_failure()
            }
        })?;
    if !response.status().is_success() {
        debug!(status = %response.status(), "ai-tools-api rejected transcription");
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "transcription_failed",
            "The host could not transcribe this recording.",
        ));
    }
    let bytes = bounded_response(response).await?;
    let transcript: TranscriptResponse =
        serde_json::from_slice(&bytes).map_err(|_| upstream_failure())?;
    validate_transcript(&transcript.text)?;
    info!(audio_bytes = audio.len(), "remote transcription completed");
    Ok(Json(transcript))
}

async fn bounded_response(response: reqwest::Response) -> Result<Vec<u8>, ApiError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_UPSTREAM_RESPONSE_BYTES as u64)
    {
        return Err(upstream_failure());
    }
    let mut stream = response.bytes_stream();
    let mut output = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| upstream_failure())?;
        if output.len() + chunk.len() > MAX_UPSTREAM_RESPONSE_BYTES {
            return Err(upstream_failure());
        }
        output.extend_from_slice(&chunk);
    }
    Ok(output)
}

fn validate_wav(audio: &[u8]) -> Result<(), ApiError> {
    if audio.len() < 44 || &audio[..4] != b"RIFF" || &audio[8..12] != b"WAVE" {
        return Err(invalid_audio());
    }
    let declared =
        u32::from_le_bytes(audio[4..8].try_into().map_err(|_| invalid_audio())?) as usize;
    if declared
        .checked_add(8)
        .is_none_or(|size| size > audio.len())
    {
        return Err(invalid_audio());
    }

    let mut offset = 12usize;
    let mut byte_rate = None;
    let mut data_bytes = None;
    while offset.checked_add(8).is_some_and(|end| end <= audio.len()) {
        let name = &audio[offset..offset + 4];
        let size = u32::from_le_bytes(
            audio[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| invalid_audio())?,
        ) as usize;
        let start = offset + 8;
        let end = start.checked_add(size).ok_or_else(invalid_audio)?;
        if end > audio.len() {
            return Err(invalid_audio());
        }
        if name == b"fmt " {
            if size < 16 {
                return Err(invalid_audio());
            }
            let format = u16::from_le_bytes(
                audio[start..start + 2]
                    .try_into()
                    .map_err(|_| invalid_audio())?,
            );
            let channels = u16::from_le_bytes(
                audio[start + 2..start + 4]
                    .try_into()
                    .map_err(|_| invalid_audio())?,
            );
            let sample_rate = u32::from_le_bytes(
                audio[start + 4..start + 8]
                    .try_into()
                    .map_err(|_| invalid_audio())?,
            );
            let rate = u32::from_le_bytes(
                audio[start + 8..start + 12]
                    .try_into()
                    .map_err(|_| invalid_audio())?,
            );
            let block_alignment = u16::from_le_bytes(
                audio[start + 12..start + 14]
                    .try_into()
                    .map_err(|_| invalid_audio())?,
            );
            let bits = u16::from_le_bytes(
                audio[start + 14..start + 16]
                    .try_into()
                    .map_err(|_| invalid_audio())?,
            );
            if format != 1
                || channels != 1
                || sample_rate != 16_000
                || rate != 32_000
                || block_alignment != 2
                || bits != 16
            {
                return Err(invalid_audio());
            }
            byte_rate = Some(u64::from(rate));
        } else if name == b"data" {
            data_bytes = Some(size as u64);
        }
        offset = end + (size & 1);
    }
    let (Some(byte_rate), Some(data_bytes)) = (byte_rate, data_bytes) else {
        return Err(invalid_audio());
    };
    if data_bytes == 0
        || data_bytes
            .saturating_mul(1_000)
            .checked_div(byte_rate)
            .is_none_or(|duration| duration > MAX_AUDIO_MILLISECONDS)
    {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_audio_duration",
            "Record no more than 60 seconds.",
        ));
    }
    Ok(())
}

fn validate_transcript(text: &str) -> Result<(), ApiError> {
    if text.trim().is_empty()
        || text.len() > MAX_TRANSCRIPT_BYTES
        || text
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
    {
        Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "invalid_transcript",
            "The host returned an invalid transcript.",
        ))
    } else {
        Ok(())
    }
}

fn invalid_audio() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_audio",
        "The recording is not a valid mono WAV file.",
    )
}

fn upstream_failure() -> ApiError {
    ApiError::new(
        StatusCode::BAD_GATEWAY,
        "transcription_unavailable",
        "Host transcription is unavailable.",
    )
}

fn authorize(headers: &HeaderMap, state: &GatewayState) -> Result<(), ApiError> {
    let authorized = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| constant_time_eq(value.as_bytes(), state.authorization.as_bytes()));
    if authorized {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "A valid bearer token is required.",
        ))
    }
}

fn decode_request(text: &str) -> Result<SurfaceRequest, MessageError> {
    if text.len() + 1 > MAX_MESSAGE_BYTES || text.contains(['\n', '\r']) {
        return Err(MessageError::TooLarge);
    }
    let mut framed = Vec::with_capacity(text.len() + 1);
    framed.extend_from_slice(text.as_bytes());
    framed.push(b'\n');
    read_surface_request(&mut Cursor::new(framed))
}

fn ensure_loopback(address: IpAddr) -> Result<(), GatewayError> {
    if address.is_loopback() {
        Ok(())
    } else {
        Err(GatewayError::Configuration(format!(
            "{LISTEN_VARIABLE} must be loopback, not {address}"
        )))
    }
}

fn ensure_loopback_url(url: &Url) -> Result<(), GatewayError> {
    let loopback = url
        .host_str()
        .and_then(|host| host.parse::<IpAddr>().ok())
        .is_some_and(|address| address.is_loopback())
        || url.host_str() == Some("localhost");
    if url.scheme() == "http" && loopback && !url.cannot_be_a_base() {
        Ok(())
    } else {
        Err(GatewayError::Configuration(format!(
            "{AI_TOOLS_API_VARIABLE} must be a loopback HTTP base URL"
        )))
    }
}

fn read_token(path: &Path) -> Result<String, GatewayError> {
    if !path.is_absolute() {
        return Err(GatewayError::Configuration(format!(
            "{TOKEN_VARIABLE} must be an absolute path"
        )));
    }
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(GatewayError::Configuration(format!(
            "{} must be a private regular file",
            path.display()
        )));
    }
    let bytes = fs::read(path)?;
    let token = std::str::from_utf8(&bytes)
        .map_err(|_| GatewayError::Configuration("the gateway token is not UTF-8".into()))?
        .strip_suffix('\n')
        .unwrap_or(std::str::from_utf8(&bytes).expect("already decoded"));
    if token.len() < MIN_TOKEN_BYTES
        || token.len() > MAX_TOKEN_BYTES
        || !token.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(GatewayError::Configuration(format!(
            "the gateway token must be {MIN_TOKEN_BYTES}..={MAX_TOKEN_BYTES} visible ASCII bytes"
        )));
    }
    Ok(token.to_owned())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        let left_byte = left.get(index).copied().unwrap_or(0);
        let right_byte = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(left_byte ^ right_byte);
    }
    difference == 0
}

#[derive(Debug, thiserror::Error)]
enum GatewayError {
    #[error("{0}")]
    Configuration(String),
    #[error("I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("protocol failed: {0}")]
    Protocol(#[from] MessageError),
    #[error("WebSocket failed: {0}")]
    WebSocket(#[from] axum::Error),
    #[error("HTTP client failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
        routing::post,
    };
    use scufris_control::{
        service::{SurfaceRegistration, SurfaceRequestBody, SurfaceResponse, SurfaceResponseBody},
        write_message,
    };
    use tower::ServiceExt;
    use tungstenite::{Message, client::IntoClientRequest};

    #[test]
    fn only_loopback_listeners_and_inference_urls_are_allowed() {
        assert!(ensure_loopback("127.0.0.1".parse().unwrap()).is_ok());
        assert!(ensure_loopback("::1".parse().unwrap()).is_ok());
        assert!(ensure_loopback("0.0.0.0".parse().unwrap()).is_err());
        assert!(ensure_loopback_url(&Url::parse("http://127.0.0.1:10300").unwrap()).is_ok());
        assert!(ensure_loopback_url(&Url::parse("http://localhost:10300").unwrap()).is_ok());
        assert!(ensure_loopback_url(&Url::parse("https://127.0.0.1:10300").unwrap()).is_err());
        assert!(ensure_loopback_url(&Url::parse("http://100.64.0.1:10300").unwrap()).is_err());
    }

    #[test]
    fn bearer_comparison_checks_content_and_length() {
        assert!(constant_time_eq(b"Bearer abc", b"Bearer abc"));
        assert!(!constant_time_eq(b"Bearer abc", b"Bearer abd"));
        assert!(!constant_time_eq(b"Bearer abc", b"Bearer abc0"));
    }

    #[test]
    fn websocket_payloads_use_the_strict_surface_decoder() {
        let request =
            decode_request(r#"{"v":4,"type":"surface.message","id":"ios-1","text":"hello"}"#)
                .unwrap();
        assert!(matches!(request.body, SurfaceRequestBody::Message { .. }));
        assert!(decode_request(r#"{"v":3,"type":"surface.hello"}"#).is_err());
        assert!(decode_request("{}\n{}").is_err());
    }

    #[tokio::test]
    async fn authenticated_websocket_still_bridges_strict_surface_v4() {
        let root =
            std::env::temp_dir().join(format!("scufris-async-gateway-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let socket_path = root.join("surface.sock");
        let unix_listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let local = std::thread::spawn(move || {
            let (stream, _) = unix_listener.accept().unwrap();
            let mut reader = io::BufReader::new(stream.try_clone().unwrap());
            let request = read_surface_request(&mut reader).unwrap();
            assert!(matches!(request.body, SurfaceRequestBody::Hello { .. }));
            write_message(
                &mut &stream,
                &SurfaceResponse::new(SurfaceResponseBody::Ready {
                    surface: "ios-test".into(),
                }),
            )
            .unwrap();
        });
        let state = GatewayState {
            authorization: "Bearer test-token".into(),
            surface_socket: socket_path.into(),
            ai_tools_api: Url::parse(DEFAULT_AI_TOOLS_API).unwrap(),
            client: Client::new(),
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server =
            tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });

        tokio::task::spawn_blocking(move || {
            let mut request = format!("ws://{address}/").into_client_request().unwrap();
            request
                .headers_mut()
                .insert("authorization", "Bearer test-token".parse().unwrap());
            let (mut websocket, _) = tungstenite::connect(request).unwrap();
            let hello = SurfaceRequest::new(SurfaceRequestBody::Hello {
                surface: SurfaceRegistration {
                    id: "ios-test".into(),
                    name: "iPhone".into(),
                    widgets: vec![],
                },
            });
            websocket
                .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
                .unwrap();
            let response = websocket.read().unwrap().into_text().unwrap();
            let mut framed = Cursor::new(format!("{response}\n"));
            assert!(matches!(
                read_surface_response(&mut framed).unwrap().body,
                SurfaceResponseBody::Ready { surface } if surface == "ios-test"
            ));
            websocket.close(None).unwrap();
        })
        .await
        .unwrap();

        local.join().unwrap();
        server.abort();
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn api_authenticates_health_and_forwards_bounded_wav_to_ai_tools() {
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_address = upstream.local_addr().unwrap();
        let upstream_app = Router::new().route(
            "/v1/audio/transcriptions",
            post(|body: Bytes| async move {
                assert!(body.windows(9).any(|part| part == b"whisper-1"));
                assert!(body.windows(4).any(|part| part == b"RIFF"));
                Json(TranscriptResponse {
                    text: "private transcript".into(),
                })
            }),
        );
        let server =
            tokio::spawn(async move { axum::serve(upstream, upstream_app).await.unwrap() });
        let state = GatewayState {
            authorization: "Bearer test-token".into(),
            surface_socket: PathBuf::from("/unused/surface.sock").into(),
            ai_tools_api: Url::parse(&format!("http://{upstream_address}/")).unwrap(),
            client: Client::builder()
                .timeout(TRANSCRIPTION_TIMEOUT)
                .build()
                .unwrap(),
        };

        let unauthorized = router(state.clone())
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let request = Request::post("/audio/transcription")
            .header("authorization", "Bearer test-token")
            .header("content-type", "audio/wav")
            .body(Body::from(wav(32_000)))
            .unwrap();
        let response = router(state).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = to_bytes(response.into_body(), MAX_UPSTREAM_RESPONSE_BYTES)
            .await
            .unwrap();
        let response: TranscriptResponse = serde_json::from_slice(&response).unwrap();
        assert_eq!(response.text, "private transcript");
        server.abort();
    }

    #[tokio::test]
    async fn transcription_rejects_wrong_media_before_contacting_inference() {
        let state = GatewayState {
            authorization: "Bearer test-token".into(),
            surface_socket: PathBuf::from("/unused/surface.sock").into(),
            ai_tools_api: Url::parse(DEFAULT_AI_TOOLS_API).unwrap(),
            client: Client::new(),
        };
        let request = Request::post("/audio/transcription")
            .header("authorization", "Bearer test-token")
            .header("content-type", "audio/mp4")
            .body(Body::from("audio"))
            .unwrap();
        let response = router(state).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[test]
    fn wav_validation_bounds_format_and_duration() {
        let second = wav(32_000);
        assert!(validate_wav(&second).is_ok());
        assert_eq!(
            validate_wav(&wav(32_000 * 61)).unwrap_err().code,
            "invalid_audio_duration"
        );
        let mut stereo = second;
        stereo[22..24].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(validate_wav(&stereo).unwrap_err().code, "invalid_audio");
        assert_eq!(
            validate_wav(b"not audio").unwrap_err().code,
            "invalid_audio"
        );
    }

    fn wav(data_bytes: usize) -> Vec<u8> {
        let mut output = Vec::with_capacity(44 + data_bytes);
        output.extend_from_slice(b"RIFF");
        output.extend_from_slice(&u32::try_from(36 + data_bytes).unwrap().to_le_bytes());
        output.extend_from_slice(b"WAVEfmt ");
        output.extend_from_slice(&16u32.to_le_bytes());
        output.extend_from_slice(&1u16.to_le_bytes());
        output.extend_from_slice(&1u16.to_le_bytes());
        output.extend_from_slice(&16_000u32.to_le_bytes());
        output.extend_from_slice(&32_000u32.to_le_bytes());
        output.extend_from_slice(&2u16.to_le_bytes());
        output.extend_from_slice(&16u16.to_le_bytes());
        output.extend_from_slice(b"data");
        output.extend_from_slice(&u32::try_from(data_bytes).unwrap().to_le_bytes());
        output.resize(44 + data_bytes, 0);
        output
    }
}
