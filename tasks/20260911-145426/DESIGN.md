# Terminal handoff: the production path

Job `54dd50dc8506`, planning round after commit `8a06007` (the debug lease
proof of concept). Design only. No production code changes were made in this
round beyond removing the attribution trailers from that commit.

## Constraint and target

Hard constraint: modify Scufris only. Pi 0.85 and its package are used as
shipped; every gap is closed with service protocol, launcher, extension, job
ownership, canonical conversation, or recovery changes.

Target, in the user's words: a normal interactive Pi in scufris2 temporarily
becomes the sole Scufris agent; ordinary terminal and phone/desktop turns
share the HUD conversation; durable jobs and briefings remain visible and
controllable across terminal and phone; native Pi Markdown and coding UX
stays intact. Images and rich widgets may remain unsupported.

Non-goals: HUD widgets from a terminal answer, images typed or pasted in the
terminal reaching the HUD, remote (phone) takeover of the agent, two agents at
once, and any change to Pi.

## Evidence this plan stands on

- The proof of concept (`RESEARCH.md`, "Proof of concept: the debug lease").
  Measured: one control verb stops and reaps the managed child; the fence
  refuses stale and unfenced hellos; typed turns and answers reach the HUD
  under `terminal`; release and disconnect both restart the child; a typed
  turn can beat the agent hello, fixed by waiting for `agent.ready`.
