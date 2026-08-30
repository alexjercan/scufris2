# Connect the iOS surface over Tailscale

- STATUS: CLOSED
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

- Rust gateway unit tests prove loopback-only binding, bearer comparison,
  strict protocol-v4 decoding, and authenticated WebSocket-to-Unix bridging.
- The Home Manager checks prove the optional persistent gateway unit, private
  token argument, service dependency, package closure, and interface.
- Xcode 26.3 simulator CI run
  [33306114900](https://github.com/alexjercan/scufris2/actions/runs/33306114900)
  built the Swift 6 client and passed its protocol tests.
- TestFlight run
  [33306287733](https://github.com/alexjercan/scufris2/actions/runs/33306287733)
  uploaded version 1.0 build 5 without errors.
- A live authenticated WSS proof reached `surface.ready` through Tailscale
  Serve while an unauthenticated loopback upgrade returned HTTP 401.
- The physical iPhone connected, replayed the canonical conversation, sent a
  text message, and displayed the assistant response.
- `staging up` and `backend` now own an isolated gateway, private token, and
  optional exact `/scufris-staging` Tailscale Serve route. The 93 Python tests,
  ShellCheck, 67 Node tests, focused Nix builds, documentation build, Rust
  tests, Clippy, and final `nix flake check` across all compatible checks
  passed.
- The deployed Home Manager generation replaced the transient process with a
  persistent non-transient unit and retained the working tailnet URL.
