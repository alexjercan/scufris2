# Scufris on more than one surface: a phone and a laptop as clients

- STATUS: CLOSED
- PRIORITY: 65
- TAGS: architecture, service, protocol

## Design (2026-08-28)

Design page: `second-surface.html` in this directory. Published copy:
https://claude.ai/code/artifact/d57cf9cb-92d1-46dc-a8c2-a35a719fdfc8

Follows `20260827-081702`, "Scufris as a service: the architecture
inversion", and revises its L1.

## Ask

From Alex, 2026-08-28: a phone app that is "just a new surface, basically
just the pill as a phone app", and a laptop connecting to the NixOS host
as a client, each with its own pill, sharing one conversation. Does that
break the one-user rule, and what does it cost in the code?

## Gate

**Nothing here starts before v0.5.0 is tagged and deployed.** The tree is
forty-four commits ahead of `v0.4.0` with the service, `scufris-ctl`, the
conversation window, the widget layer and the three journal panels all
unreleased. Protocol v4 on top of an unshipped v3 makes the release
harder for no gain.

## Finding

The one-user rule is not what breaks. L1 bundles five separate claims -
one person, one conversation, one host, no authentication, one surface.
The first three are untouched by a second surface. The fourth survives if
the transport authenticates. Only **one surface** breaks, and it does not
break gracefully.

"One host" is on the untouched side, corrected on the page after Alex's
comment on 2026-08-28: it was two claims wearing one name. The machine
that runs Scufris stays singular - the rig keeps the agent, the session
and the service, and stays the only thing that runs `pi --mode rpc`. What
leaves is only "the one device involved at all", which was never the same
claim. A surface is a client, and a client was never the host.

Half the work is already built. `push_frontends` is a loop over every
frontend, the 200-entry transcript ring is replayed to each connecting
one, a slow client is dropped rather than blocking, a concurrent submit
is already delivered as a steer, and a user line reaches the ring off
Pi's `MessageEnd` event rather than off the submit path - so typing on
one surface already shows on another. Control clients are already
unlimited.

What does not work, all in `native/scufris-service/src/service.rs`:

- `register:518` evicts the previous client in the same role. Combined
  with `link.rs` reconnecting on a bounded backoff, two companions
  **livelock**: each reconnect kicks the other off, forever, at roughly
  the five-second backoff ceiling, clearing both conversation windows on
  every cycle. This is the first thing anyone hits, and it looks like a
  broken socket forward rather than a deliberate eviction.
- `speak:601` broadcasts. Two devices say one paragraph, out of phase.
- `relay_widget:609` broadcasts, so one command gets two reports at
  `relay_report:660`.
- The catalog at `relay_report:673` is last-writer-wins, so a phone with
  no widget shelf would take the agent's widget tools away until the
  desktop reconnected.
- `relay_conversation:656` broadcasts a window raise to a device in your
  pocket.

## Proposed laws

- **L1 narrowed** to "one person, one conversation, one host". One agent,
  one session, one transcript, no tenancy, and the rig is the only thing
  that ever runs the agent. This is the part that was doing the work and
  it is untouched. Exactly one clause leaves: "one surface".
- **L5 Scufris does not know where you are.** Reframed on revision 2 from
  Alex's comment: "Scufris should not even realize we call it from a
  different machine. It should just be: I am Scufris, I am running on
  NixOS, I respond to whatever input I get." Nothing in the service
  distinguishes a local client from a remote one - no `remote` flag, no
  local-or-remote branch, no capability keyed to where a connection sits.
  This is a tripwire as well as a law: a future design that needs to know
  whether a client is remote is breaking L5, and the design is what to
  fix. The security clause then falls out rather than being argued for -
  the service keeps binding a 0600 Unix socket and authenticating
  nothing, whatever crosses a network is WireGuard's, Tailscale's or
  ssh's problem, and Scufris never opens a TCP listener under any
  conditions. `server.rs:5` stays literally true.
- **L6 Any number watch, exactly one attends.** State and transcript to
  every surface; speech, widget commands and window raises to the one
  holding presence.
- **L2 stays whole.** Settled by Alex on revision 2. A phone app is the
  first component that cannot be rebuilt in lockstep with its host, and
  the answer is to state the coupling rather than soften it: one version
  on the host, and a client that is behind is told so. Two obligations
  came with the decision. Scufris announces the mismatch unprompted -
  "update the phone app when you can" - through the road it already has
  for saying things, rather than leaving an error screen to be found.
  And **the version handshake is frozen forever**: `hello`, `welcome`
  and the version refusal must keep working across every mismatch there
  will ever be, because they are the only messages that still run when
  nothing else does. Everything above them stays free to change, which
  is what L2 is for. Get that backwards and the notice is the first
  casualty of the change it exists to announce.

