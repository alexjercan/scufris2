# Native Pi terminal Scufris clone: feasibility design

Job `54dd50dc8506`. Investigation only. No production change was made.

## Question

Make `cd ~/personal/scufris2 && pi` behave as a terminal Scufris: native Pi
TUI and coding tools, plus project discovery, durable subagent jobs with
steering and notifications, briefing run/show/publish/open, attachments where
possible, and the foreground Scufris policy. Configuration must live in the
repository's `.pi` directory. TTS may stay with the desktop service. HUD
widgets and inline images are optional.

Two readings exist:

- Reading A, "native Pi with Scufris tools": an independent terminal
  orchestrator. It owns its own Pi session, its own jobs, and never touches the
  service conversation.
- Reading B, "native Pi replacing the canonical Scufris agent": the terminal
  Pi takes over the service-owned RPC agent's slot and conversation.

Reading A satisfies the request safely. Reading B needs service changes that
no `.pi` configuration can provide and reintroduces the risks the prior study
found. Details below.

## Sources

- Repository at `768b035`: `AGENTS.md`, `agent/extensions/scufris/*`,
  `tools/jobs/scufris-jobs`, `tools/briefing/cli.py`,
  `host/service/src/service.rs`, `host/service/src/bin/scufris-ctl.rs`,
  `shared/control/src/service.rs`, `scripts/scufris-agent`,
  `scripts/scufris-dev`, `nix/launcher.nix`, `nix/resources.nix`,
  `nix/home-manager.nix`, `tests/*`.
- Pi 0.85.0 docs under
  `/nix/store/g252cvfmlmrfm9jysmfcxr6v0jyqybgy-pi-0.85.0/libexec/pi/docs/`
  (index, extensions, packages, skills, prompt-templates, security,
  environment-variables, settings, sessions, tmux, rpc, session-format, sdk,
  keybindings, tui) and examples (subagent, file-trigger, notify, event-bus,
  send-user-message, status-line, widget-placement, reload-runtime, handoff,
  dynamic-resources).
- Pi 0.85.0 source under
  `/nix/store/aikz711i5wjvq0i8jc1x154jqa7abm9y-pi-src-with-lock/dist`
  (`config.js`, `core/resource-loader.js`, `core/package-manager.js`,
  `core/trust-manager.js`, `core/project-trust.js`,
  `core/extensions/loader.js`, `core/session-manager.js`, `main.js`,
  `core/extensions/types.d.ts`).
- Prior study `tasks/20260911-102043/RESEARCH.md` in the
  `terminal-pi-surface-study` sprout.

## Facts that decide the design

### Pi 0.85

- Project resources are `.pi/extensions/*.ts` or `.pi/extensions/*/index.ts`,
  `.pi/skills`, `.pi/prompts`, `.pi/themes`, `.pi/settings.json` (arrays
  `extensions`, `skills`, `prompts`, `themes`, `packages`; paths relative to
  `.pi`), `.pi/SYSTEM.md`, `.pi/APPEND_SYSTEM.md`, and `.agents/skills`
  (`trust-manager.js:8-16`, `resource-loader.js:629-632`).
- Any of those makes the folder trust-gated. `~/.pi/agent/trust.json` already
  trusts `~/personal/scufris2`; other checkouts prompt once or follow
  `defaultProjectTrust`. Trust guards loading only. Extensions then run with
  the user's full permissions (`security.md`).
- `.pi/settings.json` cannot set environment variables. Every Scufris
  orchestrator extension returns early unless `SCUFRIS_ROLE=orchestrator`
  (`identity.ts:11`, `orchestration.ts:737`, `briefing/index.ts:86`,
  `response.ts:417`, `service/index.ts:106`). `calm.ts` has no gate.
- Extension factories run one after another in one process
  (`loader.js:462-506`). A factory that sets `process.env` before it calls
  another factory therefore controls that factory's gate. This is ordinary
  extension code, not an undocumented hook.
