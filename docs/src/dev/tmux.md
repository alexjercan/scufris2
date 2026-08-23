# Tmux

Every worker execution runs in an owned tmux session. Workers share the
default tmux server with the foreground session; there is no private server
and no ambient socket selection. The helper's tmux wrapper rejects
`kill-server` and the `-S` and `-L` socket selectors outright.

## Session identity

Execution sessions are named
`scufris-<job-id>-g<generation>-<token-prefix>`, where the token prefix is
the first 16 hex characters of a random per-execution token. The name pattern
is the worker namespace: only names matching it may ever be killed.

Creation is crash-consistent and happens in two recorded steps:

1. `prepare_execution` stores the durable intent first: session name, random
   execution token, generation, and fresh launch authority.
2. `start_execution` creates the session detached with a placeholder
   `exec sleep 2147483647` command and, in the same tmux command queue, sets
   `@scufris_job_id`, `@scufris_execution_token`, `@scufris_generation`, and
   `@scufris_phase=creating` options and window `remain-on-exit`. The
   returned session, window, and pane IDs are stored, then the pane is
   respawned with the real launch command
   (`scufris-jobs launch <job> <capability>`) and the phase moves to
   `running`.

Because intent precedes server mutation, recovery can complete either side of
a crash: finish a server-created session that never respawned, restart a
generation whose session was never created, or adopt recorded IDs.

## Ownership validation

Every state read goes through one `display-message` snapshot that returns the
session name, session, window, and pane IDs, the three Scufris options, and
pane liveness. The snapshot must match the durable record exactly; a mismatch
is an error, and an absent server or session is a clean "no execution".

## Exact termination

Termination never uses check-then-kill. It is one server-side conditional:

```text
tmux if-shell -F -t =<name>: <condition> "kill-session -t =<name>" ...
```

The condition is a formatted conjunction of session name, session ID, window
ID, pane ID, job ID, execution token, and generation. Only its true branch
kills, so the decision and the kill are serialized inside the tmux server and
unrelated sessions on the shared server always survive. The false branch
prints a marker that the helper converts into an ownership error. The name is
checked against the worker namespace before the command is built.

## Pane lifecycle

The launch command inside the pane consumes the one-use launch capability,
writes the worker's report capability into its environment, and runs the
harness in the job's working directory. `remain-on-exit` keeps the pane (and
its scrollback) visible after the harness exits. When the harness exits
without a terminal event, the wrapper publishes a trusted `failed` report for
that generation.

A terminal event stops the execution session promptly from the `events` read
path; steering kills the old execution the same exact way before starting the
next generation. Foreground shutdown exact-stops every owned execution and
marks nonterminal jobs suspended.

Humans may attach to a worker session read-only to watch. Do not type into
worker panes and never kill the shared server; use `scufris_job_stop` or
steering instead. Foreground Scufris itself is barred from `sleep` and `wait`
commands, so it never blocks on tmux state.
