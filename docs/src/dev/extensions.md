# Pi extensions

[Previous: Desktop companion](desktop.md)

```text
workflow -> decide and delegate
response -> emit one atomic answer
calm     -> reduce Pi transcript noise
service  -> connect Pi to the conversation owner
```

Scufris loads four foreground extensions from `agent/extensions/scufris/`:

- `workflow/` owns distributed worker orchestration;
- `response.ts` owns one atomic final response;
- `calm.ts` owns the reduced foreground Pi presentation; and
- `service/` owns the protocol v7 agent connection.

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

`agent.abort` calls the active extension context's abort method.

`workflow/` publishes every delegated job as one `agent.jobs` list. The list is
whole rather than incremental, because a row outlives its job: a surface that
connects this morning has to be told about the night's finished work too. The
service holds the last list and publishes it again on every reconnect. The
aggregate tray word is folded from the rows on the host; there is no separate
state message.

Two verbs come back. `agent.job_command` carries one row control: `cancel`
stops the job and keeps an unmerged branch, `archive` only files the row.
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

## Lifecycle rules

Long-lived links start from `session_start`, not from extension factories. They
stop during `session_shutdown`. UI notifications are guarded by `ctx.hasUI`.
Pi packages remain peer dependencies and runtime dependencies remain ordinary
package dependencies.

---

Next: [Jobs](jobs.md)
