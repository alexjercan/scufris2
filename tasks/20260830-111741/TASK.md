# Release Scufris 0.6.0

- STATUS: OPEN
- PRIORITY: 100
- TAGS: release

## Goal

Publish the protocol v4 and shared `ai-tools-api` migration as Scufris v0.6.0,
then provide the immutable tag for the nix.dotfiles deployment update.

## Decisions

- Use v0.6.0 because protocol v4 replaces v3 without compatibility and the
  release removes direct Whisper and Piper ownership.
- Include architecture boundaries, split staging, structured logs, and the
  shared inference migration in one release.
- Push `master` before the annotated tag, as required by `RELEASE.md`.

## Verification

Pending the complete release checklist and GitHub release workflow.
