# Scufris integration spike

## Scope

Evidence gathered on 2026-08-21 for the foreground Pi extension, delegated Pi and Claude Code workers, Plannotator review, tmux control, and dashboardd 0.2.0.

Dashboardd desktop and runtime research is complete. This spike starts at its released public control boundary. It does not reopen the Tauri implementation design.

## Versions observed

| Component                   | Version |
| --------------------------- | ------: |
| Pi                          |  0.84.2 |
| Claude Code                 | 2.1.220 |
| Plannotator                 |  0.27.3 |
| tmux                        |    3.7b |
| dashboardd control protocol |       2 |
| dashboardd                  |   0.2.0 |
| Today                       |   0.3.1 |

## Pi findings

Sources:

- [Pi extension documentation](https://github.com/earendil-works/pi-mono/blob/v0.84.2/packages/coding-agent/docs/extensions.md)
- [Pi skills documentation](https://github.com/earendil-works/pi-mono/blob/v0.84.2/packages/coding-agent/docs/skills.md)
- [Pi package documentation](https://github.com/earendil-works/pi-mono/blob/v0.84.2/packages/coding-agent/docs/packages.md)
- [Pi model documentation](https://github.com/earendil-works/pi-mono/blob/v0.84.2/packages/coding-agent/docs/models.md)
- [sendUserMessage example](https://github.com/earendil-works/pi-mono/blob/v0.84.2/packages/coding-agent/examples/extensions/send-user-message.ts)
- [subagent example](https://github.com/earendil-works/pi-mono/tree/v0.84.2/packages/coding-agent/examples/extensions/subagent)

Direct observations:

- Extension factories must not start long-lived resources. `session_start` and on-demand tool calls are valid start points. `session_shutdown` is the cleanup point.
- `session_shutdown` runs for quit, reload, new, resume, and fork. Session-scoped delegated jobs must stop for every reason.
- `ctx.ui.notify` and `ctx.ui.setStatus` provide compact, non-model UI updates.
- `pi.sendMessage` can add a custom model-visible message. `deliverAs: "followUp"` and `triggerTurn: true` wake an idle foreground model for actionable worker events.
- A custom message without `triggerTurn` can record an externally closed widget without creating an unsolicited turn.
- Agent Skills provide progressive disclosure. Only the skill name and description are always in context. This supports thin model-facing workflows around narrow native tools.
- Pi custom tools use TypeBox. String enums should use `StringEnum` for provider compatibility.
- Pi package runtime imports must be dependencies or Pi-provided peer dependencies.
- The bundled subagent example proves separate Pi processes and structured progress are practical. It is not suitable as the Scufris core because it is foreground tool execution, uses process output as the result channel, accepts model-provided working directories, and has no tmux, durable job protocol, worktree landing, or orphan boundary.

Foreground routing recommendation:

- Do not add a classifier or routing daemon.
- Let the foreground model answer normally.
- Use built-in read tools for information already available as files.
- Load a widget skill when the user asks to open or control visual UI.
- Load a delegation skill when work is long-running or should not block conversation.
- Keep version 1 reactive to user messages only.

## Startup and control latency

Method:

- Eight sequential local invocations per command.
- Python `time.perf_counter_ns` around `subprocess.run`.
- Output discarded.
- Pi ran offline to remove startup network variation.
- Results are process-start measurements, not model response latency.

| Operation                                     |   Median |  Minimum |  Maximum | Samples |
| --------------------------------------------- | -------: | -------: | -------: | ------: |
| `pi --offline --no-extensions --list-models`  | 554.1 ms | 512.5 ms | 633.7 ms |       8 |
| Same command with the empty Scufris extension | 558.7 ms | 503.0 ms | 615.1 ms |       8 |
| `dashboardctl discover`                       |  50.2 ms |  19.1 ms |  51.3 ms |       8 |
| `dashboardctl list`                           |  50.2 ms |  49.9 ms |  50.5 ms |       8 |

Observed empty-extension median difference: 4.6 ms. The sample is too small to claim a stable causal cost, but it shows no material startup regression.

Not measured:

- Provider input-to-first-token latency.
- Native tool call to visible dashboard window latency.
- A complete delegated worker launch to first status event.

Measure those in the owning vertical slices. Do not infer them from process-start results.

## Delegated harness comparison

### Pi

Observed CLI surface:

```text
pi --model <provider/model> --thinking <level> --tui-mode regular <prompt>
```

Properties:

- Positional prompt starts an interactive run.
- Model and thinking level are explicit.
- Pi has no permission system. A worker has local-user authority.
- Project trust can block a fresh worktree. Scufris can pass `--approve` because it only launches in the current user-trusted repository.
- One positional prompt must point to the immutable job prompt. Multiple positional arguments become separate queued messages.

Recommended launch shape:

```text
pi --approve --tui-mode regular [--model MODEL] [--thinking LEVEL] "Read and follow JOB/prompt.md"
```

### Claude Code

Sources:

- [Claude Code CLI reference](https://code.claude.com/docs/en/cli-reference)
- [Claude Code permissions](https://code.claude.com/docs/en/permissions)

Observed CLI surface:

```text
claude --model <model> --effort <level> --dangerously-skip-permissions <prompt>
```

Properties:

- Positional prompt starts an interactive run.
- `--allow-dangerously-skip-permissions` only enables bypass as an option. It does not activate bypass.
- `--dangerously-skip-permissions` activates unrestricted execution and matches the accepted local-user trust model.
- `--worktree` is not used. Scufris creates the worktree with sprout.
- Claude-specific background agents are not used. The Claude process itself remains visible in tmux.

Recommended launch shape:

```text
CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION=false claude --dangerously-skip-permissions [--model MODEL] [--effort LEVEL] "Read and follow JOB/prompt.md"
```

### Shared adapter boundary

Each adapter owns only:

- Executable and arguments.
- Model and effort argument mapping.
- Exit command: `/quit` for Pi, `/exit` for Claude.
- Known readiness and trust behavior.

Spawn, job files, tmux windows, status parsing, report limits, steering, stop, review, and landing remain harness-neutral.

## FirstMate evidence

Source: [FirstMate commit 03bb1d8](https://github.com/kunchenguid/firstmate/tree/03bb1d8b78a8632ae2d9cea4c10868eb100e885e)

Relevant evidence:

- [`bin/backends/tmux.sh`](https://github.com/kunchenguid/firstmate/blob/03bb1d8b78a8632ae2d9cea4c10868eb100e885e/bin/backends/tmux.sh) creates named windows, captures bounded pane tails, sends literal text, and targets exact windows for cleanup.
- [`bin/fm-send.sh`](https://github.com/kunchenguid/firstmate/blob/03bb1d8b78a8632ae2d9cea4c10868eb100e885e/bin/fm-send.sh) demonstrates why text must be typed once and why uncertain Enter delivery must not cause blind retyping.
- [`docs/tmux-backend.md`](https://github.com/kunchenguid/firstmate/blob/03bb1d8b78a8632ae2d9cea4c10868eb100e885e/docs/tmux-backend.md) records tmux liveness, composer, and delivery hazards.
- [`harness-adapters/SKILL.md`](https://github.com/kunchenguid/firstmate/blob/03bb1d8b78a8632ae2d9cea4c10868eb100e885e/.agents/skills/harness-adapters/SKILL.md) records Pi follow-up wake injection and Pi and Claude launch behavior.

What Scufris retains:

- Visible direct harness processes.
- One isolated Git worktree per job.
- Durable prompt, status, and report files.
- Sparse status events.
- Literal one-time steering.
- Harness adapters.

What Scufris rejects:

- Agent hierarchy and distribution.
- Watcher chains and recovery daemons.
- Rendered-pane state as normal protocol truth.
- Automatic adoption or restart.
- Multiple backend types.

## Plannotator findings

Sources:

- [Plannotator v0.27.3](https://github.com/backnotprop/plannotator/tree/v0.27.3)
- [Shared Pi event API](https://github.com/backnotprop/plannotator/blob/v0.27.3/apps/pi-extension/README.md#shared-plannotator-event-api)

Observed behavior:

- The public Pi extension event API accepts a `plannotator:request` event with action `code-review`.
- Its payload accepts `cwd`, `defaultBranch`, and `diffType`. Git diff types include `since-base` and `last-commit`.
- Its response includes structured `approved`, `feedback`, `annotations`, and `exit` fields.
- The event API waits for the browser decision before responding.
- The CLI does not expose the same diff-type argument and returns rendered text. Scufris does not need the CLI or output parsing.
- Code review does not retain a GitHub-style baseline for changes since the last review.

Recommended review loop:

1. Synchronize the feature branch with its landing target.
2. Read repository instructions, run applicable checks, and commit the revision.
3. Record target SHA, feature SHA, synchronization, check commands, results, and limitations in `report.md` and the project task record when required.
4. Publish `review-ready:`.
5. Verify clean worktree and ancestry.
6. Emit a `code-review` request with the exact worktree and `diffType: "since-base"`.
7. If structured feedback is returned, send it to the same worker and require one new fix commit.
8. The reviewer can request a separate `diffType: "last-commit"` session to inspect that focused delta. Ignore approval from this session.
9. Emit a new `diffType: "since-base"` request for final approval.
10. Accept only its structured `approved: true` result, then recheck both SHAs and cleanliness before dry-run landing.

Use the documented event channel as a plain string. Do not import Plannotator internals, invoke private HTTP endpoints, stage temporary patches, or parse CLI prose.

## Dashboardd and Tauri evidence

Public Scufris boundary:

- [`dashboardd-desktop-control`](../../../dashboardd/crates/dashboardd-desktop-control/src/lib.rs) defines protocol version 2, 64 KiB LF-terminated JSON messages, and typed discover, open, update, list, focus, close, and quit commands.
- [`dashboardctl`](../../../dashboardd/crates/dashboardd-desktop-control/src/bin/dashboardctl.rs) uses one Unix connection per synchronous command with five-second read and write timeouts.
- [Desktop user guide](../../../dashboardd/docs/src/user-guide/desktop.md) records independent opens, complete input replacement, native close cleanup, and Home Manager lifecycle.
- [Completed dashboard task](../../../dashboardd/tasks/20260820-094041/TASK.md) records live X11/i3 CPU and Tatr Artifact windows, focus, close, Quit, tray behavior, and deployed package verification.

Underlying Tauri evidence is already retained in dashboardd:

- [Tauri Manager API](https://docs.rs/tauri/2.11.5/tauri/trait.Manager.html)
- [Tauri JavaScript WebviewWindow API](https://v2.tauri.app/reference/javascript/api/namespacewebviewwindow/)
- `dashboardd-desktop` uses `WebviewWindowBuilder`, `run_on_main_thread`, `set_focus`, window-destroy events, and `TrayIconBuilder`.

Scufris must call `dashboardctl`. It must not import dashboardd Rust crates, access Tauri commands, open arbitrary URLs, or reproduce native lifecycle behavior.

## Conclusions

- One small Pi extension is enough for lifecycle, tools, in-memory ownership, and one polling loop.
- Skills are the correct model-facing routing layer.
- Small Python or Bash helpers are the correct deterministic mechanics layer.
- Direct tmux workers are simpler than a daemon or nested agent framework.
- Pi and Claude can share one job protocol despite different launch flags.
- Plannotator's public Pi event API provides explicit diff selection and structured review decisions.
- Dashboardd is presentation-only. Agent-facing information tools are separate future work.
- Version 1 remains reactive. Host-event automation is future work.
