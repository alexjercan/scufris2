# Jobs

[Previous: Pi extensions](extensions.md)

```text
logical job
├── durable record and artifacts
├── generation 1 -> owned tmux execution
├── generation 2 -> restored harness session
└── stop/land -> archive the workflow graph
```

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
resumes it after. Initial creation, precreated completion, restart, and recovery
revalidate the recorded workspace path, device, and inode before tmux launch.
The launch wrapper then validates its inherited current-directory inode before
starting the harness, closing the path replacement window between checking a
pathname and entering it.

## Projects and the agent menu

Projects are Git roots discovered under `SCUFRIS_PROJECT_ROOTS`. The `context`
command renders a project's `.scufris.toml` into Markdown. The file is a menu,
not a workflow. It holds two optional tables:

- `conventions` says what Scufris infers when the request is silent: tracking,
  workspace, base branch, harness. An explicit instruction in the request wins
  over any of them.
- `agents.<name>` declares one agent type. `description` is one short printable
  line that says what the agent is for. `keywords` are short exact scalars that
  say how it is run. `guidance` is prose judgement.

```toml
[conventions]
keywords = { tracking = "tatr", workspace = "sprout", base = "master" }
guidance = """
Keep the main checkout on master.
"""

[agents.work]
description = "Implement a change in the project."
keywords = { harness = "pi", model = "openai-codex/gpt-5.6-sol", thinking = "medium" }
```

Scufris starts only the agents a request names, in the order it names them. An
agent name Scufris has never seen is delegated to like any other entry, with
that entry's keywords. The menu declares no order and no gate of its own.

A menu entry can also say to reuse its job. A later round of an agent already
owned for the work is `send`, not a second `spawn`: the steered job restores
its own harness session and keeps what it already accepted, while a fresh job
keeps nothing and re-derives its findings. A review entry says this in its
`guidance`, because an implement-then-review cycle that spawns a new reviewer
each round never converges.

The rendered context includes the file's SHA-256 fingerprint. The menu is
advisory: a missing or malformed file degrades to inference, never an error.
When an entry declares `harness`, its optional `model` and `thinking` keywords
are validated by the same adapter resolver used at spawn. Supported harnesses
are `pi` and `claude`; Pi rejects `max`, and Claude rejects `off` and
`minimal`. An unsupported tuple, a missing agent description, or the retired
`preferences` workflow shape makes the file unusable and returns an
`ignored .scufris.toml` diagnostic instead of a half-read menu. Every new
project job consumes a fresh single-use context; the exact snapshot is stored
beside the job.

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
  device, and inode. Selected implicitly by `review_of`; reviewers use the
  requested Pi or Claude adapter and share the source's owner and project.

The worker prompt embeds the role (bounded worker or read-only reviewer), the
rendered project context, the request, and the reporting contract. Pi workers
launch with `--approve --no-extensions` plus only the report extension. Pi
reviewers are restricted to `read,grep,find,ls,scufris_report`. Claude
implementation workers retain the normal interactive adapter and report
through `scufris-report`. Claude reviewers instead run in print mode with
`Read,Glob,Grep`, `dontAsk`, no user/project/local settings sources, no
request-supplied MCP servers, no slash commands, and explicit denial of shell,
mutation, web, and subagent tools. The trusted wrapper stores their bounded
final Markdown response as the terminal report for the exact generation.

Both review adapters enforce read-only built-in model capabilities with a tool
allowlist. The role prompt is defense in depth, not the enforcement. This is
not an operating-system read-only filesystem sandbox: the trusted harness and
wrapper can still write their own session and report state. Claude managed
policy can also install hooks or plugin hooks outside the built-in tool list;
Scufris cannot disable that host policy while retaining normal managed
operation, so `managed-claude-policy` is an explicit trusted boundary for
Claude reviews. Spawn and inspect report the allowlist, `not-os-sandboxed`
filesystem isolation, and these trusted boundaries rather than claiming a
stronger guarantee.

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

## Quick Review target

`quick-review-target` accepts only an owned Sprout job with a clean committed
change. It returns the exact base and implementation revisions, repository, and
private state directory used to start the standalone review agent. The agent,
not the jobs helper, owns walkthrough generation and page questions.

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

---

Next: [Messages](messaging.md)
