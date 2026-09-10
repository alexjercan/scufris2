# Cut the v2.5.0 release

- STATUS: CLOSED
- PRIORITY: 0
- TAGS: release

## Purpose

Cut the stable `v2.5.0` release. `SERVICE_VERSION` moved 7 -> 8 in `c807b1c`
(durable briefing lifecycle delivery), so this is a minor bump and every
surface has to move together.

## Scope

14 commits since `v2.4.1`. Only `c807b1c` wrote a `CHANGELOG.md` entry, so
release preparation must first describe the other user-facing work.

## Follow RELEASE.md

1. Review scope. Add the missing changelog entries.
2. Bump `package.json`, `package-lock.json`, workspace `Cargo.toml`,
   `Cargo.lock`, and iOS `MARKETING_VERSION` to 2.5.0. Move `Unreleased`
   under a `2.5.0` heading and update the comparison links.
3. Run the full repository checks.
4. Tag, push master, then push the tag.
5. Verify the started workflows.
6. `gh workflow run testflight.yml --ref v2.5.0` because `SERVICE_VERSION`
   changed. The gateway compares the version with exact equality.
7. Deploy: bump the `scufris2` input in the consuming Nix configuration and
   switch, but not before the TestFlight build is installed on the phone.

## Evidence

### Scope review

- 14 commits since `v2.4.1`. `c807b1c` was the only one with a changelog
  entry. Six more carried user-facing behavior and were described during
  preparation: desktop instrument columns (`e045850`), hold release on a dead
  backend (`27ac93a`), usage backend cadence below the staleness budget
  (`5ffc6e3`), den heading refusal and `suggest` guard (`f3dc909`), one
  `displayable` predicate at all four job doors (`228ea0b`), and the briefing
  unit's memory bound (`b58c110`).
- `fb17d49` replaces refusal string literals with the constants that already
  held the same values. The wire strings do not move, so it is not a
  user-facing change and has no entry.
- `860534c` changes `.agents/skills/`, and `05feaa2`, `5b16f45`, `bc49419`,
  and `3aad9f1` change only `tasks/`. Neither ships.
- `aa51633` is the bounded profile-bounds reader. The slice's existing
  "bounded before they are consumed" entry already covers it.

### Fix committed before preparation

- `d1c1718`. `c807b1c` left three Python files failing `ruff format --check`
  and `nix/checks/briefing.nix` failing `nix fmt -- --check`. All four are
  clean at `3aad9f1`. The slice's evidence records Prettier and
  `git diff --check` and records neither formatter, so the release gate was
  the first thing to see it. Formatting only.

### Verification

- `nix develop -c npm run check`: version check, typecheck, 123 Node tests,
  and repository-wide Prettier all pass.
- `nix develop -c python3 -m unittest discover -s tests -p 'test_*.py'`: 390
  passed.
- `ruff check .` and `ruff format --check .`: clean, 254 files.
- `shellcheck scripts/scufris-agent scripts/scufris-dev scripts/scufris-staging`:
  clean.
- `cargo clippy --all-targets -- -D warnings`: clean.
  `cargo test`: 24 control, 334 desktop, 55 service, and 9 gateway passed.
- `nix fmt -- --check .`: clean, 27 files.
- `nix flake check -L`: all checks passed. Nix omitted `aarch64-darwin` and
  `aarch64-linux` as incompatible, so the Swift suite runs only in the `iOS`
  workflow.
- `git diff --check`: clean.

### Remaining

- Tag, push `master`, push the tag, verify the started workflows.
- `gh workflow run testflight.yml --ref v2.5.0`. `SERVICE_VERSION` moved 7 ->
  8, and the gateway compares it with exact equality, so the phone is refused
  at its hello the moment the machine switches.
- Deploy only after that build is installed on the phone.

### Released

- `1761adb` tagged `v2.5.0` and pushed. All five workflows green: `release`
  and `TestFlight` on the tag, `check`, `Documentation`, and `iOS` on master.
  The GitHub Release is source-only with no assets, as the process requires.
- Deployed against the process note, by explicit decision. The TestFlight
  build was uploaded but not yet installed on the phone, so the phone is
  refused at its hello until it is. Desktop and service were switched anyway.
- `nix.dotfiles` `76ea06e` bumps the `scufris` input to `v2.5.0`.
  `nix flake check` passed there. `home-manager switch --flake .#alex` moved
  `scufris-service`, `scufris-desktop`, and `scufris-surface-gateway` to
  `scufris-service-2.5.0`, and installed `scufris-briefing-reconcile.timer`.

### What the first reconciliation found

The reconciler's first tick reported `finalized: 1, refused: 0, sent: 5`. The
2026-09-09 nightly run, stuck in `collecting` since the OOM stopped its cgroup
15 hours earlier, is now `failed` with delivery `prepared` and both
contributions retained on disk. It carried a version-1 manifest, so legacy
migration and ownerless finalization both ran on real state rather than on a
fixture. This is the incident the slice was written for, cleared by the first
scheduled run after deployment.

`scufris-briefing-nightly.service` still carries its failed state from that
night. The run is terminal in the manifest, so the systemd flag is only a
leftover; it is left alone rather than reset, in case the record is wanted.
