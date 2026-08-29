//! Registered protocol v4 surface link.

use std::{
    io::BufReader,
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use scufris_control::service::{
    ConversationMessage, ScufrisState, SurfaceRegistration, SurfaceRequest, SurfaceRequestBody,
    SurfaceResponseBody, read_surface_response,
};
use scufris_control::{MessageError, write_message};

pub const MIN_BACKOFF: Duration = Duration::from_millis(250);
pub const MAX_BACKOFF: Duration = Duration::from_secs(5);
pub const UNAVAILABLE: &str = "The Scufris service is unavailable.";
pub const UPDATE_TOGETHER: &str =
    "The Scufris protocol handshake failed. Update the host and surface together.";

#[derive(Debug, Clone, PartialEq)]
pub enum LinkEvent {
    ReplayStarted,
    Ready,
    Disconnected,
    HandshakeFailed,
    Accepted(String),
    Refused(String, String),
    State(ScufrisState, String),
    Message {
        message: ConversationMessage,
        live: bool,
    },
}

pub fn next_backoff(current: Duration) -> Duration {
    (current * 2).min(MAX_BACKOFF)
}

pub struct ServiceLink {
    writer: Arc<Mutex<Option<UnixStream>>>,
    ready: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
}

impl ServiceLink {
    pub fn start(
        socket: PathBuf,
        registration: SurfaceRegistration,
        observe: impl Fn(LinkEvent) + Send + Sync + 'static,
    ) -> Self {
        let writer = Arc::new(Mutex::new(None));
        let ready = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let link = Self {
            writer: Arc::clone(&writer),
            ready: Arc::clone(&ready),
            stopped: Arc::clone(&stopped),
        };
        let observe = Arc::new(observe);
        thread::spawn({
            let writer = Arc::clone(&writer);
            let ready = Arc::clone(&ready);
            let stopped = Arc::clone(&stopped);
            move || {
                let mut backoff = MIN_BACKOFF;
                while !stopped.load(Ordering::Relaxed) {
                    match serve(&socket, &registration, &writer, &ready, observe.as_ref()) {
                        Outcome::Ready => backoff = MIN_BACKOFF,
                        Outcome::HandshakeFailed => observe(LinkEvent::HandshakeFailed),
                        Outcome::Unavailable => {}
                    }
                    ready.store(false, Ordering::Release);
                    set_writer(&writer, None);
                    observe(LinkEvent::Disconnected);
                    if stopped.load(Ordering::Relaxed) {
                        break;
                    }
                    thread::sleep(backoff);
                    backoff = next_backoff(backoff);
                }
            }
        });
        link
    }

    pub fn submit(&self, id: String, text: String) -> Result<(), String> {
        if !self.ready.load(Ordering::Acquire) {
            return Err("The Scufris surface is still loading.".into());
        }
        if !scufris_control::is_submission_text(&text) {
            return Err("That message is too long to submit.".into());
        }
        send(&self.writer, SurfaceRequestBody::Message { id, text })
    }

    pub fn abort(&self, id: String) -> Result<(), String> {
        if !self.ready.load(Ordering::Acquire) {
            return Err("The Scufris surface is still loading.".into());
        }
        send(&self.writer, SurfaceRequestBody::Abort { id })
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.ready.store(false, Ordering::Release);
        set_writer(&self.writer, None);
    }
}

fn set_writer(writer: &Arc<Mutex<Option<UnixStream>>>, stream: Option<UnixStream>) {
    let mut held = writer.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(previous) = held.take() {
        let _ = previous.shutdown(std::net::Shutdown::Both);
    }
    *held = stream;
}

fn send(writer: &Arc<Mutex<Option<UnixStream>>>, body: SurfaceRequestBody) -> Result<(), String> {
    let mut held = writer.lock().unwrap_or_else(|error| error.into_inner());
    let Some(stream) = held.as_mut() else {
        return Err(UNAVAILABLE.into());
    };
    write_message(stream, &SurfaceRequest::new(body)).map_err(|_| UNAVAILABLE.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Ready,
    HandshakeFailed,
    Unavailable,
}

fn serve(
    socket: &PathBuf,
    registration: &SurfaceRegistration,
    writer: &Arc<Mutex<Option<UnixStream>>>,
    connection_ready: &AtomicBool,
    observe: &(impl Fn(LinkEvent) + ?Sized),
) -> Outcome {
    let stream = match UnixStream::connect(socket) {
        Ok(stream) => stream,
        Err(_) => return Outcome::Unavailable,
    };
    let reading = match stream.try_clone() {
        Ok(stream) => stream,
        Err(_) => return Outcome::Unavailable,
    };
    set_writer(writer, Some(stream));
    if send(
        writer,
        SurfaceRequestBody::Hello {
            surface: registration.clone(),
        },
    )
    .is_err()
    {
        return Outcome::HandshakeFailed;
    }
    observe(LinkEvent::ReplayStarted);
    let mut reader = BufReader::new(reading);
    let mut ready = false;
    loop {
        let response = match read_surface_response(&mut reader) {
            Ok(response) => response,
            Err(MessageError::Empty) if !ready => return Outcome::HandshakeFailed,
            Err(MessageError::Empty) => return Outcome::Ready,
            Err(MessageError::UnsupportedVersion(_)) => return Outcome::HandshakeFailed,
            Err(error) => {
                tracing::warn!(%error, "invalid surface response");
                return if ready {
                    Outcome::Ready
                } else {
                    Outcome::HandshakeFailed
                };
            }
        };
        match response.body {
            SurfaceResponseBody::Message {
                role,
                surface,
                text,
                details,
                widgets,
            } => observe(LinkEvent::Message {
                message: ConversationMessage {
                    role,
                    surface,
                    text,
                    details,
                    widgets,
                },
                live: ready,
            }),
            SurfaceResponseBody::MessageAck { id } => observe(LinkEvent::Accepted(id)),
            SurfaceResponseBody::Aborted { .. } => {}
            SurfaceResponseBody::State { state, detail } => {
                observe(LinkEvent::State(state, detail))
            }
            SurfaceResponseBody::Ready { surface } if surface == registration.id => {
                ready = true;
                connection_ready.store(true, Ordering::Release);
                observe(LinkEvent::Ready);
            }
            SurfaceResponseBody::Ready { .. } => return Outcome::HandshakeFailed,
            SurfaceResponseBody::Rejected {
                id: Some(id),
                detail,
                ..
            } => observe(LinkEvent::Refused(id, detail)),
            SurfaceResponseBody::Rejected { detail, .. } => {
                tracing::warn!(%detail, "surface request rejected")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backoff_is_bounded() {
        let mut value = MIN_BACKOFF;
        for _ in 0..10 {
            value = next_backoff(value);
        }
        assert_eq!(value, MAX_BACKOFF);
    }
}
