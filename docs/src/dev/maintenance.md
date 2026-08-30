# Maintain and release

[Previous: Run staging](staging.md)

```text
change -> focused check -> broad check if needed -> docs/change log -> release
```

## Development environment

Enter the reproducible shell and install the locked JavaScript dependencies:

```bash
nix develop
npm ci
```

The shell includes Node.js, Python, Ruff, ShellCheck, Alejandra, mdBook, Git,
and tmux. On Linux it also includes PipeWire for local playback. Run the pinned
`ai-tools-api` app separately when the machine does not already deploy it.

Run working-tree Scufris with system Pi and dedicated resumable sessions:

```bash
npm run dev
```

There is no voice mode of it. This process makes no sound whatever it is
given, so hearing a working tree means running the companion; see below.

`scufris-dev` picks a resumable session directory and hands the rest to
`scripts/scufris-agent`, which strips the repository `node_modules/.bin` from
`PATH` so the system Pi runs, sets the orchestrator role, and passes the
working-tree extensions and skills. The launcher is its own script because the
background service starts an agent with a session directory of its own
choosing; see [staging](staging.md), which points `SCUFRIS_SERVICE_AGENT` at
it.

To run the whole stack from the working tree beside the deployed one, use
[staging](staging.md) rather than assembling the isolation by hand.

## The desktop companion

Build the companion and print what it resolved. This starts no window, so it
is the cheapest proof that the working tree builds and configures:

```bash
cargo run --manifest-path surfaces/desktop/Cargo.toml -- --print-config
```

```text
socket=/run/user/1000/scufris/surface.sock
command_socket=/run/user/1000/scufris/desktop.sock
state_file=/home/you/.local/state/scufris-desktop/pending.json
stt_endpoint=http://127.0.0.1:10300/v1/audio/transcriptions
stt_model=whisper-1
stt_language=auto
popup_key=Super+D
background_key=derived
abort_key=derived
terminal_command=none
restart_command=none
speak_command=none
```

Both halves are clients of the service, so exercising the companion means
running a working-tree service first. Use two terminals, both inside
`nix develop`:

```bash
# 1. the service that owns the conversation
cargo run --manifest-path host/service/Cargo.toml -- \
  --agent "$(nix build --no-link --print-out-paths .#scufris)/bin/scufris"

# 2. the companion
cargo run --manifest-path surfaces/desktop/Cargo.toml
```

`--agent` is optional when a `scufris` is already on `PATH`; the service takes
the first one it finds. It must be a program the service can start in RPC mode
on a session directory of its choosing, so `scripts/scufris-dev`, which picks
its own session directory, is not one. Name the build explicitly while a stale
`scufris` is on `PATH`: one without the `service` extension answers normally and
speaks nothing, and the service says so ten seconds in. See
[Background service](service.md).

Then hold `Super+D`, speak, and let go. The take stops and the words
arrive in a textbox above the pill, where `Enter` sends them to the agent the
service supervises as an ordinary user message and `Escape` throws them away.
Watch the same conversation from a third terminal with
`cargo run --manifest-path host/service/Cargo.toml --bin scufris-ctl -- state`.

Both processes must see the same `XDG_RUNTIME_DIR`, because that is where the
socket is. With no service the companion reports the backend as unavailable
and says so in the tray.

To hear it, give the companion a synthesiser. That is the whole of it: the
service and the agent are not configured for speech and have no setting for
it, because every answer is already one prose paragraph and the speaker is the
companion's.

```bash
SCUFRIS_DESKTOP_SPEAK_COMMAND="$(nix build --no-link --print-out-paths .#scufris-speak)/bin/scufris-speak" \
  cargo run --manifest-path surfaces/desktop/Cargo.toml
```

A companion started without it says so once in its log and stays silent, which
is not a fault. "Mute Scufris" in the tray silences one that has a
synthesiser.

### Speech inference in development

Both desktop inference requests use the shared API on port 10300. Start the
pinned complete runtime when the machine does not already deploy it:

```bash
nix run .#ai-tools-api
```

Confirm transcription independently:

```bash
curl --fail http://127.0.0.1:10300/v1/audio/transcriptions \
  -F file=@recording.wav -F model=whisper-1 \
  -F language=auto -F response_format=json
```

Synthesis returns WAV but does not play it:

```bash
curl --fail http://127.0.0.1:10300/v1/audio/speech \
  -H 'content-type: application/json' \
  -d '{"model":"piper-1","voice":"en_US-lessac-medium","input":"The API works.","response_format":"wav"}' \
  -o /tmp/scufris.wav
pw-play /tmp/scufris.wav
```

Override `SCUFRIS_STT_ENDPOINT` or `SCUFRIS_TTS_ENDPOINT` only for another
compatible deployment. Scufris does not start Whisper or Piper directly.