- Extensions can import repository files by relative path. Imports of
  `@earendil-works/pi-coding-agent`, `pi-ai`, and `pi-tui` resolve to the
  running Pi. The five Scufris extensions already load this way through
  `scripts/scufris-agent --extension`.
- `resources_discover` (fired after `session_start`, `types.d.ts:403-413`)
  lets an extension add skill, prompt, and theme paths. No `.pi/settings.json`
  is needed for skills.
- `--no-extensions` drops project and global extension discovery and keeps
  only `-e` paths. The worker harness passes it (`scufris-jobs`
  `prepare_harness`), so a project extension never loads inside a worker.
  `--no-skills` exists but the harness does not pass it, so anything placed in
  `.pi/skills` or `.pi/settings.json` `skills` would load in workers whose
  workspace is a scufris2 checkout.
- Sessions default to `~/.pi/agent/sessions/--<cwd>--/`. `-c`, `-r`, and
  `/resume` keep the session id. `/new` changes it inside the process and
  fires `session_before_switch` and `session_start`, not `session_shutdown`.
- Resuming a session opens it with the header cwd
  (`session-manager.js:1234`, `main.js:799`). A session written by the service
  at `WorkingDirectory=%h` resumes with cwd `~`, not the repository.
- `pi.sendMessage(..., { deliverAs: "followUp", triggerTurn: true })` wakes
  the interactive agent from any extension code path, including an `fs.watch`
  callback. `ctx.hasUI` is true in the TUI, so `ctx.ui.setStatus`,
  `setWidget`, `notify`, `custom`, and `Image` are available.
- There is no Pi API to push a message into a running interactive session from
  another process. Only extension-owned watchers or sockets can do it.
- The RPC mode, the SDK, and the experimental server are not needed for
  Reading A.

### Scufris

- The service is the only owner of `agent.sock`. A second agent handshake gets
  `agent.rejected` with `AGENT_EXISTS` (`service.rs:856-868`). `AgentClient`
  retries with 250 ms to 5 s backoff and never wins while the service's child
  is alive.
- `control.wake` and `control.briefing` (`scufris-ctl wake`, `scufris-ctl
briefing`) reach the service and its RPC agent only. `scufris-ctl wake`
  inserts text into the service conversation. It is not a mirror.
- No control verb speaks. Speech is surface-local: the desktop runs
  `SCUFRIS_DESKTOP_SPEAK_COMMAND` (packaged `scufris-speak`, one paragraph on
  stdin) on the responses the service forwards to it.
- Job ownership is the Pi session id (`orchestration.ts:1204`). `recover` on
  `session_start` reconciles only that owner's jobs; `orphans` reports other
  owners' live panes and cannot stop them (`reportStrayWorkers`,
  `orchestration.ts:1666-1684`). `session_shutdown` runs `suspend-owner`,
  which stops the owner's executions and marks non-terminal jobs `suspended`
  (`orchestration.ts:1748-1775`).
- Worker wakes are in-process: `fs.watch` on the job status file, `readEvents`,
  then `pi.sendMessage` with `customType: "scufris-job-event"` as a follow-up
  turn. Delivery state is persisted with `pi.appendEntry` in the session, so a
  resumed session re-delivers what it has not seen.
- `recover_job` re-adopts a running pane that is still alive and marks a dead
  pane failed (`scufris-jobs:4028-4075`). Running workers survive an
  orchestrator crash, and a resume re-adopts them. Only a clean shutdown stops
  them.
- Helpers resolve from the extension's own location (`toolPath`,
  `shared/runtime.ts:15-19`). Extensions imported from the working tree run
  the working tree's `tools/jobs/scufris-jobs` and `tools/briefing/cli.py`.
  `python3`, `tmux`, `scufris-den`, `scufris-ctl`, and `sprout` are on PATH.
- The response extension already renders in a TUI: the markdown transformer
  hides streamed assistant text and thinking, and the entry renderer for
  `scufris-response-v5` prints the final text (`response.ts:206-225`). Calm
  hides tool and custom-message noise. `scripts/scufris-dev` is exactly this
  stack in a terminal on its own session directory, with the service
  extension loaded and rejected.
