//! Minimal protocol v4 control client.

use std::{io::BufReader, os::unix::net::UnixStream, process::ExitCode};

use clap::{Parser, Subcommand};
use scufris_control::command::{
    Answer, Command as DesktopCommand, Outcome, Verb, command_socket_path,
};
use scufris_control::service::{
    ControlRequest, ControlRequestBody, ControlResponseBody, control_socket_path,
    read_control_response,
};
use scufris_control::{MessageError, read_message, write_message};

#[derive(Debug, Parser)]
#[command(name = "scufris-ctl", version, about = "Inspect the Scufris service")]
struct Options {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the current server state.
    State,
    /// Open the local voice pill.
    Open,
    /// Toggle the local conversation window.
    Hud,
    /// Show the local desktop workspace.
    Show,
    /// Hide the local desktop workspace.
    Hide,
}

fn main() -> ExitCode {
    match run(Options::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("scufris-ctl: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: Options) -> Result<(), String> {
    match options.command {
        Some(Command::Open) => return desktop(Verb::Open),
        Some(Command::Hud) => return desktop(Verb::Hud),
        Some(Command::Show) => return desktop(Verb::Show),
        Some(Command::Hide) => return desktop(Verb::Hide),
        Some(Command::State) | None => {}
    }
    let path = control_socket_path().map_err(|error| error.to_string())?;
    let mut stream = UnixStream::connect(&path)
        .map_err(|error| format!("cannot connect to {}: {error}", path.display()))?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|error| error.to_string())?);
    write_message(&mut stream, &ControlRequest::new(ControlRequestBody::Hello)).map_err(render)?;
    match read_control_response(&mut reader).map_err(render)?.body {
        ControlResponseBody::Ready => {}
        other => return Err(format!("control handshake was rejected: {other:?}")),
    }
    let id = "state-1".to_string();
    write_message(
        &mut stream,
        &ControlRequest::new(ControlRequestBody::State { id: id.clone() }),
    )
    .map_err(render)?;
    match read_control_response(&mut reader).map_err(render)?.body {
        ControlResponseBody::State {
            id: answered,
            state,
            detail,
        } if answered == id => {
            if detail.is_empty() {
                println!("{}", state.name());
            } else {
                println!("{}: {}", state.name(), detail);
            }
            Ok(())
        }
        ControlResponseBody::Rejected { code, detail, .. } => Err(format!("{code}: {detail}")),
        other => Err(format!("unexpected control response: {other:?}")),
    }
}

fn desktop(verb: Verb) -> Result<(), String> {
    let path = command_socket_path().map_err(|error| error.to_string())?;
    let mut stream = UnixStream::connect(&path)
        .map_err(|error| format!("cannot connect to {}: {error}", path.display()))?;
    write_message(&mut stream, &DesktopCommand::new(verb)).map_err(render)?;
    let answer: Answer = read_message(&mut BufReader::new(stream)).map_err(render)?;
    match answer.outcome {
        Outcome::Taken => Ok(()),
        Outcome::Refused { detail } => Err(detail),
    }
}

fn render(error: MessageError) -> String {
    match error {
        MessageError::Empty | MessageError::Io(_) => {
            "The host and client protocol handshake failed. Update the host and client together."
                .into()
        }
        other => other.to_string(),
    }
}
