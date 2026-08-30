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
