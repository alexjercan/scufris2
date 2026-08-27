//! The command socket the window manager opens the pill through.
//!
//! The companion is the server here, which is the opposite of everywhere else:
//! [`crate::link`] connects out to the Scufris service, and this listens for
//! the person's own window manager.
//!
//! One verb per connection, answered before the connection closes. There is no
//! session and nothing to keep: a key binding runs `scufris-ctl`, it says what
//! happened, and it is gone. A connection that says something else is closed
//! rather than argued with.
//!
//! The socket is the person's alone, in their own runtime directory with a
//! private directory above it. Anything that can open it can already act as
//! them.

use std::{
    fs,
    io::BufReader,
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::{Path, PathBuf},
    thread,
};

use scufris_control::{
    command::{Answer, COMMAND_VERSION, Command, Outcome, Verb},
    read_message, write_message,
};
use tracing::{debug, info, warn};

/// Permissions on the socket and the directory holding it: the person's own.
const PRIVATE: u32 = 0o700;

/// Starts the listener, and returns the path it is listening on.
///
/// `act` is called on the listener's own thread, once per verb, and answers
/// what to report. It must not block for long: the caller is a key binding and
/// the person is waiting on it.
///
/// A socket that cannot be made is reported and nothing else. The companion
/// still works from its own hotkey and its tray, and refusing to start over a
/// socket the person may not use would be the worse trade.
pub fn listen(
    path: PathBuf,
    act: impl Fn(Verb) -> Outcome + Send + Sync + 'static,
) -> Result<PathBuf, String> {
    let listener = bind(&path)?;
    let listening = path.clone();
    thread::spawn(move || {
        for connection in listener.incoming() {
            match connection {
                Ok(stream) => answer(stream, &act),
                // One refused connection is not a reason to stop listening for
                // the next one.
                Err(error) => debug!("a command connection would not open: {error}"),
            }
        }
        info!("the command socket stopped listening");
    });
    info!(socket = %listening.display(), "listening for desktop commands");
    Ok(listening)
}

/// Makes the socket, replacing one an earlier run left behind.
fn bind(path: &Path) -> Result<UnixListener, String> {
    let directory = path
        .parent()
        .ok_or_else(|| format!("{} has no directory to live in", path.display()))?;
    fs::create_dir_all(directory)
        .map_err(|error| format!("{} could not be made: {error}", directory.display()))?;
    fs::set_permissions(directory, fs::Permissions::from_mode(PRIVATE))
        .map_err(|error| format!("{} could not be made private: {error}", directory.display()))?;
    // A socket file outlives the process that made it, so a companion that was
    // killed leaves one behind that nothing is listening on. Removed rather
    // than refused: the alternative is a companion that will not start until
    // the person deletes a file.
    match fs::remove_file(path) {
        Ok(()) => debug!(socket = %path.display(), "an earlier command socket was removed"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("{} could not be cleared: {error}", path.display())),
    }
    let listener = UnixListener::bind(path)
        .map_err(|error| format!("{} could not be opened: {error}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE))
        .map_err(|error| format!("{} could not be made private: {error}", path.display()))?;
    Ok(listener)
}

/// Reads one verb off one connection and writes back what became of it.
fn answer(mut stream: std::os::unix::net::UnixStream, act: &impl Fn(Verb) -> Outcome) {
    let outcome = match read(&mut stream) {
        Ok(verb) => {
            info!(verb = verb.name(), "a desktop command arrived");
            act(verb)
        }
        Err(detail) => {
            debug!("a command was not understood: {detail}");
            Outcome::Refused { detail }
        }
    };
    if let Err(error) = write_message(&mut stream, &Answer::new(outcome)) {
        // The caller hung up before reading. Nothing is owed to it, and the
        // verb was already carried out.
        debug!("a command answer went nowhere: {error}");
    }
}

/// Reads one verb, or says why the line was not one.
fn read(stream: &mut std::os::unix::net::UnixStream) -> Result<Verb, String> {
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|error| format!("the connection could not be read: {error}"))?,
    );
    let command: Command =
        read_message(&mut reader).map_err(|error| format!("that was not a command: {error}"))?;
    if command.v != COMMAND_VERSION {
        return Err(format!(
            "this companion speaks command version {COMMAND_VERSION}, not {}",
            command.v
        ));
    }
    Ok(command.verb)
}

