# Review Scufris master for what blocks or slows its use

- STATUS: OPEN
- PRIORITY: 80
- TAGS: review

## Scope

A standing review of Scufris as it stands on `master` at `db32817`, not a
review of one change. No range, no changed-line budget, no `--live` lane and
no X display.

Alex asked for it after two findings on 2026-09-08 that were the same shape:
the machinery and the thing driving it disagreed about what was possible, and
he paid for it.

- The foreground acknowledgment gate blocked every tool except the final
  response after a successful action. He asked for a job, said "open the
  brief" while it started, and got "I still need to open today's brief in a
  separate action". Fixed in `db32817`.
- A briefing source ended its one-shot `claude --print` turn with "I'll wait
  for the two remaining G1 lanes". The lanes died with the process and 1676
  seconds produced nothing. Fixed by guidance in `315b931`.

What this review is for: more of that shape. Things that block Scufris or
make it awkward to use, ranked by what they cost him rather than by how
interesting they are.

## Method

Two reviewer agents at a time, over eight components. Every agent reads
`.agents/skills/scufris-review/lanes/reviewer.md` and the run brief. Agents
are read-only; adjudication happens in the session that dispatched them.

| G   | Component                             | Paths                                                                   |
| --- | ------------------------------------- | ----------------------------------------------------------------------- |
| G1  | Agent extension                       | `agent/extensions/scufris/**`, `agent/skills/**`                        |
| G2  | Host service and control protocol     | `host/service/src/*.rs`, `shared/control/**`                            |
| G3  | Surface gateway, attachments, content | `host/service/src/bin/**`, `attachment.rs`, service attachments         |
| G4  | Briefings                             | `tools/briefing/**`, `agent/extensions/scufris/briefing/**`             |
| G5  | Jobs helper                           | `tools/jobs/scufris-jobs`                                               |
| G6  | Desktop core                          | `surfaces/desktop/src/*.rs`                                             |
| G7  | Desktop widgets, UI, den              | `surfaces/desktop/{src/widgets,ui,widgets,shell,backends}`, `tools/den` |
| G8  | End to end and deployment             | cross-cutting seams; `nix/**`, `flake.nix`, `RELEASE.md`, CI            |

G8 is not a directory. It follows whole paths across components: a surface
message from keypress to spoken answer, a briefing from timer to published
page to wake, a job from spawn to pane to event to land, and whether a landed
fix actually reaches the running system.

## Rules for what happens after

- Fix what is solvable without asking. Do not stop for confirmation on those.
- Keep architectural findings - anything that would change how Scufris works,
  the kind of change that warrants a major version - for Alex to decide.
  Collect them and ask at the end of the session, never mid-run.
- Findings are recorded here as each group is adjudicated, so a session that
  ends early still leaves the evidence behind.

## Findings

Appended per group as each is adjudicated.

All eight groups ran and were adjudicated. Every claim below was checked
against the source before it was acted on; claims that did not survive that
check are recorded as rejected rather than dropped.

### Fixed

| Commit    | Group  | What blocked him                                                                                                                 |
| --------- | ------ | -------------------------------------------------------------------------------------------------------------------------------- |
| `7a87fae` | G6, G7 | A widget name, a stray CR, and a cancel each swallowed an answer he had already given.                                           |
| `728d4dd` | G1, G2 | A lost worker event was unrecoverable and the tray said things that were not true.                                               |
| `d677661` | G3     | One bad attachment took the whole assistant down.                                                                                |
| `e814c36` | G4     | A briefing that failed said nothing, and a missed night was lost instead of caught up.                                           |
| `997e057` | G5     | One malformed worker line wedged every job; a refused landing trapped the workflow.                                              |
| `577275e` | G6     | Two states that needed him were not drawn, and the composer ate a message.                                                       |
| `0e563e3` | G7     | An unusable surface ID was fatal at start, and a refused restart was silent.                                                     |
| `19ce35a` | G7     | Journal text could rewrite the journal's own structure, and refusals were dropped.                                               |
| `b32dd6e` | G4, G8 | A failed briefing run repeated every session with no way to close it; `RELEASE.md` did not say to deploy what was released.      |
| `c625e37` | G5     | A `launch` that refused before the harness started published nothing; the job sat at `worker starting` until he noticed by hand. |

The shape recurs. Nine of the ten are the same defect: a failure path that
reports to nobody. Under the service `ExtensionContext.hasUI` is false, a
`warn!` goes to a journal he does not read, and a bare `return` says nothing
at all. The fix each time was to give the failure a reader.

### Rejected after checking

- G8 claimed no job survives a foreground restart. `host/service/src/config.rs:164`
  starts Pi with `--continue`, and the deployed session directory holds one
  file appended from 2026-08-27 to 2026-09-08, so the session ID is stable.
  The real residue is `orphans`, which has no caller (below).
- G8 proposed `RuntimeDirectory = "scufris"` for the service. That is worse
  than removing it: systemd would relax the socket directory to 0755 and
  delete it, and the companion and gateway sockets with it, on every stop.
  Removed in `b32dd6e` with the reason recorded in `nix/home-manager.nix`.

### Left open, ranked

Minor, none of them blocking:

- G5 - `land` and `stop` hold the exclusive workflow lock across up to three
  network measurements.
- G5 - `orphans` has no caller. The documentation describes it as if it were
  used.
- G3 - the gateway has no WebSocket keepalive; `/` and `/surface` answer
  before `authorize` because `WebSocketUpgrade` runs first; the desktop
  preview cache is never cleaned and accepts `..` as a name; the content API
  holds its index mutex across file I/O on a single-threaded runtime.
- G2 - `attention`, `attention_detail`, and `associated_surface` outlive the
  agent session; a spawn failure is terminal with no retry; the service
  reports `idle` with no agent connected; `Event::MessageEnd` and
  `session_file` are unused; an abort is not broadcast; `AttachmentStore::resolve`
  runs a write and fsync under the service mutex; `contracts.md` points at a
  `refusal` module that no longer exists.
- G1 - plannotator review has no listener, timeout, or cancel; receipt
  measurement serializes wakes behind up to 90 s each; calm mode hides every
  tool error and block reason; a dead worker window is recorded and never
  acted on; a swallowed `.catch` in the Quick Review completion handler.
- G8 - `surface_abort` does not check that the aborting surface owns the turn
  and does not tell the owner; `pi` in `peerDependencies` is `"*"` while the
  repository type-checks against 0.84.2 and 0.85.0 is deployed.

### For Alex

Architectural, held back per the rules above:

1. Restore the `refusal` module the contracts still describe, or delete the
   contract text.
2. A control verb to restart a service parked at `MAX_FAILURES`. Today only a
   `systemctl restart` clears it.
3. Give `Surface::copy` a `Result` through the port and the page, so a copy
   that fails is not silently successful.
4. Call `orphans` at session start, and give `recover` a way to adopt a
   previous session's jobs.
5. Where per-profile briefing bounds live, so a hand-run briefing gets the
   profile's numbers: a new generated file, or a user-config schema change.
6. Whether a widget whose backend reports `running: true` - a timer - should
   be exempt from the exhibit sweep. This changes what "exhibit" means and
   would let the model set a real timer.