### Hooks and limits

The tray chat and restart items stay disabled until the deployment supplies
the hooks. Point them at absolute executables to exercise them:

```bash
SCUFRIS_DESKTOP_CHAT_COMMAND=/path/to/open-chat \
SCUFRIS_DESKTOP_RESTART_COMMAND=/path/to/restart-backend \
  cargo run --manifest-path surfaces/desktop/Cargo.toml
```

The companion is Linux and X11 only. Without a display it starts and does no
focus restoration; the pill needs a running X session.

## Checks

Run the cheapest relevant check first: `npm run check` for TypeScript
behavior, the focused `unittest` module for a Python helper change, and
`nix flake check` only when packaging scope warrants it.

The complete local contract:

```bash
npm run check
python3 -m unittest discover -s tests -p 'test_*.py'
ruff check .
ruff format --check .
shellcheck scripts/scufris-agent scripts/scufris-dev scripts/scufris-staging
(cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace)
nix fmt -- --check .
nix flake check -L
git diff --check
```

The control socket tests bind real Unix sockets under `TMPDIR`, and a socket
path is limited to 108 bytes. A deeply nested `TMPDIR`, such as one from a
nested `nix-shell`, fails those tests with `ENOENT` on the socket. Run them
with a short `TMPDIR` such as `/tmp`.

`npm run check` runs strict TypeScript, the Node test suite, and Prettier.
`nix flake check` builds the launcher, resources, Home Manager
configurations, API and closure separation, the speech adapter, and this manual,
and its `helper-tests` derivation runs the Python suite above. Sprout is not
part of the flake, so the two tests that make a Sprout workspace skip where it
is not installed.

Test ownership:

- `tests/*.test.ts`: extension behavior in Node with the Pi APIs stubbed:
  orchestration, response shaping, Calm, identity, and repository structure. `tests/service.test.ts` covers the agent side of protocol v4
  protocol: the hello, what the agent reports, and what it does with a widget
  report.
- `Cargo.toml`: the root Rust workspace. `shared/control/` owns the protocol encoding,
  `scufris-desktop` owns the state machine, the pending transcript store, audio
  conversion, the speaker, and the tray, and `scufris-service` owns the agent,
  the session, and the three protocol v4 sockets. Every port is faked and the service's stand-in
  agent is a `/bin/sh` script, so `cargo test` needs no display, no microphone,
  and no Pi.
- `tests/test_scufris_jobs.py`: the jobs helper and inspection CLI. Lifecycle
  tests create real tmux sessions on the default server, relocated per test
  fixture with `TMUX_TMPDIR` into an isolated server directory, and prove
  unrelated sessions survive.
- `tests/test_quick_review_agent.py`: the strict RPC adapter, pinned npm
  extension invocation, resource isolation, and completion relay.
- `tests/test_scufris_artifacts_prune.py`: sidecar pruning.
- `tests/test_usage_backends.py`: what the subscription backends make of an
  answer. Nothing here reaches the network, and one test names the only three
  fields a window may carry, because the answers behind them carry the account
  as well.
- `tests/test_scufris_staging.py`: what `scufris-staging up` arranges. Both
  binaries are stubs, so what is under test is the script: the environment, the
  seeded root, the lock that refuses a second stack, and a Ctrl+C that stops
  exactly the processes it started. Every path is inside a temporary directory,
  `HOME` and `XDG_RUNTIME_DIR` included, so a test that got isolation wrong
  fails rather than writing into the deployed Scufris.

## Documentation

This manual is an mdBook under `docs/`. Build it with the same output CI
uses:

```bash
nix build .#docs
```

The build evaluates the Home Manager module and generates
`reference/options.md` before mdBook runs; generated Markdown and HTML are
not tracked. Flake source comes from the Git worktree, so stage new files
before a Nix build:

```bash
git add docs
nix build .#docs
```

Durable documentation belongs here. `README.md` stays at the description and
Quickstart. Record every user-facing change in `CHANGELOG.md` under
`Unreleased` as it lands; the release checklist only renames that heading.

## Continuous integration

- `check.yml` runs `npm run check` and `nix flake check -L` on `master`
  pushes and pull requests, and is reused by the release workflow.
- `docs.yml` builds `packages.docs` for documentation-affecting changes and
  deploys `master` builds to GitHub Pages.
- `release.yml` runs on stable `v*` tags: it reuses the check job, verifies
  the tag matches the package version, and publishes a source-only GitHub
  Release.

## Release

Follow the repository
[release checklist](https://github.com/alexjercan/scufris2/blob/master/RELEASE.md)
for preparation, versioning, verification, tagging, and publication. Release
tags are immutable; consumers build from the tagged source flake, and no
binary assets are published.

---

Next: [Environment variables](../reference/environment.md)
