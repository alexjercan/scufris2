# Add CI and tagged GitHub releases

- STATUS: OPEN
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
