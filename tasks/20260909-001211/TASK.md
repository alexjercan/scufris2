# Review Scufris master for what blocks or slows its use

- STATUS: CLOSED
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

Six architectural questions were put to him on 2026-09-09. He asked for 3, 4
and 6, and asked for 1, 2 and 5 to be explained and decided. All six are now
settled and none of them needed a major version.

| #   | Question                                                 | Answer                                                                       | Commit    |
| --- | -------------------------------------------------------- | ---------------------------------------------------------------------------- | --------- |
| 1   | Restore the `refusal` module, or delete the contract?    | Restore. 21 literals over two languages, compared by literal at the far end. | `474a7fa` |
| 2   | A control verb to restart a service at `MAX_FAILURES`?   | No. The tray already restarts it; the detail now says so.                    | `474a7fa` |
| 3   | Give `Surface::copy` a `Result`?                         | Yes, and the page half besides, which is where the refusal actually is.      | `474a7fa` |
| 4   | Call `orphans` at session start; let `recover` adopt?    | Call it and say what it found. No adoption.                                  | `936c7ce` |
| 5   | Where per-profile briefing bounds live?                  | A generated file Home Manager writes, under the environment.                 | `ad6b97f` |
| 6   | Exempt a widget whose backend is working from the sweep? | Yes, through one reserved reading key, capped at four hours.                 | `96fb4df` |

What each answer turned on:

- **1** was never architectural. The codes were literals at 21 send and match
  sites across Rust and TypeScript, and `attachments.ts` compared one by
  literal, so a rename on the Rust side would have stopped matching and said
  nothing. Now one module, one mirror, one test that reads both.
- **2** would have cost a `SERVICE_VERSION` bump: the control socket shares
  version 6 with the agent and surface channels, so a new verb moves the
  desktop, the phone and `scufris-ctl` in lockstep, for a state that only
  happens when the agent has crashed three times inside ten seconds each - when
  something is broken enough that it must be fixed before a restart is worth
  anything. The tray's "Restart backend" already runs
  `systemctl --user restart scufris-service`. Both terminal states now say so.
- **4** kept the report and dropped the adoption. Adoption means taking
  ownership of jobs whose capabilities belong to another session, which is the
  thing the capability model is for. `orphans` now carries the tmux session
  name, which is what a person can act on.
- **5** turned out to have a second half: the `scufris_briefing_run` tool held
  the helper to its own 30-minute timeout, so raising the helper's deadline
  alone would have moved the kill from one place to another. The tool reads the
  same generated file.
- **6** needed a bound. A reading may carry `_hold`, the one key in a reading
  that is the companion's rather than the widget's; a hold nothing ends is
  capped at four hours, so a backend that says "not yet" and dies cannot own a
  slot for the day.

Also fixed on the way: `b32dd6e` removed the service unit's `RuntimeDirectory`
and left `nix/checks/service.nix` asserting it, which `nix flake check` catches
and nothing else does.
