# Spike remote laptop and iOS surfaces

- STATUS: OPEN
- PRIORITY: 65
- TAGS: architecture, spike, remote

## Ask

Turn the settled design in `20260828-170154`, "Scufris on more than one
surface", into a reviewed implementation plan for:

- a second laptop that accesses the existing Scufris host; and
- a personal iOS app that acts as another Scufris surface.

This is a spike. It may build disposable proofs to answer transport and platform
questions, but it does not land protocol v4, a supported remote deployment, or
the iOS product.

## Gate

The implementation gate from `20260828-170154` remains in force: protocol and
product work does not start before `v0.5.0` is tagged and deployed. At the time
this task was created, the latest tag and package version were still `v0.4.0`.
Research and disposable proofs in this spike do not weaken that gate.

## Settled constraints inherited from `20260828-170154`

- One host owns `pi --mode rpc`, the session, and `scufris-service`.
- A laptop or phone is a surface, not another host or conversation.
- Scufris does not distinguish local and remote clients. Do not add a `remote`
  flag or location-dependent service behavior.
- `scufris-service` keeps its mode-0600 Unix socket and never opens a TCP
  listener. SSH, WireGuard, or Tailscale owns network reachability and
  authentication.
- Any number of surfaces may watch. Exactly one surface attends: transcript and
  state fan out, while speech, widget commands, and conversation-window requests
  go to the presence holder.
- Presence first follows the last surface that submitted. A phone may later
  claim presence from its foreground state.
- The host and clients remain one tightly coupled version. The `hello`,
  `welcome`, and version-refusal exchange must remain stable across all future
  mismatches, and an outdated phone must receive a plain update notice.
- Widgets remain host processes on host monitors. The phone does not render
  them.
- There is no public endpoint, relay, account, tenancy, push-notification road,
  or second Pi client.
- The laptop is proved before the phone.

## Current code finding

Nothing from the settled multi-surface design is implemented yet.

In `native/scufris-service/src/service.rs`, registering a second client in the
same non-control role removes the first. Two desktop companions therefore fight:
each reconnect replaces the other, and `native/scufris-desktop/src/link.rs`
retries up to the five-second backoff ceiling. The conversation window clears on
each reconnect.

The current service also:

- broadcasts `speak`, causing two surfaces to speak one answer;
- broadcasts widget commands, allowing duplicate reports;
- stores one last-writer-wins widget catalog, so a phone with no widgets can
  remove the host's widget tools; and
- broadcasts conversation-window requests, including to a phone in a pocket.

Protocol v4 must resolve these before two full frontends can coexist.

## Feasibility findings

### Second laptop

This is the cheaper case when the second laptop runs Linux/X11. OpenSSH supports
forwarding a local Unix socket directly to a remote Unix socket:

```sh
ssh -N \
  -L "$HOME/.scufris-remote/service.sock":/run/user/1000/scufris/service.sock \
  den

SCUFRIS_RUNTIME_DIR="$HOME/.scufris-remote" scufris-desktop
```

The exact command needs testing, including stale local-socket removal,
`ExitOnForwardFailure`, SSH reconnects, and systemd user supervision.

The existing companion is not portable as-is to every laptop. It depends on
X11, GTK, WebKitGTK, and X11-specific focus, shape, and global-key behavior. A
Linux/X11 laptop can reuse it. A macOS, Windows, Wayland-only, or otherwise
unsupported laptop needs a new frontend and changes the estimate substantially.
The spike must record the actual target laptop OS before fixing the plan.

Before protocol v4, a remote companion can only be tested with the host
companion stopped. Running both reproduces the known eviction livelock rather
than proving the tunnel wrong.

A no-UI rung already exists for diagnosis:

```sh
ssh den scufris-ctl watch
ssh den scufris-ctl send "hello from the laptop"
```

### iPhone

A native Swift iOS app is feasible and fits the settled model: it can implement
the bounded JSON-line frontend protocol, show transcript and state, submit text,
claim presence while foregrounded, record audio, and synthesize `speak` locally.

A browser or PWA is not the preferred path. Browser code cannot directly open
an SSH channel or Unix socket. It would require an HTTP/WebSocket gateway and a
network service that the settled design explicitly excludes.

Tailscale's iOS app can provide private IP reachability to the host. It does not
make the Unix socket directly reachable. The phone app still needs an SSH
transport from the tailnet to `service.sock`. The normal OpenSSH server over the
tailnet is sufficient; Scufris does not need to know Tailscale exists.

The transport is the main unknown. Swift SSH libraries such as Citadel over
Apple's SwiftNIO SSH provide authenticated channels and streamed command output,
but direct OpenSSH `direct-streamlocal@openssh.com` support is not a safe
assumption. The spike must prove one full-duplex, binary-clean route. Candidate
routes, in preference order, are:

1. direct SSH local-to-remote Unix-socket forwarding;
2. a non-PTY SSH exec/session channel running a small host bridge whose stdin
   and stdout proxy the existing Unix socket; or
3. a narrowly restricted SSH subsystem that provides the same proxy.

The proof must reject any route that adds a Scufris TCP listener. It must pin or
validate the host key, keep the client key in Keychain, preserve message bounds,
and close cleanly when iOS suspends the app. Do not use an interactive PTY for
protocol bytes unless the proof demonstrates that line discipline cannot alter
them.

Treat the first iOS app as foreground-only. When suspended, it disconnects and
holds no presence. There is no background keepalive, push notification, or relay
requirement.

### iOS voice

Voice remains surface-local. The service sees submitted text and `speak`, never
audio.

Input has two credible designs to compare after text transport works:

- capture WAV on the phone and reach the host's loopback Whisper-compatible
  endpoint through SSH; or
- transcribe on-device and submit only the resulting text.

