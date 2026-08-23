# Remove foreground sleep and waiting

- STATUS: OPEN
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
