# Scufris implementation plan

## Goal

Deliver a small reactive personal assistant integration that:

- Keeps Pi foreground conversation responsive.
- Delegates work to visible Pi or Claude workers.
- Uses isolated sprout worktrees.
- Mediates sparse progress and completion.
- Reviews and lands exact local revisions.
- Opens and tracks dashboardd widget windows.

## Already complete

### Dashboardd

No dashboardd product work remains for Scufris version 1.

Released and deployed:

- Dashboardd 0.2.0.
- Today 0.3.1.
- Desktop protocol version 2.
- `dashboardctl` discover, open, update, list, focus, close, and quit.
- Independent native surfaces and runtime instances.
- Tatr Artifact direct input.
- Home Manager browser and desktop services.

Scufris must not modify dashboardd product code. If a missing public dashboard capability is found, record a separate dashboardd task instead of bypassing the public interface.

### Scufris scaffold and design

Complete:

- Repository scaffold.
- Pi package metadata.
- Empty extension load.
- Nix development shell.
- Migration from dashboardd.
- `SPIKE.md`, `ARCHITECTURE.md`, `PROTOCOL.md`, and this plan.

## Implementation rules

- Add each skill or helper with its first tested behavior.
- Keep the independent delegation and widget TypeScript extensions limited to Pi APIs, their own in-memory ownership, and timer coordination.
- Use Python standard library for structured filesystem and process mechanics.
- Use Bash only for small tmux or harness adapter operations where shell behavior is the subject.
- Use argument arrays. Do not compose model text into shell commands.
- Build vertical slices. Each milestone must run end to end.
- Prefer integration tests with temporary Git repositories and real tmux.

## Milestone 1: Fake-worker job loop

User outcome:

- User can spawn, list, inspect, steer, and stop a deterministic fake worker without blocking Pi.

Add:

- Built-in harness defaults and per-spawn overrides.
- Job ID and immutable job-file creation.
- One dedicated tmux server socket, one job session, exact window naming, and fixed job-ID-only launch helper.
- Fake interactive harness fixture.
- Append-only status parser with byte offsets and limits.
- One delegation-owned one-second loop.
- Native agent tools.
- Session shutdown cleanup.
- Startup orphan reporting without adoption.
- Delegation skill with its first tool workflow.

Tests:

- Spawn creates one job directory, worktree, and tmux window.
- Prompt is immutable and file modes are correct.
- Complete status lines emit once.
- Partial lines wait for LF.
- Malformed UTF-8, CRLF, oversized lines, unknown states, and oversized files produce one protocol error.
- Steering pastes literal metacharacters once.
- Stop targets only the recorded window and is idempotent.
- Runtime and tests never alter the user's default tmux server.
- Process exit without terminal status produces a failure follow-up.
- Session shutdown stops all owned windows.
- Matching directory/window pairs are reported as possible orphans and not adopted.

Checks:

```text
npm run check
python3 -m unittest
shellcheck <added-shell-scripts>
ruff check <added-python>
```

Do not add Pi or Claude launching yet.

## Milestone 2: Pi worker adapter

User outcome:

- User can delegate one real task to Pi and continue foreground conversation.

Add:

- Pi executable availability check.
- Launch argument mapping for model and thinking level.
- `--approve` and regular TUI mode.
- Immutable prompt pointer.
- Worker instructions for status, report, checks, commits, and no landing.
- Pi exit adapter.

End-to-end example:

1. Create a temporary trusted Git repository with agent instructions.
2. Spawn Pi for a bounded documentation change.
3. Observe `working:` without a foreground model turn.
4. Inspect the report.
5. Stop or receive terminal status.
6. Confirm foreground Pi remained usable.

Measure:

- Tool call to returned job ID.
- Spawn to tmux window.
- Spawn to first valid status event.
- Poll event to visible Pi notification.

## Milestone 3: Claude Code worker adapter

User outcome:

- User can select Claude for the same job contract.

Add:

- Claude executable availability check.
- `--dangerously-skip-permissions` launch.
- Model and effort mapping.
- Prompt-suggestion suppression for reliable empty composer behavior.
- Claude exit adapter.

Tests:

- Launch command activates bypass rather than only allowing it.
- Scufris owns the worktree; Claude `--worktree` is absent.
- Job files and status behavior are identical to Pi.
- Unsupported thinking fails before tmux creation. An invalid model causes a visible harness failure and normal job cleanup.

End-to-end example:

- Run the same bounded fixture once with Pi and once with Claude. Compare first-status latency, report quality, cancellation, and terminal behavior. Record observed versions.

## Milestone 4: Review feedback and guarded landing

User outcome:

- User can review a committed worker revision locally, return feedback, and land exact approved code without a forge push.

Add:

- Current feature and landing SHA checks.
- Cleanliness and ancestry guards.
- Human-readable report presentation.
- Public `plannotator:request` event integration with explicit diff types.
- Structured approval and feedback handling.
- Feedback submission to the same worker.
- One committed fix revision per round.
- Dry-run and guarded local land.

Review flow tests:

- Reject dirty feature worktree.
- Reject stale landing SHA.
- Reject feature SHA mismatch.
- Show the worker's selected checks, results, and limitations in its report and project task record when applicable.
- Ignore approval from a `last-commit` review and open a new `since-base` review.
- Reject closure without feedback.
- Reject unavailable, error, or malformed event responses.
- Invalidate approval after any Git change.
- Run dry-run before land.
- Never push.
- Preserve job evidence after land.

Manual playtest:

1. Open full `since-base` review.
2. Submit feedback.
3. Worker creates one fix commit.
4. Use `Last commit` for focused delta inspection.
5. Close the focused session and open a fresh full `since-base` review.
6. Approve the exact final revision.
7. Confirm guarded local landing and worktree cleanup.

