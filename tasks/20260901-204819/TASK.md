# Release and deploy Scufris v2.1.4

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: release, deployment

## Goal

Publish the landed desktop UI scrollbar fix as immutable Scufris v2.1.4,
deploy the tagged desktop package through the canonical nix.dotfiles pin, and
verify the release and live services.

## Release decision

- Use v2.1.4. The only product commit after v2.1.3 is
  `c15fa47e55c9bd38c9ce020d47d5c1f446e185f0`, a backward-compatible desktop
  UI bug fix. Semantic Versioning therefore requires a patch increment.
- Keep protocol version 5 unchanged.
- Align npm, Cargo, and iOS marketing metadata because the repository version
  check requires one product version.
- Publish the source-only GitHub Release through the annotated-tag workflow.
- Deploy the Linux desktop by pinning the immutable tag in the clean canonical
  `/home/alex/personal/nix.dotfiles` checkout, building its Home Manager
  activation package, switching, and checking the owned services.
- The initial release did not dispatch TestFlight because the landed change is
  confined to desktop widget CSS and TypeScript. The unsigned iOS workflow
  verified the aligned marketing metadata. In a follow-up request, the owner
  chose to deliver the aligned 2.1.4 iOS build too. Dispatch the protected
  TestFlight workflow from the immutable release commit on `master`.

## Preflight evidence

- Read the global and repository `AGENTS.md`, Tatr skill, `RELEASE.md`,
  `CHANGELOG.md`, version checker and tests, all five GitHub Actions workflows,
  iOS release notes, maintenance release documentation, recent release and UI
  tasks, Git history, tags, GitHub releases and runs, and nix.dotfiles
  deployment conventions.
- Fetched `origin`: local `master` was clean at
  `c15fa47e55c9bd38c9ce020d47d5c1f446e185f0`, exactly one commit ahead of
  `origin/master`, with no remote-only commits.
- `v2.1.3` is an annotated, unsigned tag at
  `a055e1a53c5f207f24f63439394f5896e45ebafc`; all recent stable release tags
  use the same annotated format.
- `git diff v2.1.3..master` contains the shared scrollbar lane, all four
  affected agenda/notes/macros lists, the focused source audit, and its closed
  implementation task. The task also records the conversation, form, textbox,
  and iOS scrollbar audit.
- The canonical nix.dotfiles checkout was clean and synchronized with its
  remote at `b4e5d675af2280e5c6292fe0fe81070c4c2a7bd8`, pinned to v2.1.3.
- Focused preflight passed: product/protocol version check, `git diff --check`,
  and both `tests/widget-ui.test.ts` tests.

## Release preparation

- `npm version 2.1.4 --no-git-tag-version` updated `package.json` and the root
  lock record.
- Updated the Cargo workspace and iOS marketing versions to 2.1.4. The local
  shell has no Cargo, so the checklist's Cargo refresh ran as
  `nix develop -c cargo check --workspace` and updated all three workspace lock
  records.
- Moved the scrollbar fix into the dated 2.1.4 changelog section and advanced
  the comparison links.
- The standalone Ruff steps exposed the known stale baseline recorded during
  v2.1.3. This release does not bypass it. `ruff.toml` documents the one
  generated-assembly exception: `backend.py` receives den prelude names in the
  same namespace at build time. Real datetime, import, executable-mode, test
  annotation, and unused-name findings were fixed. Ruff formatted the 13
  existing Python files that its required format gate identified. Python and
  package tests passed after these corrections.

## Verification evidence

The complete `RELEASE.md` checklist passed on 2026-09-01:

- `npm ci` in an ordinary environment with inherited Pi and worker variables
  unset: 235 locked packages installed, 0 vulnerabilities.
- `npm run check` in that ordinary environment: product 2.1.4/protocol 5,
  strict TypeScript, 89 Node tests, and Prettier passed.
- `npm run check` through `nix develop`, with the same variables unset: passed.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 277 passed after the
  final Python changes.
- `ruff check .` and `ruff format --check .`: passed; 219 files formatted.
- `shellcheck scripts/scufris-dev`: passed.
- `cargo clippy --all-targets -- -D warnings && cargo test` through
  `nix develop`: passed; 374 Rust tests passed.
- `nix fmt -- --check .`: passed on 23 Nix files.
- `nix flake check -L`: all compatible checks passed, including source
  packages, the WebKitGTK desktop package, 319 desktop tests, closures, Home
  Manager modules, resources, helpers, and documentation.
- `python3 tools/release/check_versions.py --tag v2.1.4` and
  `git diff --check`: passed.

## Publication and deployment evidence

- The source release workflow completed successfully for annotated tag
  `v2.1.4`. GitHub published the non-draft, non-prerelease release at
  <https://github.com/alexjercan/scufris2/releases/tag/v2.1.4>.
- The unsigned iOS workflow for release commit
  `d26ffa5b7da13cd25dc17b8b1a86ec222f1d080e` completed successfully.
- On the owner's follow-up request, dispatched the protected TestFlight
  workflow from `master`. Run `33545547418` archived marketing version 2.1.4,
  exported the signed application, and App Store Connect accepted the upload.
  The workflow completed successfully in 1 minute 21 seconds:
  <https://github.com/alexjercan/scufris2/actions/runs/33545547418>.
- The canonical desktop deployment and live-service checks remain pending.
