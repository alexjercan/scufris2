# Release Scufris v0.3.0

- STATUS: IN_PROGRESS
- PRIORITY: 100
- TAGS: release, deployment

## Goal

Publish the current reviewed `master` as the immutable Scufris `v0.3.0` source release.

## Scope

- Update package and lockfile metadata to `0.3.0`.
- Update installation examples to use `v0.3.0`.
- Run the complete release checklist.
- Commit and push the release preparation on `master`.
- Create and push the annotated `v0.3.0` tag.
- Verify the GitHub Actions release and published GitHub Release.

## Release review

- Base revision: `8a50b59` on clean `master`, synchronized with `origin/master`.
- Previous release: immutable `v0.2.0`, with a successful automated source release.
- Scope since `v0.2.0`: orchestration lifecycle improvements, workflow controls and diagnostics, Quick Review improvements, speech and response fixes, extension restructuring, and rewritten user and developer documentation.
- The pre-1.0 release keeps the existing package and Home Manager interfaces. No binary assets are planned.

## Verification

Follow every command and publication check in `RELEASE.md`. Record results below before closing this task.

## Preparation evidence

- `npm version 0.3.0 --no-git-tag-version` updated both package metadata files.
- Installation examples now pin immutable `v0.3.0` sources.
- The first ordinary `npm run check` passed typechecking and all 61 tests, then found Prettier violations in recently added Jarvis research artifacts. `npm run format` corrected those tracked artifacts. This fixes the formatting failure also seen in the latest `master` CI run.
- `npm ci` passed with 236 audited packages and no vulnerabilities.
- Ordinary `npm run check` with Scufris voice and Calm variables unset passed: typecheck, 61 TypeScript tests, and Prettier.
- `nix develop -c npm run check` passed with the normal voice development environment: typecheck, 61 TypeScript tests, and Prettier.
- Python unit tests passed: 51 tests.
- Ruff lint and format checks passed in `nix develop`: 97 Python files formatted.
- ShellCheck passed for `scripts/scufris-dev` in `nix develop`.
- `nix fmt -- --check .` passed for seven Nix files.
- `nix flake check -L` passed on `x86_64-linux`, including launcher, voice closure, and dashboardd integration checks. Nix omitted the configured incompatible systems.
- `git diff --check` passed.
- Direct host Ruff and ShellCheck commands were unavailable. The exact checks passed with repository-pinned tools in `nix develop`.
