# Jobs

`tools/jobs/scufris-jobs` owns the delegated job lifecycle. The orchestration
extension calls it with single-command JSON requests; a global file lock
serializes every mutating command. All state lives under
`$XDG_STATE_HOME/scufris/jobs/`.

## Model

A logical job is a durable version 2 record (`job.json`) plus artifacts in one
job directory. The record pins:

- Identity: 12-hex `job_id`, owner Pi session, random 64-hex `workflow_id`,
  `root_job`, and `parent_job`. Implementation jobs are workflow roots;
  reviewers record their parent and inherit the workflow.
- Project: opaque project ID, canonical project root and workspace paths with
  device and inode identity, context fingerprint, and the landing branch
  captured at spawn.
- Adapter: harness, per-job UUID harness session, model, and thinking.
- Progress: state, summary, creation time, generation, durable event byte
  offset, and the status file's device and inode.
- Execution: an optional replaceable tmux execution (see [Tmux](tmux.md)),
  plus an optional durable `cleanup` intent and an `archived_at` stamp.

Records are validated field-for-field on every load: exact key set, value
domains, timestamp round-trips, graph consistency, and execution consistency.
Invalid records fail closed.

The job directory holds `prompt.md` (pinned worker prompt), `report.md`
(chronological evidence), `status` (append-only event log), `conversation.md`
(foreground guidance), optional `project-context.md`, `harness-session/` (the
worker's own transcript), and private authorization files.

## Executions and generations

`working` is the only event that keeps an execution running. `blocked`,
`done`, and `failed` end the execution; the logical job survives with its
prompt, report, conversation, and workspace. Steering appends one guidance
line to `conversation.md`, increments the generation, rotates launch and
report authority, validates the recorded workspace identity, and starts a new
execution. Generation 1 opens `prompt.md`; every later generation restores the
job's own harness session, so a continuation is a true restore, not a prompt
replay. Pi passes `--session-id` every time; Claude pins the ID first and
resumes it after.

## Projects and preferences

Projects are Git roots discovered under `SCUFRIS_PROJECT_ROOTS`. The `context`
command renders a project's `.scufris.toml` into Markdown. The file holds one
`preferences` table; each entry accepts `keywords` (short exact scalars the
orchestrator must reproduce) and `guidance` (prose judgement):

```toml
[preferences.workspace]
keywords = { workspace = "sprout", base = "master" }
guidance = """
Use a Sprout workspace for project implementation.
"""
```

The rendered context includes the file's SHA-256 fingerprint. Preferences are
advisory: a missing or malformed file degrades to inference, never an error.
Every new project job consumes a fresh single-use context; the exact snapshot
is stored beside the job.

## Spawn

`spawn` validates the request, selects the adapter (`pi` defaults to
`openai-codex/gpt-5.6-sol` medium; `claude` defaults to `opus` xhigh; Pi
rejects `max` thinking and Claude rejects `off` and `minimal`), and prepares
the workspace:

- `temporary`: a private directory inside the job directory. Required for
  general jobs (no project context).
- `project`: the project root itself.
- `sprout`: a new Sprout worktree for a validated feature name.
- `review`: the exact workspace of an owned source job, revalidated by path,
  device, and inode. Selected implicitly by `review_of`; reviewers require
  the read-only Pi harness and share the source's owner and project.

The worker prompt embeds the role (bounded worker or read-only reviewer), the
rendered project context, the request, and the reporting contract. Pi workers
launch with `--approve --no-extensions` plus only the report extension;
review workers are restricted to `read,grep,find,ls,scufris_report`. Claude
workers launch with `--dangerously-skip-permissions` and report through the
`scufris-report` command line adapter.

If execution creation began, a failed spawn keeps the durable record so
recovery can finish or safely stop it; otherwise the directory and any created
Sprout are removed.

## Reporting

Workers report through one authenticated `report` path:

- Events are `working`, `blocked`, and `done` with a one-line summary and a
  Markdown evidence body. `failed` is reserved for trusted orchestration: the
  `failure` command, the launch wrapper when a harness exits without a
  terminal event, and startup reconciliation.
- Each report appends `# <event>: <summary>` plus the body to `report.md`
  under a private lock, atomically replaces the file, and only then appends
  the generation-tagged JSON event line to `status`. A visible event always
  has durable linked evidence.
- Bounds: 4 KiB event line, 512 KiB evidence body, 2 MiB report file. When a
  new entry would exceed the file bound, older history is discarded and the
  new complete entry is kept.

Authority is capability-based. `.report-auth.json` stores SHA-256 hashes for
the current generation: the one-use launch capability, the per-generation
report capability, and the owner's trusted capability, which is rotated on
every recovery. One raw value exists briefly on disk: execution preparation
writes the launch capability into a read-only `.launch-capability` file so
the pane command can present it and recovery can finish an interrupted
creation. The launch wrapper validates it against the stored hash, clears the
launch hash, installs the fresh report capability hash, and deletes the raw
file before the harness starts. An old generation's report capability cannot
publish into a new generation.

## Events

`status` is append-only JSONL: `{"generation": N, "event": "...",
"summary": "..."}`. The extension watches the file and calls `events`, which
reads from the durable `event_offset` without advancing it. Each event gets an
identity from its exact byte range and content hash. `ack-event` advances the
offset only for the exact next event, in order. Events from the current
generation update job state; a terminal event stops the execution; anything
after a terminal event in the same generation, or any unparseable line, is
surfaced as `invalid` and converted into a trusted failure. Reads batch at
1 MiB and report `more` until drained.

## Quick Review

`quick-review-build` snapshots a clean committed Sprout workspace (base branch
revision and HEAD must differ), produces a bounded since-base patch, and runs
a read-only Pi generator with only the `submit_walkthrough` tool. The
generator writes a validated exact-revision walkthrough artifact; revisions
are rechecked after generation and before every later action.
`quick-review-context` serves exact-revision file content,
`quick-review-question` answers one bounded question with a read-only model
run, and `invalidate-quick-review` removes the artifact after a change
request. The page itself is described in [Messaging](messaging.md).

## Land, stop, and archive

Cleanup is workflow-scoped and archival:

- `stop` refuses descendant IDs; the caller passes the workflow root. It
  records a durable `stop` intent, stops every execution in the graph,
  optionally removes Sprout worktrees, then archives descendants before the
  root.
- `land` requires a Sprout workflow root. It records a durable `land` intent
  with the exact revision, stops the graph, and lands with
  `sprout land --dry-run` then `sprout land`. Already-landed revisions are
  detected by ancestry or tree equality, so a retry is safe. Cleanup then
  archives the graph.
- Archiving stamps `archived_at` and moves the job directory into
  `jobs/_archive/`. Reports, prompts, conversations, and harness transcripts
  stay readable; archived jobs refuse every reviving operation.
- Sprout removal revalidates project identity and refuses drift, replacement,
  and symlinks. Missing resources count as successful cleanup. Any error
  keeps the root and the remaining records for a retry, and a graph with an
  active cleanup intent cannot be steered or gain reviewers.

## Recovery

At foreground `session_start`, `recover` reconciles every job owned by that
session: it finishes interrupted execution creation, marks lost executions
failed with a linked report entry, resolves terminal events left in `status`,
stops leftover panes, rotates the trusted capability, and returns the owned
jobs for watching. `session_shutdown` calls `suspend-owner`, which stops
executions exactly and marks nonterminal jobs `suspended`. `orphans` lists
live panes owned by other sessions without touching them.
