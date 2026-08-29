//! Frontend link to the Scufris service.
//!
//! The companion is a client. It connects to `service.sock` in the `frontend`
//! role, submits what the person said, and is pushed everything else: the
//! state, the transcript, the paragraph to speak, and the widget commands the
//! agent asks for.
//!
//! The link owns one supervised connection and reconnects with a bounded
//! backoff, so a service restart leaves the companion and the tray running. It
//! reports connection loss as its own event rather than as an assistant state,
//! because a companion that cannot reach the service knows nothing about the
//! assistant, which is not the same as knowing it is idle.

use std::{
    io::BufReader,
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use scufris_control::{
    MessageError,
    service::{
        ClientBody, ClientMessage, NoticeState, Role, ScufrisState, ServiceBody, TranscriptEntry,
        WidgetCommand, WidgetReport, read_service_message,
    },
    write_message,
};

/// Shortest wait before reconnecting.
pub const MIN_BACKOFF: Duration = Duration::from_millis(250);

/// Longest wait before reconnecting.
pub const MAX_BACKOFF: Duration = Duration::from_secs(5);

/// How long a connection has to last before it counts as having worked.
///
/// A clean end is not the same as a useful one. A service that welcomes a
/// client and immediately hangs up ends every connection cleanly, and reading
/// only that would reset the wait and reconnect four times a second. The
/// conversation window is cleared on each reconnect, so the storm is visible
/// to the person as well as expensive.
const SETTLED: Duration = Duration::from_secs(5);

/// Message shown wherever the service cannot be reached.
pub const UNAVAILABLE: &str = "The Scufris service is unavailable.";

/// One thing the service link observed.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkEvent {
    /// The service answered `hello` and gave this connection its role.
    Connected,
    /// The connection is closed and the service is unavailable.
    Disconnected,
    /// A submission entered the conversation.
    Accepted(String),
    /// A submission never left the service, so those words are still only the
    /// companion's and may be edited and retried.
    Refused(String, String),
    /// The service reported what the assistant is doing.
    State(ScufrisState, String),
    /// One more line of the conversation.
    Transcript(TranscriptEntry),
    /// One paragraph the agent wants spoken. The companion owns the speaker.
    Speak(String),
    /// One identified ambient notice was raised, replaced, or cleared.
    Notice(String, NoticeState, String),
    /// The agent asked the widgets runtime to do something. Widget traffic
    /// never reaches the pill's state machine.
    Widget(WidgetCommand),
    /// The agent asked for the conversation window, up or down. It reaches the
    /// window and nothing else: the pill goes on reporting what it was.
    Conversation(bool),
}

/// Returns the next backoff after one failed connection attempt.
pub fn next_backoff(current: Duration) -> Duration {
    (current * 2).min(MAX_BACKOFF)
}

/// A supervised connection to the service socket.
pub struct ServiceLink {
    writer: Arc<Mutex<Option<UnixStream>>>,
    stopped: Arc<AtomicBool>,
}

impl ServiceLink {
    /// Starts the supervisor and delivers every observation to `observe`.
    pub fn start(socket: PathBuf, observe: impl Fn(LinkEvent) + Send + Sync + 'static) -> Self {
        let writer: Arc<Mutex<Option<UnixStream>>> = Arc::new(Mutex::new(None));
        let stopped = Arc::new(AtomicBool::new(false));
        let link = Self {
            writer: Arc::clone(&writer),
            stopped: Arc::clone(&stopped),
        };
        let supervisor_observe = Arc::new(observe);
        let supervisor_writer = Arc::clone(&writer);
        let supervisor_stopped = Arc::clone(&stopped);
        thread::spawn(move || {
            let mut backoff = MIN_BACKOFF;
            while !supervisor_stopped.load(Ordering::Relaxed) {
                let began = Instant::now();
                let outcome = serve(&socket, &supervisor_writer, supervisor_observe.as_ref());
                // The wait is reset by a connection that lasted, not by one
                // that ended tidily. Both are `Ok(())`, and only the first is
                // evidence that reconnecting at once is worth doing.
                if outcome.is_ok() && began.elapsed() >= SETTLED {
                    backoff = MIN_BACKOFF;
                } else {
                    backoff = next_backoff(backoff);
                }
                set_writer(&supervisor_writer, None);
                supervisor_observe(LinkEvent::Disconnected);
                thread::sleep(backoff);
            }
        });
        link
    }

