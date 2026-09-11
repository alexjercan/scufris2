# Run staging

[Previous: Operate it](operation.md)

```text
deployed stack: $XDG_RUNTIME_DIR/scufris
staging stack:  $XDG_RUNTIME_DIR/scufris-staging
                 + disposable state, data, sessions, and project
```

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
active frontends different popup keys as in the example. Prefix any command with
`RUST_LOG=debug` to include protocol payloads and connection details; see
[operation](operation.md#logs) for the logging policy.

The flake app builds the service and companion from this source tree, so it
needs no dev shell and no warm Cargo target. Inside a dev shell, the script can
run directly and builds only the binary that command needs:

```bash
scripts/scufris-staging backend
scripts/scufris-staging frontend left
```

Every command stays in the foreground. Ctrl+C stops only the processes and the
exact Tailscale Serve path that command started. There is no `down`: a staging
process that outlives its terminal is one nobody remembers to stop.

A second backend or a second frontend with the same name exits 3. A frontend
also exits 3 when no staging backend is running.

## The staging environment

`up` prints this block before it starts either process.

| Variable                            | Value                                                  |
| ----------------------------------- | ------------------------------------------------------ |
| `SCUFRIS_STAGING_ROOT`              | `/tmp/scufris-staging`                                 |
| `XDG_STATE_HOME`                    | `$SCUFRIS_STAGING_ROOT/state`                          |
| `XDG_DATA_HOME`                     | `$SCUFRIS_STAGING_ROOT/data`                           |
| `SCUFRIS_RUNTIME_DIR`               | `$XDG_RUNTIME_DIR/scufris-staging`                     |
| `PI_CODING_AGENT_DIR`               | `$SCUFRIS_STAGING_ROOT/pi-agent`                       |
| `SCUFRIS_PROJECT_ROOTS`             | `["$SCUFRIS_STAGING_ROOT/projects"]`                   |
| `SCUFRIS_SERVICE_AGENT`             | `scripts/scufris-agent`                                |
| `SCUFRIS_SERVICE_CONVERSATION_FILE` | `$SCUFRIS_STAGING_ROOT/data/scufris/conversation.json` |
| `SCUFRIS_DESKTOP_HOTKEY`            | `Super+G`                                              |
| `SCUFRIS_STT_ENDPOINT`              | `http://127.0.0.1:10300/v1/audio/transcriptions`       |
| `SCUFRIS_TTS_ENDPOINT`              | `http://127.0.0.1:10300/v1/audio/speech`               |
| `SCUFRIS_STAGING_AI_TOOLS_API`      | `external`                                             |
| `SCUFRIS_STAGING_GATEWAY_PORT`      | `10441`                                                |
| `SCUFRIS_STAGING_EXTERNAL_SURFACES` | `auto`                                                 |

A named frontend instead uses
`$SCUFRIS_STAGING_ROOT/frontends/NAME/{state,data}` and
`$SCUFRIS_RUNTIME_DIR/desktop-NAME.sock`. `SCUFRIS_DESKTOP_SPEAK_COMMAND` is
in frontend output too; see [speech](#speech) below for where its value comes
from.

The root is disposable and a reboot wipes it. `SCUFRIS_STAGING_ROOT`,
`SCUFRIS_DESKTOP_HOTKEY`, both inference endpoints, and
`SCUFRIS_STAGING_AI_TOOLS_API`, `SCUFRIS_STAGING_GATEWAY_PORT`, and
`SCUFRIS_STAGING_EXTERNAL_SURFACES` are taste, so a value already in the
environment wins; the rest is isolation and the script owns it.

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

- The deployed `ai-tools-api` on port 10300 by default. One API owns Whisper,
  Piper, and their models for every frontend.
- The tmux server. Job sessions are namespaced by job ID, so the two stacks
  do not collide, and `scufris-jobs` in a staging environment reads the
  staging `XDG_STATE_HOME`.
- The Pi login, through the `auth.json` symlink.
- `~/.claude` and `~/.codex`, which the usage widgets read.

## Speech

Staging speaks through the same API contract as a deployment.
`nix run .#staging -- up` gives each companion the packaged `scufris-speak`
HTTP/playback helper. `SCUFRIS_DESKTOP_SPEAK_COMMAND` overrides it;
`SCUFRIS_STAGING_SPEAK` is the packaged default.

With none of the three the companion stays silent, and `up` says so on start
rather than leaving a missing voice to look like a broken one. A command that
is named but cannot be run is refused with exit 2, for the same reason: a
synthesiser the companion logs once and gives up on is indistinguishable from
no synthesiser at all.

Every frontend runs its own HTTP/playback helper, but all use one API. By
default staging consumes the deployed API. To make `backend` or `up` own the
pinned complete runtime instead:

```bash
SCUFRIS_STAGING_AI_TOOLS_API=managed nix run .#staging -- backend
```

That foreground command records and stops the API process it starts. Never set
managed mode on two backends. "Mute Scufris" in a staging companion's tray
silences only that frontend.

## External surfaces

`up` and `backend` start `scufris-surface-gateway` on loopback port 10441 with
a stable private token at `$SCUFRIS_STAGING_ROOT/surface-token`. They print both
the loopback URL and token path. The token is generated once with mode 0600 and
is never printed.

The staging gateway also enables an embedded Swagger UI at
`http://127.0.0.1:10441/docs/`. Through Tailscale Serve it is available below
`/scufris-staging/docs/`. The page and its OpenAPI document are deliberately
unauthenticated in staging so a browser can load them. Use Swagger's
**Authorize** control to supply the bearer token before calling any functional
route. Production does not enable either documentation route.

The default `auto` mode asks Tailscale Serve to publish the gateway at the
`/scufris-staging` path when Tailscale is available. It does not replace the
deployed `/` route. The resulting iOS settings are the displayed tailnet URL
with `/scufris-staging` and the token read from the printed path. On Ctrl+C or a
child exit, staging removes only that path.

Use `local` to prohibit the Tailscale change, or `tailscale` to make an
unavailable Serve route a startup error:

```bash
SCUFRIS_STAGING_EXTERNAL_SURFACES=local nix run .#staging -- up
SCUFRIS_STAGING_EXTERNAL_SURFACES=tailscale nix run .#staging -- up
```

## Reaching the staging stack

`scufris-ctl` resolves its socket through the same variable, so one export
points a terminal at staging instead of the deployed service:

```bash
export SCUFRIS_RUNTIME_DIR="$XDG_RUNTIME_DIR/scufris-staging"
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

## Hold the conversation from a terminal

A terminal can take the agent from the staging service and give it back. The
service offers the lease only when it is started with
`SCUFRIS_SERVICE_TERMINAL_LEASE=1`; without that every request is refused. Do
this against staging first: the lease stops the agent of whichever service it
reaches.

Start a backend that offers the lease, and a companion to watch:

```bash
SCUFRIS_SERVICE_TERMINAL_LEASE=1 nix run .#staging -- backend
nix run .#staging -- frontend left
```

Then, in a third terminal, from the repository root:

```bash
export SCUFRIS_RUNTIME_DIR="$XDG_RUNTIME_DIR/scufris-staging"
export SCUFRIS_PROJECT_ROOTS='["/tmp/scufris-staging/projects"]'
export SCUFRIS_TERMINAL_LOG=/tmp/scufris-staging/terminal.jsonl
nix run .#scufris-terminal
```

`scufris-terminal` asks the service where its sessions are and which file the
next holder continues from, then starts Pi on a fork of that file in this
directory. The fork is what gives the model the context the phone built up.
Plain `pi` with `SCUFRIS_TERMINAL=1` also works here and joins by catch-up
instead, on a session of its own.

Pi asks to trust the checkout on its first interactive start; a print-mode run
needs `--approve`, because `-p` skips an untrusted project's `.pi` without a
word.

On start the extension takes the lease, the backend logs that the agent was
stopped for a terminal, and the companion shows `starting` until this Pi has
joined, then `idle` with `terminal` as the holder. `/scufris status` prints the
same from here.

What to try, and what `SCUFRIS_TERMINAL_LOG` records for each:

| Case               | Do                                                                                                     | Expect                                                                                                                           |
| ------------------ | ------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------- |
| Typed turn         | Type a question.                                                                                       | `turn` with an id; the words appear in the companion as a user message; the answer appears under the same turn.                  |
| Surface message    | Send from the companion.                                                                               | `surface_message`; the prompt shows in this terminal; the companion shows its own message and the answer.                        |
| Commands           | `/compact`, `/model`, `/skill:den`.                                                                    | Built-in commands never reach `input`; a skill command reaches it with `command: true` and is recorded like any typed line.      |
| Compaction         | `/compact`, or fill the context.                                                                       | `session_before_compact` and `session_compact` with the reason; the HUD is unchanged.                                            |
| Steer or follow-up | Send from the companion while a turn runs.                                                             | `surface_message` with `busy: true`; the words land as a steer.                                                                  |
| Session switch     | `/new` or `/resume`.                                                                                   | `session_before_switch`, then `session_start` with the new file; the lease is kept and the new session is declared.              |
| Proactive wake     | `SCUFRIS_RUNTIME_DIR=$SCUFRIS_RUNTIME_DIR nix run .#scufris-ctl -- wake "Now."`                        | `wake`; the follow-up runs in this terminal; the answer is recorded as `unprompted`.                                             |
| Attachments        | Ask for a file to be stored.                                                                           | `store_attachment` works through the staging `content.sock`; a typed turn cannot carry one.                                      |
| Widgets            | Ask for a widget in the answer.                                                                        | Dropped from a terminal-owned answer; kept on a companion-owned one.                                                             |
| Lineage            | Say a fact from the companion, attach, ask for it here; say a second, release, ask from the companion. | The model repeats both. A plain `pi` joined by catch-up repeats them from the catch-up text instead.                             |
| Jobs               | Delegate from the companion, attach, then delegate from here and release.                              | Each job keeps its owner across the handoff; the holder that has the agent watches it and can cancel it; `orphans` stays empty.  |
| Crash              | `kill -9` this Pi.                                                                                     | The backend ends the lease with `disconnected` and starts its agent again from this terminal's session; no release was recorded. |
| Stopped terminal   | `kill -STOP` this Pi, wait 20 s, `kill -CONT`.                                                         | The backend ends the lease after three missed pings and starts its agent; the resumed Pi records `lease_lost` and reacquires.    |
| Speech             | Set `speakTerminal`, then type a question.                                                             | The companion reads the answer out; with it off the companion stays silent and only shows the words.                             |
| Clean exit         | `/quit` or Ctrl-D.                                                                                     | `session_shutdown`, `lease_released`; the backend starts its agent again with `--fork` on this terminal's session.               |
| Backend gone       | Stop the backend while leased.                                                                         | `lease_lost`; this Pi keeps running and retries, backing off from one second to thirty.                                          |
| Second terminal    | Start another `scufris-terminal`.                                                                      | `lease_refused` with `lease_held`; it runs as a plain terminal and keeps retrying.                                               |

Jobs delegated from this terminal are owned by the conversation, not by this
Pi's session, so they survive the handoff in both directions. `scufris-jobs
orphans` names the jobs of some other owner.

## What it does not touch

`$XDG_RUNTIME_DIR/scufris`, `~/.local/state/scufris`,
`~/.local/share/scufris`, and the deployed `~/.pi/agent` are never written by
a staging run. The optional Tailscale integration owns only
`/scufris-staging`, never the deployed root route. `tests/test_scufris_staging.py`
asserts process and route cleanup: it runs `up` with
`HOME` and `XDG_RUNTIME_DIR` inside a temporary directory and fails if any of
those appear.

---

Next: [Maintain and release](maintenance.md)
