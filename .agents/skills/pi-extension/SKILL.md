---
name: pi-extension
description: Change Scufris Pi extensions, native tools, lifecycle behavior, package resources, or TUI integration.
---

# Pi extension

Read the installed Pi `docs/extensions.md` completely before extension changes.
Follow its links to `docs/tui.md`, `docs/packages.md`, and relevant installed
examples when those interfaces are involved.

- Keep lifecycle events, native tools, session state, and polling in
  `extensions/scufris/`. Move deterministic mechanics to owning helpers.
- Keep native tool schemas narrow and harness-neutral. Never expose unrestricted
  commands, paths, URLs, filesystem access, or desktop operations to the model.
- Start background resources from `session_start` or on demand, never from the
  factory. Stop session resources idempotently during `session_shutdown`.
- Handle TUI, RPC, JSON, and print modes intentionally. Guard UI operations with
  `ctx.hasUI` or `ctx.mode` as required.
- Bound tool output and preserve structured details needed for rendering or
  state reconstruction.
- Put Pi-provided APIs in `peerDependencies`. Put other runtime libraries in
  `dependencies`.
- Keep distributed model workflows under `skills/`; do not mix them with
  development skills under `.agents/skills/`.

Run focused tests first, then `npm run check` when the extension surface changed.
