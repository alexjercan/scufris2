# Release and deploy Scufris v0.1.0

- STATUS: OPEN
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
