# Remove foreground sleep and waiting

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: workflow

## Goal

Guarantee that foreground Scufris never blocks while waiting for delegated
work. Filesystem notifications already provide the continuation mechanism, so
foreground code and model guidance must never use sleep, periodic waiting, or
wait loops.

## Scope

- Remove every sleep-style wait from workflow orchestration, including shutdown
  loops that repeatedly delay while an event read finishes.
- Make shutdown cancel or detach in-flight event reads safely instead of waiting
  for them.
- Add explicit foreground guidance: after spawning or steering a worker, finish
  the turn. Never call shell `sleep`, poll a job, or wait for worker progress.
- Keep filesystem watchers and event reads asynchronous. A worker event starts a
  later foreground turn when its wake policy requires one.
- Audit helper and extension paths to ensure spawning, event notification, stop,
  and shutdown return without foreground waiting unrelated to the immediate
  operation.

## Acceptance

- Workflow orchestration contains no interval, sleep, or delayed wait loop for
  delegated status.
- Foreground prompts prohibit waiting for delegated jobs.
- Starting a worker returns immediately and leaves Scufris available to the
  user.
- Cancelling or shutting down does not wait on status activity.
- Focused tests demonstrate notification-driven continuation without timing
  sleeps.

## Dependencies

None. This is the first task.

## Decisions

- Return terminating tool results from worker spawn and steering. These are
  asynchronous handoff boundaries, so another automatic model turn would only
  encourage foreground waiting.
- Enforce the no-wait rule at the Bash tool boundary as well as in prompts.
  Foreground model calls that directly invoke shell `sleep` or `wait` are
  blocked and terminated. User shell commands are unaffected.
- Cancel an in-flight status read with `AbortController` during shutdown. Do not
  delay shutdown until the read promise settles.
- Keep helper operation deadlines. They bound immediate operations and are not
  delegated-progress polling or sleep loops.

## Implemented

- Added terminating spawn and steering results.
- Added system, tool, and workflow-skill guidance to end the foreground turn
  immediately after handoff.
- Added a foreground Bash gate for direct `sleep` and `wait` commands.
- Replaced the shutdown delay loop with immediate watcher closure and status
  read cancellation.
- Documented notification-driven continuation and added focused guard tests.

## Verification evidence

- `npm run check` passes typechecking, formatting, and 50 TypeScript tests.
- Python unittest discovery passes 20 tests.
- Ruff, ShellCheck, Prettier, Alejandra, and `git diff --check` pass.
- `nix flake check` passes all supported-system checks.
- Focused tests verify direct shell waits are rejected, ordinary searches are
  unaffected, status uses `fs.watch`, shutdown aborts event reads, and no timer
  or delayed event-read loop remains in orchestration.
