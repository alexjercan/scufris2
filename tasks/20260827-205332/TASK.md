# Bound every string the service emits

- STATUS: OPEN
- PRIORITY: 100
- TAGS: bug,service,protocol

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
