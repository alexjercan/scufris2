# Build Scufris, a local personal assistant with agent-driven widgets

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: assistant,agents,orchestration,tauri,widgets,research

## Goal

Research, design, and plan Scufris, a local, low-latency personal assistant that delegates project work to specialized coding agents and opens dashboardd widget surfaces on demand.

Scufris belongs in the separate `scufris2` project. Dashboardd 0.2.0 is its external widget runtime and presentation dependency.

## User outcomes

- User can hold a fast, non-blocking conversation with one primary assistant.
- User can delegate long-running work without blocking the primary conversation.
- User can inspect, steer, stop, and collect results from delegated agents.
- User can select Pi or Claude Code for each delegated task. The design can add other harnesses later without changing its core contracts.
- User can ask for projects, tasks, machine telemetry, LLM usage, or similar information and receive a focused widget window instead of opening a full dashboard.
- User can open a specific task artifact from a project and task ID.
- Spawned widgets can refresh and run independently until the user or assistant closes them.

## Constraints

- Local-first. Keep project data, credentials, agent transcripts, and widget state local unless an explicitly selected model or tool requires network access.
- Keep the foreground path small and responsive. Long reasoning, tool work, and coding run in delegated sessions.
- Keep model selection configurable. Initial candidates include `gpt-5.6-sol` with medium thinking and `gpt-5.6-luna` with xhigh thinking.
- Do not make the primary assistant wait for delegated work. Deliver progress and completion as events.
- Do not couple orchestration semantics to one coding harness.
- Reuse dashboardd widget packages and protocols where practical. Do not require a permanent dashboard composition to show an ad hoc widget.
- Treat agent execution and widget spawning as privileged local operations with explicit policy, path boundaries, audit records, and cancellation.
- Prefer clean contracts over compatibility with the current prototype.

## Accepted design

### Project and implementation shape

- The project and repository name is `scufris2`. The user-facing assistant is Scufris.
- Package Scufris as a Pi package. Keep one narrow TypeScript Pi extension for Pi lifecycle events, native tools, job state, and the fixed polling loop.
- Prefer Agent Skills for model-facing workflows. Put deterministic process and filesystem mechanics in small Bash or Python scripts owned by those skills or by the extension.
- Use Python's standard library unless a concrete requirement justifies a package. Put Node runtime libraries in `dependencies`; put Pi-provided APIs in `peerDependencies`; use npm for TypeScript development dependencies.
- Use a minimal Nix flake for the reproducible development shell and external executables. Npm owns Pi and TypeScript packages.
- Keep research, protocol, architecture, and implementation-plan artifacts in this task directory until they become durable project documentation.

### Repository scaffold

```text
scufris2/
  AGENTS.md
  CONVENTIONS.md
  LICENSE
  README.md
  flake.nix
  flake.lock
  package.json
  package-lock.json
  tsconfig.json
  .gitignore
  extensions/
    scufris/
      index.ts
  tasks/
    20260818-222337/
      TASK.md
```

- Do not add empty skill or script placeholders. Add `skills/<skill>/SKILL.md`, skill-owned helpers, or top-level scripts with their first tested behavior.
- The initial Pi package exports `extensions/scufris/index.ts`. It gains skill paths when the first skill exists.
- Pin development against Pi 0.84.2. Keep the Pi runtime peer range open because Pi provides its own extension API.
- The Nix shell provides Node.js 22, Python 3, Bash, Git, tmux, ShellCheck, Ruff, and Alejandra. Alejandra is the flake formatter.
- Initialize Git on `master`. Use the MIT license.

### Repository documentation and conventions

- Keep `README.md` limited to the project title and Quickstart commands.
- Keep repository structure and language rules in `CONVENTIONS.md`.
- Put durable documentation in `docs/` as an mdBook when the first durable page is needed. Do not add an empty documentation scaffold or mdBook dependency.
- Keep design evidence and work records in the owning task directory until they become durable documentation.

### Version 1 scope and project policy