- Pi facts from the installed 0.85 docs and `pi --help`: `--session
<path|id>`, `--session-id <id>`, `--fork <path|id>`, `--session-dir`,
  `--continue`; `ctx.newSession`, `ctx.switchSession(path)`, `ctx.fork` exist
  on the command context only ("they can deadlock if called from event
  handlers"); `session_before_switch` and `session_before_fork` can cancel;
  `session_before_compact` can replace the summary; `pi.sendUserMessage(text,
{ expandPromptTemplates: true })` dispatches extension commands;
  `SessionManager.forkFrom(sourcePath, targetCwd, sessionDir)` is public SDK;
  `input.source` is `interactive` for typed text, also in print mode.
- One new measurement this round: from a different working directory,
  `pi --no-extensions --session-dir D --fork FILE -p ...` wrote a new session
  in `D` whose header `cwd` was the process cwd and whose `parentSession` was
  `FILE`, with every entry copied: `model_change`, `thinking_level_change`,
  both `custom` entries (`plannotator`, `scufris-response-v5`), the user,
  assistant, and toolResult messages. The print-mode prompt itself timed out
  at 120 s for an unknown reason; the fork had completed before that, so the
  copy is measured and the prompt behaviour after a CLI fork is a Phase 0
  gate.
- Scufris facts, with file references, are in the "Facts" appendix at the
  end. The body cites them by name.

## The honest boundary: what "the same conversation" means

Three things could be meant by continuity, and Scufris can offer two.

1. **Conversation continuity.** The canonical replay in the service
   (`conversation.json`, `ConversationMessage` under a surface name) is the
   one conversation every surface sees. The terminal joins it as the surface
   `terminal`. This is complete today and stays the contract.
2. **Model-context continuity.** Whether the terminal's model sees what the
   managed child saw, and the child sees what the terminal did. Pi keeps
   model context in a session file whose header `cwd` is fixed at creation
   and is used as the tools' working directory on resume. The service's
   child writes its session with cwd `$HOME`; the terminal wants the repo.
   Opening the service's file in the terminal is therefore wrong, and Pi
   offers no cwd override. What Pi does offer is `--fork FILE` and
   `SessionManager.forkFrom(file, targetCwd, dir)`: a copy of the whole
   session into a new file with a new cwd. So the design passes the session
   **lineage** between holders by forking, never by sharing a file:
   - The terminal forks the child's latest file into one with the repo cwd.
   - On release, the service starts its child with `--fork <terminal file>`,
     which yields a file with cwd `$HOME`, and `--continue` finds it next
     time.
     Model context after a handoff is the full branch up to the handoff:
     messages, tool results, compaction summaries, and the extensions'
     `custom` entries (filed rows, delivered job events, wake mode). What is
     not exact: a fork is a copy, so anything the previous holder appends
     after the copy is lost to the lineage and lives only in the canonical
     conversation; the catch-up message below covers that. Paths in old tool
     results were resolved against the other cwd. This is the best Scufris-
     only boundary and it is stated as such in the docs.
3. **Exact process continuity** (the same model context object, streaming
   state, queued steers). Not achievable without Pi changes; the handoff
   happens only between turns, and the service refuses a grant while the
   child is `working` unless the caller asks to abort it.

Fallback when forking is unavailable (plain `pi` started without the
launcher, a lineage file that cannot be read, or a Phase 0 gate that fails):
**catch-up**. The service hands the joining agent the canonical entries it has
not seen (by sequence number); the extension injects them as one
`custom_message` (participates in LLM context, `display: false`). Text only,
no tool results. Every join path performs catch-up; forking merely makes it
short.

## Roles

| Process           | Session file                       | cwd     | Owner token  | Channel         |
| ----------------- | ---------------------------------- | ------- | ------------ | --------------- |
| Managed child     | service dir, `--continue`/`--fork` | `$HOME` | `foreground` | `agent.sock`    |
| Terminal (leased) | forked into the service dir        | repo    | `foreground` | `agent.sock`    |
| Terminal (alone)  | its own                            | repo    | session id   | none            |
| Worker            | job harness session                | job     | n/a          | job status file |

"Owner token" is the job ownership key defined below. One holder at a time
carries `foreground`; the fence guarantees it.

## State machines

Service, one new field `holder` beside `lifecycle`:

```text
Managed(child) --lease_acquire ok--> Handing(gen) --child reaped, reply--> Terminal(gen)
Terminal(gen)  --lease_release-----> Returning --start child (--fork F)--> Managed
Terminal(gen)  --control closed----> Returning
Terminal(gen)  --3 pings missed----> Returning
Managed        --lease_acquire while working, no abort--> Managed (refused, `agent_busy`)
any            --stopping----------> Stopped
```

`Handing` is the blocking `Agent::stop` window (at most 5 s). The grant is
answered only after the child is reaped, so the terminal may join the agent
channel the moment it reads the reply. `Returning` is `Lifecycle::Starting`
with detail "The agent is restarting." exactly as today.

Terminal extension:

```text
Independent --acquire ok--------> Leased(gen)
Independent --acquire refused---> Independent (notify, no channel)
Leased      --release (exit, /scufris release)--> Independent
Leased      --control closed, agent rejected----> Lost --reacquire loop--> Leased | Independent
Lost        --/scufris release--> Independent
```

In `Independent` the terminal is the orchestrator the research called
Reading A: its own session id owns its jobs, no `agent.sock`, no HUD. In
`Leased` it is the sole agent. `Lost` is `Independent` plus a retry loop with
1 s to 30 s backoff, on by default, off with `/scufris release`.

## Protocol v11

`SERVICE_VERSION` goes to 11 in `protocol.ts` and in
`shared/control/src/service.rs`, and `scufrisProtocolVersion` in the iOS app
with it. Additive verbs are gated by the flag, but the surface state
message changes shape, so the bump is honest. All messages keep
`deny_unknown_fields`, the existing size bounds, and the identifier rules.

### Control channel (local only, `control.sock`)

| Request                                                                              | Response                                                                                                                                   |
| ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `control.lease_acquire {id, holder: {pid, session_file?, cwd}, abort_working: bool}` | `control.lease {id, generation, session_dir, lineage_file?, sequence, owner: "foreground"}` or `control.rejected {id, code}`               |
| `control.lease_ping {id}`                                                            | `control.lease_pong {id, generation}`                                                                                                      |
| `control.lease_release {id}`                                                         | `control.lease_released {id}`                                                                                                              |
| `control.conversation {id, since: sequence}`                                         | `control.conversation_entries {id, entries: [{sequence, role, surface, text}], more: bool}`, at most 64 entries per page, text as recorded |
| `control.state {id}`                                                                 | `control.state {id, state, detail, holder: "managed" \| "terminal" \| "none", generation?}`                                                |

`lineage_file` is the file the service will fork from: the child's last
reported session file, or the last terminal's. `sequence` is the canonical
conversation's latest sequence so the holder knows where catch-up starts.
`abort_working: true` lets the launcher take a child mid-turn; the service
then sends `agent.abort` and waits for `agent_settled` (bounded, 10 s) before
stopping it. Default `false` refuses with `agent_busy`.

Refusal codes added: `agent_busy`, `lease_ping_stale`. Renamed from the PoC:
`lease_disabled` stays; `lease_held`, `lease_required`, `not_lease_holder`
stay.

### Agent channel (`agent.sock`)

| Message                                                                | Direction | Meaning                                                                                                                                                        |
| ---------------------------------------------------------------------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `agent.hello {lease?, session?: {id, file, cwd}}`                      | to host   | The fence as built; `session` lets the service track the lineage file for both holders. The child sends it too, replacing the BOOT `get_state` file read.      |
| `agent.session {id, file, cwd}`                                        | to host   | After `/tree`, `/fork`, `/new` (when allowed), or `session_start` with a new file.                                                                             |
| `agent.turn {id, text, images: u8}`                                    | to host   | A turn typed in the terminal. Replaces `agent.lease_turn`. `id` is minted by the extension (`t-` + 16 hex).                                                    |
| `agent.turn_ack {id, sequence}` / `agent.rejected {id?, code, detail}` | to agent  | The turn is recorded under `terminal` at that sequence, or refused (`not_lease_holder`, `agent_busy` never: a typed turn always runs locally).                 |
| `agent.activity {working}`                                             | to host   | Replaces `agent.lease_activity`. Sent by any agent; the child's RPC stdout stays authoritative for the child.                                                  |
| `agent.response {…, turn_id?, proactive_id?}`                          | to host   | `turn_id` closes a terminal turn; `proactive_id` closes a briefing; neither is unprompted. Exact correlation replaces the single-slot rule (see below).        |
| `agent.handoff {generation, next: "terminal" \| "managed"}`            | to agent  | Sent before the service stops or drops this agent for a handoff. The extension skips `suspend-owner` on the shutdown that follows and appends a lineage entry. |
| `agent.catch_up {since, entries: [{sequence, role, surface, text}]}`   | to agent  | Sent after `agent.ready` when the agent's declared session is not the lineage file the last entries were made in. The extension injects one custom message.    |

`agent.jobs`, `agent.job_command`, `agent.offer_take`, `agent.message`,
`agent.wake`, `agent.abort`, `agent.proactive_*` are unchanged.

### Surface channel (`surface.sock`, also through the gateway)

- `state {state, detail, holder}` gains `holder: "managed" | "terminal"`. The
  desktop and iOS show it as a small label; nothing else changes.
- Conversation messages already carry `surface: "terminal"`. No new fields.
- `surface.message` from a surface while a terminal holds the agent works
  unchanged: the service relays it, and the leased terminal's binding turns it
  into `pi.sendUserMessage` with `deliverAs: "steer"` when busy.

### Validation and bounds

`agent.turn.text` bounded by `MAX_TEXT_BYTES` and non-empty as in the PoC;
`images` at most 8; `session.file` absolute, at most `MAX_DETAIL_BYTES`;
`cwd` the same; `holder.pid` a `u32`; `since`/`sequence` `u64`;
`conversation_entries` pages of at most 64 entries and 64 KiB. `terminal`
stays a reserved surface name that no surface may register.

## Submission and response correlation

Today one answer closes whatever `associated_surface` is open, and an
uncorrelated answer while a briefing slot is reserved is dropped. For a
terminal that can type at any moment that rule loses answers. The replacement
is exact ids:

- A terminal turn is `agent.turn {id}`; the service records the user message,
  sets `associated_surface = terminal` and `associated_turn = id`.
- A surface message keeps its `id`; the service sets `associated_surface` to
  the surface and `associated_turn` to that id.
- The response extension attaches `turn_id` to `agent.response` when the turn
  that produced the answer started from an accepted terminal turn (it learns
  the acceptance from `agent.turn_ack`), and `proactive_id` as today when the
  turn carried a proactive custom message.
- Service rule: `proactive_id` present -> briefing path (unchanged);
  `turn_id` present and equal to `associated_turn` -> answer to that turn,
  recorded under its surface; `turn_id` present but stale -> recorded under
  `terminal` with a warning, association untouched; neither -> the owner rule
  as today, and while a proactive slot is reserved, recorded as `unprompted`
  instead of dropped. Dropping stays only for a `proactive_id` that does not
  match the reserved slot.
- HUD abort of a terminal-owned turn: `surface.abort {id}` relays
  `agent.abort` as today (the abort is global in Pi); the association closes.
  The terminal's Esc aborts locally; the extension sends
  `agent.activity {working: false}` and the service closes the association
  when the next turn opens, so a stale open association never blocks a
  briefing for longer than one turn.

## Proactive wakes and briefings

- `control.wake` and `control.briefing` are unchanged and reach whichever
  agent holds the channel; the leased terminal runs the same
  `service/index.ts` wake path (`pi.sendMessage`, follow-up, trigger turn) and
  the same `proactive_started`/`proactive_settled` reporting.
- `dispatch_briefing_now` requires `Lifecycle::Idle`; the terminal supplies
  it through `agent.activity`, sent from `agent_start` and `agent_settled`.
- The briefing extension's tools (`scufris_briefing_run/show/publish/open`)
  run in the terminal against the same `tools/briefing/cli.py` and state
  directory; the collector announces through `scufris-ctl briefing`, so HUD
  briefing rows do not care who holds the agent. `briefing.dismiss` is
  surface-side and unchanged.
- The only briefing-specific change: the reserved-slot rule above, so a
  briefing answer and a typed turn can interleave without a dropped answer.

## Job ownership across the handoff

Fact: `owner_session` in `job.json` is the Pi session id, `recover` adopts
only that id, `orphans` reports the rest, and `session_shutdown` runs
`suspend-owner`. Across a handoff the session id changes twice, so today's
rule would suspend the child's jobs when it is stopped for a lease and show
them as orphans in the terminal.

Design: the owner of a conversation-owned job is the token `foreground`, not
a session id.

- `orchestration.ts` gets `owner()`: the value last set by the
  `scufris:owner` event on `pi.events`, else `SCUFRIS_JOB_OWNER` from the
  environment, else the session id. The launcher (`nix/launcher.nix`,
  `scripts/scufris-agent`) exports `SCUFRIS_JOB_OWNER=foreground` for the
  managed child. `bindService` emits `scufris:owner {owner: "foreground"}`
  after a grant and `{owner: <session id>}` on release or loss.
- On an owner change the orchestration re-runs `recover` for the new owner
  (adopting live panes, rotating the trusted capability, arming watchers) and
  drops the watchers of the old owner without stopping anything. Spawns use
  the current owner. Job commands from the HUD reach whoever holds the
  channel, which is whoever owns `foreground`.
- `suspend-owner` on `session_shutdown` is skipped when an `agent.handoff`
  was received for this shutdown. A crash of either holder leaves foreground
  jobs running until the next holder's `recover` adopts them, which is the
  crash behaviour today.
- `tools/jobs/scufris-jobs` gains `recover --owner foreground --also <session
id>` (adopt and rewrite legacy records once, idempotent) and `migrate-owner
--from A --to B` for rollback. `orphans` wording distinguishes "the
  service's" from "another terminal's" by whether the other owner is
  `foreground`.
- Capabilities stay per job: the trusted capability is rotated on every
  `recover`, so a holder that lost the lease cannot report as the new holder.

Filed rows and delivered event ids live in `custom` session entries and are
copied by the fork, so the HUD's filed set and the no-duplicate-wake rule
survive a handoff. With the catch-up path they do not; the next holder may
re-deliver at most the events since the copy, which the helper's
`event_offset` bounds.

## Session lineage, file ownership, and cwd

- The service records `lineage_file` from `agent.hello.session` and
  `agent.session`. The child's own file comes from the same hello, replacing
  the BOOT `get_state` read of `sessionFile` (kept as a fallback for one
  release).
- Grant: the terminal receives `lineage_file` and `session_dir`. The
  `scufris-terminal` launcher (below) has already started Pi with
  `--session-dir <service dir> --fork <lineage_file>`, so the terminal's
  session is a repo-cwd copy of the child's. A plain `pi` in the repo has no
  fork; it stays on its own session and relies on catch-up.
- Release: the service starts the child with `--fork <terminal file>` when
  the terminal reported a file, else `--continue`. `Config::agent_args` grows
  a `fork: Option<PathBuf>` argument; the child's fork has cwd `$HOME`, so
  `--continue` finds it on the next plain restart.
- Every session file in the lineage is in the service's session directory
  (the launcher passes it), which keeps `/resume` in the terminal listing the
  lineage and keeps backups in one place. A plain `pi` session lives in Pi's
  default directory and is forked from there by absolute path.
- Growth: each handoff copies the branch. Sessions compact, so a file is
  bounded by the context window plus tool results; a `scufris-ctl lineage
prune --keep 30d` verb removes forked files older than the window that are
  not the current lineage file. Off the critical path.
- Trust: the terminal's forked session has the repo cwd, which is trusted
  or prompted once; the child's has `$HOME` with no `.pi`, so no prompt.

## The terminal entry points

1. `scufris-terminal` (packaged launcher, `scripts/scufris-terminal` in the
   tree): asks `scufris-ctl state` for the lineage file, then runs
   `pi --session-dir <service dir> --fork <lineage_file> "$@"` from the
   current directory with `SCUFRIS_TERMINAL=1`. The project extension does
   the rest. This is the recommended path and the one that gives model-
   context continuity.
2. Plain `pi` in a trusted scufris2 checkout with `SCUFRIS_TERMINAL=1` in the
   environment: the project extension `.pi/extensions/scufris-terminal`
   acquires the lease at `session_start`, joins with catch-up, and continues
   on its own session. "Normal interactive Pi", conversation continuity only.
3. `/scufris attach` inside a running plain `pi`: a command, so
   `ctx.switchSession` is available. It acquires the lease, forks the lineage
   file with `SessionManager.forkFrom(lineage, ctx.cwd, sessionDir)`, and
   switches to it. Whether this can also run automatically at startup by
   dispatching the command through `pi.sendUserMessage("/scufris attach",
{ expandPromptTemplates: true })` is Phase 0 gate G3; if it can, plain `pi`
   gets path 1's continuity too.

The `.pi` extension moves to `agent/extensions/scufris/terminal/index.ts`
with the bootstrap in `.pi/extensions/scufris-terminal/index.ts` reduced to
the gate and the composition, so the lifecycle code stays under
`agent/extensions/scufris/` and is packaged. `SCUFRIS_DEBUG_LEASE` becomes
`SCUFRIS_TERMINAL`; `SCUFRIS_DEBUG_LEASE_LOG` becomes `SCUFRIS_TERMINAL_LOG`.

Commands, all on `/scufris`: `status` (holder, generation, lineage file,
owner, channel), `release` (go independent), `attach` (take the lease, fork
when possible), `hold` (stop the reacquire loop).

## Native Pi UX in the terminal

- The response extension's markdown transformer hides streamed assistant
  text and the entry renderer prints the final response. In the terminal
  that is the opposite of native. Terminal mode (`SCUFRIS_TERMINAL=1`) relaxes
  it: streamed Markdown and thinking render natively; tool executions follow
  `/calm` as today.
- The shared answer is the last assistant message text of the turn unless
  `scufris_final_response` was called, in which case the tool's text,
  receipts, offers, and attachments win. The extension sends
  `agent.response` at `agent_settled` from the captured `message_end` when
  no tool response was sent for that turn. The identity prompt gains a
  terminal paragraph: answer natively, the HUD mirrors the final message,
  call the tool only for receipts, offers, or attachments.
- Attachments: `store_attachment` works in the terminal (it posts to
  `content.sock`), so an answer can carry files to the HUD. HUD attachments
  arriving in the terminal render as paths; inline images stay out of scope.
- Widgets: a terminal-owned answer has no registration to validate against;
  calls are dropped and the agent is told through `agent.rejected
{code: invalid_widgets}` as today for surfaces. Unsupported by decision.
- Images pasted in the terminal: `agent.turn.images` carries the count; the
  HUD shows the text with "[n images]" appended by the service. Unsupported
  by decision.

## Slash-command policy

| Input                                                 | Reaches the model | Recorded in the conversation | Note                                                                |
| ----------------------------------------------------- | ----------------- | ---------------------------- | ------------------------------------------------------------------- |
| Typed prose                                           | yes               | yes, as typed                | `input.source == "interactive"`, no leading `/`                     |
| Built-in `/model`, `/compact`, `/session`, `/quit`... | no                | no                           | handled by the TUI before `input`; the trace records nothing        |
| Extension commands `/calm`, `/wake`, `/scufris`       | no                | no                           | dispatched before `input`                                           |
| `/skill:name args`, prompt templates                  | expansion         | yes, the raw line            | the person's words are the line; the expansion is model-facing      |
| `!cmd` shell lines                                    | no                | no                           | Phase 0 gate G4 verifies `input` does not fire                      |
| Steer typed while busy                                | yes               | yes, as a user message       | `streamingBehavior` traced; no steer marker in the canonical record |
| Surface message while the terminal is busy            | yes, as steer     | yes                          | unchanged                                                           |

## Compaction

Local to whichever Pi holds the session; nothing is recorded and nothing
changes on the HUD. `session_before_compact` and `session_compact` stay
traced. The forked lineage carries compaction entries, so a handoff after a
compaction hands over the summary plus the retained tail, exactly what the
previous holder's model saw. The catch-up message is bounded (64 entries,
text only) so it cannot itself trigger compaction on join.

