# Replace job polling and add worker reporting tool

- STATUS: IN_PROGRESS
- PRIORITY: 100
- TAGS: workflow

## Scope

Stop periodic delegated-job status polling. Deliver worker events from
filesystem change notifications. Give Pi and Claude workers one bounded report
protocol that updates detailed evidence and status without direct file writes.

## Decisions

- Use `fs.watch` on each owned status file. Read and validate appended events
  only after a filesystem notification, with one initial read to close the
  spawn race.
- Choose a dedicated reporting tool instead of enabling `write` for reviewers.
  The write tool would also permit source mutation and break independent
  read-only review.
- Load an explicit `scufris_report` Pi extension into every Pi worker. A review
  worker enables only read, grep, find, ls, and this report tool.
- Give Claude workers the same protocol through the deterministic
  `scufris-report` adapter named in their private prompt. Claude already has a
  shell tool, so this does not require a second extension protocol.
- Write the detailed report first, fsync it, then append and fsync one validated
  event. Foreground Scufris cannot observe an event before its report evidence.
- Keep a harness wrapper in the owned tmux pane. If a harness exits without a
  terminal report, the wrapper emits a terminal failure through the same
  protocol. This replaces periodic pane-liveness checks.

## Implemented

- Removed the workflow interval and periodic helper calls.
- Added per-job status watchers with event-read coalescing and exact shutdown.
- Added the `scufris_report` worker extension and `scufris-report` Claude
  adapter.
- Replaced direct status-file instructions with the shared reporting protocol.
- Renamed the helper operation from polling to event reading.
- Added focused helper, adapter, intentional-stop, tool identity, package
  structure, and resource checks.
- Documented event-driven status and read-only reviewer reporting.

## Verification evidence

- `npm run check` passes 46 TypeScript tests.
- Python unittest discovery passes 19 tests; the focused job suite passes 4.
- Ruff, Prettier, ShellCheck, Alejandra, and `git diff --check` pass.
- `nix flake check` passes all supported-system checks.