## Milestone 5: Dashboardd widget tools

User outcome:

- User can ask `show CPU usage` and receive a live native CPU window.
- User can close it through i3 and Scufris knows it is gone.

Add:

- Independent optional widget extension and matching skill.
- dashboardctl process adapter.
- Protocol version 2 response validation.
- Widget discover, open, update, list, focus, and close tools.
- Widget skill.
- In-memory ownership of opened surface IDs.
- Conditional dashboard list polling only while owned surfaces exist.
- External-close custom message without a triggered model turn.

Tests:

- No shell is used.
- Discover output is bounded.
- Open returns and tracks a new surface ID.
- Repeated open creates distinct IDs.
- Update requires inputs or presentation.
- Focus and close require explicit IDs.
- Protocol version mismatch fails.
- Stable dashboard error codes are preserved.
- External close removes ownership once and never triggers reopen or repeated close.
- Session shutdown forgets tracking but leaves user-facing widget windows open.

Live playtest:

1. Ask `show CPU usage`.
2. Confirm the tool discovers `cpu` and opens `full`.
3. Keep chatting while telemetry updates.
4. Close the window through i3.
5. Confirm one compact notification.
6. Ask another question and confirm the model knows the old surface is closed.

Measure:

- Tool call to dashboardctl completion.
- Tool call to visible native window.
- Native close to Scufris closure observation.

## Milestone 6: Tatr Artifact widget workflow

User outcome:

- User can ask `show the artifact for my current task` and receive the correct Task Artifact window.

Add:

- Skill workflow that uses existing foreground file and repository context to identify the current task.
- Deterministic construction of the existing `tatr.task-artifact-reference/v1` input.
- Widget discovery validation before open.
- Conversation retention of returned surface ID for explicit update, focus, or close.

No `scufris_task_get` tool is added. Pi already reads task files. Dashboardd remains presentation-only.

Tests:

- Reject ambiguous or absent current task identity before opening.
- Use opaque project and worktree IDs, strict task ID, and relative artifact name only.
- Open creates a new surface.
- Update replaces the complete atomic artifact reference.
- No absolute path enters the widget input.

Live playtest:

- Open `TASK.md`, change artifact in the same surface, focus it, close it natively, and observe external closure.

## Milestone 7: Hardening and package verification

User outcome:

- Scufris behaves predictably across normal session changes and malformed local state.

Add or verify:

- Bounded helper timeouts.
- No overlapping poll cycles.
- Coalesced same-cycle events.
- Resource cleanup under quit, reload, new, resume, and fork.
- Package skill exports.
- Runnable end-to-end examples retained with owning skills.
- Stable documentation moved to mdBook only when it has its first durable page.

Final checks:

```text
npm run check
nix flake check
python3 -m unittest
shellcheck <all-shell-scripts>
ruff check <all-python>
```

Also rerun:

- Pi extension load and model listing.
- Fake-worker integration suite.
- Real Pi and Claude smoke tests.
- Plannotator event-based feedback and approval playtest.
- CPU and Tatr Artifact widget playtests.

## Dependencies

Required runtime components:

- Pi 0.84.2 development target.
- Claude Code 2.1.220 or a reverified later version.
- Git.
- tmux.
- sprout.
- Plannotator Pi extension 0.27.3 or a reverified later version.
- dashboardctl protocol version 2.
- Python 3.
- Bash.

No new Python package is planned.

## Risks and controls

### Unrestricted worker authority

Risk:

- Pi and Claude can access local-user resources and network services.

Accepted control:

- Explicit local-user trust model, current trusted repository, isolated worktree, visible tmux process, stop tool, and Nix rollback boundary.

Do not describe this as a sandbox.

### TUI submission uncertainty

Risk:

- A successful tmux command does not prove a harness processed steering.

Control:

- Paste text once, send Enter once, report submitted rather than acknowledged, and require explicit inspection before any resend.

### Status corruption

Risk:

- Worker-written files can be malformed, partial, oversized, or hostile to model context.

Control:

- Byte limits, UTF-8 and LF validation, complete-line parsing, narrow states, control-character rejection, deduplicated protocol errors, and no automatic worker stop.

### Foreground wake noise

Risk:

- Progress events can interrupt conversation or cause unnecessary model turns.

Control:

- UI-only `working:` and `review-ready:`, actionable custom follow-ups only, one-second coalescing, and no unchanged events.

### Review confusion

Risk:

- Feedback rounds can review the wrong revision or diff type.

Control:

- Explicit event-request diff types, clean committed revisions, `since-base` initial and final reviews, one fix commit per round, optional non-approving `last-commit` review, and exact SHA binding.

### Landing race

Risk:

- Feature or target changes after approval.

Control:

- Recheck both SHAs, cleanliness, ancestry, and dry-run immediately before local landing.

### External widget closure

Risk:

- Model retains a stale surface ID after user closes a window.

Control:

- Poll only tracked IDs, insert one model-visible closure message, remove ownership, and never automatically reopen.

### Scope growth

Risk:

- Data queries, host events, recovery, and generic automation turn the extension into a supervisor.

Control:

- Version 1 handles direct user requests, delegated jobs, and visual widget control only. New data sources or event triggers require separate accepted designs.

## Completion criteria

- One recommended architecture remains.
- Pi and Claude use one job protocol.
- Foreground conversation stays available during jobs and widgets.
- Job progress, decisions, steering, stop, failure, shutdown, and orphan discovery work end to end.
- Review and landing bind exact local revisions and never push.
- CPU and Tatr Artifact requests open standalone live windows.
- User-driven native close updates model context without an unsolicited turn.
- No dashboardd product change, daemon, MCP bridge, generic data query, host-event trigger, or unrestricted model-facing shell interface is added.