- Handle direct user requests only. Do not add host-event triggers, background automation rules, or proactive workflows.
- Version 1 delegates only in the current trusted Git repository and does not maintain project configuration or a global project registry.
- Do not add a project harness allowlist, filesystem sandbox, or network sandbox. Workers run with the local user's authority. Nix system rollback is the accepted machine recovery boundary.
- Keep harness selection in the spawn request. Version 1 implements Pi and Claude Code adapters; later adapters do not change the native job-tool contract.
- Default Pi to `openai/gpt-5.6-sol` with medium thinking and Claude to `opus` with xhigh thinking. Let the foreground orchestrator override model and thinking for each spawn when the user requests another model.
- Launch Claude Code with `--dangerously-skip-permissions`. Pi already has unrestricted local-user authority.
- Keep dashboardd presentation-only. Version 1 adds no machine-data query, task-query, host-event, or generic data-query tool. Future agent-facing data sources require separate narrow tools.

### Assistant and agent mediation

- Build the assistant in this repository, outside dashboardd. Use Pi as the fast foreground conversation harness.
- Implement orchestration as one small TypeScript Pi extension. Do not add MCP, a supervisor daemon, an agent runner, or an RPC bridge.
- Run the actual delegated Pi, Claude Code, or future harness process in a tmux window. Keep harness differences behind small launch, send, inspect, and stop adapters.
- Give every delegated coding job its own Git worktree. Never let concurrent coding agents share a checkout.
- Use `sprout` to create, synchronize, land, and remove isolated worktrees under its standard cache location.
- Use Plannotator as the local pull-request surface for coding jobs. Call its public Pi event API with the `code-review` action in the isolated worktree and return requested changes to the same worker for another committed revision. Do not launch the CLI or use private Plannotator interfaces.
- Require one committed revision per review round. Request `diffType: "since-base"` for initial and final approval reviews. A reviewer can request a separate `diffType: "last-commit"` session for a focused feedback delta; ignore approval from that focused session. Plannotator 0.27.3 returns structured `approved` and `feedback` fields through this event API.
- Require `sprout sync <feature>` before every review. The worker reads repository instructions, reruns the applicable checks after synchronization, records commands and outcomes in `report.md` and the repository task record when required, and only then reports `review-ready:`. The extension verifies that the current landing-target commit is an ancestor of the clean feature worktree before opening Plannotator.
- Bind approval to the exact feature and landing-target commits shown during review. If either commit or the worktree changes before landing, invalidate approval and repeat synchronization, checks, and review.
- Treat `approved: true` from the final `since-base` Plannotator event response as authorization to land without another confirmation. Fail closed on feedback, closure without approval, malformed responses, or review errors.
- Run `sprout land --dry-run` after approval, then land with `sprout land` or an equivalent guarded local operation. Never push to a forge merely to obtain pull-request workflow semantics.
- Let the extension own one fixed one-second polling loop for all jobs. Start it with the Pi session and stop it during Pi shutdown. Coalesce changes found in one cycle and never emit an event for unchanged status content.
- Give every job a directory containing immutable `prompt.md`, append-only `status`, and worker-written `report.md`. Require UTF-8 with LF line endings. A tmux window is eligible for orphan discovery only when its matching job directory exists.
- Limit `prompt.md` and `report.md` to 1 MiB each, `status` to 256 KiB, and each status line to 2 KiB. Parse only complete newline-terminated `<state>: <summary>` lines. The worker writes `report.md` before publishing its related status line. Surface malformed, unknown, or oversized input as a protocol-error follow-up without interpreting it or automatically stopping the worker.
- Put complete initial instructions in `prompt.md`. Keep later steering short enough for one tmux submission.
- Submit steering literally through a tmux buffer: load and paste the text once, wait a short fixed delay, then send Enter once. Never automatically retype the text or retry Enter. A tmux failure fails the send; an uncertain harness result remains uncertain and requires explicit inspection or intervention.
- Require delegated agents to append sparse `working:`, `needs-decision:`, `blocked:`, `review-ready:`, `done:`, or `failed:` lines to their status file. Coding agents use `review-ready:` only after committing the proposed revision and recording its checks in `report.md`; this starts or restarts the Plannotator review loop. Reserve `done:` for terminal work that requires no review or landing.
- Show `working:` as a non-blocking Pi notification or compact status update. Let `review-ready:` start review and show a compact notification. Deliver `needs-decision:`, `blocked:`, `done:`, and `failed:` as Pi follow-up messages so the foreground assistant mediates between the user and worker.
- Expose native Pi operations for spawn, list, inspect, send, and stop. Keep the exact schemas narrow and harness-neutral.
- On normal Pi shutdown, stop extension-owned agent windows. After an unexpected exit, scan matching job directories and tmux windows once at startup, report possible orphans, and ask whether to retain or close them. Do not auto-adopt, restart, or reconstruct work.
- Use FirstMate only as design evidence. Its raw harness windows, prompt/status/report files, polling, and Pi wake injection validate this shape, but its agent distribution and recovery machinery are out of scope. Evidence inspected at `kunchenguid/firstmate` commit `03bb1d8b78a8632ae2d9cea4c10868eb100e885e`.