- `store_attachment` is registered inside `bindService` only
  (`service/index.ts:106-107`) and posts to `content.sock`. An attachment id
  is only useful inside a response the service forwards to a surface.
- `nix/resources.nix` copies `agent/extensions`, `scripts`, `agent/skills`,
  and `tools`. A `.pi` directory is not packaged, so it cannot affect the
  deployed service.

## Challenge of the prior study

The prior study asked whether a terminal could attach to the service's live
conversation. Its rejection of a foreground handoff stands and is confirmed
above: one agent slot, no replay of terminal-typed turns, wrong cwd and trust
on a resumed managed session, no lease fence.

Its conclusion does not bind this request. The reframed goal is a Scufris-like
orchestrator in the terminal, not the same conversation. Removing that
requirement removes every blocker: the terminal never connects to
`agent.sock`, owns its own session and jobs, and reuses the extensions
unchanged. The remaining gaps are integration extras, not safety problems.

## Reading A: independent terminal orchestrator

### Exact project files

One file is required:

```
.pi/extensions/scufris-terminal/index.ts
```

Content, in outline:

```ts
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { fileURLToPath } from "node:url";
import workflow from "../../../agent/extensions/scufris/workflow/index.ts";
import briefing from "../../../agent/extensions/scufris/briefing/index.ts";
import response from "../../../agent/extensions/scufris/response.ts";
import calm from "../../../agent/extensions/scufris/calm.ts";

const DEFAULT_ROOTS = '["~/personal","~/work","~/third-party"]';
const skills = ["workflow", "den"].map((name) =>
  fileURLToPath(new URL(`../../../agent/skills/${name}`, import.meta.url)),
);

export default function scufrisTerminal(pi: ExtensionAPI): void {
  // A launcher (scufris-agent, the packaged scufris, a worker harness) has
  // already decided what this process is. Then this file is not the owner.
  if (process.env.SCUFRIS_ROLE) return;
  process.env.SCUFRIS_ROLE = "orchestrator";
  process.env.SCUFRIS_PROJECT_ROOTS ??= DEFAULT_ROOTS;
  process.env.SCUFRIS_SURFACE = "terminal";
  workflow(pi);
  briefing(pi);
  response(pi);
  calm(pi);
  pi.on("resources_discover", () => ({ skillPaths: skills }));
}
```

Why this shape:

- The gate on a preset `SCUFRIS_ROLE` prevents double registration when
  `scripts/scufris-dev` or the packaged `scufris` runs from the repository
  root, and keeps the file inert under a worker harness even if
  `--no-extensions` were ever dropped.
- Composing the factories inside one project extension makes the environment
  assignment order-independent. Listing the repository files in
  `.pi/settings.json` `extensions` would also load them, but the role must be
  set before their factories run, and Pi's load order between the extension
  directory, settings arrays, and packages is not a contract.
- `resources_discover` carries the two product skills the launcher passes
  with `--skill`. It avoids `.pi/settings.json` `skills`, which workers would
  also load. `.agents/skills` keeps loading as today.
- The service extension is not composed. It would only be rejected by the
  service, and it registers nothing the terminal can use except
  `store_attachment`.
- No `.pi/settings.json`, `SYSTEM.md`, or `APPEND_SYSTEM.md` is needed. The
  identity and response policies come from the extensions'
  `before_agent_start` handlers.

Alternative, not recommended: `.pi/settings.json` with
`packages: [{ "source": "..", "extensions": ["!agent/extensions/scufris/service/index.ts"] }]`.
It reuses `package.json` `pi.extensions` and `pi.skills` without a copy, but
it cannot set the role, so the bootstrap file is still needed and must load
first, which reintroduces the ordering dependency.

Session directory: leave Pi's default
`~/.pi/agent/sessions/--home-alex-personal-scufris2--/`. It is separate from
`SCUFRIS_SERVICE_SESSION_DIR` and from `scufris-dev`'s
`$XDG_STATE_HOME/scufris/dev-sessions`. Users who want continuity run `pi -c`
or `/resume`.

