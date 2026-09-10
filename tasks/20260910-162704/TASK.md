# Cut the v2.6.0 release

- STATUS: CLOSED
- PRIORITY: 0
- TAGS: release

## Purpose

Cut the stable `v2.6.0` release from `c8f3404`. `SERVICE_VERSION` moved
8 -> 9 in that commit (strict `briefing.dismiss`), so this is a minor bump
and every surface has to move together.

## Scope

Two commits since `v2.5.0`. `c8f3404` (durable briefing drawer dismissal)
carries all user-facing work and already wrote its `CHANGELOG.md`
`Unreleased` entries. `ea2ca2d` changes only `tasks/` and does not ship.

## Follow RELEASE.md

1. Review scope and the `Unreleased` entries.
2. Bump `package.json`, `package-lock.json`, workspace `Cargo.toml`,
   `Cargo.lock`, and iOS `MARKETING_VERSION` to 2.6.0. Move `Unreleased`
   under a `2.6.0` heading and update the comparison links.
3. Run the full repository checks.
4. Tag, push master, then push the tag.
5. Verify the started workflows.
6. `gh workflow run testflight.yml --ref v2.6.0` because `SERVICE_VERSION`
   changed. The gateway compares the version with exact equality.
7. Deploy: bump the `scufris2` input in the consuming Nix configuration and
   switch, but not before the TestFlight build is installed on the phone.

## Evidence

### Scope review

- `git log v2.5.0..HEAD` is `ea2ca2d` and `c8f3404`.
- `c8f3404` touches docs, host service, shared control, both surfaces, and
  the Node tests. Its changelog entries describe the compact `BRIEF` drawer,
  durable per-run dismissal, protocol 9, and persistence format 2.
- `ea2ca2d` closes the v2.5.0 release task. `tasks/` only, no entry.

### Fix committed before preparation

- `077fb9c`. `c8f3404` added `BriefingStore::audit_rows`, which only the
  file's own test module calls, so the non-test `scufris-service` build failed
  `cargo clippy --all-targets -- -D warnings` as dead code. Clippy runs in
  neither `.github/workflows/check.yml` nor `nix flake check`, so the release
  gate was the first thing to see it. The method is now `#[cfg(test)]`.

### Verification

- `npm ci`: locked dependencies installed.
- `nix develop -c npm run check` with the voice variables unset: version
  check, typecheck, 124 Node tests, and repository-wide Prettier all pass.
- Step 4 does not apply. Nothing in this release touches voice configuration.
- `nix develop -c python3 -m unittest discover -s tests -p 'test_*.py'`: 390
  passed.
- `ruff check .`: clean. `ruff format --check .`: 257 files formatted.
- `shellcheck scripts/scufris-agent scripts/scufris-dev scripts/scufris-staging`:
  clean.
- `cargo clippy --all-targets -- -D warnings`: clean after `077fb9c`.
  `cargo test`: 25 control, 336 desktop, 61 service, and 9 gateway passed.
- `nix fmt -- --check .`: clean.
- `nix flake check -L`: all checks passed. Nix omitted `aarch64-darwin` and
  `aarch64-linux` as incompatible, so the Swift suite runs only in the `iOS`
  workflow.
- `git diff --check`: clean.

### Remaining after preparation

- Tag, push `master`, push the tag, verify the started workflows.
- `gh workflow run testflight.yml --ref v2.6.0`. `SERVICE_VERSION` moved 8 ->
  9, and the gateway compares it with exact equality, so the phone is refused
  at its hello the moment the machine switches.
- Deploy only after that build is installed on the phone.

### Released

- `1a0dbe7` tagged `v2.6.0` and pushed. All four workflows green: `release`
  on the tag, and `check`, `Documentation`, and `iOS` on master. The GitHub
  Release is source-only with no assets, as the process requires.
- `gh workflow run testflight.yml --ref v2.6.0` succeeded. The build is
  uploaded.
- Deployed against the process note, by explicit decision, as at v2.5.0. The
  TestFlight build was uploaded but not yet installed on the phone.
- `nix.dotfiles` `7fd5cbe` bumps the `scufris` input to `v2.6.0`.
  `nix flake check` passed there. `home-manager switch --flake .#alex` moved
  `scufris-service`, `scufris-desktop`, and `scufris-surface-gateway` to
  `scufris-service-2.6.0`.

### The gateway was already down when the deploy ran

`scufris-surface-gateway` was crash-looping before the switch, and the phone
had read OFFLINE since 16:49. The unit exited 1 on every restart with
`I/O failed: No such file or directory (os error 2)`, because its
`--token-file`, `~/.local/share/scufris/credentials/ios/surface-token`, did
not exist.

The whole Scufris data tree was recreated at 16:49. `credentials/` was gone,
`briefings.json` was absent, `conversation.json` was 2 KB, and `sessions/`
held one file starting 16:49:25. `~/.local/state/scufris` and
`~/.local/state/scufris-desktop` carried the same timestamps. The journal
shows the units stopped at 16:42:48 and an `sd-switch` reload from a tmux
pane at 16:49:24, so a separate `home-manager switch` ran there. This release
switched at 17:45, about an hour later, and did not cause it. What deleted
the tree is not identified.

No copy of the token survived under `$HOME`, in Trash, or under any name
matching `surface-token`. A new 32-byte hex token was minted at 0600 by
explicit decision, so the phone's stored token no longer matches and has to
be re-entered. All three units are active on `scufris-service-2.6.0` and the
gateway listens on `127.0.0.1:10440`.

### Remaining

- Install the `v2.6.0` TestFlight build on the phone and enter the new token.
  Until both are done the phone stays OFFLINE: protocol 9 is compared with
  exact equality, and the old token no longer authenticates.
- The briefing audit, conversation history, and attachments from before 16:49
  are gone. Nothing here recovers them.
