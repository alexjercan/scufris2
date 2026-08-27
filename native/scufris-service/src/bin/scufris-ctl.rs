//! Talk to Scufris from a terminal.
//!
//! One verb per run. Most verbs open the service socket, ask one thing, read
//! the answer, and go away; that is what the `control` role is for, and it
//! works with no graphical session at all, which is how the service gets
//! tested without a display and how the conversation stays reachable over ssh.
//!
//! Three verbs still go to the companion's own socket instead, because the
//! pill still has keys of its own. They go away with the pill's keys.
//!
//! `debug` is the one verb that does more than send a line. It takes the agent
//! away from the service, starts `pi` on the same session in this terminal,
//! and gives the agent back on the way out. There is deliberately no `detach`
//! and no `attach`: a pair of verbs is a sequence to remember and a state to
//! get stuck in, and the whole point is that there is no way to be left
//! detached with nothing to put it back. The lease is this process's
//! connection. When it closes - a clean exit, a Ctrl-C, a closed terminal, a
//! kill - the service starts the agent again.
//!
//! Exit status is what a binding can branch on without parsing anything: 0 it
//! worked, 1 it did not, 2 the run was wrong. `debug` exits with whatever
//! `pi` exited with.

use std::{
    io::{BufReader, IsTerminal, Write},
    os::unix::net::UnixStream,
    process::{Command, ExitCode},
    time::Duration,
};

use clap::{Parser, Subcommand};
use nix::sys::signal::{SigSet, Signal};
use scufris_control::{
    command::{
        Answer, COMMAND_VERSION, Command as PillCommand, Outcome, Verb, command_socket_path,
    },
    read_message,
    service::{
        ClientBody, ClientMessage, Role, ServiceBody, read_service_message, service_socket_path,
    },
    write_message,
};

/// What the two failing exits mean. Clap exits with `MISUSED` too, on a run
/// it could not parse.
const REFUSED: u8 = 1;
const MISUSED: u8 = 2;

/// How long to wait for the service to answer one request.
const ANSWER: Duration = Duration::from_secs(30);

/// The identifier this run correlates its one request by.
const REQUEST: &str = "ctl";

#[derive(Debug, Parser)]
#[command(
    name = "scufris-ctl",
    version,
    about = "Talk to Scufris from a terminal",
    long_about = "Talk to Scufris from a terminal.\n\n\
                  Exit status: 0 it worked, 1 it did not, 2 the run was wrong. \
                  debug exits with whatever pi exited with."
)]
struct Options {
    #[command(subcommand)]
    verb: Spoken,
}

#[derive(Debug, Subcommand)]
enum Spoken {
    /// Say something to Scufris.
    Send {
        /// What to say. Several words are joined by spaces.
        #[arg(required = true, num_args = 1.., trailing_var_arg = true)]
        words: Vec<String>,
    },
    /// Print what Scufris is doing.
    State,
    /// Follow the state and the conversation until interrupted.
    Watch,
    /// End the run that is in progress.
    Abort,
    /// Take the agent and open its session in this terminal.
    Debug,
    /// Bring the pill up and start recording.
    Open,
}

fn main() -> ExitCode {
    match Options::parse().verb {
        Spoken::Send { words } => report(ask(ClientBody::Submit {
            id: REQUEST.into(),
            text: words.join(" "),
        })),
        Spoken::State => report(ask(ClientBody::GetState { id: REQUEST.into() })),
        Spoken::Watch => report(watch()),
        Spoken::Abort => report(ask(ClientBody::Abort { id: REQUEST.into() })),
        Spoken::Debug => debug(),
        Spoken::Open => report(pill(Verb::Open)),
    }
}

/// Prints what a verb produced and turns it into an exit status.
fn report(outcome: Result<Option<String>, String>) -> ExitCode {
    match outcome {
        Ok(Some(said)) => {
            println!("{said}");
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("scufris-ctl: {reason}");
            ExitCode::from(REFUSED)
        }
    }
}

