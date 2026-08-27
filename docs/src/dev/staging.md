# Staging

One command runs the working tree's Scufris beside the deployed one. The
deployed stack keeps its sockets, its jobs, its sessions, and `Super+D`;
staging gets its own of each and answers `Super+G`.

```bash
nix run .#staging -- up
```

The flake app builds the service and the companion from this source tree, so
it needs no dev shell and no warm `cargo` target directory. From inside a dev
shell the script runs directly and compiles what it needs:

```bash
scripts/scufris-staging up
```

Both stay in the foreground and stream the two processes. Ctrl+C is the
teardown, the way `docker compose up` works, and it stops exactly the two
processes the run started. There is no `down`: a staging stack that outlives
its terminal is one nobody remembers to stop.

A second `up` while one runs exits 3 without starting anything.

## The staging environment

`up` prints this block before it starts either process.

| Variable                 | Value                                |
| ------------------------ | ------------------------------------ |
| `SCUFRIS_STAGING_ROOT`   | `/tmp/scufris-staging`               |
| `XDG_STATE_HOME`         | `$SCUFRIS_STAGING_ROOT/state`        |
| `XDG_DATA_HOME`          | `$SCUFRIS_STAGING_ROOT/data`         |
| `SCUFRIS_RUNTIME_DIR`    | `$XDG_RUNTIME_DIR/scufris-staging`   |
| `PI_CODING_AGENT_DIR`    | `$SCUFRIS_STAGING_ROOT/pi-agent`     |
| `SCUFRIS_PROJECT_ROOTS`  | `["$SCUFRIS_STAGING_ROOT/projects"]` |
| `SCUFRIS_SERVICE_AGENT`  | `scripts/scufris-agent`              |
| `SCUFRIS_DESKTOP_HOTKEY` | `Super+G`                            |
| `SCUFRIS_STT_ENDPOINT`   | `http://127.0.0.1:10301/inference`   |

The root is disposable and a reboot wipes it. `SCUFRIS_STAGING_ROOT`,
`SCUFRIS_DESKTOP_HOTKEY`, and `SCUFRIS_STT_ENDPOINT` are taste, so a value
already in the environment wins; the rest is isolation and the script owns it.

`XDG_RUNTIME_DIR` itself is not overridden. PipeWire and the session bus live
in it, and a socket path is capped at 108 bytes, which a runtime directory
under `/tmp/scufris-staging` would start eating into.

The agent is `scripts/scufris-agent`, the same launcher `npm run dev` runs. It
starts the system Pi with the extensions and skills from this checkout rather
than from the packaged resources, so a staging conversation exercises working-
tree agent code as well as working-tree Rust.

A fresh root is seeded once: one toy git repository under `projects/`, so the
menu is not empty, and a `pi-agent/` with `auth.json` symlinked to the
deployed login and `settings.json` copied from it. The symlink means a token
refreshed on either side is refreshed for both. The copy means staging can be
pointed at a different model without editing the deployed file.

## What is shared on purpose

- The transcription server on port 10301. One whisper server, one model in
  memory, and a transcription carries no session state.
- The tmux server. Job sessions are namespaced by job ID, so the two stacks
  do not collide, and `scufris-jobs` in a staging environment reads the
  staging `XDG_STATE_HOME`.
- The Pi login, through the `auth.json` symlink.
- `~/.claude` and `~/.codex`, which the usage widgets read.

Speech is not shared and not configured. Staging sets no
`SCUFRIS_DESKTOP_SPEAK_COMMAND`, so the staging companion stays silent. Give
it one to hear it:

```bash
SCUFRIS_DESKTOP_SPEAK_COMMAND="$(nix build --no-link --print-out-paths .#scufris-speak)/bin/scufris-speak" \
  nix run .#staging -- up
```

## Reaching the staging stack

`scufris-ctl` resolves its socket through the same variable, so one export
points a terminal at staging instead of the deployed service:

```bash
export SCUFRIS_RUNTIME_DIR="$XDG_RUNTIME_DIR/scufris-staging"
scufris-ctl state
scufris-ctl send what is in this project
scufris-ctl watch
```

Without that variable the same commands reach the deployed service. That is
the one thing to keep straight while both are running.

## What it does not touch

`$XDG_RUNTIME_DIR/scufris`, `~/.local/state/scufris`,
`~/.local/share/scufris`, and the deployed `~/.pi/agent` are never written by
a staging run. `tests/test_scufris_staging.py` asserts it: it runs `up` with
`HOME` and `XDG_RUNTIME_DIR` inside a temporary directory and fails if any of
those appear.