/// Removes the socket file. The companion is going away.
///
/// Not required for correctness - the next start clears whatever it finds -
/// but a socket file with nothing behind it makes `scufris-ctl` report a
/// connection refused rather than a companion that is not running.
pub fn unbind(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => debug!(socket = %path.display(), "the command socket was removed"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => warn!("{} could not be removed: {error}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufReader, Write},
        os::unix::net::UnixStream,
        sync::{
            Arc, Mutex,
            atomic::{AtomicU32, Ordering},
        },
    };

    use super::*;

    static SEQUENCE: AtomicU32 = AtomicU32::new(0);

    /// A listening socket, taken down with the test that made it.
    struct Listening {
        path: PathBuf,
        heard: Arc<Mutex<Vec<Verb>>>,
    }

    impl Listening {
        fn new(name: &str, outcome: Outcome) -> Self {
            let path = std::env::temp_dir()
                .join(format!(
                    "scufris-command-{}-{}-{name}",
                    std::process::id(),
                    SEQUENCE.fetch_add(1, Ordering::Relaxed)
                ))
                .join("desktop.sock");
            let heard = Arc::new(Mutex::new(Vec::new()));
            let told = Arc::clone(&heard);
            listen(path.clone(), move |verb| {
                told.lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .push(verb);
                outcome.clone()
            })
            .expect("the socket is made");
            Self { path, heard }
        }

        /// Sends one already-encoded line and reads the answer.
        fn say(&self, line: &str) -> Answer {
            let mut stream = UnixStream::connect(&self.path).expect("the socket is listening");
            stream.write_all(line.as_bytes()).expect("the line is sent");
            stream.write_all(b"\n").expect("the line is terminated");
            stream.flush().expect("the line is flushed");
            stream
                .shutdown(std::net::Shutdown::Write)
                .expect("the write side closes");
            let mut reader = BufReader::new(stream);
            read_message(&mut reader).expect("an answer comes back")
        }

        fn verbs(&self) -> Vec<Verb> {
            self.heard
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone()
        }
    }

    impl Drop for Listening {
        fn drop(&mut self) {
            if let Some(directory) = self.path.parent() {
                let _ = fs::remove_dir_all(directory);
            }
        }
    }

    #[test]
    fn a_verb_reaches_the_pill_and_the_caller_is_told_it_did() {
        let socket = Listening::new("taken", Outcome::Taken);
        let line = serde_json::to_string(&Command::new(Verb::Open)).expect("it encodes");
        assert_eq!(socket.say(&line).outcome, Outcome::Taken);
        assert_eq!(socket.verbs(), vec![Verb::Open]);
    }

    #[test]
    fn what_the_pill_refuses_comes_back_as_the_refusal_it_gave() {
        let socket = Listening::new(
            "refused",
            Outcome::Refused {
                detail: "the pill is blind".into(),
            },
        );
        let line = serde_json::to_string(&Command::new(Verb::Open)).expect("it encodes");
        assert_eq!(
            socket.say(&line).outcome,
            Outcome::Refused {
                detail: "the pill is blind".into()
            }
        );
    }

    #[test]
    fn a_line_that_is_not_a_command_is_refused_and_never_reaches_the_pill() {
        let socket = Listening::new("nonsense", Outcome::Taken);
        for line in [
            "not json at all",
            r#"{"v":1,"verb":"quit"}"#,
            // The two the textbox took over. A binding that still sends them
            // is refused rather than answered.
            r#"{"v":1,"verb":"accept"}"#,
            r#"{"v":1,"verb":"cancel"}"#,
            r#"{"v":99,"verb":"open"}"#,
            r#"{"verb":"open"}"#,
        ] {
            assert!(
                matches!(socket.say(line).outcome, Outcome::Refused { .. }),
                "{line}"
            );
        }
        assert!(socket.verbs().is_empty(), "{:?}", socket.verbs());
    }

    #[test]
    fn a_socket_an_earlier_run_left_behind_is_replaced_rather_than_refused() {
        let socket = Listening::new("stale", Outcome::Taken);
        // The file is there and something is behind it. A second listener on
        // the same path is what a companion restarting looks like.
        let second = listen(socket.path.clone(), |_| Outcome::Taken).expect("it binds again");
        assert_eq!(second, socket.path);
        let line = serde_json::to_string(&Command::new(Verb::Open)).expect("it encodes");
        assert_eq!(socket.say(&line).outcome, Outcome::Taken);
        // The first listener no longer has the name, so it heard nothing.
        assert!(socket.verbs().is_empty());
    }

    #[test]
    fn a_socket_that_is_gone_is_gone_and_removing_it_twice_is_not_an_error() {
        let socket = Listening::new("unbind", Outcome::Taken);
        unbind(&socket.path);
        assert!(!socket.path.exists());
        unbind(&socket.path);
        assert!(UnixStream::connect(&socket.path).is_err());
    }
}
