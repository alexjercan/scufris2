# Lane: contracts and lockstep

Judge what the change breaks outside the code that compiles it: the
pairs of files that must move together, the strings no compiler
checks, and the documents that describe them.

## The lockstep pairs

Grep both sides of every pair the change touches:

- Protocol: `native/control-protocol-v*.json` fixtures are read by
  the Rust suite (`native/scufris-control/src/lib.rs`, `include_str`)
  and the TS suite (`tests/desktop.test.ts`); `PROTOCOL_VERSION` lives
  in both `scufris-control` and
  `extensions/scufris/desktop/protocol.ts`. A message shape changed on
  one side only is a broken pair.
- Identity: the sentence in `extensions/scufris/workflow/identity.ts`
  is asserted byte-for-byte in `tests/identity.test.ts`.
- Launcher: the argv built in `nix/launcher.nix` is asserted exactly
  in `nix/checks/launcher.nix`. A new skill directory or extension
  entry needs both, and `scripts/scufris-dev` besides.
- Capabilities: the window labels in `capabilities/default.json` must
  cover every label `src/` creates - a window outside the list loses
  its IPC silently.
- Frames: the window sizes pinned in `src/pill.rs` and `src/review.rs`
  are exactly the layouts `ui/pill.css` and `ui/review.css` build; the
  comments state the arithmetic. A size changed on one side is a
  frame that clips or a window that shows its ground.
- Versions: `Cargo.toml`, `tauri.conf.json`, and `package.json` state
  the same version. The 0.3.0/0.4.0 drift shipped once.
- The nix ripple: `package.json` (`pi.extensions`, `pi.skills`) ->
  `scripts/scufris-dev` -> `flake.nix` -> `nix/scufris.nix` ->
  `nix/launcher.nix` -> `nix/home-manager.nix` -> `nix/checks/*`. A
  surface added or removed at one end reaches the other.

## Also

- A breaking `programs.scufris.*` module option needs a `CHANGELOG.md`
  entry and a migration note. An option that never shipped needs
  neither: remove its documentation instead.
- Documentation the change invalidated: the mdBook under `docs/`
  describes mechanisms, and it has gone stale twice (a wave, a glow,
  and a privacy ring that no longer existed). Re-derive a claim from
  the code; do not only grep for the name.
- The records under `tasks/` are append-only history: scope every
  "no references remain" grep to non-`tasks/` paths, and never flag
  the archive.
