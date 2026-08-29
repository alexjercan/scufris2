//! Three private typed Unix socket listeners.

use std::{
    io::{self, BufReader},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::Path,
    sync::{
        Arc,
        mpsc::{SyncSender, sync_channel},
    },
    thread,
};

use scufris_control::service::{
    AgentRequestBody, AgentResponse, ControlRequestBody, ControlResponse, ControlResponseBody,
    SurfaceRequestBody, SurfaceResponse, read_agent_request, read_control_request,
    read_surface_request,
};
use scufris_control::{MessageError, write_message};
use tracing::{debug, error, warn};

use crate::service::Service;

pub(crate) const OUTBOX: usize = 256;

#[derive(Debug, Clone, Copy)]
pub enum Channel {
    Surface,
    Agent,
    Control,
}
impl Channel {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Surface => "surface",
            Self::Agent => "agent",
            Self::Control => "control",
        }
    }
}

pub fn bind(path: &Path) -> io::Result<UnixListener> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("the socket path has no directory"))?;
    std::fs::create_dir_all(directory)?;
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("another service is listening on {}", path.display()),
            ));
        }
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

pub fn serve(service: Arc<Service>, listener: UnixListener, channel: Channel) {
    let mut next = 0;
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                next += 1;
                let connection = next;
                let held = Arc::clone(&service);
                thread::spawn(move || match channel {
                    Channel::Surface => surface(held, stream, connection),
                    Channel::Agent => agent(held, stream, connection),
                    Channel::Control => control(held, stream, connection),
                });
            }
            Err(error) => {
                warn!(%error, channel = channel.name(), "a socket stopped accepting");
                return;
            }
        }
    }
}

fn writer<T: serde::Serialize + Send + 'static>(
    stream: &UnixStream,
    connection: u64,
) -> io::Result<(SyncSender<T>, thread::JoinHandle<()>)> {
    let mut writing = stream.try_clone()?;
    let (outbox, inbox) = sync_channel::<T>(OUTBOX);
    let handle = thread::spawn(move || {
        while let Ok(message) = inbox.recv() {
            if let Err(error) = write_message(&mut writing, &message) {
                if matches!(error, MessageError::TooLarge) {
                    error!(connection, "the service built an oversized message");
                }
                break;
            }
        }
        let _ = writing.shutdown(std::net::Shutdown::Both);
    });
    Ok((outbox, handle))
}

fn protocol_error(channel: Channel, connection: u64, error: &MessageError) {
    match error {
        MessageError::UnsupportedVersion(version) => warn!(
            channel = channel.name(),
            connection, version, "wrong protocol version; closing without response"
        ),
        MessageError::Empty => debug!(channel = channel.name(), connection, "connection closed"),
        _ => warn!(channel = channel.name(), connection, %error, "invalid channel message"),
    }
}

fn surface(service: Arc<Service>, stream: UnixStream, connection: u64) {
    let Ok((outbox, writing)) = writer::<SurfaceResponse>(&stream, connection) else {
        return;
    };
    let mut reader = BufReader::new(&stream);
    let registration = match read_surface_request(&mut reader) {
        Ok(request) => match request.body {
            SurfaceRequestBody::Hello { surface } => surface,
            _ => {
                warn!(connection, "surface did not say hello first");
                return;
            }
        },
        Err(error) => {
            protocol_error(Channel::Surface, connection, &error);
            return;
        }
    };
    let generation = service.register_surface(connection, registration, outbox);
    loop {
        match read_surface_request(&mut reader) {
            Ok(request) => match request.body {
                SurfaceRequestBody::Hello { .. } => {
                    warn!(connection, "surface said hello twice");
                    break;
                }
                SurfaceRequestBody::Message { id, text } => {
                    service.surface_message(connection, id, text)
                }
                SurfaceRequestBody::Abort { id } => service.surface_abort(connection, id),
            },
            Err(MessageError::Empty) => break,
            Err(error) => {
                protocol_error(Channel::Surface, connection, &error);
                break;
            }
        }
    }
    service.unregister_surface(connection, generation);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    let _ = writing.join();
}

fn agent(service: Arc<Service>, stream: UnixStream, connection: u64) {
    let Ok((outbox, writing)) = writer::<AgentResponse>(&stream, connection) else {
        return;
    };
    let mut reader = BufReader::new(&stream);
    match read_agent_request(&mut reader) {
        Ok(request) if request.body == AgentRequestBody::Hello => {}
        Ok(_) => {
            warn!(connection, "agent did not say hello first");
            return;
        }
        Err(error) => {
            protocol_error(Channel::Agent, connection, &error);
            return;
        }
    }
    if !service.register_agent(connection, outbox) {
        let _ = writing.join();
        return;
    }
    loop {
        match read_agent_request(&mut reader) {
            Ok(request) => {
                if request.body == AgentRequestBody::Hello {
                    warn!(connection, "agent said hello twice");
                    break;
                }
                service.agent_request(connection, request.body);
            }
            Err(MessageError::Empty) => break,
            Err(error) => {
                protocol_error(Channel::Agent, connection, &error);
                break;
            }
        }
    }
    service.unregister_agent(connection);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    let _ = writing.join();
}

fn control(service: Arc<Service>, stream: UnixStream, connection: u64) {
    let Ok((outbox, writing)) = writer::<ControlResponse>(&stream, connection) else {
        return;
    };
    let mut reader = BufReader::new(&stream);
    match read_control_request(&mut reader) {
        Ok(request) if request.body == ControlRequestBody::Hello => {
            let _ = outbox.try_send(ControlResponse::new(ControlResponseBody::Ready));
        }
        Ok(_) => {
            warn!(connection, "control did not say hello first");
            return;
        }
        Err(error) => {
            protocol_error(Channel::Control, connection, &error);
            return;
        }
    }
    loop {
        match read_control_request(&mut reader) {
            Ok(request) => match request.body {
                ControlRequestBody::Hello => {
                    let _ = outbox.try_send(ControlResponse::new(ControlResponseBody::Rejected {
                        id: "hello".into(),
                        code: "duplicate_hello".into(),
                        detail: "Control already completed its handshake.".into(),
                    }));
                }
                ControlRequestBody::State { id } => {
                    let (state, detail) = service.control_state();
                    let _ = outbox.try_send(ControlResponse::new(ControlResponseBody::State {
                        id,
                        state,
                        detail,
                    }));
                }
            },
            Err(MessageError::Empty) => break,
            Err(error) => {
                protocol_error(Channel::Control, connection, &error);
                break;
            }
        }
    }
    drop(outbox);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    let _ = writing.join();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binding_is_mode_0600() {
        let root = std::env::temp_dir().join(format!("scufris-v4-bind-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let path = root.join("surface.sock");
        let listener = bind(&path).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(listener);
        std::fs::remove_dir_all(root).unwrap();
    }
}
