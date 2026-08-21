# Restore Sprout-owned landing and cleanup policy

- STATUS: CLOSED
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

## Completion record

- Added optional `cleanup: remove | retain` to delegated spawn. Omission defaults to `remove`. Spawn results, immutable job records, owned listing, and inspection preserve the selected policy.
- Replaced temporary Git squash, commit, worktree removal, and branch deletion code with installed `sprout land` after the existing dry-run and exact revision recheck.
- Added ordered post-land handling: stop the exact worker, then call `sprout rm` for remove or preserve Sprout resources for retain.
- Added one terminal `landed-with-retained-resources` result for stop or remove failures after successful landing. It retains merge evidence, gives a policy-specific manual action, and never attempts rollback.
- Preserved opaque cross-project selection, no push, exact approval binding, narrow schemas, exact tmux ownership, and immutable launch evidence.
- Updated architecture, protocol, and delegation guidance. Remove cleanup now documents accepted eviction of remaining feature-session windows.
- TypeScript tests cover remove order, retain order, and cleanup failure semantics. Python integration tests use real Git, installed Sprout, isolated tmux, and fake harnesses to cover default and explicit policy persistence, schema rejection, dry-run -> exact recheck -> land -> stop -> remove order, retained landing, failed cleanup with an intact landed commit, and cross-project mechanics.
- Verification passed before evidence commit: `npm run check` (12 tests); `python3 -m unittest discover -s tests -p 'test_*.py'` (14 tests); `ruff check scripts/scufris-job tests`; `ruff format --check scripts/scufris-job tests`; `nix flake check`; `git diff --check`.
- Nix checked launcher resources and Home Manager integration on x86_64-linux. Flake checking omitted incompatible configured systems.
- Live foreground provider and Plannotator playtests were not practical in the delegated worker. Focused lifecycle tests and real Sprout process integration cover both cleanup policies.
- Revisions at implementation evidence: landing baseline `51b36416295760276769fecdfb1fc8dd7a5e357d`; implementation `b9db39908fcde866c9c18428c1791c75de2f6e10`.
- `sprout sync sprout-cleanup-policy` reported already up to date. Post-sync verification passed: `npm run check`; 14 Python integration tests; Ruff lint and format checks; `nix flake check`.
- Review correction: Plannotator exact `approved: true` with empty annotations now approves even when feedback contains informational LGTM text. Nonempty annotations remain actionable and prevent approval. Without exact approval, nonempty feedback remains actionable; an empty outcome remains blocked. Focused tests preserve malformed and oversized response guards.