## Session switching while leased

`/new`, `/resume`, `/fork`, and `/clone` are cancelled by
`session_before_switch` and `session_before_fork` while the lease is held,
with a notice naming `/scufris release`. Reason: the lineage must stay a
single chain so the fork-back on release is well defined; a `/resume` into an
unrelated session would hand that context to the managed child. `/tree` is
allowed (same file, the fork-back copies the active branch) and reported
through `agent.session` for the trace. After `/scufris release` the terminal
switches freely as an independent orchestrator.

## Lease fencing, heartbeat, crash and orphan recovery

- Fence: as built. `lease_generation` only counts up; a hello must carry the
  held generation; `agent.turn`, `agent.activity`, `agent.session`, and
  `agent.handoff` acknowledgements are holder-only.
- Heartbeat: the holder sends `control.lease_ping` every 5 s; the service
  ends the lease after 3 missed pings (15 s) as if the connection closed,
  which covers a stopped or suspended terminal that keeps its socket open.
  A terminal that wakes from suspend gets `lease_ping_stale` and reacquires.
- Process ownership: the service stops the child with `Agent::stop` (stdin
  close, SIGTERM at 2.5 s, SIGKILL at 5 s, process group), reaped before the
  grant; the same path ends every generation. Jobs live in the tmux server,
  outside the group, and are unaffected.