/// One open connection to the service, in one role.
struct Link {
    writer: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Link {
    /// Connects, says hello, and waits to be welcomed.
    fn open(role: Role) -> Result<Self, String> {
        let path = service_socket_path().map_err(|error| error.to_string())?;
        let writer = UnixStream::connect(&path).map_err(|error| {
            format!(
                "the service is not listening on {}: {error}",
                path.display()
            )
        })?;
        writer
            .set_read_timeout(Some(ANSWER))
            .map_err(|error| error.to_string())?;
        let reader = BufReader::new(writer.try_clone().map_err(|error| error.to_string())?);
        let mut link = Self { writer, reader };
        link.send(ClientBody::Hello { role })?;
        match link.read()? {
            ServiceBody::Welcome { role: given } if given == role => Ok(link),
            other => Err(format!("the service answered hello with {}", other.name())),
        }
    }

    /// Writes one request.
    fn send(&mut self, body: ClientBody) -> Result<(), String> {
        write_message(&mut self.writer, &ClientMessage::new(body))
            .map_err(|error| error.to_string())
    }

    /// Reads one message.
    fn read(&mut self) -> Result<ServiceBody, String> {
        read_service_message(&mut self.reader)
            .map(|message| message.body)
            .map_err(|error| error.to_string())
    }
}

/// Sends one request as a control client and reports its answer.
fn ask(body: ClientBody) -> Result<Option<String>, String> {
    let mut link = Link::open(Role::Control)?;
    link.send(body)?;
    match link.read()? {
        ServiceBody::Ok { .. } => Ok(None),
        ServiceBody::State { state, detail, .. } => Ok(Some(said(state.name(), &detail))),
        ServiceBody::Refused { code, detail, .. } => Err(if detail.is_empty() {
            code
        } else {
            format!("{detail} ({code})")
        }),
        // A control client is not pushed anything, so anything else is the
        // service and this build disagreeing about the protocol.
        other => Err(format!("the service answered with {}", other.name())),
    }
}

/// Follows the state and the conversation until the socket closes.
fn watch() -> Result<Option<String>, String> {
    let mut link = Link::open(Role::Frontend)?;
    link.writer
        .set_read_timeout(None)
        .map_err(|error| error.to_string())?;
    let mut out = std::io::stdout();
    loop {
        let line = match link.read() {
            Ok(ServiceBody::State { state, detail, .. }) => said(state.name(), &detail),
            Ok(ServiceBody::Transcript { entry }) => {
                format!("{}: {}", speaker(entry.speaker), entry.text)
            }
            Ok(other) => format!("({})", other.name()),
            // The service went away, which is the ordinary end of a watch.
            Err(_) => return Ok(None),
        };
        if writeln!(out, "{line}").is_err() {
            return Ok(None);
        }
        let _ = out.flush();
    }
}

/// Takes the agent away and runs `pi` on the same session in this terminal.
fn debug() -> ExitCode {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        eprintln!("scufris-ctl: debug needs a terminal to hand you");
        return ExitCode::from(MISUSED);
    }
    let mut link = match Link::open(Role::Control) {
        Ok(link) => link,
        Err(reason) => {
            eprintln!("scufris-ctl: {reason}");
            return ExitCode::from(REFUSED);
        }
    };
    let asked = link
        .send(ClientBody::Debug { id: REQUEST.into() })
        .and_then(|()| link.read());
    let (program, args) = match asked {
        Ok(ServiceBody::Debug { program, args, .. }) => (program, args),
        Ok(ServiceBody::Refused { code, detail, .. }) => {
            eprintln!("scufris-ctl: {detail} ({code})");
            return ExitCode::from(REFUSED);
        }
        Ok(other) => {
            eprintln!("scufris-ctl: the service answered with {}", other.name());
            return ExitCode::from(REFUSED);
        }
        Err(reason) => {
            eprintln!("scufris-ctl: {reason}");
            return ExitCode::from(REFUSED);
        }
    };

    // The service has already stopped its agent, so from here the session
    // belongs to this terminal until this process exits.
    let mut child = match Command::new(&program).args(&args).spawn() {
        Ok(child) => child,
        Err(error) => {
            eprintln!("scufris-ctl: {program} would not start: {error}");
            return ExitCode::from(REFUSED);
        }
    };

    // After the spawn, never before: the mask is inherited across exec, and
    // blocking first would leave `pi` unable to see its own Ctrl-C. Blocking
    // now keeps the terminal's interrupt from killing this process out from
    // under the agent it is holding the session for.
    let mut held = SigSet::empty();
    held.add(Signal::SIGINT);
    held.add(Signal::SIGQUIT);
    if let Err(error) = held.thread_block() {
        eprintln!("scufris-ctl: interrupts could not be held: {error}");
    }

    let status = child.wait();
    // Closing the connection is what gives the agent back, and it happens
    // here whether `pi` exited cleanly or was killed.
    drop(link);
    match status {
        Ok(status) => match status.code() {
            Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(REFUSED)),
            None => ExitCode::from(REFUSED),
        },
        Err(error) => {
            eprintln!("scufris-ctl: {program} could not be waited for: {error}");
            ExitCode::from(REFUSED)
        }
    }
}

/// Sends one verb to the companion's own command socket.
fn pill(verb: Verb) -> Result<Option<String>, String> {
    let path = command_socket_path().map_err(|error| error.to_string())?;
    let mut stream = UnixStream::connect(&path).map_err(|error| {
        format!(
            "the companion is not listening on {}: {error}",
            path.display()
        )
    })?;
    write_message(&mut stream, &PillCommand::new(verb)).map_err(|error| error.to_string())?;
    // Half-closed rather than left open. The companion reads one verb per
    // connection, and this is what says there is no second one coming.
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(stream);
    let answer: Answer = read_message(&mut reader).map_err(|error| error.to_string())?;
    if answer.v != COMMAND_VERSION {
        return Err(format!(
            "the companion speaks command version {}, this speaks {COMMAND_VERSION}",
            answer.v
        ));
    }
    match answer.outcome {
        Outcome::Taken => Ok(None),
        Outcome::Refused { detail } => Err(detail),
    }
}

/// One state line: the name, and the detail when there is one.
fn said(state: &str, detail: &str) -> String {
    if detail.is_empty() {
        state.to_string()
    } else {
        format!("{state}: {detail}")
    }
}

/// The word one speaker is printed as.
fn speaker(speaker: scufris_control::service::Speaker) -> &'static str {
    match speaker {
        scufris_control::service::Speaker::User => "user",
        scufris_control::service::Speaker::Assistant => "scufris",
    }
}
