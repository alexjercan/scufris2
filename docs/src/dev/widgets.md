# Widgets

Widgets are desktop presentation components. They are not Pi tools and they do
not have a result protocol.

The desktop catalog is built from `surfaces/desktop/widgets/` and optional
`SCUFRIS_WIDGET_PATH` roots. Each installed widget contributes a bounded
`WidgetDefinition` to `surface.hello`:

```json
{
  "name": "cpu",
  "description": "CPU: Show processor usage",
  "input_schema": {
    "type": "object",
    "additionalProperties": true
  }
}
```

The service stores that registration on the surface connection. Every
`agent.message` carries a fresh snapshot of the selected surface's definitions.
The model may include calls in its one final response:

```json
{
  "id": "widget-call-1",
  "name": "cpu",
  "arguments": {}
}
```

The service validates each name and argument object against the selected
surface registration. It then records the calls as part of the canonical
assistant message and broadcasts that message to every surface.

Only a ready surface whose own ID matches `message.surface` executes calls, and
only when the message is live. Other surfaces store the same metadata without
executing it. Replay also stores calls without executing them.

On the desktop, a call opens the named widget as an exhibit and passes
`arguments` as its initial data. Window creation, backend supervision, aging,
pinning, and local user actions remain desktop-local. Any rendering failure is
logged or shown locally. It never sends a widget acknowledgement, result,
update, close, or failure over protocol v4.
