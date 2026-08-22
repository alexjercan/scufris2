# Add CI and tagged GitHub releases

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: ci, release

## Goal

Add GitHub Actions checks and automatic GitHub Release publication for future semantic-version tags. Document the manual release contract.

## Accepted design

- Follow the focused reusable-check and tag-release shape used by the Today repository.
- CI runs on pushes to `master`, pull requests, and manual dispatch.
- One reusable check workflow installs Node 22 and Nix, installs locked npm dependencies, runs the JavaScript repository check, and runs the full Nix flake check with logs.
- Branch filtering prevents a release tag from running the same suite twice.
- Release triggers only for exact stable semantic-version tags in the form `vX.Y.Z`.
- Release calls the reusable check workflow and cannot publish until it passes.
- Release verifies that the tag equals `v` plus the root package version.
- Release creates one source-only GitHub Release with generated notes and tag verification. No binary assets: consumers use the flake source and Nix builds its outputs.
- Use least GitHub token permissions. Checks are read-only. Only the publication job gets contents write.
- Add `RELEASE.md` with preparation, version, verification, annotated-tag, push, workflow, and immutability rules.
- The existing `v0.1.0` tag predates this automation. Do not move or recreate it. Backfill its GitHub Release separately after this workflow lands.

## Non-goals

- Documentation site or mdBook.
- GitHub Pages.
- Binary archives or package-registry publication.
- Runtime deployment or Home Manager activation.
- Product behavior changes.

## Verification

- Validate workflow YAML and exact event filters.
- Verify the reusable workflow and least permissions.
- Run `npm run check`.
- Run `nix flake check`.
- Run formatting and diff whitespace checks.
- Record implementation and verification evidence here.

## Completion criteria

- Branch and pull-request changes run the complete check suite.
- A future exact semantic-version tag runs checks, validates package version, and creates a GitHub Release.
- Invalid or mismatched tags cannot publish.
- `RELEASE.md` is sufficient to cut the next release without prior conversation context.

## Implementation evidence

Implementation:

- Base revision: `60a1737` on `release-automation` and `master` before this work.
- `.github/workflows/check.yml` is the reusable read-only suite. It runs for `master` pushes, pull requests, manual dispatch, and release workflow calls. Its branch-only push filter excludes tag pushes.
- The check job uses Node 22, `npm ci`, the complete `npm run check`, flakes-enabled Nix, and `nix flake check -L`.
- `.github/workflows/release.yml` triggers on stable numeric `vX.Y.Z` tags, calls the reusable suite first, and keeps default contents access read-only.
- Only the publication job receives `contents: write`. It rejects non-canonical semantic versions and package-version mismatches before `gh release create --verify-tag --generate-notes` creates a source-only release.
- `RELEASE.md` records clean `master` preparation, task and documentation review, package and lockfile versioning, relevant local environments, full checks, annotated immutable tags, ordered pushes, automatic verification, and source-only policy. Historical `v0.1.0` backfill context remains in this task instead of the generic checklist.
- No release assets, product behavior, Nix outputs, deployment, tags, or remote state changed.

Verification before synchronization:

- `npm ci` - pass; installed the lockfile dependency graph with zero reported vulnerabilities.
- `nix run nixpkgs#actionlint -- .github/workflows/check.yml .github/workflows/release.yml` - pass; both workflow files are syntactically and structurally valid.
- Parsed both workflows through `yq` and asserted the complete check event set, exact `master` branch filter, absence of check tag filters, sole release tag event, and representative accepted and rejected tag names - pass.
- Extracted the release version guard from the parsed workflow and executed it against the current `v0.1.0`, mismatched `v0.2.0`, and non-canonical `v01.1.0` cases - pass; only the matching canonical tag succeeded.
- `env -u SCUFRIS_DEV_VOICE -u SCUFRIS_PIPER_MODEL -u SCUFRIS_PIPER_CONFIG -u SCUFRIS_SPEECH -u SCUFRIS_CALM npm run check` - pass; typecheck, 33 TypeScript tests, and Prettier passed in the clean ordinary environment.
- `nix fmt -- --check .` - pass; all six Nix files comply with Alejandra.
- `nix flake check -L` - pass for `x86_64-linux`, including the real Piper fixture. Nix omitted the configured incompatible systems.
- `git diff --check` - pass.

Setup note:

- The first YAML conversion attempt used an unsupported `-o=json` flag with the available Python `yq`. Re-running its default JSON output form enabled all event assertions. No repository change was needed.

Synchronization and final verification:

- Committed the implementation as `78ed1a0` (`Add tagged release automation`).
- `sprout sync release-automation` - pass; merged current `master` revision `ccc8379` and its unrelated mdBook planning task as merge revision `f07ab2f`.
- Repeated the clean-environment `npm run check` after synchronization - pass; typecheck, 33 TypeScript tests, and Prettier passed.
- Repeated `nix fmt -- --check .` and `nix flake check -L` after synchronization - pass with the same incompatible-system notice.
- Repeated Actionlint and `git diff --check` after synchronization - pass.
- Final review revision: `release-automation` HEAD after this evidence commit. The worktree is clean.

Review correction:

- Removed the historical `v0.1.0` backfill note from `RELEASE.md` so the checklist stays generic. The accepted backfill constraint remains recorded in this task.
