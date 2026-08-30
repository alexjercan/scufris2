# Release and deploy Scufris v1.1.0

- STATUS: OPEN
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
