//! The socket the service serves, and one connection's life on it.
//!
//! A same-user Unix socket in the session runtime directory, under a private
//! directory, mode 0600. There is no authentication and there is nothing to
//! authenticate: by L1 this is one person on one machine, and anything that
//! can open the socket can already act as them.
//!
//! Each connection gets two threads: one reading requests and one draining a
//! bounded outbox onto the socket. The split is what lets the service push a
//! state change to a frontend without ever waiting on it, and what lets a
//! client that stopped reading be dropped instead of blocking everyone else.
//!
//! The reader ending is the event that matters. It is how a client goes away,
//! and it is how a debug lease is released, so the two are the same code path
//! and there is no way to have one without the other.

use std::{
    io::{self, BufReader},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::Path,
    sync::{Arc, mpsc::sync_channel},
    thread,
};

use scufris_control::{
    MessageError,
    service::{ClientBody, ServiceMessage, read_client_message},
};
use tracing::{debug, error, warn};

use crate::service::Service;

/// How many messages one client may fall behind by before it is dropped.
pub(crate) const OUTBOX: usize = 256;

/// Binds the service socket, refusing to displace a service that is running.
///
/// A stale socket file outlives the process that made it, so one is removed.
/// A socket that still answers is a second service, and the right thing then
/// is to fail loudly rather than to take the conversation away from it.
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
                format!("another service is already listening on {}", path.display()),
            ));
        }
        debug!(socket = %path.display(), "a stale socket was removed");
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Accepts connections until the listener fails.
pub fn serve(service: Arc<Service>, listener: UnixListener) {
    let mut next: u64 = 0;
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                next += 1;
                let client = next;
                let held = Arc::clone(&service);
                if let Err(error) = thread::Builder::new()
                    .name(format!("scufris-client-{client}"))
                    .spawn(move || handle(held, stream, client))
                {
                    warn!(%error, client, "a connection could not be given a thread");
                }
            }
            Err(error) => {
                warn!(%error, "the service socket stopped accepting");
                return;
            }
        }
    }
}

/// Runs one connection from its hello to its close.
fn handle(service: Arc<Service>, stream: UnixStream, client: u64) {
    let Ok(writing) = stream.try_clone() else {
        warn!(client, "a connection could not be split for writing");
        return;
    };
    let (outbox, inbox) = sync_channel::<ServiceMessage>(OUTBOX);
    let writer = thread::Builder::new()
        .name(format!("scufris-client-{client}-out"))
        .spawn(move || {
            let mut writing = writing;
            while let Ok(message) = inbox.recv() {
                if let Err(error) = scufris_control::write_message(&mut writing, &message) {
                    // A peer that went away is ordinary and says nothing at
                    // the default level. A message this service built and
                    // cannot send is a defect in the service, and it closes
                    // every connection it is replayed on, so it is said out
                    // loud. `ServiceBody::bounded` exists to keep this from
                    // happening; reaching it means a field escaped it.
                    if matches!(error, MessageError::TooLarge) {
                        error!(
                            client,
                            body = message.body.name(),
                            "the service built a message it cannot send"
                        );
                    } else {
                        debug!(client, %error, "a client could not be written to");
                    }
                    break;
                }
            }
            // Wakes the reader, so a client that went away is forgotten
            // whether it was the reading or the writing half that noticed.
            let _ = writing.shutdown(std::net::Shutdown::Both);
        });
    let writer = match writer {
        Ok(handle) => handle,
        Err(error) => {
            warn!(%error, client, "a connection could not be given a writer");
            return;
        }
    };

    let mut reader = BufReader::new(&stream);
    match read_client_message(&mut reader) {
        Ok(message) => match message.body {
            ClientBody::Hello { role } => service.register(client, role, outbox),
            other => {
                warn!(
                    client,
                    said = other.name(),
                    "a client did not say hello first"
                );
                drop(outbox);
                let _ = writer.join();
                return;
            }
        },
        Err(error) => {
            debug!(client, %error, "a connection ended before it said hello");
            drop(outbox);
            let _ = writer.join();
            return;
        }
    }

    loop {
        match read_client_message(&mut reader) {
            Ok(message) => act(&service, client, message.body),
            Err(MessageError::Empty) => break,
            Err(error) => {
                warn!(client, %error, "a client sent something unreadable");
                break;
            }
        }
    }

    // Both halves of goodbye, in this order. The lease is released by the
    // unregister, and the writer stops as soon as its last sender is gone.
    service.unregister(client);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    let _ = writer.join();
    debug!(client, "a client disconnected");
}