### Landed dashboardd foundation

- Dashboardd 0.2.0 provides the completed external widget infrastructure. Scufris does not own widget execution or native windows.
- `dashboardd-runtime` is a transport-free Rust domain library. `dashboardd-server` owns HTTP, SSE, OpenAPI, and Dashboard assets. `dashboardd-desktop` embeds an independent runtime and owns Tauri IPC, tray state, and native surfaces; it has no external-runtime or loopback-HTTP mode.
- Runtime instances are independent memory-only resources. Browser-local documents own Dashboard composition. Desktop surfaces own their instance lifecycle.
- Typed direct inputs use `{ "type": "<versioned-manifest-type>", "value": <opaque-json> }` envelopes. Widget-owned launch frontends resolve friendly required inputs where available.
- The desktop control socket is `$XDG_RUNTIME_DIR/dashboardd-desktop.sock`. Protocol version 2 exposes discover, open, update, focus, list, close, and quit through `dashboardctl` with metadata-only audit records.
- Every desktop open creates a new surface, instance, and window. Later operations require its explicit surface ID.
- The Home Manager deployment starts both browser and desktop services automatically. Browser access listens on `0.0.0.0:8000`; clean desktop Quit does not restart it.
- Dashboardd and Today remain pinned external packages. Scufris consumes their public commands and protocols instead of depending on their worktrees.

### Assistant widget tools

- Use native Pi extension tools for widget discovery, open, update, focus, list, and close operations.
- Make every `open` operation create a new surface, runtime instance, and native window with a generated `surface_id`. Closing the window deletes that surface and instance; opening the same widget and inputs afterward creates a new surface normally. Omit implicit reuse, semantic deduplication, and a `show` operation from version 1. Require explicit surface IDs for later update, focus, and close operations.
- Track surfaces opened by Scufris through the same one-second loop. If a user closes one through i3 or native controls, remove it from extension state, show a compact notification, and add a model-visible custom message without triggering a turn. Never call close again or reopen it automatically.

## Completed dashboardd dependency

- Dashboardd task `20260820-094041`, "Build desktop-hosted standalone widget surfaces", is closed in the dashboardd repository.
- Dashboardd 0.2.0 and Today 0.3.1 are released, pinned, deployed, and verified with browser and native widget discovery.
- Scufris work starts at the Pi integration boundary. Do not recreate the completed runtime or desktop lifecycle spike.

## Research questions

### Foreground assistant

- Which Pi extension structure and model configuration provide the lowest practical input-to-response latency?
- How should the foreground model decide between answering, querying local data, opening a widget, and delegating work?
- Which progress events belong in compact UI, and which should trigger a mediated model follow-up?

### Agent mediation