The host currently runs its bundled Whisper endpoint on loopback, normally
`127.0.0.1:10302`, and the companion accepts a configured endpoint. Reusing the
host model avoids a second model and keeps results aligned, but adds an audio
transport and latency. On-device transcription avoids sending audio and may
work without the host endpoint, but adds a model or depends on iOS speech
availability and behavior. This spike should select a direction, not build both.

Output can use native iOS speech synthesis for `speak`. Protocol v4 presence
must prevent both laptop and phone from speaking the same paragraph. Starting a
phone recording must visibly indicate microphone use and cut phone-local speech,
matching the desktop's barge-in rule.

### Personal iOS distribution

A native personal app is possible without public App Store release, but signing
is operational work:

- A free Xcode Personal Team permits personal on-device testing. Apple states
  that App IDs, registered test devices, and provisioning profiles expire after
  seven days, requiring periodic rebuild and reinstall.
- Apple Developer Program membership is USD 99 per membership year and enables
  app distribution and fuller capabilities.
- TestFlight builds can be tested for up to 90 days.

For one personal phone, a paid development/ad hoc installation on registered
devices is likely less disruptive than free weekly reprovisioning. TestFlight is
useful for review builds but is not permanent installation. The spike must
record the chosen route and the update procedure.

## Questions this spike must answer

1. What operating system, display stack, and architecture does the second
   laptop use?
2. Does the documented OpenSSH Unix-socket forwarding command work end to end
   with the existing companion when the host companion is stopped?
3. What unit or wrapper reliably owns tunnel startup, stale socket cleanup,
   reconnect, and shutdown?
4. Can an iOS Swift proof hold a full-duplex, non-PTY SSH channel to the service
   socket without any network listener in Scufris?
5. Which SSH library and authentication method will be maintained and small
   enough for a personal app?
6. How does a foreground phone claim and release presence without making the
   service know it is a phone or remote?
7. What exact stable mismatch exchange lets an old app receive the host's
   update notice when all newer messages are incompatible?
8. Does phone voice use the host Whisper endpoint through SSH or on-device
   transcription?
9. Which iOS signing and update route will be used?
10. Is protocol v4 still one service file plus the shared Rust and TypeScript
    protocol definitions, or has the current tree changed that estimate?

## Proposed implementation tasks after the spike

Create these as separate tasks only after this spike is reviewed. Keep the
order and dependencies explicit.

1. **Release and deploy v0.5.0**
   - Satisfy the existing implementation gate.

2. **Protocol v4: multiple frontends and presence**
   - Remove frontend eviction while retaining one agent.
   - Fan out transcript and state.
   - Route speech, widget commands, and window requests to presence.
   - Infer presence from submit and support an explicit claim message.
   - Register the union of widget tools and activate only the presence holder's
     catalog, deferring activation changes at turn boundaries.
   - Refuse widget races when the selected surface cannot render one.
   - Freeze and test the version handshake and update notice.
   - Add focused two-frontend integration tests, including reconnect, slow
     client removal, concurrent submit, presence movement, and catalog changes.

3. **Run Scufris desktop from a second Linux laptop**
   - Package the existing companion for the target laptop.
   - Add the supervised SSH Unix-socket tunnel and runtime directory.
   - Keep microphone, transcription, and speech local to that laptop unless a
     reviewed deployment explicitly forwards its Whisper endpoint.
   - Verify laptop/host handoff, disconnects, sleep/wake, and service restart.

4. **Spike an iOS SSH transport to the Scufris Unix socket**
   - If the proof is not completed in this task, promote it as the next blocking
     task before any iOS UI.
   - Prove Tailscale reachability, SSH host-key validation, Keychain key storage,
     a full-duplex bridge, disconnect, and reconnect.

5. **Build the iOS text surface**
   - Transcript, state, submit, abort, reconnect, and bounded protocol parsing.
   - Foreground presence claim and release.
   - Protocol mismatch and plain update notice.
   - No widgets, background notifications, public relay, or second conversation.

6. **Add voice to the iOS surface**
   - Microphone permission, visible recording state, transcription, editable
     result, and submit.
   - Native local speech for `speak`, mute, and barge-in.
   - Test presence changes during recording and playback.

7. **Package and operate the personal iOS app**
   - Signing, provisioning, installation, updates, Tailscale enrollment, SSH key
     rotation, host replacement, and recovery documentation.

## Sources checked

- Settled local design: `tasks/20260828-170154/TASK.md`
- Current service protocol: `docs/src/dev/service.md`
- Current desktop behavior: `docs/src/dev/desktop.md`
- OpenSSH `-L local_socket:remote_socket` forwarding:
  https://man.openbsd.org/ssh
- Tailscale installation on iOS:
  https://tailscale.com/docs/install/ios
- Tailscale SSH platform and client notes:
  https://tailscale.com/kb/1193/tailscale-ssh
- Apple membership and Personal Team limits:
  https://developer.apple.com/support/compare-memberships/
- Apple TestFlight overview:
  https://developer.apple.com/help/app-store-connect/test-a-beta-version/testflight-overview
- Citadel Swift SSH API, as one candidate rather than a decision:
  https://github.com/orlandos-nl/Citadel

## Completion criteria

- The target laptop platform is recorded.
- The laptop Unix-socket tunnel is demonstrated or rejected with exact evidence.
- The iOS SSH-to-Unix-socket transport is demonstrated or rejected with exact
  evidence. No Scufris TCP listener is introduced.
- Presence, version mismatch, voice transport, and iOS distribution decisions
  are recorded.
- The implementation task list is revised from evidence, sized, ordered, and
  ready for creation.
- Review concludes with a clear go/no-go for the Linux laptop surface and the
  personal iOS app.
