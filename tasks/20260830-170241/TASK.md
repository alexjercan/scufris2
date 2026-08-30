# Add protocol v5 attachments

- STATUS: OPEN
- PRIORITY: 80
- TAGS: protocol, attachments, surfaces

## Goal

Add managed attachments in both directions for the desktop and iOS surfaces,
using protocol v5 references and authenticated HTTP transfer.

## Decisions

- `scufris-service` owns attachment metadata, bytes, retention, and canonical
  references through a private local content API. No surface sees a host path.
- Evolve the existing `scufris-surface-gateway` on loopback port 10440 into the
  authenticated HTTP and WebSocket edge. Do not create a second public port.
- Remote API:
  - `POST /attachments` uploads one bounded object and returns its descriptor.
  - `GET /attachments/{id}` downloads an object by opaque ID.
  - `HEAD` and HTTP Range support may share the GET implementation so previews
    and later video do not require another protocol revision.
- Every remote route requires the same bearer authentication as WSS. IDs are
  random and unguessable, but are not authorization by themselves.
- Protocol v5 adds optional attachment references. Surface submissions contain
  IDs; canonical and outbound messages contain immutable descriptors with
  `id`, `name`, `media_type`, and `size`.
- The agent receives service-owned local descriptors for user uploads. It never
  receives a user-selected host path.
- Product decision: the desired agent interface is a direct
  `store_attachment(path) -> id` call for any file readable by the Scufris
  process. The product owner accepts the disclosure risk and does not want an
  outbox copy step or configured attachment roots. Basic regular-file and byte
  bounds remain correctness requirements, not path authorization.
- Trusted local frontends may call the private content API's import operation
  with a file-picker path. That operation is never forwarded by the remote
  gateway and still rejects directories, devices, FIFOs, symlinks, and files
  outside the byte bound.
- Agent responses reference stored IDs. The service never reads an arbitrary
  model-provided path while constructing a response.
- Desktop and iOS both render safe previews and offer explicit open, share, or
  download behavior. They never execute attachment content.
- Start with images and PDFs. General files follow; video requires tested Range
  behavior and explicit larger limits.
- Unreferenced uploads expire. Referenced files survive canonical replay under
  a documented quota and retention policy.

## API shape

```text
Tailscale HTTPS :443
  -> scufris-surface-gateway 127.0.0.1:10440
       GET  /                         WebSocket protocol channel
       POST /attachments             authenticated upload
       GET  /attachments/{id}        authenticated download
       POST /audio/transcription     authenticated bounded dictation
  -> private service content API     storage and ai-tools-api forwarding
```

Example upload response and protocol descriptor:

```json
{
  "id": "att_opaque",
  "name": "diagram.png",
  "media_type": "image/png",
  "size": 184223
}
```

Errors use bounded typed JSON with a stable code and safe detail. Upload and
transcription bodies never enter INFO or DEBUG logs.

## Delivery plan

1. [x] Define the strict protocol-v5 attachment descriptor and reference bounds
       across Rust, TypeScript, desktop, and iOS. Keep production pinned to v1.1.1.
2. [x] Add the service-owned durable attachment store and private local content API.
       Prove atomic writes, opaque IDs, direct regular-file import, quotas, expiry,
       traversal rejection, and symlink rejection.
3. [x] Proxy authenticated upload, lookup, download, HEAD, and Range through the
       existing gateway. Keep its listener loopback-only.
4. [x] Add the agent `store_attachment` tool and resolve attachment IDs at the
       service boundary in both message directions.
5. [ ] Add desktop file selection, upload, rendering, open, and save behavior.
6. [ ] Add iOS document and photo selection, upload, rendering, download, and share
       behavior.
7. [ ] Deploy a complete protocol-v5 stack to staging, run end-to-end and physical
       tests, then prepare the coordinated v2.0.0 replacement.

## Progress

- Closed the accepted v1.1.0/v1.1.1 release task before starting v5.
- Added the first protocol-v5 contract slice: bounded immutable descriptors,
  bounded unique ID references, strict Rust and TypeScript decoding, canonical
  message fields, agent prompt carriage, final-response attachment IDs, and
  matching desktop and iOS wire types.
