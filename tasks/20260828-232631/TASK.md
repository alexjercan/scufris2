# Spike remote laptop and iOS surfaces

- STATUS: IN_PROGRESS
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

### iOS build environment decision (2026-08-29)

Alex has no Mac with Xcode and decided not to build the iOS demo now. Nix can
provide a Swift compiler on Linux, but it cannot provide Apple's proprietary iOS
SDK, Xcode build tools, device signing, provisioning, or direct installation to
an iPhone. A Linux-only build would therefore not produce a testable native iOS
app. Cloud macOS is possible but makes signing and interactive device testing
awkward and is not justified for this disposable proof.

The transport research also narrowed the library choice. Citadel 0.12.1 is
maintained and supports non-interactive command execution and streamed command
output. Its documented high-level client API only streams command stdin through
a PTY or TTY, so it cannot by itself provide the required binary-clean,
full-duplex exec bridge. It remains suitable for a command-based demo using
`scufris-ctl send` and `scufris-ctl watch`; a direct protocol client needs a
lower-level SwiftNIO SSH channel or another proved library.

Outcome: defer the native app and its SSH transport proof until a Mac with Xcode
is available. Do not add an unbuildable iOS project to this repository.

### iOS interaction design (2026-08-29)

Interactive design: `ios-app-design.html` in this directory. The prototype
loads the same vendored `thinking-orbs` engine and state-to-orb mapping as the
desktop pill rather than carrying a second approximation.

The phone is one screen rather than a pill page, a review page, and a HUD page.
The conversation occupies the available height. A compact typed composer and
the orb share one bottom interaction workspace. Holding the orb records,
releasing it transcribes, and the editable review box expands in that workspace
without hiding the conversation. The orb keeps the desktop state grammar and
barge-in rule. Typed input remains available while Scufris works so it can
steer, and a visible stop action ends the run.

The phone has no widget runtime. It announces an empty catalog, and when it
holds presence the service must make widget tools inactive so the agent answers
in ordinary conversation text and local speech. Desktop widgets remain host
processes on host monitors.

Separate observations were intentionally parked rather than folded into this
design: the current four-widget limit, Claude usage polling eventually receiving
HTTP 429, Dashboardd replacement coverage, and general desktop polish. They
need use evidence and separate work if promoted.

### Presence lease research (2026-08-29, proposed)

Implementation plan: `MULTI_SURFACE_PLAN.md` in this directory. Work is paused
before implementation and the foreground-versus-first-touch trigger is not yet
settled.

"Sleep" must mean passive, not disconnected. Every connected surface continues
to receive state, transcript, and notices. Exactly one active surface receives
speech, widget commands, and conversation-window requests. It is therefore a
presence lease in the service and an `active` or `watching` state in the UI;
calling a watcher asleep would hide that it remains current and can still submit
or abort.

A connection-only lease is too sticky, and a submit-only lease is too late for
unprompted contact. The proposed acquisition rule combines lifecycle and
intent:

- the first frontend becomes active;
- a phone claims when its app enters the foreground;
- either surface claims on deliberate Scufris interaction: microphone start,
  composer or HUD activation, or desktop activation gesture;
- every submit also claims as a final invariant; and
- a claim is revocable immediately by a newer claim and never blocks one.

A foreground event is only a claim, not ownership for the app's whole lifetime.
If the desktop hotkey or HUD is used while the phone stays open, the desktop
claims presence back. Opening the phone to read briefly moves output there;
touching Scufris on the desktop moves it back.

The service should keep claimants in recency order. Claim moves a connected
surface to the top. Explicit release, backgrounding, SSH loss, or frontend
disconnect removes it; the most recent still-connected claimant resumes. Thus a
phone foregrounds over the desktop and the desktop wakes automatically when the
phone backgrounds, without naming either one as local, remote, primary, or
fallback. If no claimant remains, there is no side-effect destination until a
surface acts. Do not add a timer: expiry during a long visible answer would move
speech unexpectedly, while connection loss already gives the lease a bounded
lifetime.

Protocol v4 needs explicit `claim` and `release` requests plus a pushed presence
update so every UI knows whether it is active or watching. The service can still
infer claim from `submit`. Connection IDs are sufficient for lease ownership;
no remote bit or device class is needed. A stable surface identifier may be
useful for diagnostics but must not decide routing.

The implementation shape remains bounded but is larger than removing frontend
eviction:

- retain all frontends and their individual widget catalogs;
- add recency and one active frontend to service state;
- fan state, transcript, and notices out to all frontends;
- route speech, widgets, and conversation requests only to presence;
- remove a disconnected holder and restore the previous connected claimant;
- make desktop activation paths claim without making ordinary transcript replay
  claim;
- make phone foreground claim and background release, with disconnect as the
  authoritative fallback; and
- defer active widget-tool changes to the turn boundary while preserving the
  catalog of a widget already executing.

The unresolved product fork is whether merely foregrounding the phone should
claim, or whether the first touch inside Scufris should claim. Foreground claim
is recommended: the whole app is a Scufris surface, iOS provides a reliable
foreground transition, and any later desktop interaction can revoke it. A
touch-only claim avoids moving output when the app is opened only to read, but
makes passive reading disagree with where unprompted speech and attention go.

