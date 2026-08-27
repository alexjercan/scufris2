# Build scufris-staging: one-command parallel staging stack

- STATUS: OPEN
- PRIORITY: 80
- TAGS: staging, deploy

## Goal

One command runs the real Scufris stack from the working tree, in parallel
with the deployed Scufris, without touching its state, sockets, or keys.

Plan and research: https://claude.ai/code/artifact/317bc25b-694e-4e03-85ed-ce65c1e145c6
(Scufris Staging Lane, 2026-08-27). This task is Phase 0 (scufris2 part) and
Phase 1 of that plan.

## Contract

- `scufris-staging up` prepares a disposable staging root, verifies the
  environment, starts `scufris-service` and `scufris-desktop` built from the
  working tree, and stays in the foreground streaming their output.
- Ctrl+C (SIGINT, and SIGTERM) is the teardown, like `docker compose up`.
  It stops both processes by recorded PID and exits with their status.
  There is no separate `down` subcommand.
- A second `up` while one runs must fail fast with a clear message.
- The deployed Scufris must keep `Super+D`, `Mod4+s`, its sockets, jobs,
  and sessions. Staging uses `Super+G` and its own directories.

## Staging environment

Set by `up` for both processes (and print it on start):

```
STAGING=/tmp/scufris-staging              # disposable; reboot wipes it
XDG_STATE_HOME=$STAGING/state             # jobs, jobs.lock, dev sessions, pending.json
XDG_DATA_HOME=$STAGING/data               # service sessions, webview data
SCUFRIS_RUNTIME_DIR=$XDG_RUNTIME_DIR/scufris-staging   # both sockets; new knob, step 1
SCUFRIS_PROJECT_ROOTS=["$STAGING/projects"]
SCUFRIS_DESKTOP_HOTKEY=Super+G            # cancel/stop keys derive from it
SCUFRIS_STT_ENDPOINT=http://127.0.0.1:10301/inference  # shared on purpose
PI_CODING_AGENT_DIR=$STAGING/pi-agent     # seed: symlink auth.json, copy settings.json
```

Do not override `XDG_RUNTIME_DIR` itself: PipeWire and the session bus live
there. Keep socket paths short (108-byte sockaddr limit).

Shared with production on purpose: whisper-server on 10301, Piper, the tmux
server (sessions are namespaced), `~/.claude`, `~/.codex`.

## Steps

1. **Add `SCUFRIS_RUNTIME_DIR` to `scufris-control`.** The socket directory
   resolution (`native/scufris-control/src/lib.rs`, around line 50) currently
   uses only `$XDG_RUNTIME_DIR/scufris`. Read `SCUFRIS_RUNTIME_DIR` first,
   fall back to the current behavior. This one choke point moves
   `service.sock` and `desktop.sock` for the service, the companion, and
   `scufris-ctl` together (`bin/scufris-ctl.rs` has no override today).
   Extend the `--print-config` golden check (`nix/checks/desktop.nix`) and
   the Rust socket tests to cover the override.
2. **Write `scripts/scufris-staging`.** Bash, following the repo helper
   conventions (quote expansions, command arrays, preserve exit codes).
   `up` does, in order: create `$STAGING` dirs and the runtime dir (0700);
   seed `$STAGING/projects` with one toy git repo if empty; seed
   `$STAGING/pi-agent` if empty; run `scufris-desktop --print-config` with
   the staging env and fail on error; start `scufris-service` (with
   `--agent` pointing at the working-tree launcher) and `scufris-desktop`
   as children; record both PIDs; trap INT/TERM and stop exactly those
   PIDs; wait. Reuse the isolation recipe already proven by
   `scripts/scufris-dev` and `tests/test_scufris_jobs.py` (fixture around
   line 127).
3. **Surface it in the flake.** `apps.staging` (`nix run .#staging`) wraps
   the script with working-tree builds of the service and companion. Keep
   the script runnable directly from a dev shell too.
4. **Test.** A focused integration test that runs `up` in a scratch
   `XDG_RUNTIME_DIR`, waits for `service.sock`, sends SIGINT, and asserts
   both processes are gone and production paths were never created. Python
   stdlib, next to the existing jobs tests.
5. **Document.** A short page under `docs/src/dev/` (staging loop: command,
   env table, what is shared, how to reach it with
   `SCUFRIS_RUNTIME_DIR=... scufris-ctl ...`). Update the env table in
   `installation.md` if `SCUFRIS_RUNTIME_DIR` belongs there.

## Out of scope (tracked elsewhere)

- `desktop.sock` liveness steal: tasks/20260827-205350 (m3). Parallel
  instances raise its priority; do not duplicate the fix here.
- nix.dotfiles work: the stale `checks.nix` revision assert, the module
  migration to the service interface, `--override-input` release gate,
  rollback pin. Phases 2-3 of the plan artifact; different repo.
- `Mod4+Shift+s` staging popup keybind, `scufris-staging run -- <cmd>`
  passthrough: nice follow-ups, not part of this task.

## Verification

Record under this task directory:

- Transcript of `up`, an interaction through `Super+G`, and a Ctrl+C
  teardown, with `ls` of `$STAGING` and the runtime dir before and after.
- Evidence production was untouched: `systemctl --user status` of the real
  units and mtimes of `~/.local/state/scufris` unchanged across the run.
- `npm run check` and `nix flake check` output.