- Terminal dies: the control connection closes; the service restarts the
  child with `--fork <terminal file>` (the file is complete up to the last
  appended entry) and the child adopts `foreground` jobs on `recover`.
- Service dies while leased: the terminal sees the control and agent sockets
  close, enters `Lost`, emits `scufris:owner {session id}` (drops foreground
  watchers, keeps its own), and retries acquisition with backoff. The
  restarted service starts its child first (as today), so the reacquire stops
  that child again; catch-up covers what it answered meanwhile.
- Service restart bounds stay: a child that dies three times in ten seconds
  puts the service in `Failed`, and a lease request then gets `agent_busy`
  with the recovery sentence.
- Two terminals: the second gets `lease_held` and runs independent; its
  `/scufris attach` retries later. No queueing of leases.

## Rollout flags, migration, rollback

| Flag                                     | Where                                   | Default | Meaning                                                                           |
| ---------------------------------------- | --------------------------------------- | ------- | --------------------------------------------------------------------------------- |
| `SCUFRIS_SERVICE_TERMINAL_LEASE=1`       | service env / `--terminal-lease`        | off     | offer the lease on `control.sock`; `--debug-lease` stays an alias for one release |
| `programs.scufris.service.terminalLease` | home-manager option                     | false   | sets the flag on the unit                                                         |
| `SCUFRIS_TERMINAL=1`                     | terminal env, set by `scufris-terminal` | unset   | the project extension composes and attaches; unset means inert                    |
| `SCUFRIS_TERMINAL_LOG`                   | terminal env                            | unset   | JSON Lines trace                                                                  |
| `SCUFRIS_JOB_OWNER=foreground`           | launcher env for the managed child      | unset   | conversation-owned jobs                                                           |
| `speakTerminal`                          | desktop config / env                    | false   | speak answers owned by `terminal`                                                 |

