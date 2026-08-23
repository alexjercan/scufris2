# Simplify delegated worker statuses

- STATUS: CLOSED
- PRIORITY: 90
- TAGS: workflow

## Goal

Reduce delegated worker states to one predictable workflow vocabulary.

## State model

Worker-callable states are:

- `working`: the worker is actively doing assigned work.
- `blocked`: the worker cannot continue and needs mediation. Foreground Scufris
  decides whether it can resolve the blockage or must ask the user.
- `done`: the worker completed its current assignment. Foreground Scufris uses
  project preferences to decide whether to review, request more work, open
  Quick Review, land, or stop.

`failed` is runtime-generated only. A worker cannot select it. Scufris emits it
when the harness exits unexpectedly or the reporting protocol fails.

## Scope

- Remove `ready` and `needs-decision` from schemas, prompts, parsers, tools,
  skills, documentation, and tests.
- Prevent workers from submitting `failed` through the report tool or adapter.
- Keep `done` nonterminal for the worker channel. Continue watching the worker
  so foreground Scufris can send more work and receive a later `working` event.
- Treat only runtime failure, explicit stop, and landing as terminal ownership
  states.
- Rewrite workflow guidance so a `done` event never implies review, landing, or
  shutdown. Foreground Scufris always selects the next phase.

## Acceptance

- The report surface exposes only `working`, `blocked`, and `done` to workers.
- Runtime failure remains available only to trusted orchestration code.
- Independent review and implementation use the same state vocabulary.
- A worker can report `done`, receive more instructions, and report `working`
  again.
- No `ready` or `needs-decision` workflow behavior remains.

## Dependencies

Run after `20260823-153351`.

## Decisions

- Keep `failed` in the trusted event parser because filesystem and harness
  failures must still wake foreground Scufris. Exclude it from every
  worker-callable reporting schema and adapter.
- Keep `done` out of terminal ownership state. The status watcher, tmux worker,
  active-job context, and steering path remain available after completion.
- Treat an actual harness exit after `done` as `failed`. A completed assignment
  does not imply permission for the reusable worker process to disappear.
- End a Pi worker's current model turn after `blocked` or `done`, but keep its
  interactive process alive for foreground steering. A `working` report does
  not end its turn.
- Use `done` for completed Quick Review mediation. It reports completion but
  still leaves the next workflow decision to foreground Scufris.

## Implemented

- Reduced worker report events to `working`, `blocked`, and `done`.
- Removed worker `ready` and `needs-decision` parsing, prompts, guidance,
  documentation, and tests.
- Restricted both the Pi report extension and Claude adapter. The private helper
  also rejects worker-submitted `failed` even if an adapter is bypassed.
- Added an internal-only runtime failure write path for unexpected harness exit.
- Made `done` nonterminal and retained steering after completion.
- Updated Quick Review completion events from `ready` to `done`.

## Verification evidence

- `npm run check` passes typechecking, formatting, and 51 TypeScript tests.
- Python unittest discovery passes 21 tests; the focused jobs suite passes 5.
- Ruff, ShellCheck, Prettier, Alejandra, and `git diff --check` pass.
- `nix flake check` passes all supported-system checks.
- Focused integration tests prove a `done` worker remains available, workers
  cannot report `failed` through either adapter or the helper, and an actual
  harness exit after `done` generates trusted `failed` status.
