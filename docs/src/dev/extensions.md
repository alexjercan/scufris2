# Pi extensions

[Previous: Desktop companion](desktop.md)

```text
workflow -> decide and delegate
response -> emit one atomic answer
calm     -> reduce Pi transcript noise
service  -> connect Pi to the conversation owner
terminal -> hold the agent from a terminal instead
```

Scufris loads four foreground extensions from `agent/extensions/scufris/`:

- `workflow/` owns distributed worker orchestration;
- `response.ts` owns one atomic final response;
- `calm.ts` owns the reduced foreground Pi presentation; and
- `service/` owns the protocol v11 agent connection.

`terminal/` is the fifth, and it is loaded only by a terminal that means to
become the agent. See [The terminal extension](#the-terminal-extension).

Only an orchestrator process loads foreground behavior. Worker Pi processes do
not connect as another agent or offer desktop presentation.

## Service extension

`service/index.ts` opens `agent.sock` during `session_start` and closes it
idempotently during `session_shutdown`. It does not start sockets from the
extension factory.

Every `agent.message` contains the original surface text and current widget
definitions. The extension serializes them deterministically into one
XML-delimited block. User text and widget JSON are JSON encoded, and XML
characters inside that JSON are escaped. The complete block is sent through
`pi.sendUserMessage()`. A busy Pi receives it with `deliverAs: "steer"`.

`calm.ts` draws that block as the words that were sent. The widget catalogue is
the right thing to give the model and four thousand characters of schema around
one sentence to put in front of a person, so the person is given the sentence,
with any attachment named. It is display only, through
`pi.registerMarkdownTransformer`: the session, the model's context, and the
conversation the service replays all keep the block, and `/calm off` shows it.

`agent.abort` calls the active extension context's abort method.

`workflow/` publishes every delegated job as one `agent.jobs` list. The list is
whole rather than incremental, because a row outlives its job: a surface that
connects this morning has to be told about the night's finished work too. The
service holds the last list and publishes it again on every reconnect. The
aggregate tray word is folded from the rows on the host; there is no separate
state message.

Two verbs come back. `agent.job_command` carries one row control: `cancel`
stops the job and keeps an unmerged branch, `archive` only files the row. A
filing is written into the session against the current execution generation,
so it survives a restart without hiding later work: `recover` hands back every
job that was never stopped or landed, and a row filed in memory alone would
come back the next morning. Steering starts a new generation under the same
logical job ID, so that active row appears again and must be filed separately
when it finishes. Filing never touches the job record, so `scufris-jobs` still
lists it until `stop` or `land` archives it.
`agent.offer_take` carries one offer identifier. The words behind an offer
never cross the socket: `response.ts` stores the prompt it composed and runs
that, so no surface control can put a sentence into the conversation.

The extension also registers `store_attachment`. It accepts one bounded file
path, resolves relative paths against Pi's current working directory, infers a
media type from a small suffix table, and asks the private `content.sock` API to
import the file. The service validates and copies the regular file. The tool
returns only the opaque managed ID for use in `scufris_final_response`.

There is no RPC prompt path, context queue, context acknowledgement, dynamic
widget tool registration, or desktop command relay in the extension.

## Response extension

`response.ts` registers `scufris_final_response`. The tool accepts:

- mandatory bounded plain `text`;
- optional bounded Markdown `details`;
- optional managed attachment IDs; and
- optional bounded widget calls with `id`, `name`, and `arguments`.

It also accepts optional `offers`: at most four `{job_id, label, prompt}`
entries naming a job this answer reports on. The extension assigns each one an
identifier, stores the prompt against it, and drops an offer that names a job
this message carries no badges for.

It emits the complete value once to the service extension. Details are not
stored in sidecar artifacts and there is no ordinary `/detail` command. Widget
calls are presentation metadata, not native Pi tools, and do not wait for a
result.

## Receipts

No model writes a badge. `workflow/citation.ts` reads the measured receipt the
jobs helper produced - its `facts`, `claims`, and `unavailable` - and maps it
into badges in the vocabulary `receipt_sentences` already uses. There are four
states and not a fifth:

- `measured`: the fact is true;
- `refuted`: measured and false;
- `claimed`: the worker said it and no fact backs it; and
- `unknown`: in `unavailable` with a reason, and never drawn as a no.

Badges reach the next final response grouped by job, at most four groups to a
message. The job identifier is what binds a badge to what it is about, so
nothing parses the model's prose and nothing breaks it.

## The terminal extension

`terminal/index.ts` is how an interactive Pi becomes the agent for a while.
`.pi/extensions/scufris-terminal` loads it, and that file is the gate: with no
`SCUFRIS_TERMINAL=1` in the environment an ordinary `pi` in this checkout gets
nothing from it. The lifecycle itself lives under `agent/extensions/scufris/`,
so it is packaged like everything else.

Three states, and nothing between them:

```text
Independent --acquire ok--------> Leased
Independent --acquire refused---> Independent (a notice, and no channel)
Leased      --release, exit-----> Independent
Leased      --lease ended-------> Lost --reacquire--> Leased | Independent
Lost        --/scufris hold-----> Independent
```

`Independent` is an ordinary Pi: no `agent.sock`, no HUD, its own session id
owning its own jobs. `Leased` is the sole agent. `Lost` is `Independent` plus a
retry loop, one second doubling to thirty, which `/scufris hold` stops.

`/scufris` carries all of it: `status` (state, channel, generation, owner,
lineage, and what the host says), `attach`, `release`, `hold`.

While the lease is held, `/new`, `/resume`, and `/fork` are cancelled with a
notice naming `/scufris release`. The lineage has to stay one chain, because
the fork-back on release copies this session and a `/resume` into an unrelated
one would hand that context to the managed child. `/tree` is allowed: it is the
same file, and the move is reported with `agent.session`.

`attach` forks the lineage into this directory and switches to the copy, so
model context follows the conversation. It asks only when this session is not
already the lineage or a fork of it. That guard is not cosmetic: a switch
starts a session, which would ask for another switch, and Phase 0 gate G3
measured 3560 switches in 25 seconds without it.

In a terminal the response extension renders natively. Streamed Markdown and
thinking are no longer hidden, and the shared answer is the last assistant
message of the turn unless `scufris_final_response` was called, in which case
the tool's text, receipts, offers, and attachments win.

The composition is the checkout's, but the programs under it are the
deployment's. `nix/agent-runtime.nix` names them once - the interpreter the
helper scripts import from, tmux, the journal, and the briefing - and both
`nix/launcher.nix` and `nix/terminal.nix` take that list, so a tool that works
for the managed child works in a terminal.

## Lifecycle rules

Long-lived links start from `session_start`, not from extension factories. They
stop during `session_shutdown`. UI notifications are guarded by `ctx.hasUI`.
Pi packages remain peer dependencies and runtime dependencies remain ordinary
package dependencies.

---

Next: [Jobs](jobs.md)