Migration:

- Protocol 10 -> 11. All clients are in this repository and deployed from one
  flake; the iOS app pins the constant and must ship with the same change.
  Order: host (service, gateway, desktop) then app; the gateway refuses a
  version-10 app with the existing "update the host and surface together"
  path, so the window is visible, not silent.
- Job records: `recover --owner foreground --also <old session id>` rewrites
  the managed child's jobs to `foreground` once. Rollback:
  `migrate-owner --from foreground --to <session id>`.
- Conversation file: format 1 unchanged; the `terminal` surface name is a
  string like any other.
- Session directory: nothing to migrate; forks are new files.

Rollback: turn the flags off (the service refuses `lease_disabled`; the
extension is inert), or switch to the previous home-manager generation. The
protocol version reverts with the generation. Lineage files created by forks
remain valid Pi sessions in either version.

## Security

- The lease is local only: `control.sock` is 0600 in a 0700 directory, the
  gateway never opens it, and the surface channel cannot express a control
  verb. This stays an invariant with a test beside the gateway binary (`host/service/src/bin/scufris-surface-gateway.rs`).
- Gated off by default at the service; a deployed service offers nothing on
  `control.sock` that it did not before.
- A same-uid process can already read the conversation and `scufris-ctl
wake`; taking the agent adds no new trust boundary. The terminal Pi runs
  with the user's permissions like the managed child.
- Text bounds, identifier rules, and page limits apply to every new message;
  `holder.cwd` and `session.file` are recorded for logs and forks, never
  executed.
- Trust: the project extension loads only in a trusted checkout; the
  launcher passes nothing that bypasses Pi's trust decision.
- Capabilities: per job, rotated on every `recover`; a former holder cannot
  act for the new one.

## TTS

Fact: the desktop speaks live assistant messages whose `surface` equals its
own surface name (`surfaces/desktop/src/main.rs`, `local_presentation`), so a
`terminal` answer is silent everywhere today, and the phone has no TTS at all.