/// Carries out one request.
fn act(service: &Arc<Service>, client: u64, body: ClientBody) {
    match body {
        ClientBody::Hello { role } => {
            // The role is settled by the first message and never changes. A
            // second hello is a client that has lost track of itself.
            warn!(client, role = role.name(), "a client said hello twice");
        }
        ClientBody::Submit { id, text } => service.submit(client, id, text),
        ClientBody::Abort { id } => service.abort(client, id),
        ClientBody::GetState { id } => service.report_state(client, id),
        ClientBody::Debug { id } => service.begin_debug(client, id),
        ClientBody::Said { text } => service.said(client, text),
        ClientBody::Speak { text } => service.speak(client, text),
        ClientBody::Notice { id, state, detail } => service.notice(client, id, state, detail),
        ClientBody::Widget { command } => service.relay_widget(client, command),
        ClientBody::Conversation { id, up } => service.relay_conversation(client, id, up),
        ClientBody::Report { report } => service.relay_report(client, report),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        path::PathBuf,
        time::{Duration, Instant},
    };

    use scufris_control::service::{
        ClientMessage, Role, ScufrisState, ServiceBody, read_service_message,
    };

    use super::*;
    use crate::{config::Config, service::Service};

    fn scratch(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("scufris-server-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        directory.join("service.sock")
    }

    /// Writes a stand-in agent that answers the one command the service sends
    /// unasked, records every start, and exits when its stdin closes.
    ///
    /// The paths are written into the program rather than passed in the
    /// environment, because setting one in a test would be setting it for
    /// every other test in the process.
    fn fake_agent(directory: &Path, session: &Path, starts: &Path) -> PathBuf {
        std::fs::create_dir_all(directory).expect("the directory is made");
        let program = directory.join("fake-agent");
        let answer = format!(
            "{{\"type\":\"response\",\"id\":\"boot\",\"success\":true,\
             \"data\":{{\"sessionFile\":\"{}\",\"isStreaming\":false}}}}",
            session.display()
        );
        let script = format!(
            "#!/bin/sh\n\
             echo start >> '{starts}'\n\
             while IFS= read -r line; do\n\
             \tcase \"$line\" in\n\
             \t*get_state*) printf '%s\\n' '{answer}' ;;\n\
             \tesac\n\
             done\n",
            starts = starts.display(),
        );
        std::fs::write(&program, script).expect("the program is written");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))
            .expect("it is executable");
        program
    }

    /// One connection, held open, in the role it said hello in.
    ///
    /// Everything here goes over the real socket rather than through the
    /// service's own methods, because holding the connection open is the
    /// mechanism under test and a direct call would not hold anything.
    struct Client {
        writer: UnixStream,
        reader: BufReader<UnixStream>,
    }

    impl Client {
        fn open(path: &Path, role: Role) -> Self {
            let writer = UnixStream::connect(path).expect("it connects");
            let reader = BufReader::new(writer.try_clone().expect("it splits"));
            let mut client = Self { writer, reader };
            client.send(ClientBody::Hello { role });
            assert_eq!(client.read(), ServiceBody::Welcome { role });
            client
        }

        fn send(&mut self, body: ClientBody) {
            scufris_control::write_message(&mut self.writer, &ClientMessage::new(body))
                .expect("the request is written");
        }

        fn read(&mut self) -> ServiceBody {
            read_service_message(&mut self.reader)
                .expect("the service answers")
                .body
        }

        fn state(&mut self) -> ScufrisState {
            self.send(ClientBody::GetState { id: "c".into() });
            match self.read() {
                ServiceBody::State { state, .. } => state,
                other => panic!("get_state was answered with {}", other.name()),
            }
        }
    }

    /// Waits until the service reports the state that was asked for.
    fn until(path: &Path, wanted: ScufrisState) {
        let mut client = Client::open(path, Role::Control);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let seen = client.state();
            if seen == wanted {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {}, stuck at {}",
                wanted.name(),
                seen.name()
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// How many times the stand-in agent has been started.
    fn starts(path: &Path) -> usize {
        std::fs::read_to_string(path)
            .map(|held| held.lines().count())
            .unwrap_or(0)
    }

    #[test]
    fn binding_makes_a_private_directory_and_a_private_socket() {
        let path = scratch("private");
        let listener = bind(&path).expect("it binds");
        let directory = path.parent().expect("it has a directory");
        assert_eq!(
            std::fs::metadata(directory)
                .expect("the directory is there")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path)
                .expect("the socket is there")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        drop(listener);
        std::fs::remove_dir_all(directory).expect("the directory is removed");
    }

    #[test]
    fn a_stale_socket_is_replaced_and_a_live_one_is_not() {
        let path = scratch("stale");
        let listener = bind(&path).expect("it binds");
        // A service that still answers owns the conversation. Taking the
        // socket from it would leave two services and one agent each.
        let error = bind(&path).expect_err("the second bind is refused");
        assert_eq!(error.kind(), io::ErrorKind::AddrInUse);
        // A socket file with nothing behind it is what a crash leaves.
        //
        // The wait is about this test process, not about the service. Another
        // test spawns children, and a child holds a copy of every open
        // descriptor between the fork and the exec that drops them, so for a
        // few microseconds a closed listener still answers.
        drop(listener);
        let deadline = Instant::now() + Duration::from_secs(10);
        while UnixStream::connect(&path).is_ok() {
            assert!(
                Instant::now() < deadline,
                "the closed listener still answers"
            );
            thread::sleep(Duration::from_millis(20));
        }
        let replacement = bind(&path).expect("a stale socket is replaced");
        drop(replacement);
        std::fs::remove_dir_all(path.parent().expect("it has a directory"))
            .expect("the directory is removed");
    }

    #[test]
    fn a_connection_that_does_not_say_hello_first_is_closed() {
        let path = scratch("hello");
        let listener = bind(&path).expect("it binds");
        let service = crate::service::Service::new(crate::config::Config {
            agent: PathBuf::from("/nonexistent/scufris"),
            session_dir: std::env::temp_dir().join("scufris-server-sessions"),
            socket: path.clone(),
            working_dir: std::env::temp_dir(),
        });
        let serving = thread::spawn(move || serve(service, listener));

        let mut client = UnixStream::connect(&path).expect("it connects");
        client
            .write_all(b"{\"v\":3,\"type\":\"abort\",\"id\":\"c-1\"}\n")
            .expect("the line is written");
        let mut answer = Vec::new();
        std::io::Read::read_to_end(&mut client, &mut answer).expect("the socket is read");
        assert!(
            answer.is_empty(),
            "nothing is answered before hello: {}",
            String::from_utf8_lossy(&answer)
        );

        std::fs::remove_file(&path).expect("the socket is removed");
        let _ = UnixStream::connect(&path);
        drop(serving);
        std::fs::remove_dir_all(path.parent().expect("it has a directory"))
            .expect("the directory is removed");
    }

    #[test]
    fn a_debug_lease_hands_the_session_over_and_closing_gives_it_back() {
        let path = scratch("lease");
        let directory = path.parent().expect("it has a directory").to_path_buf();
        let session = directory.join("sessions").join("one.jsonl");
        let record = directory.join("starts");
        let agent = fake_agent(&directory, &session, &record);
        let listener = bind(&path).expect("it binds");
        let service = Service::new(Config {
            agent: agent.clone(),
            session_dir: directory.join("sessions"),
            socket: path.clone(),
            working_dir: directory.clone(),
        });
        service.start_agent();
        let serving = Arc::clone(&service);
        thread::spawn(move || serve(serving, listener));
        until(&path, ScufrisState::Idle);
        assert_eq!(starts(&record), 1);

        let mut terminal = Client::open(&path, Role::Control);
        terminal.send(ClientBody::Debug { id: "c-1".into() });

        // The terminal is handed the exact file the agent named, so there is
        // no question about which session it takes over.
        assert_eq!(
            terminal.read(),
            ServiceBody::Debug {
                id: "c-1".into(),
                program: agent.display().to_string(),
                args: vec![
                    "--session-dir".into(),
                    directory.join("sessions").display().to_string(),
                    "--session".into(),
                    session.display().to_string(),
                ],
            }
        );
        assert_eq!(terminal.state(), ScufrisState::Detached);
        assert_eq!(starts(&record), 1, "the agent is stopped, not restarted");
        // A second lease has nothing to take.
        let mut other = Client::open(&path, Role::Control);
        assert!(matches!(
            {
                other.send(ClientBody::Debug { id: "c-2".into() });
                other.read()
            },
            ServiceBody::Refused { ref code, .. }
                if code == scufris_control::service::refusal::DEBUG_HELD
        ));

        // Nothing else is sent. The connection closing is the whole signal,
        // and it arrives the same way whether a terminal exited or was killed.
        drop(terminal);
        until(&path, ScufrisState::Idle);
        assert_eq!(starts(&record), 2);

        service.shutdown();
        std::fs::remove_dir_all(&directory).expect("the directory is removed");
    }
}
