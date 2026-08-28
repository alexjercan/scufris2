# Scufris on more than one surface: a phone and a laptop as clients

- STATUS: OPEN
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
- **L5 One trust domain, and never a listener.** The service keeps
  binding a 0600 Unix socket and keeps authenticating nothing. Whatever
  crosses a network is WireGuard's, Tailscale's or ssh's problem.
  Scufris never opens a TCP listener, under any conditions. This keeps
  `server.rs:5` literally true rather than falsifying the security model.
- **L6 Any number watch, exactly one attends.** State and transcript to
  every surface; speech, widget commands and window raises to the one
  holding presence.
- **L2 is under strain.** A phone app is the first component that cannot
  be rebuilt in lockstep with its host. L2 does not break, but it stops
  being free.

## To settle

- **D1** Narrow L1 and add L5 and L6, or refuse the design. Everything
  else is consequence. Recommended: narrow it, and add L5 in the same
  breath so the security clause is not quietly lost.
- **D2** Version policy for a surface that cannot be rebuilt with the
  host. Recommended: keep L2 whole - one version, and a stale app shows
  one screen telling you to update.
- **D3** How presence moves: inferred from the last submit, an explicit
  claim, or both in stages. Recommended: infer now, add the claim when a
  phone exists to send it.
- **D4** The catalog: union at registration, or exactly the presence
  holder's. Blocked on reading whether Pi can re-register tools on a live
  RPC session. Recommended: check first, then prefer the presence
  holder's if Pi allows it.
- **D5** Whether the phone is wanted at all once the laptop works.
  Recommended: decide after living on rung 3.

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

## Completion criteria

Design only for now. This task is complete when D1 to D5 are settled on
the page and the outcome is recorded here. Implementation, if D1 is
accepted, becomes its own task after v0.5.0.
