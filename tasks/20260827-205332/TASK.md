# Bound every string the service emits

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: bug, service, protocol

## Source

Review round 1 of `20260827-081702`, range `185034a..a13cb38`. Findings
B1, m2 and m16. Full record: `tasks/20260827-081702/REVIEW.md`.

## Fault

The service can build a message its own reader refuses, and the failure
is silent.

`service.rs:381` adopts the agent's `error` string into `detail` with no
bound, and `service.rs:415` does the same for a refusal. The service
accepts up to 4 MiB from the agent (`service.rs:57`); `write_message`
refuses anything over 64 KiB (`scufris-control/src/lib.rs:154`).
`set_state` stores the string and `register` replays `State { detail }`
to every frontend that connects (`service.rs:538`), so one oversize
error poisons every later connection.

The write fails, logs at `debug!` (`server.rs:102`) under a default
`info` filter (`logging.rs:13`), breaks the loop and shuts the socket
down. Every client gets `welcome` and then EOF: the pill, the
conversation window and `scufris-ctl` alike. The service still reports
that it is listening.

The companion amplifies it. A clean EOF returns `Ok(())`, which resets
the backoff to `MIN_BACKOFF` (`link.rs:98`), so it reconnects four times
a second and clears the conversation window on each `Connected`. The
replay that would refill the window is the message that cannot be
written.

Reproduced by the red team lane against a real service with a stand-in
agent.

## Work

- Bound every string the service puts into a `ServiceBody` at the moment
  it adopts it. Reuse the char-boundary `truncate` at `rpc.rs:215`. The
  invariant to hold: the service never emits a message its own reader
  would reject.
- Raise the unwritable-message log above `debug`, and distinguish
  `MessageError::TooLarge` - the service built something it cannot send
  - from a peer that went away. (m2)
- Widen the companion backoff on a clean EOF, or count repeated
  short-lived connections, so a service in this state does not draw four
  reconnects a second. (`link.rs:98`)
- Fold the three byte-bound cutters into one. `rpc.rs:215` and
  `speech.rs:231` are the same `is_char_boundary` walk in two crates
  that both already depend on `scufris-control`, where the bound itself
  lives. (m16; the TypeScript third copy is `client.ts:498`, queued
  separately as m1.)

## Proof

- A unit test that an oversize agent error is truncated before it
  reaches `set_state`, so `register` replays something writable.
- The red team reproduction re-run against the fix: a stand-in agent
  returns a 128 KiB error, and clients still receive state after it.
- `cd native && TMPDIR=/tmp nix develop --offline -c cargo test
--workspace`.

## Open

The outbox overflow path was not checked at all (see "Not checked" in
the review). Its recovery is the same non-widening 250 ms reconnect this
task touches, so it is worth measuring here rather than separately.

## Outcome (2026-08-27)

Done. The invariant is held structurally rather than by remembering it at
each site.

- `ServiceBody::bounded` clamps every free-text field the reader does not
  otherwise refuse, and `ServiceMessage::new` applies it. There is no
  construction path that skips it, including sites nobody has written
  yet. Identifiers, codes and widget payloads are still refused rather
  than shortened: a truncated identifier names the wrong thing, where a
  truncated sentence is still the sentence.
- `MAX_DETAIL_BYTES` (4 KiB) is declared beside the other service bounds,
  and `is_detail_text` now refuses an oversize detail on read, so both
  ends measure it the same way. Before this, `detail` was the one
  free-text field with no bound anywhere.
- `set_state` truncates on adoption as well. The state detail is kept and
  replayed to every frontend that connects, so storing the whole 4 MiB
  would hold the memory and re-cut it on every replay.
- `Speak.text` had the same defect: validated on read, never bounded on
  write. It is bounded now.
- m2: an unwritable message logs at `error!` with the body name, and is
  told apart from a peer that went away, which stays at `debug!`.
- The companion resets its backoff only when a connection lasted at least
  `SETTLED` (5 s), not merely when it ended tidily. A service in this
  state ends every connection cleanly, and reading only that is what drew
  four reconnects a second.
- m16: one `truncate`, in `scufris-control`. The copies in `rpc.rs` and
  `speech.rs` are gone.

### Proof

- `the_service_cannot_build_a_message_its_own_reader_would_reject` walks
  all five bodies that carry adopted text, writes each and reads it back.
- `an_oversize_agent_error_does_not_become_a_message_no_client_can_read`
  drives the real B1 path: a 2 MiB agent error through `apply`, then a
  frontend connects and every replayed message is written.
- `a_detail_is_cut_on_a_character_boundary_and_kept_readable` and
  `truncation_cuts_on_a_character_boundary_and_never_inside_one`.
- The transcript bound test now builds its oversize message without
  `new`, because `new` clamps. It is the reader's rule and it has to hold
  against a peer that did not clamp.
- `cargo test --workspace`: 335 pass, 0 fail. `cargo fmt --check` and
  `cargo clippy --workspace --all-targets` clean.

Not done here: the outbox overflow pass the review listed as unchecked.
It is still unchecked.