### What loads unchanged

| Component                                          | Loads unchanged | Note                                              |
| -------------------------------------------------- | --------------- | ------------------------------------------------- |
| `workflow/index.ts` (identity, orchestration)      | yes             | all job tools, project discovery, wakes, status   |
| `briefing/index.ts`                                | yes             | run/show/publish/open, reconcile at start         |
| `response.ts`                                      | yes             | final response policy, entry renderer, offers     |
| `calm.ts`                                          | yes             | `/calm`, hidden noise                             |
| `service/index.ts`                                 | no              | would be rejected; `store_attachment` unavailable |
| `worker-report.ts`                                 | n/a             | worker side, unchanged                            |
| `agent/skills/workflow`, `agent/skills/den`        | yes             | through `resources_discover`                      |
| `tools/jobs/scufris-jobs`, `tools/briefing/cli.py` | yes             | working-tree copies                               |

### How tool calls reach the backends

- Job tools call `runHelper` on `tools/jobs/scufris-jobs` from the working
  tree. The helper writes `$XDG_STATE_HOME/scufris/jobs/<id>` and starts
  workers in the shared tmux server. Worker prompts point at the working tree's
  `tools/jobs/scufris-report`. This is the same path `scufris-dev` uses today.
- Briefing tools call `tools/briefing/cli.py`, which uses `scufris-den` from
  PATH and the same briefing state directory as the service.
- Workers run the system `pi` from PATH. A shell that puts the repository's
  `node_modules/.bin` on PATH would run the npm shim instead. `scufris-agent`
  filters that entry; a plain `pi` start relies on the shell not adding it.
  This is a documentation point, not a blocker.

### Owner and session identity

The owner is the terminal Pi's session id. The service's jobs belong to its
session id. Neither can steer or stop the other's jobs; each sees the other's
live panes through `orphans` at `session_start` and says so once. That
message wording ("from a previous foreground session") is wrong when the
other owner is the always-on service; stage 2 refines the report or filters
by an owner marker.

`/new` in the terminal starts a fresh owner. Watchers for the old owner stay
in memory until exit, but `session_shutdown` then suspends only the new
owner's jobs. A `session_before_switch` handler that runs `suspend-owner` for
the old id, or refuses the switch while jobs run, closes this hole. The
service never switches sessions, so this is new to the terminal.

### Completion events inside interactive Pi

Unchanged: `fs.watch` on the status file, `readEvents`, then a follow-up
message with `triggerTurn: true`. The agent answers in the TUI, calm hides
the custom message, and `ctx.ui.setStatus("scufris", "N delegated jobs")`
shows the count. On `pi -c` or `/resume`, `recover` reconciles owned jobs and
undelivered events are delivered from the persisted cursor.

### Durable notifications and job lifetime

Today a clean exit runs `suspend-owner`, which stops the terminal's workers.
Jobs are durable across a crash and a resume, but not across a deliberate
exit. Two options:

- Keep it. The terminal is a session; closing it parks the work, and
  `scufris_job_resume` restarts a suspended job after `/resume`.
- Add a keep-running mode (stage 3): when `SCUFRIS_SURFACE=terminal` and the
  user opts in, `session_shutdown` closes watchers and skips `suspend-owner`.
  `recover` re-adopts live panes and reads terminal events on the next
  resume. A worker that finishes while no owner is alive is noticed on
  resume, not before. A desktop toast for that case needs a new control verb.

### Briefings

Run, show, publish, and open work in-process. The scheduled collector
announces through `scufris-ctl briefing` to `control.sock`, so the daily wake
reaches the service's agent only. The terminal sees a scheduled run through
`reconcile` at `session_start` and `scufris_briefing_show`. A terminal wake
for scheduled runs would need either a second announcement path or a watcher
on the briefing state directory, which is a small later addition.

### Final responses, desktop mirror, TTS

The `scufris_final_response` tool works unchanged and renders as the
`scufris-response-v5` entry. Nothing is sent to the service, so the desktop
conversation is never duplicated.

