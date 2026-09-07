# Prepare Scufris v2.1.7 release

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: release

## Goal

Prepare the landed safe Markdown details feature as one clean Scufris patch
release commit. Do not land, push, tag, publish, deploy, run TestFlight,
activate Home Manager, change installed software, restart services, or touch
`/home/alex/personal/nix.dotfiles`.

## Decision

Use v2.1.7. Published v2.1.6 is the latest stable release. The landed change is
a backward-compatible presentation and security feature, so the established
2.1.x patch sequence advances by one. Keep surface protocol v5 because the
canonical response shape and protocol did not change.

## Preflight

- The release Sprout, local `master`, and freshly fetched `origin/master` all
  started at `00915dc155d8c88a1113876437e0d1bd7c70d804` (`Render Markdown
details safely`) with clean worktrees and no divergence.
- The immutable annotated `v2.1.6` tag resolves to release commit `7dca61e`.
  Its published GitHub Release is non-draft and non-prerelease. No remote
  `v2.1.7` tag exists.
- Recent v2.1.x release commits align npm, Cargo, and iOS marketing versions,
  refresh both lock files, date the existing changelog entry, and update
  comparison links.
- The only user-facing change since v2.1.6 is safe Markdown details on desktop
  and iPhone. Full Markdown is limited to `details`; required `text` remains
  literal with safe HTTP(S) bare-URL autolinks. Gruber styling is retained and
  unsafe content stays inert.

## Release preparation

- `npm version 2.1.7 --no-git-tag-version` updated `package.json` and the root
  `package-lock.json` records.
- The Cargo workspace and its three package lock records now use 2.1.7. The iOS
  marketing version is also 2.1.7.
- The factual Markdown change moved from `Unreleased` to the dated 2.1.7
  section. Changelog comparison links now start from `v2.1.7`.
- No other documentation or generated release file needs a release-only
  update. The landed implementation already updated the desktop, surface, and
  iOS documentation.
- Surface protocol v5 remains aligned in Rust, TypeScript, and Swift.

## Verification

The complete local release gate passed on 2026-09-07 without landing, pushing,
tagging, publishing, deploying, running TestFlight, changing installed
software, restarting services, activating Home Manager, or touching
`/home/alex/personal/nix.dotfiles`:

- Focused product and tag validation passed for product 2.1.7 and unchanged
  surface protocol 5. Both version-check unit tests and `cargo check
--workspace` passed.
- `npm ci` installed 235 locked packages with 0 vulnerabilities.
- `npm run check` passed in ordinary and Nix development environments: version
  validation, strict TypeScript, 103 Node tests, and Prettier all passed. The
  first ordinary attempt inherited the worker's Pi 0.85 `PI_PACKAGE_DIR`,
  which redirected the locked Pi 0.84 theme loader to a moved store path. The
  clean rerun unset that harness-only variable and all voice development
  variables. It changed no source.
- The complete Python suite passed with 281 tests. Ruff check and format passed
  for 229 files. ShellCheck passed for the agent, development, and staging
  launchers.
- Cargo Clippy passed for the workspace and all targets with warnings denied.
  Workspace tests passed: 17 control, 321 desktop, 36 service, and 9 gateway
  tests.
- `nix fmt -- --check .` passed for 23 Nix files. `nix flake check -L` passed
  all compatible-system checks and builds; it omitted only incompatible
  aarch64 systems.
- This Linux host has no `xcodegen`, `xcodebuild`, or `swift`, so no local iOS
  simulator command is available. The landed implementation's macOS CI run
  `34048735190` generated the project and passed the unsigned simulator build
  and tests at exact base commit `00915dc`. The release commit's iOS marketing
  version change must trigger and pass that workflow again after landing and
  pushing.

## Handoff

Keep the release commit unlanded, unpushed, and untagged during this phase.
After it lands on a still-current `master`, fetch and confirm the release
commit, clean status, exact 2.1.7 product alignment, and protocol v5. Push
`master` first. Wait for the repository check, documentation build, and
unsigned iOS workflow from that push to pass. Then create immutable annotated
tag `v2.1.7` on that exact commit and push only the tag. Verify the release
workflow reuses the repository check, validates tag-to-product alignment, and
publishes one source-only, non-draft, non-prerelease GitHub Release with
generated notes. Do not add assets or run TestFlight as part of these release
actions.