- Measure complete Pi and Claude worker launch-to-first-status latency.
- Verify that workers follow repository instructions when selecting checks and recording evidence.
- Verify process-exit reporting when no matching terminal status exists.

### Widget integration

- Verify explicit update, focus, close, and external-close tracking with returned surface IDs.
- Verify Task Artifact references derived from foreground repository context.
- Measure native tool and standalone-window latency on the foreground path.

### Safety and operations

- Verify resource limits, timeouts, cancellation, unexpected-exit reporting, and orphan cleanup without adding recovery machinery.

## Required artifacts

- `SPIKE.md`: findings with links to Pi documentation, Pi examples, FirstMate evidence, Claude Code interfaces, Tauri window APIs, and applicable dashboardd code.
- `ARCHITECTURE.md`: recommended process boundaries, ownership, data flow, trust boundaries, and lifecycle diagrams.
- `PROTOCOL.md`: draft native Pi orchestration tools, job-file/status grammar, and dashboard widget control API with example messages.
- `PLAN.md`: phased implementation plan split between dashboardd changes and a new assistant repository. Include dependencies, risks, and small end-to-end milestones.
- The retained `dashboardd-desktop` tray, Unix-socket, and static-window lifecycle spike.

## Acceptance criteria

- Records measured or directly observed evidence for the important latency and lifecycle claims.
- Compares at least Pi and Claude Code as delegated harnesses.
- Identifies existing subagent solutions but does not adopt one only because it exists.
- Separates the assistant extension, delegated harness processes, widget runtime, and desktop presentation responsibilities.
- Defines how a request such as `show CPU usage` opens a standalone live widget.
- Defines how a request such as `show the artifact for my current task` resolves context and opens or updates one Task Artifact window.
- Defines non-blocking job progress, steering, cancellation, completion, shutdown, and orphan discovery behavior.
- Defines explicit security and permission boundaries.
- Ends with one recommended architecture and a sequence of independently testable implementation tasks.

## Artifact record

- Added `SPIKE.md`, `ARCHITECTURE.md`, `PROTOCOL.md`, and `PLAN.md` after reconciling the completed dashboardd dependency with the accepted skills-and-scripts implementation shape.
- Inspected installed Pi 0.84.2 documentation and examples, Claude Code 2.1.220 CLI and permission documentation, FirstMate commit `03bb1d8b78a8632ae2d9cea4c10868eb100e885e`, Plannotator 0.27.3 source, and dashboardd 0.2.0 control code and task evidence.
- Measured local offline Pi startup with and without the empty extension and measured dashboardctl discover and list process latency. Provider response, visible window, and worker first-status latency remain implementation-slice measurements.
- Chose explicit unrestricted local-user workers and current-repository-only operation for version 1. Removed project configuration because defaults and spawn overrides cover the required behavior. Project trust, visible tmux ownership, isolated worktrees, exact stop targeting, and Nix rollback remain the practical recovery boundaries.
- Kept dashboardd presentation-only and deferred agent-facing data sources and host-event triggers.
- Removed `.scufris/config.json`, configured checks, approval hashing, machine-readable worker evidence, oversized steering-file machinery, the Plannotator upstream proposal, and unused jq. Built-in model defaults remain overridable per spawn. Workers select checks from repository instructions and record visible evidence in `report.md` and the project task record when required.
- Replaced the Plannotator CLI and output parsing with its public Pi `plannotator:request` event API. The `code-review` action accepts an explicit diff type and returns structured approval and feedback. This removes a process adapter, approval text parsing, and the unsupported CLI-selector workaround.
- Verification: `npm run check` and `nix flake check` pass. The flake check evaluated the local system and omitted incompatible configured systems.

## Non-goals

- Implement the full personal assistant in this task.
- Add all missing widgets.
- Build voice input, wake-word detection, home automation, or cloud synchronization.
- Make dashboardd the source of truth for projects or tasks.
- Let an LLM issue unrestricted shell or desktop commands through the widget API.
