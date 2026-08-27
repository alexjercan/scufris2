//! Control-socket link to the Scufris daemon.
//!
//! The link owns one supervised connection. It reconnects with a bounded
//! backoff so a backend crash leaves the companion and the tray running, and it
//! reports connection loss as its own event rather than as an assistant state.

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

use scufris_control::{
    AssistantState, ClientBody, ClientMessage, DaemonBody, MessageError, Posture,
    read_daemon_message, write_message,
};

/// Shortest wait before reconnecting.
pub const MIN_BACKOFF: Duration = Duration::from_millis(250);

/// Longest wait before reconnecting.
pub const MAX_BACKOFF: Duration = Duration::from_secs(5);

/// Interval between liveness probes on an open connection.
pub const PING_INTERVAL: Duration = Duration::from_secs(15);

/// One widget command the daemon asked for.
///
/// Every variant carries the correlation identifier its answer must echo. The
/// daemon may have several commands in flight, and an answer nobody can match
/// is an answer that can act on the wrong surface.
#[derive(Debug, Clone, PartialEq)]
pub enum WidgetCommand {
    /// Open one widget from the announced catalog.
    Open {
        /// Correlation identifier the answer echoes.
        id: String,
        /// Widget to open.
        widget: String,
        /// Where the surface lives once it is open.
        posture: Posture,
        /// Widget-defined spawn payload.
        data: serde_json::Value,
    },
    /// Send new data to one open surface.
    Update {
        /// Correlation identifier the answer echoes.
        id: String,
        /// Surface to update.
        surface: String,
        /// Widget-defined payload.
        data: serde_json::Value,
    },
    /// Close one open surface.
    Close {
        /// Correlation identifier the answer echoes.
        id: String,
        /// Surface to close.
        surface: String,
    },
    /// Close every surface the runtime owns.
    Clear {
        /// Correlation identifier the answer echoes.
        id: String,
    },
}

impl WidgetCommand {
    /// Returns the correlation identifier the answer must echo.
    pub fn id(&self) -> &str {
        match self {
            Self::Open { id, .. }
            | Self::Update { id, .. }
            | Self::Close { id, .. }
            | Self::Clear { id } => id,
        }
    }
}

/// One thing the daemon link observed.
#[derive(Debug, Clone, PartialEq)]
pub enum DaemonEvent {
    /// The daemon answered `hello` and named its session.
    Connected(String),
    /// The connection is closed and the backend is unavailable.
    Disconnected,
    /// A submission entered the conversation.
    Acknowledged(String),
    /// A submission was dispatched once already and the daemon cannot say
    /// whether it landed.
    Uncertain(String, String),
    /// A submission never left the daemon, so those words are still only the
    /// companion's and may be edited and retried.
    Refused(String, String),
    /// The daemon reported an assistant state.
    State(AssistantState, String),
    /// The daemon asked the widgets runtime to do something. Widget traffic
    /// never reaches the pill's state machine.
    Widget(WidgetCommand),
}

/// Returns the next backoff after one failed connection attempt.
pub fn next_backoff(current: Duration) -> Duration {
    (current * 2).min(MAX_BACKOFF)
}

/// A supervised connection to the daemon control socket.
pub struct DaemonLink {
    writer: Arc<Mutex<Option<UnixStream>>>,
    stopped: Arc<AtomicBool>,
}

impl DaemonLink {
    /// Starts the supervisor and delivers every observation to `observe`.
    pub fn start(socket: PathBuf, observe: impl Fn(DaemonEvent) + Send + Sync + 'static) -> Self {
        let writer: Arc<Mutex<Option<UnixStream>>> = Arc::new(Mutex::new(None));
        let stopped = Arc::new(AtomicBool::new(false));
        let link = Self {
            writer: Arc::clone(&writer),
            stopped: Arc::clone(&stopped),
        };
        let observe = Arc::new(observe);
        let supervisor_observe = Arc::clone(&observe);
        let supervisor_writer = Arc::clone(&writer);
        let supervisor_stopped = Arc::clone(&stopped);
        thread::spawn(move || {
            let mut backoff = MIN_BACKOFF;
            while !supervisor_stopped.load(Ordering::Relaxed) {
                match serve(&socket, &supervisor_writer, supervisor_observe.as_ref()) {
                    Ok(()) => backoff = MIN_BACKOFF,
                    Err(()) => backoff = next_backoff(backoff),
                }
                set_writer(&supervisor_writer, None);
                supervisor_observe(DaemonEvent::Disconnected);
                thread::sleep(backoff);
            }
        });
        let ping_writer = Arc::clone(&writer);
        let ping_stopped = Arc::clone(&stopped);
        thread::spawn(move || {
            while !ping_stopped.load(Ordering::Relaxed) {
                thread::sleep(PING_INTERVAL);
                let _ = send(&ping_writer, ClientBody::Ping);
            }
        });
        link
    }

