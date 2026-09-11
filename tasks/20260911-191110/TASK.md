# Fix the terminal lease variable and the terminal launcher PATH

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: bug, nix, service

## Purpose

`scufris-terminal` in a checkout fails twice: the service is not there to hand
the agent over, and the briefing helper cannot import `markdown_it`.

## Evidence

- `systemctl --user status scufris-service`: `activating auto-restart`, restart
  counter over 40, with `error: invalid value '1' for '--terminal-lease'
[possible values: true, false]`. The unit written by the Home Manager module
  sets `SCUFRIS_SERVICE_TERMINAL_LEASE=1`, which is the documented contract in
  `docs/src/reference/environment.md` and `docs/src/dev/operation.md`, and
  clap's parser for a `bool` field takes only `true` and `false`. The service
  has never started since `terminalLease` was turned on, so
  `/run/user/1000/scufris/control.sock` does not exist and every lease request
  fails with `ENOENT`.
- `nix/terminal.nix` gives the terminal launcher `scufris-ctl` and `pi` only.
  The checkout composition it starts loads the workflow and briefing
  extensions and the den skill, which run `tools/briefing/cli.py`,
  `tools/jobs/scufris-jobs`, `tmux`, and `scufris-den`. The ambient `python3`
  has no markdown-it-py, so the briefing fails at its first import, exactly
  the failure `nix/python.nix` describes for a path that forgets it.

## Scope

- Accept the deployment's boolean spelling for the terminal lease variable,
  with the flag unchanged, and test it through the real binary.
- Give both launchers the same agent runtime programs from one declaration,
  and assert that they carry the same set.
- Changelog entries. No `nix.dotfiles` edit and no Home Manager switch.

## Investigation

- `journalctl --user -u scufris-service`: 43 restarts, each `error: invalid
value '1' for '--terminal-lease' [possible values: true, false]`, exit 2.
  `Options.terminal_lease` is a clap `bool`, whose value parser takes `true`
  and `false` only, and clap runs that parser over the environment value too.
  The unit sets `1`, which `nix/checks/service.nix` asserts and
  `docs/src/reference/environment.md` documents, so the deployment and the
  binary disagreed and no check ran the binary with the variable set.
- `/run/user/1000/scufris` held `desktop.sock` only. Every `control.lease_acquire`
  therefore failed with `ENOENT`, which is the "agent was not handed over"
  notice the terminal extension prints.
- The second failure is unrelated to the lease. `.pi/extensions/scufris-terminal`
  composes the workflow and briefing extensions and the den skill, which run
  `tools/briefing/cli.py` and `tools/jobs/scufris-jobs` by path. Both carry a
  `#!/usr/bin/env python3` shebang, and `nix/terminal.nix` gave the launcher
  `scufris-ctl` and `pi` only, so `page.py` imported `markdown_it` from the
  ambient interpreter and did not find it.

## Implementation

- `host/service/src/main.rs` names `BoolishValueParser` for `terminal_lease`,
  with `ArgAction::SetTrue` unchanged, so `1`, `true`, `yes`, `on` and their
  negatives are all read and the bare flag still means true.
- `nix/agent-runtime.nix` is the one declaration of the programs the agent
  runs: the interpreter from `nix/python.nix`, tmux, the journal, and the
  briefing. `nix/launcher.nix` and `nix/terminal.nix` both take it, and
  `nix/home-manager.nix` passes the packages it already resolved to the
  terminal it installs.
- `host/service/tests/lease.rs` starts the harness by flag or by variable, and
  a new test takes the lease from a service started the way the unit starts it.
- `nix/checks/launcher.nix` gains `launcher-runtime`, which asserts both
  wrappers carry every program in that list.

## Verification

- `cargo test -p scufris-service --test lease`: 4 passed. With the parser
  change reverted, `the_variable_the_deployment_sets_offers_the_lease` fails on
  the missing control socket; with `nix/terminal.nix` reverted to `[ctl
piPackage]`, `launcher-runtime` fails. Both are the reported failures.
- `cargo test -p scufris-service`: 107 passed across the four suites.
- Under the built terminal launcher's PATH, `tools/briefing/cli.py --help` from
  this checkout prints its usage instead of raising `ModuleNotFoundError`.
- `nix develop -c npm run check`: TypeScript, 157 tests, and Prettier passed.
- `nix flake check`: all checks passed on x86_64 Linux. The first run caught
  `nix/home-manager.nix` building its own terminal without the new arguments.
- `cargo fmt --all --check`, `alejandra`, and `git diff --check`: clean.
- Not landed and not deployed. `personal/nix.dotfiles` pins
  `github:alexjercan/scufris2/v2.8.0`, so the running service keeps failing
  until a release carries this.

## Release and deployment

- Released as `v2.8.1`: version commit `e1b3b4b`, annotated tag on it, `master`
  pushed first and the tag second. `SERVICE_VERSION` stayed at 11, so the
  TestFlight step does not apply and the phone is unaffected.
- Pre-release checks on the version commit: `npm run check`, 411 Python helper
  tests, `ruff check`, `ruff format --check`, `shellcheck`, `cargo clippy -D
warnings`, `cargo test` (476 across seven suites), `nix fmt --check`, `nix
flake check -L`, and `git diff --check`. All passed.
- CI on the push: `release`, `check`, `Documentation`, and `iOS` all green. The
  release job published a source-only GitHub Release with generated notes and
  no assets.
- Deployed: `personal/nix.dotfiles` now pins `v2.8.1` (commit `af918ff`, not
  pushed), built `.#homeConfigurations.alex.activationPackage`, and
  `home-manager switch --flake .#alex`.
- After the switch: `scufris-service` runs
  `scufris-service-2.8.1`, is `active` with `scufris-desktop` and
  `scufris-surface-gateway`, and binds all five sockets including
  `control.sock`. `scufris-ctl state` answers `idle`, `holder: managed`, with a
  lineage file. The journal shows the agent connected and briefing ingress
  stored, and no restart loop.
- The reported failure is gone end to end: under the deployed
  `scufris-terminal` PATH, `tools/briefing/cli.py state` from this checkout
  answers `collected` instead of raising `ModuleNotFoundError`.
