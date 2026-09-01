# Release and deploy Scufris v2.1.5

- STATUS: OPEN
- PRIORITY: 100
- TAGS: release, deployment

## Goal

Publish the recovered conversation UI as Scufris v2.1.5, verify the immutable
release, and update the canonical Nix deployment from v2.1.4 to v2.1.5.

## Decision

Use a patch release. The only product change since v2.1.4 is the compatible
conversation layout and scroll behavior from task `20260901-181703`; protocol 5
is unchanged. Align npm, Cargo, and iOS marketing versions. Publish the normal
source-only GitHub release, then pin the immutable tag in nix.dotfiles.

## Preflight

- Restored and committed the lost staged implementation as `2351e62`.
- Focused desktop tests passed: 46 Node tests and 4 glyph generator tests.
- The full pre-release Node and Python suites passed: 99 and 281 tests.
- The desktop closure built after adding recovered files to the Git index and
  ran all 319 desktop Rust tests.
- The canonical nix.dotfiles checkout contains only the expected, uncommitted
  v2.1.3 to v2.1.4 pin from the prior deployment. It will be advanced directly
  to v2.1.5 and committed after the new immutable tag is published.

## Release verification

The complete local release gate passed on 2026-09-01:

- `npm ci`: 235 locked packages installed, 0 vulnerabilities.
- `npm run check` in ordinary and `nix develop` environments: product 2.1.5,
  protocol 5, strict TypeScript, 99 Node tests, and Prettier passed.
- Python: 281 tests passed; Ruff check and format passed for 223 files.
- ShellCheck passed for the agent, development, and staging launchers.
- Cargo Clippy passed with warnings denied; workspace tests passed with 16
  control, 319 desktop, 30 service, and 9 gateway tests.
- `nix fmt -- --check .` passed for 23 Nix files.
- `nix flake check -L` passed all compatible checks and package builds.
- `python3 tools/release/check_versions.py --tag v2.1.5` and
  `git diff --check` passed.
- iOS build and tests remain assigned to the macOS GitHub workflow.
