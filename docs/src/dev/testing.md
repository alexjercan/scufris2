# Test a change

[Previous: Add a widget](widgets.md)

Start with the smallest box that contains the change.

```text
TypeScript extension -> npm run check
Python/Bash helper   -> focused unittest / ShellCheck
Rust protocol/host   -> cargo test for one package
Desktop UI           -> Rust tests, then X11 staging
Nix/module/package   -> focused flake check, then nix flake check
Surface              -> codec tests, service integration, staging
Docs                 -> mdBook build, then nix build .#docs
```

## Platform matrix

| Machine                 | What you can prove                                                                |
| ----------------------- | --------------------------------------------------------------------------------- |
| NixOS + X11             | Complete deployment and desktop behavior                                          |
| Other Linux + X11 + Nix | Same packages through `nix run`; no NixOS required                                |
| Headless Linux + Nix    | Service, protocol, jobs, Rust, Python, and flake checks                           |
| macOS + Nix             | Agent launcher, TypeScript, Python, docs, platform-independent Nix outputs        |
| Linux without Nix       | Node, Python, Rust, shell, and source-level service tests                         |
| macOS without Nix       | Node, Python, shared Rust tests, iOS simulator; not Linux desktop/service runtime |
| iOS simulator/device    | Native surface protocol and UI                                                    |

Home Manager does not require NixOS. NixOS is one deployment host, not a build
requirement.

## With Nix

Enter the pinned toolchain:

```bash
nix develop
npm ci
```

Then choose one lane:

```text
agent/extensions/**/*.ts
  -> npm run check

tools/**/*.py or tests/test_*.py
  -> python3 -m unittest tests.test_NAME

Rust crate
  -> cargo test -p PACKAGE
  -> cargo clippy -p PACKAGE --all-targets -- -D warnings

Nix module or package
  -> nix flake check -L

documentation
  -> nix build .#docs
```

The broad local contract is:

```bash
npm run check
python3 -m unittest discover -s tests -p 'test_*.py'
ruff check .
ruff format --check .
shellcheck scripts/scufris-agent scripts/scufris-dev scripts/scufris-staging
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
nix fmt -- --check .
nix flake check -L
git diff --check
```

Use a short `TMPDIR` for Unix socket tests:

```bash
TMPDIR=/tmp cargo test --workspace
```

Unix socket paths are limited to 108 bytes. A deeply nested temporary path can
fail with `ENOENT` before the tested code runs.

## Test the product on non-NixOS Linux

Nix is enough. Do not install a NixOS module.

```bash
nix run .#staging -- up
```

For separate terminals:

```bash
nix run .#staging -- backend
nix run .#staging -- frontend laptop
```

```text
working-tree packages + isolated state + isolated sockets
                         |
                         +-> same runtime behavior as NixOS units
```

What differs is process supervision: staging owns foreground child PIDs instead
of systemd user units.

## Test headless

The Rust service tests use a shell-script agent. They need no display,
microphone, Pi login, or network:

```bash
cargo test -p scufris-service
```

Smoke-test the packaged binary:

```bash
nix run .#scufris-service -- --help
nix run .#scufris-ctl -- --help
```

## Test the desktop without opening it

```bash
nix run .#scufris-desktop -- --print-config
# or from source
cargo run --manifest-path surfaces/desktop/Cargo.toml -- --print-config
```

Then use X11 staging for real windows, hotkeys, microphone, speech, and widgets:

```bash
RUST_LOG=scufris_desktop=debug nix run .#staging -- up
```

## Without Nix

Install these with the host package manager:

```text
required by lane
  Node.js + npm              TypeScript tests
  Python 3                   helper tests
  Rust + Cargo               host, protocol, desktop tests
  tmux + Git                 full jobs tests
  Bash + ShellCheck          scripts
  Ruff                       Python lint/format
  mdBook                     manual render

Linux desktop runtime also needs the native Tauri/WebKitGTK, GTK, X11,
PipeWire, and system libraries named in nix/desktop.nix.
```

Run source checks directly:

```bash
npm ci
npm run check
python3 -m unittest discover -s tests -p 'test_*.py'
cargo test --workspace
mdbook build docs
```

Without Nix, package closure checks and Home Manager evaluation are not
available. Use CI or a Nix host before release. Some jobs tests skip Sprout when
it is not installed.

## Test iOS

On macOS with Xcode and XcodeGen:

```bash
xcodegen generate --spec surfaces/ios/project.yml
xcodebuild \
  -project surfaces/ios/Scufris.xcodeproj \
  -scheme Scufris \
  -configuration Debug \
  -destination 'generic/platform=iOS Simulator' \
  CODE_SIGNING_ALLOWED=NO \
  clean build
```

Then connect a simulator or physical device to a staging backend as described
in [Add a surface](surfaces.md#test-the-new-surface).

## Test ownership map

```text
tests/*.test.ts                 Pi extension behavior
 tests/test_scufris_jobs.py      durable jobs + real isolated tmux
 tests/test_quick_review_agent.py isolated review adapter
 tests/test_scufris_staging.py   environment + exact PID/route cleanup
 tests/test_today_backend.py     journal backend
 tests/test_usage_backends.py    account usage backends
 shared/control                  framing, types, bounds
 host/service                    agent + three sockets + gateway
 surfaces/desktop               state, windows, audio, widgets, child ownership
 surfaces/ios/Tests             Swift protocol client
 nix/checks                     closures, module, resources, launchers, services
```

## Documentation proof

```bash
nix build .#docs
```

This evaluates the Home Manager module, generates
[Home Manager options](../reference/options.md), and runs mdBook. Nix snapshots
tracked files, so stage a new documentation file before this check:

```bash
git add docs
nix build .#docs
```

---

Next: [Background service](service.md)