Optional audio mirror, stage 2: subscribe to `pi.events` event
`scufris:agent-response` in a small terminal glue module and pipe
`response.text` to a command named by `SCUFRIS_TERMINAL_SPEAK_COMMAND`, off
when unset. The packaged `scufris-speak` accepts one paragraph on stdin, the
same contract the desktop uses. This is audio-only and requires no service
change. A `control.say` verb that makes the desktop companion speak would be
cleaner (one audio device, one mute state) and needs service plus desktop
changes.

`scufris-ctl wake` is not a mirror. It creates a turn in the service
conversation and duplicates the exchange.

### Attachments, widgets, offers, images

- Attachments: in the terminal a file is a path. Optional stage: register an
  entry renderer that shows image attachments with `pi-tui` `Image` in Kitty,
  Ghostty, WezTerm, iTerm2, and Warp. `store_attachment` stays unavailable
  because nothing forwards the response to a surface.
- Widgets: HUD widgets have no terminal target. `ctx.ui.setWidget` could carry
  a job table under `/scufris` later. Optional.
- Offers: `response.ts` restores offer prompts and handles the take event.
  Without a surface, a `/offer <n>` command or `ctx.ui.select` would emit it.
  Optional.

### Coding-agent UX

Pi keeps read, bash, edit, write, grep, find, ls, and its keybindings, `/resume`,
`/fork`, `/model`. The identity prompt adds the foreground policy: answer
narrow questions directly, delegate minutes-long work as jobs, one verb per
job. Two properties to know:

- Streamed assistant text is hidden until the agent calls
  `scufris_final_response`. That is the Scufris shape. A later terminal option
  could relax the transformer, which is a one-line change gated on
  `SCUFRIS_SURFACE`.
- `/calm off` reveals tool executions for code work.

### Security and trust

- The repository is already trusted; the new file changes nothing about who
  can execute code, since the extensions already run under the launchers.
- Per-job capabilities stay per job and per owner. Cross-owner steering is
  refused by the helper. tmux operations target owned panes only.
- The `SCUFRIS_ROLE` gate means a checkout used as a worker workspace runs
  no orchestrator code even if a harness lost `--no-extensions`.
- Project trust in other checkouts: sprouts under `~/.cache/sprouts/scufris2`
  already prompt because `.agents/skills` exists. No new prompt class.

### Coexistence with the service agent

Both run at once. Shared: job store, tmux server, briefing state, den state.
Separate: sessions, owners, sockets, conversations, surfaces. The desktop HUD
shows service jobs only. Two orchestrators running the same briefing date at
once is the one shared-state race to test in stage 1.

### Failure and reconnect semantics

| Event                         | Effect                               | Recovery                                      |
| ----------------------------- | ------------------------------------ | --------------------------------------------- |
| Terminal Pi killed            | workers keep running                 | `pi -c` re-adopts live panes, delivers events |
| Clean `/quit`                 | workers suspended (default)          | `/resume`, then `scufris_job_resume`          |
| Worker pane dies              | `recover_job` writes a failed report | wake on next read                             |
| Service down                  | job and briefing tools unaffected    | none needed                                   |
| Helper timeout                | tool returns the error text          | retry the tool                                |
| Two owners, one briefing date | possible file race                   | serialize by date lock, stage 1 test          |

### What cannot work without Pi upstream changes

- Setting environment or role from `.pi/settings.json`. Worked around by the
  bootstrap file.
- Waking a running interactive session from another process. Worked around
  by extension-owned watchers; a desktop-to-terminal push would need an
  extension-owned socket.
- Per-extension configuration in `.pi/settings.json`. Worked around by
  environment variables.
- Inline images outside the supported terminals.

## Reading B: replacing the canonical agent

What it would take, and why each step is outside `.pi` configuration:

1. The service must stop supervising its own child and accept an external
   agent. `SCUFRIS_SERVICE_AGENT` is spawned and restarted after one second
   (`service.rs:1450`); there is no attach mode. Service change.