    /// Submits one accepted transcript as a normal user message.
    pub fn submit(&self, id: String, text: String) -> Result<(), String> {
        // Checked here as well as on the wire: both sides measure the
        // transcript in UTF-8 bytes, so what one accepts the other accepts.
        if !scufris_control::is_submission_text(&text) {
            return Err("That transcript is too long to submit.".into());
        }
        send(&self.writer, ClientBody::Submit { id, text })
    }

    /// Ends the agent's current run.
    pub fn abort(&self, id: String) -> Result<(), String> {
        send(&self.writer, ClientBody::Abort { id })
    }

    /// Tells the agent what became of one of its widgets.
    ///
    /// Answers, surface notices, and the catalog all travel this way, on the
    /// same connection and through the same writer the submissions use.
    pub fn report(&self, report: WidgetReport) -> Result<(), String> {
        send(&self.writer, ClientBody::Report { report })
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
        return Err(UNAVAILABLE.into());
    };
    write_message(stream, &ClientMessage::new(body)).map_err(|error| match error {
        MessageError::TooLarge => "That transcript is too long to submit.".to_string(),
        _ => UNAVAILABLE.to_string(),
    })
}

fn serve(
    socket: &PathBuf,
    writer: &Arc<Mutex<Option<UnixStream>>>,
    observe: &(impl Fn(LinkEvent) + ?Sized),
) -> Result<(), ()> {
    let stream = UnixStream::connect(socket).map_err(|_| ())?;
    let reading = stream.try_clone().map_err(|_| ())?;
    set_writer(writer, Some(stream));
    send(
        writer,
        ClientBody::Hello {
            role: Role::Frontend,
        },
    )
    .map_err(|_| ())?;

    let mut reader = BufReader::new(reading);
    loop {
        let message = match read_service_message(&mut reader) {
            Ok(message) => message,
            Err(MessageError::Empty) => return Ok(()),
            Err(error) => {
                tracing::warn!("rejected service message: {error}");
                return Err(());
            }
        };
        match message.body {
            ServiceBody::Welcome { role } => {
                debug_assert_eq!(role, Role::Frontend);
                observe(LinkEvent::Connected)
            }
            ServiceBody::Ok { id } => observe(LinkEvent::Accepted(id)),
            ServiceBody::Refused { id, code, detail } => {
                // The code is what a program would branch on and the detail is
                // what a person reads. The pill shows one line, so it shows the
                // detail, and the code goes to the log.
                tracing::debug!(id = %id, code = %code, "submission refused");
                observe(LinkEvent::Refused(id, detail))
            }
            ServiceBody::State { state, detail, .. } => observe(LinkEvent::State(state, detail)),
            ServiceBody::Transcript { entry } => observe(LinkEvent::Transcript(entry)),
            ServiceBody::Speak { text } => observe(LinkEvent::Speak(text)),
            ServiceBody::Notice { id, state, detail } => {
                observe(LinkEvent::Notice(id, state, detail))
            }
            // Widget commands belong to the widgets runtime, never to the
            // pill's state machine.
            ServiceBody::Widget { command } => observe(LinkEvent::Widget(command)),
            ServiceBody::Conversation { up } => observe(LinkEvent::Conversation(up)),
            // A frontend sends reports and never receives one, and a debug
            // lease belongs to whoever asked for it in a terminal.
            ServiceBody::Report { .. } | ServiceBody::Debug { .. } => {
                tracing::warn!("service sent a frontend {}", message.body.name())
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

    use scufris_control::service::{Posture, Speaker};

    use super::*;

    fn socket_directory(tag: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "scufris-{tag}-{}-{:?}",
            std::process::id(),
            thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

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
    fn submitting_without_a_connection_reports_an_unavailable_service() {
        let link = ServiceLink {
            writer: Arc::new(Mutex::new(None)),
            stopped: Arc::new(AtomicBool::new(true)),
        };
        assert_eq!(
            link.submit("pill-1".into(), "hello".into()),
            Err(UNAVAILABLE.into())
        );
    }

    #[test]
    fn the_link_says_it_is_a_frontend_and_forwards_what_it_is_pushed() {
        let directory = socket_directory("link");
        let socket = directory.join("service.sock");
        let listener = UnixListener::bind(&socket).unwrap();

        let (events, received) = mpsc::channel();
        let link = ServiceLink::start(socket.clone(), move |event| {
            let _ = events.send(event);
        });

        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            line.trim_end(),
            "{\"v\":3,\"type\":\"hello\",\"role\":\"frontend\"}"
        );

        let mut writer = stream;
        writer
            .write_all(b"{\"v\":3,\"type\":\"welcome\",\"role\":\"frontend\"}\n")
            .unwrap();
        writer
            .write_all(b"{\"v\":3,\"type\":\"state\",\"state\":\"working\",\"detail\":\"\"}\n")
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            LinkEvent::Connected
        );
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            LinkEvent::State(ScufrisState::Working, String::new())
        );
        writer
            .write_all(
                b"{\"v\":3,\"type\":\"notice\",\"id\":\"job-one\",\"state\":\"attention\",\
                  \"detail\":\"Job job-one is blocked\"}\n",
            )
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            LinkEvent::Notice(
                "job-one".into(),
                NoticeState::Attention,
                "Job job-one is blocked".into(),
            )
        );

        link.submit("pill-1".into(), "hello there".into()).unwrap();
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            line.trim_end(),
            "{\"v\":3,\"type\":\"submit\",\"id\":\"pill-1\",\"text\":\"hello there\"}"
        );

        writer
            .write_all(b"{\"v\":3,\"type\":\"ok\",\"id\":\"pill-1\"}\n")
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            LinkEvent::Accepted("pill-1".into())
        );

        // A refusal carries the identifier it answers, and the link keeps it:
        // the companion holds one transcript at a time and must not apply an
        // answer to the one that replaced it.
        writer
            .write_all(
                b"{\"v\":3,\"type\":\"refused\",\"id\":\"pill-2\",\"code\":\"agent_unavailable\",\
                  \"detail\":\"no agent\"}\n",
            )
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            LinkEvent::Refused("pill-2".into(), "no agent".into())
        );

        // The conversation and the speech are two different strings, and they
        // arrive as two different messages.
        writer
            .write_all(
                b"{\"v\":3,\"type\":\"transcript\",\"entry\":{\"speaker\":\"assistant\",\
                  \"text\":\"it is raining\"}}\n",
            )
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            LinkEvent::Transcript(TranscriptEntry {
                speaker: Speaker::Assistant,
                text: "it is raining".into(),
            })
        );
        writer
            .write_all(b"{\"v\":3,\"type\":\"speak\",\"text\":\"it is raining\"}\n")
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            LinkEvent::Speak("it is raining".into())
        );

        link.stop();
        drop(writer);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn widget_commands_are_routed_away_from_the_pill_and_answered_by_their_id() {
        let directory = socket_directory("wg");
        let socket = directory.join("service.sock");
        let listener = UnixListener::bind(&socket).unwrap();

        let (events, received) = mpsc::channel();
        let link = ServiceLink::start(socket.clone(), move |event| {
            let _ = events.send(event);
        });

        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            line.trim_end(),
            "{\"v\":3,\"type\":\"hello\",\"role\":\"frontend\"}"
        );

        let mut writer = stream;
        writer
            .write_all(
                b"{\"v\":3,\"type\":\"widget\",\"command\":{\"type\":\"open\",\"id\":\"w-1\",\
                  \"widget\":\"note\",\"posture\":\"exhibit\",\"data\":{\"text\":\"hi\"}}}\n",
            )
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            LinkEvent::Widget(WidgetCommand::Open {
                id: "w-1".into(),
                widget: "note".into(),
                posture: Posture::Exhibit,
                data: serde_json::json!({ "text": "hi" }),
            })
        );

        // The answer goes back through the connection the command arrived on,
        // carrying the identifier the agent is waiting for.
        link.report(WidgetReport::Opened {
            id: "w-1".into(),
            surface: "widget-3".into(),
        })
        .unwrap();
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            line.trim_end(),
            "{\"v\":3,\"type\":\"report\",\"report\":{\"type\":\"opened\",\"id\":\"w-1\",\
             \"surface\":\"widget-3\"}}"
        );

        writer
            .write_all(
                b"{\"v\":3,\"type\":\"widget\",\"command\":{\"type\":\"clear\",\"id\":\"w-2\"}}\n",
            )
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            LinkEvent::Widget(WidgetCommand::Clear { id: "w-2".into() })
        );
        link.report(WidgetReport::Failed {
            id: "w-2".into(),
            code: "no_runtime".into(),
            detail: "the widgets runtime is not started".into(),
        })
        .unwrap();
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            line.trim_end(),
            "{\"v\":3,\"type\":\"report\",\"report\":{\"type\":\"failed\",\"id\":\"w-2\",\
             \"code\":\"no_runtime\",\"detail\":\"the widgets runtime is not started\"}}"
        );

        link.stop();
        drop(writer);
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
