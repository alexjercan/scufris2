# Prepare Scufris v2.1.6 release

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: release

## Goal

Prepare the landed canonical conversation replay persistence fix as a clean
Scufris release commit. Do not land, tag, publish, deploy, activate Home
Manager, change the installed package, or modify nix.dotfiles or device state.

## Decision

Use v2.1.6. The shipped behavior is a backward-compatible fix after v2.1.5:
the service now persists its canonical latest-200-message replay across
restarts and Home Manager switches. Product version metadata stays aligned and
surface protocol 5 does not change.

## Preflight

- The assigned Sprout started clean at merge commit `5f4f10b`, equal to local
  `master` and freshly fetched `origin/master`.
- The latest immutable release is annotated tag and GitHub Release `v2.1.5` at
  `288cb19`. Recent compatible fixes use patch increments and aligned npm,
  Cargo, and iOS marketing versions.
- Pull request 1 and the resulting `master` merge both passed repository and
  documentation CI. The landed implementation task records focused replay,
  restart, recovery, staging-isolation, Rust, JavaScript, Python, Nix, and
  documentation checks.
- The only user-facing changelog entry after v2.1.5 is the canonical replay
  persistence fix. Protocol version 5 is unchanged.

## Release preparation

- `npm version 2.1.6 --no-git-tag-version` updated `package.json` and the root
  `package-lock.json` records.
- The Cargo workspace and its three lock records now use 2.1.6. The iOS
  marketing version required by the repository version validator is also
  aligned to 2.1.6.
- The factual replay fix moved from `Unreleased` to the dated 2.1.6 section.
  Changelog comparison links now start from `v2.1.6`.
- No other documentation or generated release file needed a release-only
  update. The landed change already contains its durable service,
  configuration, staging, and environment documentation.

## Verification

The complete local release gate passed on 2026-09-06 without starting or
restarting Scufris, activating Home Manager, changing the installed package,
deploying, publishing, or touching external configuration:

- Focused product/tag validation passed for product 2.1.6 and unchanged surface
  protocol 5. Its two unit tests and both staged and unstaged diff whitespace
  checks passed.
- `cargo check --workspace` refreshed only the three Scufris package versions
  in `Cargo.lock` and passed.
- `npm ci` installed 235 locked packages with 0 vulnerabilities.
- `npm run check` passed in ordinary and Nix development environments: version
  validation, strict TypeScript, 99 Node tests, and Prettier all passed.
- The full Python suite passed with 281 tests. Ruff check and format passed for
  227 files. ShellCheck passed for the agent, development, and staging
  launchers.
- Cargo Clippy passed for the workspace and all targets with warnings denied.
  Workspace tests passed: 16 control, 319 desktop, 36 service, and 9 gateway
  tests.
- `nix fmt -- --check .` passed for 23 Nix files.
- `nix flake check -L` passed all checks and builds for the compatible local
  system. Nix reported only the expected omitted incompatible aarch64 systems.

## Handoff

The release preparation remains intentionally unpushed and untagged. After its
commit lands on current `master`, push `master` first. Confirm the branch CI
passes, create immutable annotated tag `v2.1.6` on that exact release commit,
and push only that tag. Then verify the release workflow reuses the check job,
validates the version, and publishes the source-only, non-draft,
non-prerelease GitHub Release with generated notes. Do not update nix.dotfiles,
activate Home Manager, deploy, restart services, publish TestFlight, or change
any device configuration as part of this request.
