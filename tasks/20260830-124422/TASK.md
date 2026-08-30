# Connect the iOS surface over Tailscale

- STATUS: OPEN
- PRIORITY: 100
- TAGS: ios, protocol

## Goal

Connect the TestFlight iOS companion to the existing Scufris conversation over
a private Tailscale path without exposing agent or control access.

## Scope

- Correct generated iOS version metadata for subsequent builds.
- Define a protocol-v4 WebSocket transport for surface traffic only.
- Add an explicit loopback network listener to the backend.
- Require an independently generated bearer token and bounded handshakes.
- Put TLS and tailnet access in front of the listener through Tailscale Serve.
- Store the backend URL and token in the iOS Keychain.
- Implement connect, reconnect, canonical replay, and text input in SwiftUI.
- Add host protocol tests, Swift tests, CI, deployment documentation, and a
  physical-device TestFlight check.

## Decisions

- Tailscale is already active on the backend machine and is the private network
  boundary for the first iOS deployment.
- The network listener exposes only surface protocol capabilities. Agent and
  control sockets remain local Unix sockets.
- The application does not make the local Unix protocol or `ai-tools-api`
  publicly reachable.
- Voice and widgets remain out of scope until text conversation and replay are
  reliable on a physical device.

## Verification

Pending design review, implementation, CI, deployment, and TestFlight testing.