    /// Submits one accepted transcript as a normal user message.
    ///
    /// `force` carries the person's own decision to send words that may already
    /// be in the conversation. A retry, a reconnect, and a restart never set it.
    pub fn submit(&self, id: String, text: String, force: bool) -> Result<(), String> {
        // Checked here as well as on the wire: both sides measure the
        // transcript in UTF-8 bytes, so what one accepts the other accepts.
        if !scufris_control::is_submission_text(&text) {
            return Err("That transcript is too long to submit.".into());
        }
        send(&self.writer, ClientBody::Submit { id, text, force })
    }

    /// Sends one companion message that is not a submission.
    ///
    /// Widget answers, surface events, and the catalog all travel this way, on
    /// the same connection and through the same writer the submissions use.
    pub fn report(&self, body: ClientBody) -> Result<(), String> {
        send(&self.writer, body)
    }

    /// Stops the supervisor and closes any open connection.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        set_writer(&self.writer, None);
    }
}

fn set_writer(writer: &Arc<Mutex<Option<UnixStream>>>, stream: Option<UnixStream>) {
    let mut guard = writer.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(previous) = guard.take() {
        let _ = previous.shutdown(std::net::Shutdown::Both);
    }
    *guard = stream;
}

fn send(writer: &Arc<Mutex<Option<UnixStream>>>, body: ClientBody) -> Result<(), String> {
    let mut guard = writer.lock().unwrap_or_else(|error| error.into_inner());
    let Some(stream) = guard.as_mut() else {
        return Err("The Scufris backend is unavailable.".into());
    };
    write_message(stream, &ClientMessage::new(body)).map_err(|error| match error {
        MessageError::TooLarge => "That transcript is too long to submit.".to_string(),
        _ => "The Scufris backend is unavailable.".to_string(),
    })
}

