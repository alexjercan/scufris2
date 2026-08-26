# Extensions

All five extensions are listed in `package.json`, by the development launcher,
and by the Nix launcher, together with the workflow and widgets skills.

## workflow

`extensions/scufris/workflow/index.ts` composes two orchestrator modules:

- `identity.ts` appends the canonical Scufris identity policy to the system
  prompt: foreground pairing, one decision at a time, delegation by scope and
  latency, and concise spoken responses.
- `orchestration.ts` owns delegated jobs. It registers the workflow tools,
  watches owned status files, delivers worker events, and manages session
  recovery and shutdown. See [Jobs](jobs.md) and [Messaging](messaging.md).

Orchestration tools:

- `scufris_projects`: list opaque project IDs discovered under
  `SCUFRIS_PROJECT_ROOTS`.
- `scufris_project_context`: render one project's advisory `.scufris.toml`
  into Markdown guidance and return a single-use 24-hex context ID.
- `scufris_job_spawn`: start one worker. Optional harness, model, thinking,
  workspace, feature, and `review_of` selections. A project context ID is
  consumed exactly once; `project` and `sprout` workspaces require one. Review
  jobs honor Pi or Claude selections and return the enforced built-in tool
  allowlist, explicit `not-os-sandboxed` filesystem isolation, and trusted host
  boundaries. Claude names managed hook/plugin policy as trusted.
- `scufris_job_list`, `scufris_job_inspect`: owned-job index and bounded
  inspection with optional report, context, prompt, and conversation.
- `scufris_job_send`: append one guidance line and restart the job in a new
  generation.
- `scufris_job_stop`: stop and archive one owned workflow graph from its root.
- `scufris_job_quick_review`: start a separate Pi RPC agent with the pinned
  standalone npm extension for an exact committed Sprout range.
- `scufris_job_plannotator_review`: open a Plannotator since-base review.
- `scufris_job_land`: guarded Sprout landing after user approval.

The module also registers `/wake`, blocks foreground `sleep` and `wait` bash
commands, and enforces the acknowledgment gate described in
[Messaging](messaging.md). `quick-review-agent.ts` owns the bounded subprocess
protocol and exact shutdown of the separate review agent.

`worker-report.ts` is the only extension a Pi worker loads. It registers
`scufris_report`, which forwards the event, summary, and Markdown report to
the jobs helper with the worker's report capability. `blocked` and `done`
results terminate the worker's turn.

## voice

`voice/index.ts` always loads `response.ts` and loads `speech.ts` only when
`SCUFRIS_VOICE_AVAILABLE=1` (the module is absent from normal resources).

`response.ts` owns response shaping for the orchestrator:

- Registers `scufris_final_response` with `spoken` and optional `detail`
  parameters. The spoken paragraph must be safe plain prose: one bounded
  paragraph, no Markdown, paths, URLs, code, or control characters
  (`plainProseParagraph`).
- Rewrites assistant messages at `message_end`: streaming text and text
  beside tool calls are discarded; a plain final message is split into a
  spoken paragraph and stored detail; unsafe output is stored in full and
  replaced with a fixed safe sentence.
- Stores detail in an `ArtifactStore` sidecar beside the session file with
  strict ownership and mode checks, bounded to 256 KiB per artifact and 128
  artifacts. Registers `/detail` (Plannotator annotate with private feedback
  return) and `/scufris-prompt` (prompt inspection artifact).

`speech.ts` owns playback in the orchestrator TUI:

- `/speech on|off|once|replay`; the mode persists as a custom session entry.
- After each settled agent run it extracts the last safe assistant paragraph
  produced by that run and plays it once through the `scufris-speak` helper
  (Piper to PipeWire). Input cancels playback; failures notify without
  failing the turn. Playback is bounded to 1000 UTF-8 bytes and 65 seconds.

## calm

`calm.ts` patches the Pi TUI transcript renderers once per process. With Calm
enabled it hides thinking blocks, tool call text, tool execution rows, and
`scufris-job-event` and `scufris-widget-event` rows. `/calm on|off` persists
the state as a session entry; the default is on. Calm is presentation only:
the underlying entries stay in the session.

## widgets

`widgets/index.ts` owns the desktop companion's widgets. It registers
`scufris_widget_open`, `scufris_widget_update`, `scufris_widget_close`, and
`scufris_widget_clear`, typed from the catalog the companion announces on each
connection, and turns a surface the user closed into a `scufris-widget-event`
follow-up message. It reaches the control socket through the desktop extension,
which emits the two verbs it needs when it starts serving and withdraws them
when it stops. See [Widgets](widgets.md).
