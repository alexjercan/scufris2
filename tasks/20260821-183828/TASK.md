# Restore Sprout-owned landing and cleanup policy

- STATUS: IN_PROGRESS
- PRIORITY: 100
- TAGS: agents, sprout, review

## Goal

Return Scufris landing to the installed Sprout contract and make post-land cleanup explicit per delegated job.

## Accepted design

- Add optional spawn field `cleanup` with enum `remove | retain`.
- Default is `remove`. The orchestrator passes `retain` only when the user asks to keep landed resources.
- Preserve the selected policy in immutable job state and owned in-memory state.
- After exact structured approval and required worker `done`, reverify the approved snapshot and call `sprout land` without `--remove`.
- Remove the temporary Git-only squash, commit, worktree removal, and branch deletion implementation from Scufris.
- After successful landing, stop the exact owned worker window.
- For `cleanup: remove`, call `sprout rm <feature>` after worker stop. It may remove the worktree, branch, and complete feature tmux session, including user-created windows. The user accepts eviction after landing.
- For `cleanup: retain`, skip `sprout rm` and preserve landed branch and worktree. The worker window is still stopped.
- Cleanup failure does not undo or misreport a successful landing. Report landed-with-retained-resources and preserve evidence for manual cleanup.
- Keep exact revision binding, fresh snapshot verification, dry-run, no push, and narrow model-facing schemas.
- No compatibility alias or deprecated boolean.

## Definition of done

- Default delegated jobs land through `sprout land`, stop the worker, and run `sprout rm`.
- Retained jobs land through `sprout land`, stop the worker, and keep branch/worktree resources.
- The selected cleanup policy appears in spawn results, owned job inspection/listing where useful, and job records.
- Cleanup failure leaves the landed commit intact and gives one actionable foreground notification.
- Cross-project landing uses the selected repository and installed Sprout.
- The temporary Git-only landing implementation is removed.
- Tests cover default, retain, cleanup failure after successful land, exact call order, and schema validation.
- Architecture, protocol, delegation guidance, and task evidence match implementation.

## Verification

- `npm run check`.
- Python integration tests and Ruff checks.
- Focused Nix launcher and Home Manager checks.
- `nix flake check` before landing.
- Live playtest for both cleanup policies when practical.
