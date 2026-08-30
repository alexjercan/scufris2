# Protocol v5 attachment contract

Protocol v5 replaces v4. It is not negotiated. Every service, agent, gateway,
desktop, and iOS component in one deployment must move together.

## Bounds

- At most 8 attachment references per message.
- One attachment is at most 16 MiB in the initial image, PDF, and general-file
  phase.
- IDs use the existing 1 to 64 byte ASCII identifier grammar.
- Names are 1 to 255 UTF-8 bytes. They are display names, not paths. Slash,
  backslash, NUL, and control characters are invalid.
- Media types are 1 to 127 ASCII bytes and contain one non-empty type and
  subtype. Parameters are not stored in the descriptor.
- Descriptor IDs in one message are unique.
- Bytes never enter a protocol message or log.

## Descriptor

```json
{
  "id": "att_opaque",
  "name": "diagram.png",
  "media_type": "image/png",
  "size": 184223
}
```

The service creates immutable descriptors from stored content. Surfaces and the
agent can refer to an attachment by ID, but cannot provide descriptor metadata
for canonical messages.

## Messages

A surface submission may add IDs:

```json
{
  "v": 5,
  "type": "surface.message",
  "id": "ios-message-1",
  "text": "What does this show?",
  "attachments": ["att_opaque"]
}
```

The service resolves every ID before accepting the submission. It sends the
agent and all surfaces canonical immutable descriptors:

```json
{
  "v": 5,
  "type": "agent.message",
  "id": "ios-message-1",
  "text": "What does this show?",
  "widgets": [],
  "attachments": [
    {
      "id": "att_opaque",
      "name": "diagram.png",
      "media_type": "image/png",
      "size": 184223
    }
  ]
}
```

An `agent.response` may add stored IDs. The service rejects missing, expired,
or otherwise unavailable IDs. Canonical `surface.message` entries contain
resolved descriptors and retain them during replay.

Empty attachment arrays may be omitted. Unknown fields, duplicate IDs, invalid
descriptors, and out-of-bound arrays close or reject the operation according to
the channel's existing strict behavior.

## Transfer boundary

The existing bearer-authenticated gateway exposes:

```text
POST /attachments
GET  /attachments/{id}
HEAD /attachments/{id}
```

The gateway forwards these operations to the service-owned private content API.
It does not own metadata or bytes. Upload and download bodies are bounded and
never logged. GET accepts one closed, open-ended, or suffix byte range and
returns standard `206`, `416`, `Accept-Ranges`, and `Content-Range` metadata.
The import operation remains local-only.

## Incremental safety

The service accepts a non-empty reference only when it resolves against its own
durable store. Missing, expired, and invented IDs return
`attachments_unavailable`. No v5 source build is deployed to production before
the complete coordinated staging rollout.
