# Release Scufris 0.6.0

- STATUS: CLOSED
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

- The complete local release checklist passed, including 67 Node tests, 92
  Python tests, 350 Rust tests, Clippy with warnings denied, and all compatible
  `nix flake check` checks.
- Annotated tag `v0.6.0` points to
  `d757b951ef03bfe8cca0201373f8c19fd24f4901`.
- GitHub Actions run `33301739392` passed repository checks, verified the tag
  against the package version, and published the source-only release at
  <https://github.com/alexjercan/scufris2/releases/tag/v0.6.0>.
- nix.dotfiles pins the immutable tag, follows its root `ai-tools-api` input,
  and passed its seven flake checks and Home Manager activation package build.
