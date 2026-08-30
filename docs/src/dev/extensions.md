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
- `service/` owns the protocol v4 agent connection.

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

`agent.abort` calls the active extension context's abort method. Workflow
attention notices are aggregated locally and sent as one `agent.state` with
`failed`, `blocked`, or `clear`.

There is no RPC prompt path, context queue, context acknowledgement, dynamic
widget tool registration, or desktop command relay in the extension.

## Response extension

`response.ts` registers `scufris_final_response`. The tool accepts:

- mandatory bounded plain `text`;
- optional bounded Markdown `details`; and
- optional bounded widget calls with `id`, `name`, and `arguments`.

It emits the complete value once to the service extension. Details are not
stored in sidecar artifacts and there is no ordinary `/detail` command. Widget
calls are presentation metadata, not native Pi tools, and do not wait for a
result.

## Lifecycle rules

Long-lived links start from `session_start`, not from extension factories. They
stop during `session_shutdown`. UI notifications are guarded by `ctx.hasUI`.
Pi packages remain peer dependencies and runtime dependencies remain ordinary
package dependencies.

---

Next: [Jobs](jobs.md)
