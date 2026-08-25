# Research: widget runtime implementation

Date: 2026-08-25. Two explorer sweeps mapped the code the plan changes.
The plan itself is `widget-runtime-plan.html` (published as an
artifact); this file keeps the raw inventory the plan condenses.

## Companion facts that shape the plan

- No async runtime. The companion is `std::thread` everywhere: the
  `Executor` trait spawns a thread per task (`app.rs:161-190`), the
  daemon link runs two dedicated threads, reqwest is blocking. Backend
  supervision therefore uses reader threads and a mutexed table, not
  the tokio-select shape dashboardd used.
- No `tauri::ipc::Channel` anywhere yet. IPC today is global
  `AppHandle::emit` (four events) plus six invoke commands; the
  per-window Channel for widgets is new plumbing.
- Protocol v1 lives in `desktop/scufris-control/src/lib.rs`:
  `PROTOCOL_VERSION = 1` (:22), tagged `ClientBody` (hello, submit,
  ping) and `DaemonBody` (welcome, ack, uncertain, refused, state,
  pong), 64 KiB line cap, LF-terminated JSON lines. The TS mirror is
  `extensions/scufris/desktop/protocol.ts` (`PROTOCOL_VERSION` :4).
  Shared fixtures `desktop/control-protocol-v1.json` (canonical,
  tolerated, rejected, both sides) are read by the Rust suite
  (`lib.rs:529`, include_str) and by `tests/desktop.test.ts:711`.
- Every daemon-to-companion write today is a broadcast
  (`ControlServer.broadcast`, `server.ts:567`) or an inline answer in
  the connection handler. There is no targeted, correlated
  request/response path in either direction of the stack; building it
  (extension `request()` with a pending map, companion answer writes
  through the link's writer) is the largest new mechanism.
- Window recipe that generalizes: `pill.rs:59-71` (min == max size
  hints float it under i3 and make GTK honor the size, decorations
  off, always-on-top, skip taskbar, opaque, visible(false),
  focused(false)); `show_passive()` (`pill.rs:152-169`) never touches
  the keyboard; `bottom_center()` (`pill.rs:30-43`) is the pure,
  unit-tested anchor idiom to copy for slot geometry. Sticky
  (`visible_on_all_workspaces`) is available but unused today.
- Config gates: `capabilities/default.json` scopes permissions to the
  single label "pill"; `tauri.conf.json` CSP has no custom scheme;
  `"windows": []` so all creation is programmatic.
- `state.rs` has a `dismissed` flag set only by Escape - the natural
  hook for "the user hid the pill" (scratchpad hide) as opposed to the
  automatic posture-off after an interaction.
- Drift noted: `tauri.conf.json` version says 0.3.0, crate is 0.4.0.
- Test gotcha: `tests/desktop.test.ts` binds real Unix sockets; socket
  paths cap at 108 bytes, so a nested nix-shell TMPDIR fails ~48 tests
  with ENOENT. Run `TMPDIR=/tmp npm run check`
  (`docs/src/dev/maintenance.md:135-138`).

## dashboardd removal checklist (complete touchpoint inventory)

Delete outright (~1,155 lines):

- `extensions/scufris/dashboard/index.ts` (694) - five tools, 1 Hz
  poll, catalog validator, helper bridge.
- `tools/dashboard/scufris-dashboard` (235) - Python dashboardctl
  adapter.
- `tests/test_scufris_dashboard.py` (205).
- `skills/dashboard/SKILL.md` (21).

Edits, in landing order:

1. `package.json:23` - drop the extension entry from `pi.extensions`
   (`pi.skills` is directory-level, no edit).
2. `scripts/scufris-dev:64,69` - drop the extension and skill lines.
3. `extensions/scufris/workflow/identity.ts:4` - rewrite the
   "dashboard orchestration" sentence, byte-identical in lockstep with
   `tests/identity.test.ts:10`.
4. `extensions/scufris/calm.ts:15` - KEEP `"scufris-widget-event"`;
   the new runtime emits the same custom type for follow-ups.
5. Tests: `tests/structure.test.ts:26,47`,
   `tests/dev_helper.test.ts:143,156`.
6. Nix chain: `flake.nix:18-21,92` (input + dashboardctlPackage) ->
   `nix/scufris.nix:17,19,22,42` -> `nix/launcher.nix:5,6,27-32,46` ->
   `nix/home-manager.nix:37,38,79-92` (the whole
   `programs.scufris.dashboard` option group - a breaking module
   interface change, CHANGELOG entry required) -> checks
   `nix/checks/launcher.nix:39-42,84-87`,
   `nix/checks/resources.nix:20,32,37`, `nix/checks/homes.nix:28`,
   optionally `nix/checks/desktop.nix:171` (stale grep alternation).
7. Regenerate `flake.lock` - drops the dashboardd node and its
   transitive crane / rust-flake / rust-overlay toolchain pin (~120 of
   327 lines).
8. Docs sweep: `docs/src/overview.md:5,15,23,34,43-44`,
   `dev/architecture.md:8,16,20,36,64-65`, `dev/extensions.md:3-6,83,
87-104`, `dev/desktop.md:7-9` (the paragraph that justifies Tauri
   via dashboardd needs the native-runtime rationale instead),
   `dev/operation.md:83-84`, `dev/maintenance.md:161-163`,
   `guide/installation.md:72-75`, `guide/using.md:64-69`; plus
   `README.md:3` and `AGENTS.md:8`.

Scope note: ~30 historical task documents under `tasks/` mention
dashboardd and stay untouched (append-only records). The acceptance
grep "no dashboardd anywhere" is scoped to non-`tasks/` paths.

## Extension facts that shape the plan

- Tools are `pi.registerTool(defineTool({name, label, description,
promptSnippet, promptGuidelines?, parameters: TypeBox, execute}))`;
  results go through `toolResult()` from `shared/runtime.ts:43`.
  Late registration after discovery is proven
  (`dashboard/index.ts:664` registered tools on session_start after a
  successful discover).
- Lifecycle hooks in use: session_start, session_shutdown,
  before_agent_start, agent_start/agent_settled, input, message_end,
  tool_call, tool_result. Follow-ups via `pi.sendMessage(msg,
{deliverAs: "followUp", triggerTurn: false})`.
- The daemon side of the socket is `ControlServer`
  (`extensions/scufris/desktop/server.ts`, 747 lines): flock
  ownership, 0600 socket, broadcast fan-out, submission dedup.
  Widgets ride the same server; the popup Pi process
  (`SCUFRIS_DAEMON=1`) is the only server.
- `skills/` is distributed via `pi.skills: ["./skills"]` plus explicit
  `--skill` argv in `nix/launcher.nix`; the launcher checks assert
  exact argv, so a new `skills/widgets` means launcher + checks edits.
- CI (`npm run check` + `nix flake check -L`) does not run the Python
  unittest suite, cargo, ruff, or shellcheck - those are local.