2. The terminal must open the service session file with the repository cwd.
   `SessionManager.open` takes the header cwd; the CLI has no cwd override
   for a resumed session. Pi change or a launcher that copies the session.
3. Turns typed in the terminal never pass through `surface.message`, so the
   desktop replay lacks the prompts while it receives the responses. Service
   protocol change.
4. No lease fence exists between two agents that believe they own the
   conversation. Service change.
5. Speech and HUD widgets then follow the service path, which is the one
   benefit over Reading A, and it is reachable from Reading A with a
   `control.say` verb at a fraction of the risk.

Verdict: reject for this request. The request allows an independent
orchestrator, and that reading has no such blockers.

## Capability matrix

Columns: desktop through the service today; Reading A stage 1; Reading A
after stages 2 and 3; Reading B.

| Capability                            | Desktop | A1            | A2-3                    | B             |
| ------------------------------------- | ------- | ------------- | ----------------------- | ------------- |
| Native TUI and coding tools           | no      | yes           | yes                     | yes           |
| Project discovery, `scufris_projects` | yes     | yes           | yes                     | yes           |
| Spawn, steer, stop, land jobs         | yes     | yes           | yes                     | yes           |
| Job wakes in the conversation         | yes     | yes           | yes                     | yes           |
| Job count status                      | HUD     | status line   | status line, `/scufris` | HUD           |
| Jobs survive closing the surface      | yes     | crash only    | opt-in                  | yes           |
| Briefing run/show/publish/open        | yes     | yes           | yes                     | yes           |
| Scheduled briefing wake               | yes     | at start only | watcher                 | yes           |
| Foreground policy, final response     | yes     | yes           | yes                     | yes           |
| TTS                                   | yes     | no            | audio mirror            | yes           |
| Attachments to a surface              | yes     | path only     | inline images           | yes           |
| HUD widgets                           | yes     | no            | optional widget         | yes           |
| Offers                                | yes     | no            | `/offer`                | yes           |
| Calm mode                             | yes     | yes           | yes                     | yes           |
| Shares the desktop conversation       | yes     | no            | no                      | broken replay |
| Safe beside the running service       | yes     | yes           | yes                     | no            |
| Requires service changes              | none    | none          | optional                | yes           |

## Staged prototype plan

Stage 0, proof without files. Run from the repository root with the service
up:

```
SCUFRIS_ROLE=orchestrator pi \
  -e agent/extensions/scufris/workflow/index.ts \
  -e agent/extensions/scufris/briefing/index.ts \
  -e agent/extensions/scufris/response.ts \
  -e agent/extensions/scufris/calm.ts \
  --skill agent/skills/workflow --skill agent/skills/den
```

Expect: tools present, a spawned job wakes the TUI, `orphans` reports the
service's workers, no `agent.rejected` log. Nothing to commit.

Stage 1, project bootstrap.

- Add `.pi/extensions/scufris-terminal/index.ts` as outlined.
- Add `tests/terminal.test.ts` in the style of `tests/identity.test.ts`:
  call the default export with a fake `pi` and assert (a) with
  `SCUFRIS_ROLE` preset nothing registers; (b) unset, the role becomes
  `orchestrator`, `SCUFRIS_PROJECT_ROOTS` defaults, and the fake receives
  `scufris_projects`, `scufris_briefing_run`, `scufris_final_response`, and
  the `calm` command; (c) `resources_discover` returns the two skill paths.
  Restore the environment after each case.
- Extend `tests/structure.test.ts` to pin the bootstrap path and to assert
  the file does not import `service/index.ts`.
- Add `docs/src/guide/terminal.md` and a `SUMMARY.md` entry: how to start,
  what is shared with the service, `pi -c` for continuity, the PATH note.
- Checks: `env -u PI_PACKAGE_DIR npm run check`; `nix flake check` for the
  docs build. `.pi` is outside the packaged resources.

Stage 2, terminal glue, in `agent/extensions/scufris/terminal.ts` so Pi
lifecycle stays under `agent/extensions/scufris/`, composed by the bootstrap.

