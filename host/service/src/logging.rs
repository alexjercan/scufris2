//! Log initialization: journald under systemd, stderr when run by hand.
//!
//! The same policy `scufris-desktop` runs on, because the two are read side by
//! side in one journal. INFO is lifecycle and state transitions only, DEBUG is
//! per-connection detail, WARN is degraded, ERROR is a user-visible failure.
//! `RUST_LOG` overrides the level either way.

use std::io::IsTerminal;

use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Directives used when `RUST_LOG` is unset: quiet steady state.
const DEFAULT_DIRECTIVES: &str = "info";

/// Installs the global subscriber.
///
/// A terminal on stderr means a person is watching, so the logs go where they
/// can see them rather than into a journal they would have to go and open.
pub fn init() -> Result<(), String> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_DIRECTIVES));
    let installed = if std::io::stderr().is_terminal() {
        stderr(filter)
    } else {
        match tracing_journald::layer() {
            Ok(journald) => tracing_subscriber::registry()
                .with(filter)
                .with(journald)
                .try_init(),
            // No journald socket: a container, or a session without systemd.
            Err(_) => stderr(filter),
        }
    };
    installed.map_err(|error| format!("logging would not initialize: {error}"))
}

fn stderr(filter: EnvFilter) -> Result<(), tracing_subscriber::util::TryInitError> {
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(std::io::stderr().is_terminal())
                .with_writer(std::io::stderr),
        )
        .try_init()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_init_reports_instead_of_panicking() {
        assert_eq!(init(), Ok(()));
        assert!(init().unwrap_err().contains("logging"));
    }
}
