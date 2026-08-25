# Maintenance

## Development environment

Enter the reproducible shell and install the locked JavaScript dependencies:

```bash
nix develop
npm ci
```

The shell includes Node.js, Python, Ruff, ShellCheck, Alejandra, mdBook, Git,
and tmux. On Linux it also includes the private Piper runtime, PipeWire, and
trusted model paths, and it exports the voice development variables.

Run working-tree Scufris with system Pi and dedicated resumable sessions:

```bash
npm run dev
npm run dev:voice   # requires the repository development shell
```

`scufris-dev` strips the repository `node_modules/.bin` from `PATH` so the
system Pi runs, sets the orchestrator role and default project roots, and
passes the working-tree extensions and skills.

## The desktop companion

Build the companion and print what it resolved. This starts no window, so it
is the cheapest proof that the working tree builds and configures:

```bash
cargo run --manifest-path desktop/scufris-desktop/Cargo.toml -- --print-config
```

```text
socket=/run/user/1000/scufris/daemon.sock
state_file=/home/you/.local/state/scufris-desktop/pending.json
stt_endpoint=http://127.0.0.1:10301/inference
hotkey=Super+D
chat_command=none
restart_command=none
```

To exercise the companion against a working-tree backend, run that backend in
the popup's daemon role so it serves the control socket, then start the
companion against the same runtime directory. Use two terminals, both inside
`nix develop`:

```bash
# 1. the backend that answers the pill
SCUFRIS_DAEMON=1 npm run dev

# 2. the companion
cargo run --manifest-path desktop/scufris-desktop/Cargo.toml
```

Then press `Super+D`, speak, and press `Enter`. The words arrive in the Pi
session of terminal 2 as an ordinary user message, with a muted note that they
were spoken through the pill. `Escape` discards the recording, and the
accelerator again opens the transcript for editing instead of sending it.

The daemon role is what opens `$XDG_RUNTIME_DIR/scufris/daemon.sock`; without
it the desktop extension loads and does nothing, and the companion reports the
backend as unavailable. Both processes must see the same `XDG_RUNTIME_DIR`.

### Transcription in development

The companion posts to `SCUFRIS_STT_ENDPOINT`, which defaults to
`http://127.0.0.1:10301/inference`. A whisper server already listening there,
such as the one an existing speech-to-text configuration runs, needs no
override and no second server. Confirm it answers before blaming the pill:

```bash
curl -s -X POST http://127.0.0.1:10301/inference \
  -F file=@recording.wav -F response_format=json
```

```json
{ "text": " the words it heard" }
```

Any whisper-server-compatible endpoint works. Name another one instead of the
default:

```bash
SCUFRIS_STT_ENDPOINT=http://127.0.0.1:10302/inference \
  cargo run --manifest-path desktop/scufris-desktop/Cargo.toml
```

With no server anywhere, start one on its own port:

```bash
curl -L -o /tmp/ggml-base.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
nix shell nixpkgs#whisper-cpp -c whisper-server \
  --model /tmp/ggml-base.bin \
  --host 127.0.0.1 --port 10302 --inference-path /inference --language auto
```

### Hooks and limits

The tray chat and restart items stay disabled until the deployment supplies
the hooks. Point them at absolute executables to exercise them:

```bash
SCUFRIS_DESKTOP_CHAT_COMMAND=/path/to/open-chat \
SCUFRIS_DESKTOP_RESTART_COMMAND=/path/to/restart-backend \
  cargo run --manifest-path desktop/scufris-desktop/Cargo.toml
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
shellcheck scripts/scufris-dev
(cd desktop && cargo clippy --all-targets -- -D warnings && cargo test)
nix fmt -- --check .
nix flake check -L
git diff --check
```

The control socket tests bind real Unix sockets under `TMPDIR`, and a socket
path is limited to 108 bytes. A deeply nested `TMPDIR`, such as one from a
nested `nix-shell`, fails those tests with `ENOENT` on the socket. Run them
with a short `TMPDIR` such as `/tmp`.

`npm run check` runs strict TypeScript, the Node test suite, and Prettier.
`nix flake check` builds the launchers, resources, Home Manager
configurations, closure separation, the real Piper fixture, and this manual.

Test ownership:

- `tests/*.test.ts`: extension behavior in Node with the Pi APIs stubbed:
  orchestration, response shaping, speech,
  Calm, identity, and repository structure. `tests/desktop.test.ts` covers the
  daemon side of the control protocol: socket ownership, submission
  acknowledgment, and the accepted, uncertain, and unsent answers.
- `desktop/`: the companion in Rust. `scufris-control` owns the protocol
  encoding, and `scufris-desktop` owns the state machine, the pending
  transcript store, audio conversion, and the tray. Every port is faked, so
  `cargo test` needs no display, no microphone, and no socket.
- `tests/test_scufris_jobs.py`: the jobs helper and inspection CLI. Lifecycle
  tests create real tmux sessions on the default server, relocated per test
  fixture with `TMUX_TMPDIR` into an isolated server directory, and prove
  unrelated sessions survive.
- `tests/test_quick_review_agent.py`: the strict RPC adapter, pinned npm
  extension invocation, resource isolation, and completion relay.
- `tests/test_scufris_dashboard.py` and
  `tests/test_scufris_artifacts_prune.py`: the dashboard adapter and sidecar
  pruning.

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
