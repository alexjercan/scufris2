//! Minimal protocol v11 control client.

use std::{
    fs,
    io::{BufRead, BufReader},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, SystemTime},
};

use clap::{Parser, Subcommand};
use scufris_control::command::{
    Answer, COMMAND_VERSION, Command as DesktopCommand, Outcome, Verb, command_socket_path,
};
use scufris_control::service::{
    BriefingRow, BriefingWake, ControlRequest, ControlRequestBody, ControlResponseBody,
    control_socket_path, read_control_response,
};
use scufris_control::{MessageError, read_message, write_message};
use serde::Deserialize;
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
    /// Import a durable briefing lifecycle update and optional terminal wake.
    Briefing {
        /// JSON object with `briefing` and optional `wake` fields.
        update: String,
    },
    /// Work on the chain of session files the conversation has been through.
    Lineage {
        #[command(subcommand)]
        command: LineageCommand,
    },
}

#[derive(Debug, Subcommand)]
enum LineageCommand {
    /// Remove forked session files nothing continues from any more.
    ///
    /// Every handoff copies the branch into a new file, so a long-lived
    /// conversation leaves the old copies behind. Only a fork is ever removed,
    /// never the file the next holder would start from, and never one touched
    /// inside the window.
    Prune {
        /// Days a file is kept after it was last written.
        #[arg(long, default_value_t = 30)]
        keep: u64,
        /// Say what would go, and remove nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BriefingUpdate {
    briefing: BriefingRow,
    #[serde(default)]
    wake: Option<BriefingWake>,
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
        Some(Command::Briefing { update }) => {
            let update: BriefingUpdate = serde_json::from_str(&update)
                .map_err(|error| format!("briefing update is not valid JSON: {error}"))?;
            ControlRequestBody::Briefing {
                id: WAKE_ID.into(),
                briefing: update.briefing,
                wake: update.wake,
            }
        }
        Some(Command::Lineage {
            command: LineageCommand::Prune { keep, dry_run },
        }) => return prune(keep, dry_run),
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
            holder,
            generation,
            session_dir,
            lineage_file,
        } if answered == STATE_ID => {
            if detail.is_empty() {
                println!("{}", state.name());
            } else {
                println!("{}: {}", state.name(), detail);
            }
            match generation {
                Some(generation) => println!("holder: {} {generation}", holder.name()),
                None => println!("holder: {}", holder.name()),
            }
            println!("sessions: {session_dir}");
            if let Some(file) = lineage_file {
                println!("lineage: {file}");
            }
            Ok(())
        }
        // A wake that did not land is a failure the caller has to see: its own
        // durable state is the fallback, and it can only keep one if it knows.
        ControlResponseBody::WakeAck { id: answered } if answered == WAKE_ID => {
            println!("woken");
            Ok(())
        }
        ControlResponseBody::BriefingAck { id: answered } if answered == WAKE_ID => {
            println!("recorded");
            Ok(())
        }
        ControlResponseBody::Rejected { code, detail, .. } => Err(format!("{code}: {detail}")),
        other => Err(format!("unexpected control response: {other:?}")),
    }
}

/// Asks the service where the lineage lives and what it is now.
fn lineage_state() -> Result<(PathBuf, Option<PathBuf>), String> {
    let path = control_socket_path().map_err(|error| error.to_string())?;
    let mut stream = UnixStream::connect(&path)
        .map_err(|error| format!("cannot connect to {}: {error}", path.display()))?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|error| error.to_string())?);
    write_message(&mut stream, &ControlRequest::new(ControlRequestBody::Hello)).map_err(render)?;
    match read_control_response(&mut reader).map_err(render)?.body {
        ControlResponseBody::Ready => {}
        other => return Err(format!("control handshake was rejected: {other:?}")),
    }
    write_message(
        &mut stream,
        &ControlRequest::new(ControlRequestBody::State {
            id: STATE_ID.into(),
        }),
    )
    .map_err(render)?;
    match read_control_response(&mut reader).map_err(render)?.body {
        ControlResponseBody::State {
            session_dir,
            lineage_file,
            ..
        } => Ok((PathBuf::from(session_dir), lineage_file.map(PathBuf::from))),
        ControlResponseBody::Rejected { code, detail, .. } => Err(format!("{code}: {detail}")),
        other => Err(format!("unexpected control response: {other:?}")),
    }
}

