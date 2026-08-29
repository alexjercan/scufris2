# Reorganize Scufris by architecture boundary

- STATUS: CLOSED
- PRIORITY: 70
- TAGS: architecture, maintenance

## Ask

Replace the implementation-language-oriented `native/` layout with product
architecture boundaries before adding remote surfaces. Keep behavior, package
names, installed resource paths, protocols, and deployment defaults unchanged.
This is a mechanical reorganization, not protocol v4 or an iOS implementation.

## Target layout

```text
agent/
  extensions/
  skills/

host/
  service/

surfaces/
  desktop/
    backends/
    widgets/

shared/
  control/

Cargo.toml
Cargo.lock
nix/
docs/
tests/
tasks/
```

Create no empty `host/gateway/` or `surfaces/ios/` placeholder. Add those paths
only with their first tested behavior.

## Moves

- Move `extensions/` to `agent/extensions/`.
- Move product `skills/` to `agent/skills/`. Keep development skills in
  `.agents/skills/`.
- Move `native/scufris-service/` to `host/service/`.
- Move `native/scufris-desktop/` to `surfaces/desktop/`.
- Fold `native/scufris-widgets/widgets/` into
  `surfaces/desktop/widgets/`.
- Fold `native/scufris-widgets/backends/` into
  `surfaces/desktop/backends/`.
- Move `native/scufris-control/` to `shared/control/` without renaming the
  `scufris-control` crate.
- Move the Rust workspace manifest and lock file from `native/` to the
  repository root and update workspace member paths.
- Remove `native/` and the separate `scufris-widgets` wrapper after all tracked
  content has moved.

## Compatibility constraints

- Keep executable and package names: `scufris`, `scufris-service`,
  `scufris-desktop`, and `scufris-ctl`.
- Keep flake outputs, Home Manager option names, systemd unit names, environment
  variables, runtime paths, and installed `share/scufris` resource paths.
- Keep protocol v3 bytes and behavior unchanged.
- Keep the current desktop, service, widgets, and Pi resources in the same
  package closures.
- Do not move Whisper or Piper into a new inference project in this task.
- Do not revise historical task records merely to replace old source paths.
  Update active source documentation and tests.
- Do not mix widget limit, Claude usage HTTP 429, desktop polish, TCP gateway,
  multi-surface, or iOS behavior into this move.

## Required updates

- Rust workspace member and path dependencies.
- Desktop `build.rs` widget and backend source paths.
- Nix source roots, filters, builds, checks, and development shell commands.
- Agent resource packaging and launcher paths.
- TypeScript and Python test fixtures that resolve product resources.
- Structure tests and maintained documentation source links.
- CI commands and contributor commands that enter `native/`.
- Ignore rules for the root Rust `target/` and generated widget output.

## Verification

Run focused checks after each boundary move, then the broad checks once the
mechanical migration is complete:

1. `cargo test -p scufris-control`
2. `cargo test -p scufris-service`
3. Focused desktop and widget tests.
4. `npm run check`
5. `python3 -m unittest discover -s tests -p 'test_*.py'`
6. Relevant Nix package and structure checks.
7. `nix flake check` before completion.

Also compare the flake output names and built package closures before and after
the move. No new runtime dependency should appear solely because a directory
changed.

## Completion criteria

- The target layout contains all current tracked implementation files.
- `native/` no longer exists.
- Desktop widgets and backends are owned by `surfaces/desktop/`.
- Product extensions and skills are owned by `agent/`.
- Rust shared code is under `shared/`; the authoritative process is under
  `host/`; the Linux companion is under `surfaces/`.
- Existing packages, modules, units, protocol behavior, and installed resource
  paths remain compatible.
- Maintained documentation describes the new architecture paths.
- Focused checks and `nix flake check` pass with evidence recorded here.

## Decisions

- Keep the installed `share/scufris/extensions` and `share/scufris/skills`
  layout. Source resources now come from `agent/`, but launchers and deployed
  paths do not change.
- Use one filtered root Rust source containing `Cargo.toml`, `Cargo.lock`,
  `host/`, `shared/`, and `surfaces/` for Nix Rust builds.
- Preserve historical task records. Two pre-existing design records that do
  not match current Prettier output are ignored instead of reformatted.

## Result

- Moved Pi extensions and product skills under `agent/`.
- Moved the service under `host/service/`, the control crate under
  `shared/control/`, and the desktop with its widgets and backends under
  `surfaces/desktop/`.
- Moved the Cargo workspace manifest and lock file to the repository root.
- Removed `native/` and the separate `scufris-widgets` wrapper.
- Updated Rust dependencies, build inputs, Nix sources, launchers, tests,
  contributor commands, active documentation, and ignore rules.

## Verification evidence

Completed on 2026-08-29:

- `cargo test -p scufris-control`: 26 passed.
- `cargo test -p scufris-service`: 49 passed.
- `cargo test -p scufris-desktop`: 314 passed, including widget catalog,
  backend, shell, and runtime tests.
- `npm run check` with the harness-only `PI_PACKAGE_DIR` unset: 92 tests passed;
  type checking and Prettier checks passed.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 91 passed.
- `.agents/skills/scufris-review/check-briefs.py`: 46 paths and 28 symbols
  resolved.
- Target structure assertions passed. `native/`, `host/gateway/`, and
  `surfaces/ios/` do not exist.
- `nix flake check -L`: all checks passed on `x86_64-linux`.
- Package output names and app output names before and after the move are
  identical.
- Runtime closure name sets are identical before and after for `scufris` (37
  paths), `resources` (1), `scufris-desktop` (186), `scufris-service` (8), and
  `scufris-ctl` (8). The move added no runtime dependency.
- Installed resource locations remain under `share/scufris/extensions` and
  `share/scufris/skills`. Resource source text changed only where repository
  source locators had to follow the move.