- Desktop: a `speak_terminal` config value (`SCUFRIS_DESKTOP_SPEAK_TERMINAL=1`,
  home-manager `programs.scufris.desktop.speakTerminal`) extends the rule to
  `surface == "terminal"` for live messages only. Default off, because the
  person typing in a terminal is usually at that machine and reads the
  answer there.
- Terminal: no speech. The launcher does not compose a speak hook; a typed
  conversation is a read conversation.
- Unprompted answers stay silent as today; a briefing answered by the leased
  terminal is `unprompted` and follows that rule.
- Phone: unchanged, no TTS.

## Ordered implementation plan

Each phase is one reviewable commit or a short series, lands with its tests
and its docs, and keeps every existing check green (`npm run check`, the
Python suite, `cargo fmt --check`, `cargo clippy`, `cargo test`, `nix flake
check`). Nothing before Phase 6 changes a deployed default.

### Phase 0: metadata and gates

Files: none in the tree beyond `tasks/20260911-120039/`.

1. Attribution cleanup: done in this round by amending `8a06007` into
   `6af196f` (message unchanged, trailers removed). No further step.
2. Measure the gates below and record the results in `RESEARCH.md`. Each gate
   is one script run under the staging harness (`docs/src/dev/staging.md`)
   with the scratch service, so no deployed state is touched.

| Gate | Question                                                                                                                                                       | Pass condition                                                                                                                     | On fail                                                                                |
| ---- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| G1   | Does `pi --session-dir D --fork FILE` followed by a typed turn append to the new file, in TUI and RPC mode?                                                    | The new file gains the turn; `parentSession` is `FILE`; the prompt does not stall (the 120 s timeout seen with `-p` is explained). | Fork-back in Phase 3 uses `--continue` plus catch-up only; lineage is one-directional. |
| G2   | Does `--continue` with `--session-dir D` from cwd `$HOME` find a file whose header cwd is `$HOME` and skip repo-cwd files?                                     | The child resumes the fork-back file, not the terminal's.                                                                          | The service passes `--session <file>` explicitly to the child on every start.          |
| G3   | Can `pi.sendUserMessage("/scufris attach", { expandPromptTemplates: true })` at `session_start` run a command that calls `ctx.switchSession` without deadlock? | The switch happens and `session_start {reason: "resume"}` fires.                                                                   | Plain `pi` gets catch-up only; `scufris-terminal` stays the fork path.                 |
| G4   | Do built-in commands, extension commands, and `!` lines fire `input`?                                                                                          | None do.                                                                                                                           | The `input` handler filters by leading `/` and `!` and a test pins the list.           |
| G5   | Does `session_before_switch` cancel `/new` and `/resume` in the TUI with the notice visible?                                                                   | The session file is unchanged after each command.                                                                                  | The notice moves to `session_start` with a forced `/scufris release`.                  |
| G6   | Does a `custom_message` injected at `session_start` with `display: false` reach the model on the next turn?                                                    | A turn that asks "what did I say before" reflects the catch-up text.                                                               | Catch-up is injected as `deliverAs: "nextTurn"` on the first typed turn instead.       |

Go/no-go: G1 or its fallback decides the shape of Phase 3; G4 and G5 decide
the Phase 2 policies; G3 and G6 decide Phase 4. No gate blocks Phase 1.

### Phase 1: protocol v11 and correlation in the service

Files: `shared/control/src/service.rs` (verbs above, `SERVICE_VERSION = 11`),
`shared/control/src/refusal.rs` (`agent_busy`, `lease_ping_stale`),
`host/service/src/service.rs` (holder state machine, `associated_turn`,
exact-id recording rule, heartbeat deadline, `lineage_file`, catch-up pages),
`host/service/src/server.rs` (ping, conversation, state handlers),
`host/service/src/config.rs` and `main.rs` (`terminal_lease` flag,
`--debug-lease` alias, `fork` argument), `host/service/src/agent.rs`
(`start(fork: Option<&Path>)`), `host/service/tests/lease.rs` and the
`leased-agent` fixture, `agent/extensions/scufris/service/protocol.ts`
(mirror), `surfaces/ios/Sources/Protocol.swift` and
`surfaces/ios/Tests/ProtocolTests.swift` (version and `holder` field),
`surfaces/desktop/src/state.rs` (`holder`), `tests/service.test.ts` (parity),
`docs/src/dev/service.md`, `docs/src/dev/messaging.md`.

Acceptance tests:

