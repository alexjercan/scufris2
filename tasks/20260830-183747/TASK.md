# Release and deploy Scufris v1.1.0

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: release, deployment

## Goal

Publish one immutable v1.1.0 product release for the protocol-v4 thinking and
iOS host-transcription milestones, then deploy and verify both current surfaces.

## Scope

- Align npm, Cargo, iOS marketing, changelog, and release tag at 1.1.0.
- Add a repository check that keeps product versions and protocol constants
  consistent while retaining protocol version 4 as an independent value.
- Run the complete release checklist and publish the immutable tag and GitHub
  release.
- Update and activate the pinned `nix.dotfiles` deployment.
- Verify authenticated production WSS and HTTPS transcription without printing
  the bearer token, audio, or transcript text.
- Upload the signed iOS 1.1.0 TestFlight build and perform physical review of
  thinking feedback and dictation.

## Safety

- Never print or commit signing material or the gateway bearer token.
- Keep the gateway loopback-only and Tailscale Serve private. Do not use Funnel.
- Do not move or replace a release tag.
- Protocol v4 remains unchanged. Attachments remain v2.0.0 protocol-v5 work.

## Verification

- Product version 1.1.0 is aligned across npm, Cargo, and iOS. The new
  `tools/release/check_versions.py` check independently confirms surface
  protocol version 4 across Rust, TypeScript, and Swift.
- The complete local release checklist passed: locked npm checks, 95 Python
  tests, Ruff, ShellCheck, 360 Rust tests, Clippy with warnings denied, Nix
  formatting, development-shell checks, and `nix flake check -L`.
- Master check run
  [33320586936](https://github.com/alexjercan/scufris2/actions/runs/33320586936)
  passed before tagging.
- Immutable tag `v1.1.0` points to
  `07a4f776010370368d6cbc1ec47796f7856cf835`. Release run
  [33321163420](https://github.com/alexjercan/scufris2/actions/runs/33321163420)
  passed and published the source release.
- `nix.dotfiles` commits `cd273ee` and `3003a4a` pin, activate, and record the
  production v1.1.0 deployment. All owned services are active; production
  authenticated HTTPS health, WSS protocol v4, and host transcription checks
  pass without exposing credentials or transcript text.
- TestFlight run
  [33322073768](https://github.com/alexjercan/scufris2/actions/runs/33322073768)
  uploaded iOS `1.1.0 (7)` and removed signing material.

## v1.1.1 lifecycle follow-up

A later Home Manager activation restarted `scufris-service`, and the gateway's
`Requires` dependency stopped it. Since its unit had not changed, Home Manager
did not include it in the start set. The reusable module now adds the reverse
`scufris-service Wants scufris-surface-gateway` start dependency.

v1.1.1 release run
[33323767222](https://github.com/alexjercan/scufris2/actions/runs/33323767222)
passed and published the source release from `22ca92c`. iOS simulator run
[33323219649](https://github.com/alexjercan/scufris2/actions/runs/33323219649)
also passed. `nix.dotfiles` commit `7d5b0fb` pins and activates v1.1.1. An
explicit `scufris-service` restart restarted the gateway, left both units
active, and preserved authenticated production health.

## Completion

The product owner accepted the deployed v1.1.1-compatible release on 2026-08-30.
Any later physical review is product follow-up, not a release blocker.