/// Whether this file is a copy of another session rather than an original.
///
/// Pi writes the source in the new session's header when it forks, so the
/// header is the whole test. A file whose header cannot be read is left alone:
/// nothing here deletes what it did not understand.
fn is_fork(path: &Path) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let mut first = String::new();
    if BufReader::new(file).read_line(&mut first).is_err() {
        return false;
    }
    let Ok(header) = serde_json::from_str::<Value>(&first) else {
        return false;
    };
    header.get("type").and_then(Value::as_str) == Some("session")
        && header
            .get("parentSession")
            .and_then(Value::as_str)
            .is_some()
}

/// The forked session files in `session_dir` that nothing needs any more.
///
/// Four things save a file: it is not a session file, it is the one the next
/// holder would fork from, it was written inside the window, or its header
/// does not say it was forked. The order is sorted so the printed report and
/// the tests read the same on every filesystem.
fn prunable(
    session_dir: &Path,
    lineage_file: Option<&Path>,
    cutoff: SystemTime,
) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(session_dir)
        .map_err(|error| format!("cannot read {}: {error}", session_dir.display()))?;
    let mut stale = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read a session entry: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }
        if lineage_file == Some(path.as_path()) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        match metadata.modified() {
            Ok(modified) if modified < cutoff => {}
            _ => continue,
        }
        if !is_fork(&path) {
            continue;
        }
        stale.push(path);
    }
    stale.sort();
    Ok(stale)
}

fn prune(keep: u64, dry_run: bool) -> Result<(), String> {
    let (session_dir, lineage_file) = lineage_state()?;
    let window = Duration::from_secs(keep.saturating_mul(24 * 60 * 60));
    let cutoff = SystemTime::now()
        .checked_sub(window)
        .ok_or_else(|| "--keep is further back than time goes".to_string())?;
    let stale = prunable(&session_dir, lineage_file.as_deref(), cutoff)?;
    for path in &stale {
        if dry_run {
            println!("would remove {}", path.display());
        } else {
            fs::remove_file(path)
                .map_err(|error| format!("cannot remove {}: {error}", path.display()))?;
            println!("removed {}", path.display());
        }
    }
    println!(
        "{} forked session{} older than {keep} days",
        stale.len(),
        if stale.len() == 1 { "" } else { "s" }
    );
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn sessions(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("scufris-prune-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("a session directory");
        root
    }

    /// Writes one session file with the header Pi would have written, aged.
    fn session(dir: &Path, name: &str, parent: Option<&str>, age: Duration) -> PathBuf {
        let path = dir.join(name);
        let header = match parent {
            Some(parent) => {
                format!("{{\"type\":\"session\",\"id\":\"{name}\",\"parentSession\":\"{parent}\"}}")
            }
            None => format!("{{\"type\":\"session\",\"id\":\"{name}\"}}"),
        };
        write_aged(&path, &format!("{header}\n"), age);
        path
    }

    fn write_aged(path: &Path, body: &str, age: Duration) {
        let file = fs::File::create(path).expect("write a session");
        use std::io::Write;
        (&file)
            .write_all(body.as_bytes())
            .expect("write the header");
        file.set_modified(SystemTime::now() - age)
            .expect("age the session");
    }

    #[test]
    fn only_old_forks_outside_the_lineage_are_pruned() {
        let root = sessions("mixed");
        let old_fork = session(&root, "old-fork.jsonl", Some("/s/root.jsonl"), DAY * 40);
        let recent_fork = session(&root, "recent-fork.jsonl", Some("/s/root.jsonl"), DAY);
        let original = session(&root, "root.jsonl", None, DAY * 40);
        let lineage = session(&root, "live.jsonl", Some("/s/root.jsonl"), DAY * 40);
        write_aged(&root.join("notes.txt"), "not a session\n", DAY * 40);

        let cutoff = SystemTime::now() - DAY * 30;
        let stale = prunable(&root, Some(lineage.as_path()), cutoff).expect("a listing");

        assert_eq!(stale, vec![old_fork]);
        assert!(recent_fork.exists() && original.exists() && lineage.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_that_is_not_a_readable_session_is_left_alone() {
        let root = sessions("partial");
        write_aged(
            &root.join("half-written.jsonl"),
            "{\"type\":\"session\",\"id\":\"half",
            DAY * 40,
        );

        let cutoff = SystemTime::now() - DAY * 30;
        assert!(prunable(&root, None, cutoff).expect("a listing").is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
