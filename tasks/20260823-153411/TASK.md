# Simplify delegated worker statuses

- STATUS: OPEN
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
