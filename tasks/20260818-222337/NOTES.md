# Launcher composition note

## Accepted 2026-08-21

Scufris composes with the user's Pi configuration without changing normal Pi sessions.

Launcher precedence:

1. Use `pi` from the caller's `PATH` when present.
2. Otherwise use the Pi package pinned by the Scufris flake or configured through the Home Manager module.
3. Add only the enabled Scufris extensions and skills to that Pi invocation.

This makes system Pi extensions and themes, such as Plannotator and Gruber Darker, available in Scufris. Normal `pi` does not load delegation or widgets. Ambient composition is intentionally environment-dependent. The pinned fallback keeps the flake app usable on systems without Pi.

## Calm presentation accepted 2026-08-21

Scufris starts in Calm mode. `/calm` toggles it for the current Scufris process. The state survives session replacement and reload, then resets to on for a new process. Normal Pi sessions are unaffected.

Calm shows genuine user prompts, final assistant replies, the standard working indicator, Scufris footer status and notifications, and final model, abort, or truncation errors. It hides thinking, intermediate assistant text before tool calls, tool call and result rows, and Scufris job and widget event transcript rows.

Calm changes presentation only. Session storage, model context, resume, compaction, and exports retain the complete content. Keep the required Pi renderer patches isolated in one Scufris-only extension and fail checks when those renderer seams become incompatible.

## Cross-project delegation accepted 2026-08-21

Scufris can run from any directory and delegate into a different discovered Git repository. The model uses an opaque project ID, not a filesystem path. Project IDs are repository paths relative to configured discovery roots, such as `personal/nix.dotfiles`.

`scufris_agent_projects` lists valid IDs. `scufris_agent_spawn.project` selects one. When omitted, spawn uses the current repository if the session is inside one. Outside a repository, an explicit discovered project is required. Unknown, duplicate, non-Git, and escaping targets fail before Sprout runs. Worktree isolation and review guarantees remain unchanged.

## Visible worktree sessions accepted 2026-08-21

Delegated workers use detached Sprout-named worktree sessions on the user's normal tmux server. Scufris rejects a matching existing session, creates the selected feature session and an exact worker window, and leaves the current client unchanged. It never attaches, selects, switches, or kills a session or server. The opaque job ID and exact session, window, and pane IDs bind steering and cleanup independently from display names. Failed panes remain visible for manual debugging.

A Pi worker invokes ambient `pi` directly. It does not invoke the Scufris launcher and does not load Scufris extensions or skills. Safe path and configuration variables from the orchestrator environment are applied to the worker window so stale tmux-server environment does not select another Pi or state directory.

The live cross-project playtest also corrected the Sol provider from `openai/gpt-5.6-sol` to the OAuth-backed `openai-codex/gpt-5.6-sol`.

## Review-ready mediation accepted 2026-08-21

`review-ready` is actionable and triggers one foreground model turn. The prior notification-only behavior assumed automated Plannotator review that was not implemented and left completed work waiting until the user spoke. `working` remains notification-only. Poll offsets ensure each review-ready event triggers once.

## Independent preflight implementation 2026-08-22

Every spawn now stores an immutable narrow review policy. Landable jobs run the exact clean snapshot through a separate read-only Sol session before Plannotator. Non-landable jobs can only finish with `done`.

The private helper owns patch generation, Pi process limits, session identity, environment stripping, strict JSON validation, and pre/post mutation checks. The extension owns feedback-cycle state, exact revision binding, findings routing, Plannotator ordering, approval invalidation, and shutdown cancellation. This split keeps model-facing orchestration narrow and deterministic process mechanics testable without exposing paths or commands through native tools.

The current reviewer session remains as bounded job evidence and verifies correction commits. A fresh sequence deletes the invalidated session before creating a new dedicated session. Reviewer prompt and output scratch data use temporary files or bounded memory and do not remain in job state.

The first fake-Pi fixture wrote escaped newline text instead of JSONL frames. The integration test exposed the error on continued-session parsing. The fixture now emits real LF records. Future process integrations should test the first and second invocation together before adding failure cases.

Verification after `sprout sync preflight-review` reported already up to date:

- `npm run check` - pass, 38 TypeScript tests.
- `python3 -m unittest discover -s tests -p 'test_*.py'` - pass, 26 Python tests.
- `ruff check .` and `ruff format --check .` - pass.
- `shellcheck scripts/scufris-dev` - pass. Other packaged helpers are Python, not ShellCheck inputs.
- `nix fmt -- --check .` - pass.
- `nix flake check` - pass on x86_64-linux. Nix reported the expected unsupported-system omissions and existing unknown custom output warnings.
- `git diff --check` - pass.

## Visible preflight implementation 2026-08-22

The actual Sol reviewer initially ran in one input-disabled `preflight-<review_id>` window in the worker's exact feature tmux session. A narrow launcher mirrored bounded stdout and stderr to the remain-on-exit pane while a separate scratch result file carried strict JSON to the controller. Pane content remained presentation only.

Paired ownership and Pi session records bind the job and review IDs to the exact session, window, pane, launcher PID, and reviewer PID. Correction review respawns the same pane and continues the saved Pi session. Plannotator feedback validates and removes only that reviewer window before a new sequence. Direct stop and shutdown stop the exact reviewer process before removing its window. Landing preserves the reviewer for retain cleanup or subsequent Sprout removal.

The integration fixture initially exposed two tmux details: `remain-on-exit` is formatted as `#{remain-on-exit}`, and respawn needs an explicit safe environment because the tmux server environment can be stale. The result channel remains independent from pane capture. Tests use isolated `TMUX_TMPDIR` servers and compare the default server identity before and after each case.

The final synchronization merged the independently landed 1800-second deadline. The non-tmux implementation emitted readiness directly after child creation. The visible design needs two hops: the pane launcher starts the deadline and writes the exact reviewer PID, then the parent helper validates that PID before emitting the private readiness line to the extension. A matching parent deadline remains as a fail-closed backup and keeps the short timeout integration test deterministic.

## Interactive preflight implementation 2026-08-22

The reviewer now runs Pi's regular interactive TUI directly on the owned pane terminal. The pane remains input-capable by explicit user acceptance. Scufris still owns sequence creation, exact result consumption, correction routing, cancellation, invalidation, and landing. Foreground policy does not expose reviewer control.

A private explicit Pi extension adds only `submit_preflight`. The model uses read-only repository tools, submits the final structured verdict through that tool, and the extension writes one bounded scratch result before graceful TUI shutdown. The controller still applies the existing strict JSON parser and never parses pane output. The TUI now shows the prompt, progress, tool calls, and final verdict while the paired ownership and session records preserve exact lifecycle behavior.