- Parity: every refusal code and every verb string is identical in Rust, TS,
  and the Swift test table (extend the existing "named the same on both
  sides" test to the verbs).
- Unit, `service.rs`: a `turn_id` answer closes only its own association; a
  stale `turn_id` records under `terminal` and leaves the association open;
  an uncorrelated answer while a briefing slot is reserved records as
  `unprompted`; a `proactive_id` mismatch is dropped; `abort_working: false`
  against a working child refuses `agent_busy`; `abort_working: true` aborts,
  waits for settle, stops, then grants.
- Unit: three missed pings end the lease and restart the child; a ping after
  the end refuses `lease_ping_stale`.
- Binary, `tests/lease.rs`: grant reply arrives after the child is reaped
  (`waitpid` observed); `agent.hello.session` sets `lineage_file` and the
  next child start receives `--fork <file>`; `control.conversation` pages at
  64 entries with `more`.
- Surface: `state` carries `holder`; a version-10 hello is refused with the
  existing version message.

Go/no-go: all of the above plus `cargo test` for the desktop crate and the
iOS protocol tests.

### Phase 2: the terminal extension

Files: `agent/extensions/scufris/terminal/index.ts` (new; the composition,
state machine, `/scufris` commands, heartbeat, reacquire loop, catch-up
injection, session-switch cancellation, `input` policy),
`agent/extensions/scufris/service/lease.ts` (ping, holder info,
conversation paging), `agent/extensions/scufris/service/client.ts`
(`turn`, `turn_ack`, `activity`, `session`, `handoff`, `catch_up`),
`agent/extensions/scufris/service/index.ts` (turn ids, `agent.activity`
from `agent_start`/`agent_settled`, `scufris:owner` events, handoff-aware
shutdown), `agent/extensions/scufris/response.ts` (native terminal mode,
`turn_id` on responses, last-message answer), `.pi/extensions/scufris-terminal/index.ts`
(replaces `scufris-lease`), `tests/lease.test.ts` renamed to
`tests/terminal.test.ts`, `tests/response.test.ts`, `docs/src/dev/extensions.md`,
`docs/src/reference/environment.md`.

Acceptance tests (fake client, as in the PoC harness):

- A typed turn sends `agent.turn {id}` and the answer of that turn carries the
  same `turn_id`; a turn refused with `not_lease_holder` still runs locally
  and moves the extension to `Lost`.
- `Lost` retries with backoff and stops on `/scufris hold`; `/scufris
release` sends the release and emits `scufris:owner {session id}`.
- `session_before_switch` and `session_before_fork` are cancelled while
  leased and allowed when independent.
- `input` records only interactive prose and `/skill:` lines; `/model`,
  `/scufris`, and `!ls` do not reach the channel (pinned by G4's result).
- The catch-up entries become exactly one `custom_message` with
  `display: false`; an empty catch-up injects nothing.
- Response: in terminal mode the last assistant text of a turn is sent at
  `agent_settled` when the tool was not called; when the tool was called the
  tool's payload wins and no second response is sent.
- `session_shutdown` after `agent.handoff` does not call `suspend-owner`;
  without a handoff it does.

Go/no-go: `npm run check` green; the manual scenarios S1 to S4 below pass on
the staging harness with the Phase 1 service.

### Phase 3: session lineage and the launcher

Files: `scripts/scufris-terminal` (new), `nix/launcher.nix` (package it,
export `SCUFRIS_JOB_OWNER=foreground` for the managed launcher),
`scripts/scufris-agent` (same env), `host/service/src/bin/scufris-ctl.rs`
(`state` prints the lineage file; `lineage prune`), `docs/src/dev/staging.md`
(replace "Drive the conversation from a terminal"), `docs/src/dev/operation.md`
(the terminal section), `tests/structure.test.ts` (launcher shape).

Acceptance tests:

- Launcher test (Bash, under `tests/`): with a fake `scufris-ctl` that prints a
  lineage file, `scufris-terminal` execs `pi` with `--session-dir` and
  `--fork <file>`; with no lineage it execs without `--fork`; it preserves
  `"$@"` and exit codes.
- Binary test: after a release the child is started with `--fork <terminal
file>`; after a plain restart with `--continue`.
- G1 and G2 results decide whether `--session <file>` replaces `--continue`.

Go/no-go: scenario S5 (fork both ways, model recalls a fact across two
handoffs) passes on the staging harness.

### Phase 4: job ownership

Files: `tools/jobs/scufris-jobs` (`recover --owner --also`, `migrate-owner`,
`orphans` wording), `tests/test_jobs.py` (new, first tested behaviour of the
verbs), `agent/extensions/scufris/workflow/orchestration.ts` (`owner()`,
re-recover on `scufris:owner`, watcher hand-over, handoff-aware
`suspend-owner`), `tests/agents.test.ts`, `docs/src/dev/jobs.md`.

Acceptance tests:

- Python: `recover --owner foreground --also S` rewrites `owner_session` from
  `S` to `foreground` once, rotates the trusted capability, and is idempotent;
  `migrate-owner` reverses it; `orphans` names the other owner's kind.
- TS: an owner change re-runs `recover` for the new owner, drops the old
  watchers without `stop`, and spawns use the new owner; a job event that
  arrived under the old owner is not re-delivered after the fork (custom
  entry present) and is re-delivered at most once after catch-up.

Go/no-go: scenario S6 (a job spawned by the child is controllable from the
terminal and then from the phone after release) passes.

### Phase 5: TTS and surfaces

Files: `surfaces/desktop/src/config.rs`, `surfaces/desktop/src/main.rs`
(`speak_terminal` in `local_presentation`), `nix/desktop.nix` or
`nix/home-manager.nix` (option), `surfaces/desktop/src/hud.rs` and the iOS
state view (holder label), `tests/desktop-ui.test.ts`, `docs/src/dev/desktop.md`.

Acceptance tests: `local_presentation` speaks a live `terminal` answer only
when the option is on; replayed messages are never spoken; the holder label
renders for both values.

Go/no-go: scenario S7.

### Phase 6: rollout

Files: `nix/home-manager.nix` (`terminalLease` option, `SCUFRIS_SERVICE_TERMINAL_LEASE`
on the unit), `nix/service.nix`, `docs/src/reference/options.md`,
`docs/src/dev/operation.md` (enable, disable, roll back), `CHANGELOG.md` and the
version bump that `tools/release/check_versions.py` checks.

Acceptance tests: `nix flake check`; a home-manager evaluation with the
option on yields the flag in the unit; the iOS build pins version 11.

Go/no-go for enabling on the user's host: S1 to S8 pass on the staging
harness against the release build; the previous home-manager generation is
known and boots; the job records migration ran with `--also` and `orphans`
is empty.

## End-to-end manual scenarios

All run on the staging harness with a scratch runtime dir under
`$XDG_RUNTIME_DIR` (short path; a Unix socket path must stay under 108
bytes), a scratch state dir, and `SCUFRIS_TERMINAL_LOG` set. Each scenario
lists the observation that proves it.

- S1 Attach and type. `scufris-terminal` in the repo; the HUD shows holder
  `terminal`; a typed turn appears in the HUD under `terminal` with the answer
  attached to the same turn (`turn_id` in the trace); the service log shows
  one grant after one reap.
- S2 Phone while attached. A gateway `surface.message` while the terminal is
  idle appears in the terminal, the answer is recorded under that surface, and
  the phone receives it; while the terminal is busy the message steers and the
  phone gets no `busy` rejection.
- S3 Wake and briefing while attached. `scufris-ctl wake` and a
  `scufris-ctl briefing` announcement run in the terminal; the briefing
  answer is `unprompted`; a turn typed during the briefing turn is answered
  and neither answer is dropped.
- S4 Release and crash. `/scufris release` restarts the child within 1 s of
  the release; killing the terminal with SIGKILL restarts the child after the
  socket closes; stopping the terminal with SIGSTOP restarts the child after
  15 s and the resumed terminal reports `Lost` then reacquires.
- S5 Lineage. Tell the child a fact from the phone; attach; the terminal's
  model repeats the fact; tell the terminal a second fact; release; ask from
  the phone; the child repeats both. Then attach a plain `pi` without the
  launcher; the model repeats both from the catch-up text.
- S6 Jobs. Spawn a job from the phone; attach; the terminal shows the job
  row, receives its next event, and `cancel` from the phone stops it; spawn
  one from the terminal; release; the child's `recover` adopts it and the
  phone can cancel it. `orphans` is empty throughout.
- S7 Speech and state. With `speakTerminal` on, the desktop speaks the
  terminal's answer to S1 and shows holder `terminal`; with it off, silence.
- S8 Two terminals and a service restart. A second `scufris-terminal` gets
  `lease_held` and runs independent with its own jobs; restart the service
  under the first terminal; the first terminal reacquires within 30 s and the
  child started by the restart is stopped once.
- S9 Rollback. Disable the flag; the extension reports `lease_disabled`,
  stays inert, and the HUD conversation continues from the managed child
  with the lineage intact.

## Risks and open questions

- Pi undocumented behaviour (G1 to G6). Every gate has a fallback that keeps
  the design Scufris-only; the fallbacks trade continuity, not safety.
- Fork growth: bounded by compaction and by `lineage prune`.
- Catch-up re-delivery of job events: bounded by the helper's `event_offset`
  and the dedupe of `event_id` in session entries; worst case is one repeated
  wake per event since the last fork.
- The `input` event sees text after extension commands and before skill
  expansion; a future Pi change to that order would move `/skill:` lines out
  of the record. A test pins the current order.
- The `-p` timeout seen in the fork measurement is unexplained; G1 covers it
  before any code depends on the fork path.

## Facts

Scufris facts the design relies on, with where they live today.

- Service child ownership, restart bounds, and `Agent::stop` timing:
  `host/service/src/agent.rs`, `host/service/src/service.rs` (HEALTHY 10 s,
  RESTART_DELAY 1 s, MAX_FAILURES 3, HELLO_GRACE 10 s).
- Single-slot correlation and the drop of an uncorrelated answer while a
  proactive slot is reserved: `host/service/src/service.rs` around the
  `record_delivery` and `active_proactive` handling.
- Debug lease as built: `control_lease_acquire`, `control_lease_release`,
  `control_disconnected`, `end_lease`, `admit_agent` in the same file;
  `LEASE_SURFACE = "terminal"` and the codes in `shared/control/src`.
- Conversation replay: `host/service/src/conversation.rs`, 200 entries,
  format 1, `StoredEntry {sequence, delivery_id?, message}`.
- Job records and verbs: `tools/jobs/scufris-jobs`; `owner_session` set from
  `ctx.sessionManager.getSessionId()` in
  `agent/extensions/scufris/workflow/orchestration.ts` (spawn, `orphans`,
  `recover`, `suspend-owner`).
- Briefing flow: `tools/briefing/briefing.py`, `scufris-ctl briefing`,
  `host/service/src/briefings.rs`, `agent/extensions/scufris/briefing/`.
- Desktop speech rule: `surfaces/desktop/src/main.rs` (`local_presentation`),
  `surfaces/desktop/src/config.rs` (`SCUFRIS_DESKTOP_SPEAK_COMMAND`).
- Gateway verbs and the surface-only bridge:
  `host/service/src/bin/scufris-surface-gateway.rs`.
- Launchers: `nix/launcher.nix`, `scripts/scufris-agent`,
  `scripts/scufris-dev`; service unit: `nix/home-manager.nix`
  (`SCUFRIS_SERVICE_AGENT`, `SCUFRIS_SERVICE_SESSION_DIR`,
  `SCUFRIS_SERVICE_CONVERSATION_FILE`, `Restart=on-failure`).
- Protocol constants: `SERVICE_VERSION` in
  `agent/extensions/scufris/service/protocol.ts` and
  `shared/control/src/service.rs`, `scufrisProtocolVersion` in
  `surfaces/ios/Sources/Protocol.swift`.