- Audio mirror on `scufris:agent-response` through
  `SCUFRIS_TERMINAL_SPEAK_COMMAND`, stdin paragraph, off when unset.
- `session_before_switch` and `session_before_fork`: suspend the old owner's
  jobs or refuse while jobs run.
- `orphans` wording when the other owner is the service.
- `/scufris` command: owned job rows in a `ctx.ui.setWidget` table.
- Tests: `tests/terminal.test.ts` with a fake event bus and a fake spawn.

Stage 3, keep-running exit. Gate `suspend-owner` in `session_shutdown` on an
opt-in when `SCUFRIS_SURFACE=terminal`; rely on `recover` re-adoption. Test
in `tests/agents.test.ts` style with a fake `runHelper`. Add a briefing
state watcher for scheduled runs.

Stage 4, optional service work. `control.say` for desktop TTS; terminal job
rows in the HUD; briefing announcements fanned out to registered terminal
owners. Each is a protocol version bump.

## Proof of concept: the debug lease (Reading B, measured)

After the research, the preferred model changed to a single-owner hijack: one
terminal Pi takes the service's agent and gives it back. The four blockers
listed under Reading B were built as a debug-only path and measured rather
than argued. Off by default on both sides: the service needs `--debug-lease`
or `SCUFRIS_SERVICE_DEBUG_LEASE=1`, the terminal needs `SCUFRIS_DEBUG_LEASE=1`.
Protocol and files are described in `docs/src/dev/service.md` ("The debug
lease") and the manual procedure in `docs/src/dev/staging.md`.

What was built:

1. Attach mode: `control.lease_acquire` on `control.sock`. The service bumps
   the process generation, stops and reaps its `pi --mode rpc` child, and
   answers `control.lease {generation, session_dir, session_file?}`. The lease
   is the control connection; its close, released or not, restarts the child.
2. Writer fence: `agent.hello {lease}` must carry the granted generation while
   a lease is held (`lease_required` otherwise); with no lease, naming one is
   `not_lease_holder`. Holder-only verbs `agent.lease_turn` and
   `agent.lease_activity` are refused for any other agent connection.
3. Replay of terminal-typed turns: `agent.lease_turn` records a user message
   under the reserved surface name `terminal`, which makes the terminal the
   owner of the next `agent.response`; both reach every connected surface and
   the canonical replay.
4. Session cwd: not solved. See "What fails".

Automated harness, all green:

- `host/service/tests/lease.rs` drives the built `scufris-service` binary with
  a stand-in agent that records its pid. Measured: the child pid is dead
  before the grant is answered; unfenced and stale hellos are refused;
  a typed turn and its answer reach a surface under `terminal`; activity
  drives `working`/`idle`; a surface message and a `control.wake` reach the
  leased terminal; a surface that connects late replays the terminal's turn
  in order; release restarts the child (the pid file grows); a disconnect of a
  second holder restarts it too; `lease_disabled` without the flag.
- Service unit tests cover the same paths on `Service` directly, and the
  protocol crate pins the new verbs and refusal codes. TypeScript tests cover
  `LeaseClient` (grant, idempotent acquire, release, refusal, loss reported
  once), the fenced hello, text bounds, `bindService` with a lease (acquire
  before start, typed `interactive` input recorded, slash and extension input
  not, activity on `agent_start`/`agent_settled`, session switch re-acquires,
  loss stops the client, shutdown releases) and the project extension gate.
- `cargo test -p scufris-service -p scufris-control`, `cargo clippy`,
  `npm run check`, and the Python suite pass.

Live run, one scratch service with the real `scripts/scufris-agent` child and
one `pi --approve -p` from this repository:

- Run 1, before the channel wait: the grant came 0.10 s after
  `session_start`; the prompt arrived 6 ms after the grant and 10 ms before
  the service admitted the terminal, so `agent.lease_turn` and the first
  `agent.lease_activity` were dropped as "the channel is down". The answer
  `pong` came 3 s later over the then-open channel and was recorded as
  `unprompted`: the HUD would have shown an answer with no question. Fixed
  by making a leased `session_start` wait for `agent.ready`, bounded and
  traced as `channel_ready` or `channel_timeout`, with a unit test for each.
- Run 2: `session_start`, `lease_acquired` at +0.10 s, `channel_ready` at
  +0.11 s, `input` with `source: interactive` and 33 bytes, `agent_start`,
  `agent_settled` at +3.2 s, `session_shutdown`, `lease_released`. The
  service journal shows the child stopped for the lease, the terminal
  admitted, the turn recorded, the lease ended on release, and a new child
  connected 0.5 s later. The canonical replay holds two entries, a user
  message and the assistant answer, both under `terminal`. The child's pid
  was gone after the grant and a different one alive after release; the
  state read `idle` before and after.
- Print mode reports typed input as `interactive`, so `pi -p` stands in for
  a typed turn. Project trust matters: `-p` from an untrusted checkout skips
  `.pi/extensions` silently, so the run passed `--approve`; an interactive
  `pi` asks once and `/trust` saves the decision.

What works:

- The hijack itself: one control verb stops the managed child cleanly, the
  terminal joins as the sole agent, and the service restarts its child on
  release or on any loss of the control connection. No orphan and no window
  with two writers, because the fence is a generation the service hands out.
- Typed turns and final answers appear in the shared conversation under
  `terminal`, and late surfaces replay them.
- Surface messages, aborts, wakes, job commands, offers, and briefing dispatch
  reach the terminal unchanged, because it is the same agent channel.
- Job and briefing tools are the same extensions, composed in-process.

What fails or is not supported, measured with the trace log:

- Built-in slash commands never reach `input`, so they are not recorded;
  `/skill:` and extension commands reach it with `command: true` and are
  skipped on purpose. The HUD sees neither.
- Compaction is traced (`session_before_compact`, `session_compact`) and not
  recorded; a steer or follow-up from a surface during a turn works, but a
  steer typed in the terminal is a second `interactive` input and is recorded
  as a plain turn, not marked as a steer.
- Session switching (`/new`, `/resume`, fork) keeps the lease and re-runs
  `session_start`, but the HUD conversation does not follow the terminal's
  session: the canonical replay is the service's, and the restarted child
  continues the service's own session file, not the terminal's. A terminal
  started with `--session-dir` of the service and `--resume` of the file the
  grant names continues that session, with tools running in `$HOME`.
- Submission IDs: a typed turn has none, so `agent.proactive_started` and
  the per-message abort have nothing to correlate. Attachments: a typed turn
  cannot carry one; `store_attachment` still works for answers. Widgets: a
  terminal has no registration, so widget calls in a terminal-owned answer
  are dropped and the agent is not told.
- Proactive wakes run in the terminal and are recorded as `unprompted`, but
  an uncorrelated terminal answer while a proactive slot is reserved is
  dropped by the existing response path, which is logged, not fixed.
- Crash recovery: a terminal that dies restarts the child within the service's
  stop bound; a service that dies leaves the terminal running with no
  channel, reported once as `lease_lost`. Jobs are owned by Pi session id, so
  jobs started by the child show in the terminal as orphans and the reverse.
- Protocol version 10 was kept because the verbs are additive and gated; a
  production path would bump it and mirror the codes in both surfaces.

Production-safety verdict: the mechanism can be made production-safe, the
experience cannot yet. The lease, fence, and restart are small, tested, and
fail closed. What stands between this and a product path is session identity:
one conversation with two session files, no submission ids, no attachments,
and answers that lose their widgets. Those need a design decision (the
terminal adopts the service session, or the service adopts the terminal's),
not more protocol.

## Recommendation

Revised after the proof of concept. Reading B is buildable and its first three
blockers are solved by about four hundred lines, gated off by default; keep
it as the debug path it is. Reading A, stages 1 and 2, remains the answer for
a terminal that works beside the service. Promote the lease only after
deciding which session file the shared conversation follows and adding
submission ids to typed turns; until then it stays a debug tool.
