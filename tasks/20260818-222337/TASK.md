# Build Scufris, a local personal assistant with agent-driven widgets

- STATUS: OPEN
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
- The Nix shell provides Node.js 22, Python 3, Bash, Git, tmux, jq, ShellCheck, Ruff, and Alejandra. Alejandra is the flake formatter.
- Initialize Git on `master`. Use the MIT license.

### Repository documentation and conventions

- Keep `README.md` limited to the project title and Quickstart commands.
- Keep repository structure and language rules in `CONVENTIONS.md`.
- Put durable documentation in `docs/` as an mdBook when the first durable page is needed. Do not add an empty documentation scaffold or mdBook dependency.
- Keep design evidence and work records in the owning task directory until they become durable documentation.

### Assistant and agent mediation

- Build the assistant outside this repository. Use Pi as the fast foreground conversation harness.
- Implement orchestration as one small TypeScript Pi extension. Do not add MCP, a supervisor daemon, an agent runner, or an RPC bridge.
- Run the actual delegated Pi, Claude Code, or future harness process in a tmux window. Keep harness differences behind small launch, send, inspect, and stop adapters.
- Give every delegated coding job its own Git worktree. Never let concurrent coding agents share a checkout.
- Place worktrees under `${XDG_CACHE_HOME:-$HOME/.cache}/sprouts/<project>/<branch>`. Interoperate with `sprout` when useful, but do not require it; an internal implementation must preserve the same path convention.
- Use Plannotator as the local pull-request surface for coding jobs. Run `plannotator review --git` in the isolated worktree and return requested changes to the same worker for another committed revision.
- Require `sprout sync <feature>` before every review. The worker reruns applicable checks after synchronization, records the evidence in `report.md`, and only then reports `review-ready:`. The extension verifies that the current landing-target commit is an ancestor of the clean feature worktree before opening Plannotator.
- Bind approval to the exact feature and landing-target commits shown during review. If either commit or the worktree changes before landing, invalidate approval and repeat synchronization, checks, and review.
- Treat explicit Plannotator approval as authorization to land without another confirmation. Until review exposes structured approval output, require an exact full-output match for the configured approval response. Fail closed on feedback, closure without feedback, unknown output, or process failure.
- Run `sprout land --dry-run` after approval, then land with `sprout land` or an equivalent guarded local operation. Never push to a forge merely to obtain pull-request workflow semantics.
- Let the extension own one fixed one-second polling loop for all jobs. Start it with the Pi session and stop it during Pi shutdown. Coalesce changes found in one cycle and never emit an event for unchanged status content.
- Give every job a directory containing immutable `prompt.md`, append-only `status`, and worker-written `report.md`. Require UTF-8 with LF line endings. A tmux window is eligible for orphan discovery only when its matching job directory exists.
- Limit `prompt.md` and `report.md` to 1 MiB each, `status` to 256 KiB, and each status line to 2 KiB. Parse only complete newline-terminated `<state>: <summary>` lines. The worker writes `report.md` before publishing its related status line. Surface malformed, unknown, or oversized input as a protocol-error follow-up without interpreting it or automatically stopping the worker.
- Put complete initial and long follow-up instructions in files. Use short tmux submissions for ordinary steering.
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

- Use native Pi extension tools for widget discovery, query, open, update, focus, list, and close operations.
- Keep data queries separate from visual window operations so simple questions do not need to open a widget.
- Make every `open` operation create a new surface, runtime instance, and native window with a generated `surface_id`. Closing the window deletes that surface and instance; opening the same widget and inputs afterward creates a new surface normally. Omit implicit reuse, semantic deduplication, and a `show` operation from version 1. Require explicit surface IDs for later update, focus, and close operations.

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

- What exact allowlisted launch commands and permission modes should the Pi and Claude Code tmux adapters use?
- Which repository checks are applicable to each coding job, and how does the extension verify the worker recorded post-sync evidence before opening Plannotator?
- Which permissions, credentials, prompts, and approval requests can delegated harnesses expose safely?
- How should a worker process exit that has no matching terminal status be reported?

### Widget service and desktop shell

- Which current dashboardd backend-process, event, health, shared-state, and frontend SDK contracts move into `dashboardd-runtime` unchanged?
- Which contracts must change when instances no longer belong to Dashboard composition?
- Which Tauri APIs and Linux native dependencies are required for a hidden resident tray process, UI-thread window creation, X11 focus, and clean shutdown?
- How should startup validate and safely replace a stale same-user socket without following a malicious path?
- How do window placement, sizing, focus, refresh, multi-monitor behavior, and cleanup work?
- How does an agent retain and use a returned surface ID for explicit update, focus, and close operations?
- How does a Task Artifact surface validate and resolve a direct project/worktree/task reference?

### Tool boundary

- Define narrow native Pi tool schemas. Avoid exposing generic shell, arbitrary URLs, raw filesystem paths, or unrestricted window creation.
- Define separate read/query and visual surface operations.
- Measure native tool and standalone-window latency on the foreground path.

### Safety and operations

- Define capability grants for repositories, commands, network use, credentials, harnesses, and widget types.
- Define minimal audit events, resource limits, timeouts, cancellation, unexpected-exit reporting, and orphan cleanup without adding recovery machinery.
- Identify sensitive values that must never enter widget payloads, logs, model context, or browser storage.

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

## Non-goals

- Implement the full personal assistant in this task.
- Add all missing widgets.
- Build voice input, wake-word detection, home automation, or cloud synchronization.
- Make dashboardd the source of truth for projects or tasks.
- Let an LLM issue unrestricted shell or desktop commands through the widget API.
