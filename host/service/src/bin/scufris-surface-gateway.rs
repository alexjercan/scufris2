//! Authenticated loopback WebSocket bridge for remote Scufris surfaces.

use std::{
    fs,
    io::{self, Cursor},
    net::{IpAddr, SocketAddr, TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{TryRecvError, sync_channel},
    },
    thread,
    time::Duration,
};

use clap::Parser;
use scufris_control::{
    MAX_MESSAGE_BYTES, MessageError,
    service::{
        SurfaceRequest, SurfaceResponse, read_surface_request, read_surface_response,
        surface_socket_path,
    },
    write_message,
};
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;
use tungstenite::{
    Error as WebSocketError, Message, WebSocket,
    handshake::server::{Callback, ErrorResponse, Request, Response},
    http::StatusCode,
    protocol::{CloseFrame, WebSocketConfig, frame::coding::CloseCode},
};

const FAILED: u8 = 1;
const DEFAULT_LISTEN: &str = "127.0.0.1:10440";
const MIN_TOKEN_BYTES: usize = 32;
const MAX_TOKEN_BYTES: usize = 256;
const READ_TIMEOUT: Duration = Duration::from_millis(100);
const OUTBOX: usize = 256;
const TOKEN_VARIABLE: &str = "SCUFRIS_GATEWAY_TOKEN_FILE";
const LISTEN_VARIABLE: &str = "SCUFRIS_GATEWAY_LISTEN";
static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);

struct Authorization {
    expected: String,
}

impl Callback for Authorization {
    // Tungstenite fixes this callback's error type to its full HTTP response.
    #[allow(clippy::result_large_err)]
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        let authorized = request
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| constant_time_eq(value.as_bytes(), self.expected.as_bytes()));
        if authorized {
            Ok(response)
        } else {
            let mut rejected = ErrorResponse::new(Some("Unauthorized".into()));
            *rejected.status_mut() = StatusCode::UNAUTHORIZED;
            Err(rejected)
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "scufris-surface-gateway",
    version,
    about = "Bridge authenticated WebSocket surfaces to the local Scufris service"
)]
struct Options {
    /// Loopback address to accept from a local TLS or tailnet proxy.
    #[arg(long, env = LISTEN_VARIABLE, default_value = DEFAULT_LISTEN)]
    listen: SocketAddr,

    /// Absolute mode-0600 file containing the bearer token.
    #[arg(long, env = TOKEN_VARIABLE, value_name = "PATH")]
    token_file: PathBuf,
}

#[derive(Debug)]
enum LocalEvent {
    Response(SurfaceResponse),
    Closed,
}

fn main() -> ExitCode {
    if let Err(error) = run() {
        eprintln!("scufris-surface-gateway: {error}");
        ExitCode::from(FAILED)
    } else {
        ExitCode::SUCCESS
    }
}

fn run() -> Result<(), GatewayError> {
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
    let token = Arc::new(read_token(&options.token_file)?);
    let surface_socket =
        surface_socket_path().map_err(|error| GatewayError::Configuration(error.to_string()))?;
    let listener = TcpListener::bind(options.listen)?;
    info!(listen = %options.listen, "the remote surface gateway is listening");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let connection = NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed);
                let token = Arc::clone(&token);
                let surface_socket = surface_socket.clone();
                thread::spawn(move || {
                    if let Err(error) =
                        serve_connection(stream, &surface_socket, &token, connection)
                    {
                        warn!(connection, %error, "remote surface disconnected");
                    }
                });
            }
            Err(error) => warn!(%error, "the remote surface listener stopped accepting once"),
        }
    }
    Ok(())
}

fn serve_connection(
    stream: TcpStream,
    surface_socket: &Path,
    token: &str,
    connection: u64,
) -> Result<(), GatewayError> {
    let authorization = Authorization {
        expected: format!("Bearer {token}"),
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES));
    let mut websocket = tungstenite::accept_hdr_with_config(stream, authorization, Some(config))
        .map_err(|error| GatewayError::Handshake(error.to_string()))?;
    websocket.get_mut().set_read_timeout(Some(READ_TIMEOUT))?;

    let local = std::os::unix::net::UnixStream::connect(surface_socket)?;
    let mut local_writer = local.try_clone()?;
    let (sender, receiver) = sync_channel(OUTBOX);
    thread::spawn(move || {
        let mut reader = io::BufReader::new(local);
        loop {
            match read_surface_response(&mut reader) {
                Ok(response) => {
                    if sender.send(LocalEvent::Response(response)).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    debug!(%error, "the local surface connection closed");
                    let _ = sender.send(LocalEvent::Closed);
                    return;
                }
            }
        }
    });

    info!(connection, "remote surface authenticated");
    loop {
        loop {
            match receiver.try_recv() {
                Ok(LocalEvent::Response(response)) => {
                    let text = serde_json::to_string(&response)?;
                    websocket.send(Message::Text(text.into()))?;
                }
                Ok(LocalEvent::Closed) | Err(TryRecvError::Disconnected) => {
                    close(&mut websocket, CloseCode::Away, "service unavailable");
                    return Ok(());
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        match websocket.read() {
            Ok(Message::Text(text)) => {
                let request = decode_request(text.as_str())?;
                write_message(&mut local_writer, &request)?;
            }
            Ok(Message::Close(_)) => return Ok(()),
            Ok(Message::Ping(_) | Message::Pong(_)) => websocket.flush()?,
            Ok(Message::Binary(_) | Message::Frame(_)) => {
                close(
                    &mut websocket,
                    CloseCode::Unsupported,
                    "text frames required",
                );
                return Ok(());
            }
            Err(WebSocketError::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                websocket.flush()?;
            }
            Err(WebSocketError::ConnectionClosed | WebSocketError::AlreadyClosed) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
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

fn close(websocket: &mut WebSocket<TcpStream>, code: CloseCode, reason: &'static str) {
    let _ = websocket.close(Some(CloseFrame {
        code,
        reason: reason.into(),
    }));
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
    #[error("WebSocket handshake failed: {0}")]
    Handshake(String),
    #[error("WebSocket failed: {0}")]
    WebSocket(#[from] WebSocketError),
    #[error("JSON encoding failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use scufris_control::service::{SurfaceRegistration, SurfaceRequestBody, SurfaceResponseBody};
    use tungstenite::client::IntoClientRequest;

    #[test]
    fn only_loopback_listeners_are_allowed() {
        assert!(ensure_loopback("127.0.0.1".parse().unwrap()).is_ok());
        assert!(ensure_loopback("::1".parse().unwrap()).is_ok());
        assert!(ensure_loopback("0.0.0.0".parse().unwrap()).is_err());
        assert!(ensure_loopback("100.64.0.1".parse().unwrap()).is_err());
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

    #[test]
    fn an_authenticated_websocket_bridges_the_surface_protocol() {
        let root =
            std::env::temp_dir().join(format!("scufris-gateway-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let socket_path = root.join("surface.sock");
        let unix_listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let tcp_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = tcp_listener.local_addr().unwrap();
        let token = "a-secure-test-token-with-more-than-32-bytes";

        let local = thread::spawn(move || {
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
        let gateway_socket = socket_path.clone();
        let gateway = thread::spawn(move || {
            let (stream, _) = tcp_listener.accept().unwrap();
            serve_connection(stream, &gateway_socket, token, 1).unwrap();
        });

        let mut request = format!("ws://{address}").into_client_request().unwrap();
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        let (mut websocket, _) =
            tungstenite::client(request, TcpStream::connect(address).unwrap()).unwrap();
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

        local.join().unwrap();
        gateway.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
