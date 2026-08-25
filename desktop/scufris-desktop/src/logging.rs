//! Log initialization: journald for the service, pretty stderr for the dev CLI.
//!
//! One binary serves both. The service path logs structured fields to journald;
//! `--foreground`, or an unreachable journald, writes to stderr with ANSI
//! colors when stderr is a terminal. `RUST_LOG` overrides the level policy
//! either way. The policy itself: INFO is lifecycle and state transitions
//! only, DEBUG is per-request detail, WARN is degraded, ERROR is a
//! user-visible failure. Log-crate records from dependencies flow into the
//! same subscriber through the tracing-log bridge `try_init` installs.

use std::io::IsTerminal;

use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Directives used when `RUST_LOG` is unset: quiet steady state.
const DEFAULT_DIRECTIVES: &str = "info";

/// Installs the global subscriber. `foreground` forces the stderr layer.
pub fn init(foreground: bool) -> Result<(), String> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_DIRECTIVES));
    let installed = if foreground {
        stderr(filter)
    } else {
        match tracing_journald::layer() {
            Ok(journald) => tracing_subscriber::registry()
                .with(filter)
                .with(journald)
                .try_init(),
            // No journald socket - a container, a session without systemd. The
            // logs still have to go somewhere readable.
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
        assert_eq!(init(true), Ok(()));
        assert!(init(true).unwrap_err().contains("logging"));
    }
}
