# Release and deploy Scufris v0.1.0

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: release, deployment

## Goal

Publish the working Scufris baseline as `v0.1.0` and replace the canonical machine-local dotfiles input with the tagged GitHub release.

## Accepted design

- Canonical remote: `git@github.com:alexjercan/scufris2.git`.
- Release tag: `v0.1.0`.
- Package metadata version: `0.1.0`.
- Treat the Home Manager interface as pre-1.0. Breaking changes remain allowed.
- Fix check isolation before tagging. Tests must pass with and without inherited Scufris voice development variables.
- Tag only a clean reviewed commit after all Scufris checks pass.
- Push the release commit and tag only after local review and explicit Pair verification.
- Replace the canonical `nix.dotfiles` `git+file` input with the GitHub tag input after the tag is available remotely.
- Preserve the existing exact revision check or replace it with an equally strict release-source assertion.
- Activate only after dotfiles review and checks pass.

## Scope

### Scufris release preparation

- Make the Calm and voice development tests hermetic against inherited environment.
- Audit tracked release content for secrets and unintended machine-specific data.
- Update package metadata to `0.1.0` and refresh the lockfile consistently.
- Record verification and release evidence here.

### Release and deployment

- Configure and verify the canonical Git remote.
- Run all repository checks from an ordinary environment and the repository development environment.
- Create and push `v0.1.0`.
- Update the canonical `nix.dotfiles` input to the tagged GitHub source.
- Run focused and full dotfiles checks, build the activation package, and review before activation.
- Verify popup startup and toggle, resumable conversation, STT, and TTS after activation.

## Non-goals

- Voice UX changes.
- Piper upgrade.
- Native RPC frontend.
- Orchestration feature changes.
- Archiving unrelated repositories.

## Verification

Scufris:

- `npm run check` with voice variables absent.
- `npm run check` with the repository development-shell voice variables present.
- Python tests, Ruff, ShellCheck, Nix formatting, `nix flake check`, and `git diff --check`.

Dotfiles:

- Focused Scufris input and Home Manager checks.
- `nix flake check`.
- `nix build .#homeConfigurations.alex.activationPackage --no-link`.
- `git diff --check`.
- Structured review before activation.

## Completion criteria

- Scufris `v0.1.0` resolves from GitHub without a local source path.
- Canonical dotfiles no longer use `git+file` for Scufris.
- Both repositories are clean after accepted commits.
- Live STT, TTS, popup toggle, and session resume still work.

## Release preparation evidence

Implementation:

- Base revision: `b7196a7` (`master` and `release-prep` before this work).
- `tests/calm.test.ts` removes inherited `SCUFRIS_CALM` before initial Calm setup and restores the caller value with test teardown, including assertion failures.
- `tests/dev_helper.test.ts` removes inherited voice-shell, Piper, speech, and Calm variables from the shared unavailable-shell fixture. The available-shell case still sets its trusted voice variables explicitly and verifies speech and Calm output.
- `package.json` and the root package in `package-lock.json` now use version `0.1.0`.
- Product behavior is unchanged.

Tracked release audit:

- Inspected tracked names, ignored tracked files, text credential patterns, local source forms, user-specific absolute paths, and generated cache patterns.
- No credentials, local absolute source references, generated caches, or unintended machine-specific release data found.
- Matches were bounded fixtures or generic test paths: diagnostics redaction terms and a synthetic token, `/home/user` diagnostics fixtures, and `/home/scufris-test` Nix fixtures.
- No release-blocking audit changes required.

Verification before synchronization:

- `npm ci` - pass; installed the locked development dependencies, with zero reported vulnerabilities.
- `env -u SCUFRIS_DEV_VOICE -u SCUFRIS_PIPER_MODEL -u SCUFRIS_PIPER_CONFIG -u SCUFRIS_SPEECH -u SCUFRIS_CALM npm run check` - pass; 33 TypeScript tests, typecheck, and Prettier.
- `SCUFRIS_DEV_VOICE=1 SCUFRIS_PIPER_MODEL=/nix/store/representative-model.onnx SCUFRIS_PIPER_CONFIG=/nix/store/representative-model.onnx.json SCUFRIS_SPEECH=1 SCUFRIS_CALM=1 npm run check` - pass; same 33 tests and static checks with inherited voice development values.
- `python3 -m unittest discover -s tests -p 'test_*.py'` - pass; 22 tests.
- `ruff check .` - pass.
- `ruff format --check .` - pass; 28 Python files already formatted.
- `shellcheck scripts/scufris-dev` - pass; it is the repository Bash script. Other executable helpers are Python and are covered by Ruff and Python tests.
- `nix fmt -- --check .` - pass; six Nix files comply with Alejandra.
- `nix flake check` - pass for the current `x86_64-linux` system, including the real Piper fixture. Nix reported the configured incompatible systems as omitted.
- `git diff --check` - pass.

Setup corrections:

- The first two JavaScript check attempts failed before typechecking because the fresh worktree had no `node_modules`; `npm ci` supplied the lockfile dependencies, then both exact checks passed.
- An initial broad ShellCheck invocation included Python executables and produced `SC1071`; selecting the Bash executable only passed. No source defect was involved.

Synchronization and final verification:

- Committed the complete release-preparation change as one commit on `release-prep`; `master` remains at base revision `b7196a7`.
- `sprout sync release-prep` - pass; already up to date.
- Repeated both exact JavaScript environment checks after synchronization - pass; 33 tests in each environment, typecheck, and Prettier.
- Repeated Python tests - pass; 22 tests.
- Repeated Ruff lint and format checks, ShellCheck, Nix formatting, full Nix flake check, and diff whitespace check - pass.
- Final review revision: `release-prep` HEAD (this evidence commit). Worktree clean after the evidence amendment and final synchronization.
