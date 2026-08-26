//! Press the pill's keys without holding the keyboard.
//!
//! One verb per run, one line onto the companion's command socket, one line
//! back. It exists so a window manager binding can be the thing that reads the
//! key: i3 grabs Escape inside its own mode and runs this, and the pill never
//! has to be the focused window for its own keys to work.
//!
//! Exit status is what a binding can branch on without parsing anything: 0 the
//! verb reached the pill, 1 it did not, 2 the run was wrong.

use std::{
    io::{BufReader, Write},
    os::unix::net::UnixStream,
    process::ExitCode,
};

use scufris_control::{
    command::{Answer, COMMAND_VERSION, Command, Outcome, Verb, command_socket_path},
    read_message, write_message,
};

/// What the two failing exits mean.
const REFUSED: u8 = 1;
const MISUSED: u8 = 2;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(word), None) = (args.next(), args.next()) else {
        return usage();
    };
    if word == "--help" || word == "-h" {
        println!("{}", help());
        return ExitCode::SUCCESS;
    }
    let Some(verb) = Verb::named(&word) else {
        eprintln!("scufris-ctl: {word} is not a verb");
        return usage();
    };
    match send(verb) {
        Ok(Outcome::Taken) => ExitCode::SUCCESS,
        Ok(Outcome::Refused { detail }) => {
            eprintln!("scufris-ctl: {detail}");
            ExitCode::from(REFUSED)
        }
        Err(reason) => {
            eprintln!("scufris-ctl: {reason}");
            ExitCode::from(REFUSED)
        }
    }
}

/// Sends one verb and waits for the answer.
fn send(verb: Verb) -> Result<Outcome, String> {
    let path = command_socket_path().map_err(|error| error.to_string())?;
    let mut stream = UnixStream::connect(&path).map_err(|error| {
        format!(
            "the companion is not listening on {}: {error}",
            path.display()
        )
    })?;
    write_message(&mut stream, &Command::new(verb)).map_err(|error| error.to_string())?;
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
    Ok(answer.outcome)
}

fn help() -> String {
    format!(
        "usage: scufris-ctl <verb>\n\n\
         verbs:\n  \
         {open:<8}bring the pill up and start recording\n  \
         {cancel:<8}cancel what is running, or put a resting pill away\n  \
         {accept:<8}accept what the pill is showing\n\n\
         exit: 0 the verb reached the pill, {REFUSED} it did not, {MISUSED} the run was wrong",
        open = Verb::Open.name(),
        cancel = Verb::Cancel.name(),
        accept = Verb::Accept.name(),
    )
}

fn usage() -> ExitCode {
    let mut err = std::io::stderr();
    let _ = writeln!(err, "{}", help());
    ExitCode::from(MISUSED)
}