- Added the service-owned durable store and mode-0600 `content.sock` HTTP API.
  It supports bounded upload, trusted local regular-file import, lookup,
  download, and HEAD. Metadata and bytes use atomic private files under the
  service data directory.
- Added 512-object and 256 MiB quotas, 24-hour unreferenced and 30-day referenced
  retention, startup expiry and orphan cleanup, 192-bit opaque IDs, and path,
  symlink, type, media, and byte validation.
- Surface and agent IDs now resolve to immutable descriptors before entering
  agent messages, canonical broadcast, or replay. Missing and invented IDs are
  rejected with `attachments_unavailable`.
- Extended the existing authenticated loopback gateway with bounded raw upload,
  GET, HEAD, and single-range transfer. It proxies only to `content.sock`,
  preserves safe response metadata and typed errors, and does not expose the
  trusted local import operation.
- Added the orchestrator-only `store_attachment(path)` tool. It normalizes a
  bounded relative or absolute path, infers a conservative media type, imports
  through `content.sock`, validates the returned descriptor, and returns the
  opaque ID for `scufris_final_response`.

## Verification

The initial contract slice passes:

- `env -u PI_PACKAGE_DIR npm run check` with 69 TypeScript tests.
- `cargo test --workspace` with 361 Rust tests.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- 95 Python tests.
- `nix flake check -L`.
- Product/protocol consistency at product 1.1.1 and protocol 5.

Swift protocol types and tests are updated. This Linux host has no `swiftc` or
Xcode. iOS simulator run
[33325496014](https://github.com/alexjercan/scufris2/actions/runs/33325496014)
passed on Xcode 26.3. Repository run
[33325495864](https://github.com/alexjercan/scufris2/actions/runs/33325495864)
and documentation run
[33325495888](https://github.com/alexjercan/scufris2/actions/runs/33325495888)
also passed for commit `af6643c`.

The durable-store slice passes 366 Rust tests, Clippy with warnings denied, the
69 TypeScript tests, 95 Python tests, formatting, and `nix flake check -L`.
Focused tests cover private durable files, restart loading, expiry, orphan
cleanup, upload, import, download, typed missing-ID errors, path and symlink
rejection, byte bounds, and canonical descriptor resolution. Repository run
[33327728103](https://github.com/alexjercan/scufris2/actions/runs/33327728103)
and documentation run
[33327728066](https://github.com/alexjercan/scufris2/actions/runs/33327728066)
passed for store commit `3473592`.

The gateway-transfer slice passes 367 Rust tests, 69 TypeScript tests, 95
Python tests, Clippy with warnings denied, focused formatting checks, and
`nix flake check -L`. Focused integration tests cover bearer authentication,
raw upload and descriptor forwarding, the 16 MiB gateway bound, local-import
exclusion, GET, HEAD, closed and suffix ranges, standard range headers, and
bounded typed failures. Repository run
[33329028473](https://github.com/alexjercan/scufris2/actions/runs/33329028473)
and documentation run
[33329028456](https://github.com/alexjercan/scufris2/actions/runs/33329028456)
passed for gateway commit `90bbffb`.

The agent-import slice passes TypeScript compilation, 74 TypeScript tests, 95
Python tests through the Nix helper check, focused Prettier checks, and
`nix flake check -L`. Focused tests cover content-socket resolution,
conservative media types, the exact private import request, relative and `@`
path normalization, opaque-ID results, typed service failures, and strict
response descriptor validation.

## Acceptance

- Protocol v5 is strict, typed, bounded, and replaces v4 without hidden markup
  or base64 message payloads.
- Upload, import, lookup, replay, download, authentication, quota, expiry,
  traversal, symlink, media-type, and byte-bound tests pass.
- Desktop and iOS send, render, download, and share the initial supported types.
- An old or incapable surface cannot receive a message shape it cannot decode;
  rollout and protocol replacement are documented.
- The agent tool imports the requested regular file directly and returns its
  stored attachment ID.