## Hostname and private reachability finding (2026-08-28)

The website name and the private machine name are separate DNS records. They do
not require the machine to serve the public website.

Recommended topology:

```text
alexjercan.dev, www.alexjercan.dev  -> GitHub Pages
nixos.alexjercan.dev                -> the host's stable Tailscale IPv4
                                            |
iPhone with Tailscale -> SSH over WireGuard -> host Unix socket bridge
```

`alexjercan.dev` can be the GitHub Pages custom domain. The existing website is
already in `alexjercan/alexjercan.github.io`, but it has no `CNAME` file and its
generated canonical URLs still use `https://alexjercan.github.io/`. Moving it
requires both the GitHub Pages custom-domain setting and source/template URL
changes. GitHub recommends verifying the domain with a DNS TXT record before
attaching it. The registrar may also host DNS, but keeping the registrar and DNS
provider separate is valid.

`nixos.alexjercan.dev` should not point to the router's public address and should
not expose port 22 by router forwarding. A public DNS A record may contain the
host's stable Tailscale `100.64.0.0/10` address. The record is visible publicly,
but it does not make the address publicly routable: only an authenticated member
of the tailnet can reach it. This gives the app the requested branded hostname
without putting Scufris or SSH on the public Internet. The simpler alternative
is to use the host's Tailscale MagicDNS `*.ts.net` name and reserve the bought
domain for the website.

A CNAME from `nixos.alexjercan.dev` to a MagicDNS name is not the preferred
plan. MagicDNS is tailnet-local, resolver behavior through a public CNAME is an
extra dependency, and it discloses the tailnet DNS name. Split DNS can keep the
record private, but needs a separately reachable DNS resolver and is unnecessary
for one stable host.

The `.dev` registry is HSTS-preloaded. All browser-facing pages therefore need
HTTPS. GitHub Pages can provision that certificate for the website. This does
not affect SSH to the private subdomain; SSH authenticates the pinned SSH host
key, not a Web PKI certificate.

Tailscale is preferable to the other deployment shapes:

- Direct SSH port forwarding needs router control, dynamic DNS or a static
  public IP, source filtering, and continuous hardening. It exposes an attack
  surface and may fail behind carrier-grade NAT.
- Cloudflare Tunnel plus Access is useful for an HTTP application, but the
  settled phone design is SSH-to-Unix-socket, not a browser application. It
  adds a public relay and conflicts with the no-public-endpoint constraint.
- The host already runs LogMeIn Hamachi, but the phone plan needs a maintained
  iOS path and the prior design already selected Tailscale as the candidate.
  Do not run both indefinitely without a reason.

The NixOS host is close but not ready for this deployment. OpenSSH is active on
all addresses and allows only user `alex`, but its effective configuration still
has password and keyboard-interactive authentication enabled. Before remote use,
bind exposure to the tailnet with the firewall, use a dedicated phone key held
in iOS Keychain, pin the host key, disable password and interactive login, and
restrict that key to the socket bridge command if the Swift transport uses an
exec channel. Scufris keeps its mode-0600 Unix socket and opens no TCP port.

The app can use `nixos.alexjercan.dev:22` after Tailscale connects. DNS is only a
name; the actual phone transport proof, protocol v4, foreground presence, and
personal iOS signing work remain as specified below.

The name's availability and live registrar prices were not verified because DNS
and outbound name resolution were unavailable in this research environment.
Check the exact name at an ICANN-accredited registrar before choosing the final
label. Do not use registrar search results as proof until registration succeeds.

### Proposed rollout

1. Register `alexjercan.dev`, enable registrar MFA and transfer lock, and keep
   renewal enabled.
2. Verify the domain in GitHub. Configure the apex and `www` records for GitHub
   Pages, set the Pages custom domain, then enable HTTPS.
3. Enroll the host and iPhone in one locked-down Tailscale tailnet. Do not enable
   public Funnel. Use ACL grants so only Alex's devices can reach host TCP 22.
4. Harden NixOS SSH and its firewall. Add and test a dedicated phone key before
   disabling password authentication.
5. Choose either the MagicDNS hostname or public `nixos.alexjercan.dev` A record
   to the stable Tailscale IPv4. Test it on iPhone cellular data, not only Wi-Fi.
6. Prove the non-PTY SSH bridge to the Scufris Unix socket. Then continue the
   protocol and iOS tasks already ordered in this spike.
7. Document recovery for a lost phone, key revocation, tailnet removal, host-key
   replacement, domain renewal, and DNS-provider loss.

### Additional official references

- GitHub Pages custom domains and DNS records:
  https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site
- GitHub domain verification:
  https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/verifying-your-custom-domain-for-github-pages
- Tailscale device IP stability:
  https://tailscale.com/kb/1033/ip-and-dns-addresses
- Tailscale DNS and MagicDNS:
  https://tailscale.com/kb/1054/dns
- Tailscale access control grants:
  https://tailscale.com/kb/1324/grants
- `.dev` HTTPS/HSTS policy:
  https://get.dev/

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
