# Release and deploy Scufris v2.1.5

- STATUS: CLOSED
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

## Publication and deployment

- Pushed release commit `288cb19` and annotated tag `v2.1.5`.
- Release workflow `33549439496` passed its JavaScript and Nix checks, verified
  the tag version, and published the source-only GitHub release:
  <https://github.com/alexjercan/scufris2/releases/tag/v2.1.5>.
- The ordinary iOS workflow passed for the release commit.
- TestFlight workflow `33550889532` built, signed, exported, and uploaded the
  2.1.5 iPhone application to App Store Connect successfully.
- nix.dotfiles commit `49be2bc` pins v2.1.5. Home Manager activated the 2.1.5
  desktop, service, and surface gateway; all three units are active and their
  executables resolve to 2.1.5 Nix store paths.