## Decisions

- **D1 SETTLED: narrowed.** L1 keeps one person, one conversation and
  one host; exactly one clause leaves it. L5 and L6 land with it, so the
  security clause is restated rather than dropped.
- **D2 SETTLED (revision 2): A, L2 stays whole.** One version on the
  host; a client that is behind is told so plainly. The two obligations
  it created are in the laws above.
- **D3 SETTLED: infer now, claim later.** Presence follows the last
  surface to submit, which needs no new client message. An explicit claim
  is added when a surface exists that knows its own foreground state - a
  phone app does, an X11 window mostly does not.
- **D4 SETTLED: register the union, activate the presence holder's.**
  The blocking check is done. `docs/extensions.md:1342`:
  `pi.registerTool()` "works both during extension load and after
  startup ... callable by the LLM without `/reload`", and beside it "use
  `pi.setActiveTools()` to enable or disable tools ... at runtime".
  Registration and activation are separate levers, which is the shape
  this wanted: the registered set never churns, and the active set is
  what the surface you are looking at can actually draw. Neither option
  offered in revision 1 was right - the union alone offers panels that
  cannot be placed, and the presence holder's catalog alone re-registers
  tools every time you pick up your phone. One constraint on the seam: a
  `setActiveTools` call made during a tool's own execution must be
  additive, so presence that moves mid-turn defers its activation change
  to the turn boundary. Keep the `relay_widget` refusal anyway, for the
  race between the model choosing a widget and the command landing.
- **D5 SETTLED: deferred on purpose.** Rung 3 gives Scufris on the
  laptop with everything working, and rung 4 is most of the remaining
  cost. Decided after living on rung 3 - a decision not to decide yet,
  rather than an open question.

## The ladder

1. **No code.** `ssh den scufris-ctl watch` and `scufris-ctl send`.
   Control clients are already unlimited. Reachable tonight.
2. **Tunnel and forward, still no service code.** OpenSSH forwards Unix
   sockets, and `SCUFRIS_RUNTIME_DIR` already moves a whole stack onto a
   different socket - the staging mechanism is the remote-surface
   mechanism. This rung is where the livelock bites; it is a diagnosis,
   not a destination.
3. **Multi-frontend.** Drop the eviction, add presence, settle the
   catalog. Protocol v4. One file plus a field in `hello`.
4. **The phone.** Transcript, submit, state, microphone. Plus D2, which
   becomes real when the first build leaves the machine.

The laptop runs the real `scufris-desktop` unchanged over a forwarded
socket, so rung 3 needs no new user interface. The phone needs rung 3
plus a new surface plus the version discipline. That order proves the
protocol work against something rebuildable before anything unrebuildable
depends on it.

## Not doing

- Any TCP listener, under any conditions. L5.
- Accounts, tenancy, or a second conversation. L1's surviving half.
- Widgets on the phone. They are host processes on host monitors.
- Reachability from outside the tunnel: no public endpoint, no relay, no
  push road. A phone with no tunnel has no Scufris, which is correct.
- Session sharing inside Pi. Already answered in `20260827-081702`: RPC
  mode is bound to one client for its lifetime. Surfaces multiply in
  front of the service, never behind it.

## Outcome (2026-08-28)

All five decisions settled across revisions 2 and 3, and the design page
records each one where it applies rather than only in the decision list.

Two of them were changed by Alex rather than merely approved, and both
changes made the design smaller or sharper:

- L1 keeps "one host". It was two claims wearing one name - the machine
  that runs Scufris, and the only device involved at all - and only the
  second leaves. The rig stays the only thing that runs `pi --mode rpc`.
- L5 is Alex's sentence rather than mine: "Scufris should not even
  realize we call it from a different machine." That replaced a security
  argument with an invariant, and gave the design a tripwire it did not
  have.

One decision was settled by reading rather than by choosing: D4's
blocking question about Pi is answered, and the answer was better than
either option on offer.

Implementation is a separate task, after v0.5.0. Nothing here is built.

## Completion criteria

Met. D1 to D5 are settled on the page and the outcome is recorded
above.
