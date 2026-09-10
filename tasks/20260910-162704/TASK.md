# Cut the v2.6.0 release

- STATUS: OPEN
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

### Remaining

- Tag, push `master`, push the tag, verify the started workflows.
- `gh workflow run testflight.yml --ref v2.6.0`. `SERVICE_VERSION` moved 8 ->
  9, and the gateway compares it with exact equality, so the phone is refused
  at its hello the moment the machine switches.
- Deploy only after that build is installed on the phone.