fn serve(
    socket: &PathBuf,
    writer: &Arc<Mutex<Option<UnixStream>>>,
    observe: &(impl Fn(DaemonEvent) + ?Sized),
) -> Result<(), ()> {
    let stream = UnixStream::connect(socket).map_err(|_| ())?;
    let reading = stream.try_clone().map_err(|_| ())?;
    set_writer(writer, Some(stream));
    send(writer, ClientBody::Hello).map_err(|_| ())?;

    let mut reader = BufReader::new(reading);
    loop {
        let message = match read_daemon_message(&mut reader) {
            Ok(message) => message,
            Err(MessageError::Empty) => return Ok(()),
            Err(error) => {
                tracing::warn!("rejected daemon message: {error}");
                return Err(());
            }
        };
        match message.body {
            DaemonBody::Welcome { session } => observe(DaemonEvent::Connected(session)),
            DaemonBody::Ack { id } => observe(DaemonEvent::Acknowledged(id)),
            DaemonBody::Uncertain { id, detail } => observe(DaemonEvent::Uncertain(id, detail)),
            DaemonBody::Refused { id, detail } => observe(DaemonEvent::Refused(id, detail)),
            DaemonBody::State { state, detail } => observe(DaemonEvent::State(state, detail)),
            DaemonBody::Pong => {}
            // Widget commands belong to the widgets runtime, never to the
            // pill's state machine.
            DaemonBody::WidgetOpen {
                id,
                widget,
                posture,
                data,
            } => observe(DaemonEvent::Widget(WidgetCommand::Open {
                id,
                widget,
                posture,
                data,
            })),
            DaemonBody::WidgetUpdate { id, surface, data } => {
                observe(DaemonEvent::Widget(WidgetCommand::Update {
                    id,
                    surface,
                    data,
                }))
            }
            DaemonBody::WidgetClose { id, surface } => {
                observe(DaemonEvent::Widget(WidgetCommand::Close { id, surface }))
            }
            DaemonBody::WidgetClear { id } => {
                observe(DaemonEvent::Widget(WidgetCommand::Clear { id }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
        sync::mpsc,
    };

    use super::*;

    #[test]
    fn backoff_grows_and_stays_bounded() {
        let mut backoff = MIN_BACKOFF;
        for _ in 0..10 {
            backoff = next_backoff(backoff);
        }
        assert_eq!(backoff, MAX_BACKOFF);
        assert_eq!(next_backoff(MIN_BACKOFF), MIN_BACKOFF * 2);
    }

    #[test]
    fn submitting_without_a_connection_reports_an_unavailable_backend() {
        let link = DaemonLink {
            writer: Arc::new(Mutex::new(None)),
            stopped: Arc::new(AtomicBool::new(true)),
        };
        assert_eq!(
            link.submit("pill-1".into(), "hello".into(), false),
            Err("The Scufris backend is unavailable.".into())
        );
    }

    #[test]
    fn the_link_greets_the_daemon_and_forwards_state_and_acknowledgments() {
        let directory = std::env::temp_dir().join(format!(
            "scufris-link-{}-{:?}",
            std::process::id(),
            thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let socket = directory.join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();

        let (events, received) = mpsc::channel();
        let link = DaemonLink::start(socket.clone(), move |event| {
            let _ = events.send(event);
        });

        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line.trim_end(), "{\"v\":2,\"type\":\"hello\"}");

        let mut writer = stream;
        writer
            .write_all(b"{\"v\":2,\"type\":\"welcome\",\"session\":\"popup-1\"}\n")
            .unwrap();
        writer
            .write_all(b"{\"v\":2,\"type\":\"state\",\"state\":\"working\",\"detail\":\"\"}\n")
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            DaemonEvent::Connected("popup-1".into())
        );
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            DaemonEvent::State(AssistantState::Working, String::new())
        );

        link.submit("pill-1".into(), "hello there".into(), false)
            .unwrap();
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            line.trim_end(),
            "{\"v\":2,\"type\":\"submit\",\"id\":\"pill-1\",\"text\":\"hello there\"}"
        );

        writer
            .write_all(b"{\"v\":2,\"type\":\"ack\",\"id\":\"pill-1\"}\n")
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            DaemonEvent::Acknowledged("pill-1".into())
        );

        // Both answers about a submission carry the identifier they answer,
        // and the link keeps it: the companion holds one transcript at a time
        // and must not apply an answer to the wrong one.
        writer
            .write_all(
                b"{\"v\":2,\"type\":\"refused\",\"id\":\"pill-2\",\"detail\":\"no session\"}\n",
            )
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            DaemonEvent::Refused("pill-2".into(), "no session".into())
        );
        writer
            .write_all(
                b"{\"v\":2,\"type\":\"uncertain\",\"id\":\"pill-3\",\"detail\":\"unknown\"}\n",
            )
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            DaemonEvent::Uncertain("pill-3".into(), "unknown".into())
        );

        link.stop();
        drop(writer);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn widget_commands_are_routed_away_from_the_pill_and_answered_by_their_id() {
        let directory = std::env::temp_dir().join(format!(
            "scufris-wg-{}-{:?}",
            std::process::id(),
            thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let socket = directory.join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();

        let (events, received) = mpsc::channel();
        let link = DaemonLink::start(socket.clone(), move |event| {
            let _ = events.send(event);
        });

        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line.trim_end(), "{\"v\":2,\"type\":\"hello\"}");

        let mut writer = stream;
        writer
            .write_all(
                b"{\"v\":2,\"type\":\"widget_open\",\"id\":\"w-1\",\"widget\":\"note\",\
                  \"posture\":\"exhibit\",\"data\":{\"text\":\"hi\"}}\n",
            )
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            DaemonEvent::Widget(WidgetCommand::Open {
                id: "w-1".into(),
                widget: "note".into(),
                posture: Posture::Exhibit,
                data: serde_json::json!({ "text": "hi" }),
            })
        );

        // The answer goes back through the connection the command arrived on,
        // carrying the identifier the daemon is waiting for.
        link.report(ClientBody::WidgetOpened {
            id: "w-1".into(),
            surface: "widget-3".into(),
        })
        .unwrap();
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            line.trim_end(),
            "{\"v\":2,\"type\":\"widget_opened\",\"id\":\"w-1\",\"surface\":\"widget-3\"}"
        );

        writer
            .write_all(b"{\"v\":2,\"type\":\"widget_clear\",\"id\":\"w-2\"}\n")
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            DaemonEvent::Widget(WidgetCommand::Clear { id: "w-2".into() })
        );
        link.report(ClientBody::WidgetFailed {
            id: "w-2".into(),
            code: "no_runtime".into(),
            detail: "the widgets runtime is not started".into(),
        })
        .unwrap();
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            line.trim_end(),
            "{\"v\":2,\"type\":\"widget_failed\",\"id\":\"w-2\",\"code\":\"no_runtime\",\
             \"detail\":\"the widgets runtime is not started\"}"
        );

        link.stop();
        drop(writer);
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
