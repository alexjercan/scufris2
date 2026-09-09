//! Minimal protocol v7 control client.

use std::{io::BufReader, os::unix::net::UnixStream, process::ExitCode};

use clap::{Parser, Subcommand};
use scufris_control::command::{
    Answer, COMMAND_VERSION, Command as DesktopCommand, Outcome, Verb, command_socket_path,
};
use scufris_control::service::{
    ControlRequest, ControlRequestBody, ControlResponseBody, control_socket_path,
    read_control_response,
};
use scufris_control::{MessageError, read_message, write_message};
use serde_json::Value;

/// Custom message type a wake carries when the caller names none.
const DEFAULT_WAKE_TYPE: &str = "scufris-wake";
const STATE_ID: &str = "state-1";
const WAKE_ID: &str = "wake-1";

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
    /// Wake the foreground conversation with a proactive message.
    Wake {
        /// Words the conversation receives without the owner having typed them.
        text: String,
        /// Custom message type an existing wake handler matches.
        #[arg(long, default_value = DEFAULT_WAKE_TYPE)]
        custom_type: String,
        /// JSON object of structured facts carried beside the words.
        #[arg(long)]
        details: Option<String>,
    },
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
    let request = match options.command {
        Some(Command::Open) => return desktop(Verb::Open),
        Some(Command::Hud) => return desktop(Verb::Hud),
        Some(Command::Show) => return desktop(Verb::Show),
        Some(Command::Hide) => return desktop(Verb::Hide),
        Some(Command::Wake {
            text,
            custom_type,
            details,
        }) => ControlRequestBody::Wake {
            id: WAKE_ID.into(),
            custom_type,
            text,
            details: details.as_deref().map(wake_details).transpose()?,
        },
        Some(Command::State) | None => ControlRequestBody::State {
            id: STATE_ID.into(),
        },
    };
    let path = control_socket_path().map_err(|error| error.to_string())?;
    let mut stream = UnixStream::connect(&path)
        .map_err(|error| format!("cannot connect to {}: {error}", path.display()))?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|error| error.to_string())?);
    write_message(&mut stream, &ControlRequest::new(ControlRequestBody::Hello)).map_err(render)?;
    match read_control_response(&mut reader).map_err(render)?.body {
        ControlResponseBody::Ready => {}
        other => return Err(format!("control handshake was rejected: {other:?}")),
    }
    write_message(&mut stream, &ControlRequest::new(request)).map_err(render)?;
    match read_control_response(&mut reader).map_err(render)?.body {
        ControlResponseBody::State {
            id: answered,
            state,
            detail,
        } if answered == STATE_ID => {
            if detail.is_empty() {
                println!("{}", state.name());
            } else {
                println!("{}: {}", state.name(), detail);
            }
            Ok(())
        }
        // A wake that did not land is a failure the caller has to see: its own
        // durable state is the fallback, and it can only keep one if it knows.
        ControlResponseBody::WakeAck { id: answered } if answered == WAKE_ID => {
            println!("woken");
            Ok(())
        }
        ControlResponseBody::Rejected { code, detail, .. } => Err(format!("{code}: {detail}")),
        other => Err(format!("unexpected control response: {other:?}")),
    }
}

/// Reads `--details` as the JSON object the wake carries.
///
/// Rejected here rather than on the socket, because an invalid control
/// message closes the connection without a response and the caller would
/// otherwise be told the two versions disagree.
fn wake_details(raw: &str) -> Result<Value, String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|error| format!("--details is not JSON: {error}"))?;
    if value.is_object() {
        Ok(value)
    } else {
        Err("--details must be a JSON object".into())
    }
}

fn desktop(verb: Verb) -> Result<(), String> {
    let path = command_socket_path().map_err(|error| error.to_string())?;
    let mut stream = UnixStream::connect(&path)
        .map_err(|error| format!("cannot connect to {}: {error}", path.display()))?;
    write_message(&mut stream, &DesktopCommand::new(verb)).map_err(render)?;
    let answer: Answer = read_message(&mut BufReader::new(stream)).map_err(render)?;
    // The companion checks the version of what it is sent; this is the other
    // half of that gate. It matters more than it looks: `scufris-ctl` ships
    // from the service package and `desktop.sock` is served by the desktop
    // package, so a partial deployment can leave the two at different
    // versions with nothing between them but this.
    if answer.v != COMMAND_VERSION {
        return Err(format!(
            "this client speaks command version {COMMAND_VERSION} and the companion answered {}. \
             Update the host and client together.",
            answer.v
        ));
    }
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
