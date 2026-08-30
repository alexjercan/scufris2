# Staging

The working tree can run beside the deployed Scufris as one stack or as one
backend with several local frontends. The deployed stack keeps its sockets,
jobs, sessions, and `Super+D`.

The one-terminal form remains:

```bash
nix run .#staging -- up
```

For multi-surface testing, keep these commands in separate terminals:

```bash
nix run .#staging -- backend
nix run .#staging -- frontend left
SCUFRIS_DESKTOP_HOTKEY=Super+H nix run .#staging -- frontend right
```

`backend` runs the service and Pi agent. Each `frontend NAME` runs only one
companion against that backend. The name is stable: it selects a private state
directory, persistent surface identity, data directory, command socket, and
frontend lock. Different names therefore register as different surfaces even
on one machine, and the name appears in backend INFO logs. Give simultaneously
active frontends different hotkeys as in the example. Prefix any command with
`RUST_LOG=debug` to include protocol payloads and connection details; see
[operation](operation.md#logs) for the logging policy.

The flake app builds the service and companion from this source tree, so it
needs no dev shell and no warm Cargo target. Inside a dev shell, the script can
run directly and builds only the binary that command needs:

```bash
scripts/scufris-staging backend
scripts/scufris-staging frontend left
```

Every command stays in the foreground. Ctrl+C stops only the processes that
command started. There is no `down`: a staging process that outlives its
terminal is one nobody remembers to stop.

A second backend or a second frontend with the same name exits 3. A frontend
also exits 3 when no staging backend is running.

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

A named frontend instead uses
`$SCUFRIS_STAGING_ROOT/frontends/NAME/{state,data}` and
`$SCUFRIS_RUNTIME_DIR/desktop-NAME.sock`. `SCUFRIS_DESKTOP_SPEAK_COMMAND` is
in frontend output too; see [speech](#speech) below for where its value comes
from.

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

## Speech

Staging speaks. `nix run .#staging -- up` gives the companion the packaged
`scufris-speak`, the same synthesiser a deployment runs, which binds Piper, the
model, and the configuration itself. The voice you hear from staging is the
voice the deployment would have.

The script looks in three places, in the order of who knows most about the
machine:

1. `SCUFRIS_DESKTOP_SPEAK_COMMAND`, which is a person saying so.
2. `SCUFRIS_STAGING_SPEAK`, which the flake wrapper sets to the packaged
   `scufris-speak`.
3. `tools/voice/scufris-speak` from this checkout, used only where
   `SCUFRIS_PIPER_MODEL` and `SCUFRIS_PIPER_CONFIG` are bound. That is what a
   dev shell does, so `scripts/scufris-staging up` inside `nix develop` speaks
   with the working tree's helper.

With none of the three the companion stays silent, and `up` says so on start
rather than leaving a missing voice to look like a broken one. A command that
is named but cannot be run is refused with exit 2, for the same reason: a
synthesiser the companion logs once and gives up on is indistinguishable from
no synthesiser at all.

Every frontend runs its own synthesiser process. Nothing is shared here but
the pinned voice, and two Scufrises talking at once is the ordinary cost of
running two. "Mute Scufris" in a staging companion's tray silences that one.

## Reaching the staging stack

`scufris-ctl` resolves its socket through the same variable, so one export
points a terminal at staging instead of the deployed service:

```bash
export SCUFRIS_RUNTIME_DIR="$XDG_RUNTIME_DIR/scufris-staging"
scufris-ctl state
scufris-ctl state
journalctl --user -u scufris-service.service -f
```

Without that variable the same commands reach the deployed service. Local
window commands can target one named frontend through its command socket:

```bash
SCUFRIS_DESKTOP_COMMAND_SOCKET="$XDG_RUNTIME_DIR/scufris-staging/desktop-left.sock" \
  scufris-ctl show
```

These are the variables to keep explicit while several stacks are running.

## What it does not touch

`$XDG_RUNTIME_DIR/scufris`, `~/.local/state/scufris`,
`~/.local/share/scufris`, and the deployed `~/.pi/agent` are never written by
a staging run. `tests/test_scufris_staging.py` asserts it: it runs `up` with
`HOME` and `XDG_RUNTIME_DIR` inside a temporary directory and fails if any of
those appear.
