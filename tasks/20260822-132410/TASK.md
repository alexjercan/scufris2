# Show preflight reviewers in worker tmux sessions

- STATUS: OPEN
- PRIORITY: 100
- TAGS: agents, review, tmux, visibility

## Goal

Run each independent preflight reviewer visibly in a dedicated window within its implementation worker's feature tmux session. Preserve cold review context, exact ownership, automated result handling, and guarded cleanup.

## Accepted design

- Create one exact owned reviewer window named `preflight-<review_id>` in the existing feature tmux session.
- Run the actual reviewer process in that window. Do not present a copied transcript from a separate hidden reviewer.
- Show bounded live reviewer activity, read-only tool use, the final verdict or findings, and failure or timeout state.
- Disable keyboard input to the reviewer pane. The user can inspect it but cannot steer or contaminate the independent review.
- Never attach, select, switch, or otherwise change the user's current tmux client.
- Notify the foreground user of the reviewer window name when preflight starts.
- Keep the finished pane visible with remain-on-exit until the owning lifecycle removes it.
- Allow 1800 seconds for one independent reviewer run. Give the enclosing helper only a small fixed shutdown margin beyond that deadline.
- Reuse the same reviewer window and saved Pi reviewer session for correction verification within one preflight sequence.
- Human Plannotator feedback invalidates the sequence. Remove only the exact owned prior reviewer window before creating a fresh reviewer window and session for the next sequence.
- Default post-landing remove cleanup continues through Sprout and can remove the complete feature session. Retain cleanup keeps reviewer evidence with the retained feature resources.

## Independence and review contract

- Co-location in tmux does not share conversational context.
- Continue to exclude the implementation transcript, worker report, reasoning, and claims from reviewer input.
- Continue to provide only repository rules, review profile, accepted outcome and audience, exact patch, and read-only access to surrounding feature-worktree files.
- Keep Sol at medium thinking and the read-only `read`, `grep`, `find`, and `ls` allowlist.
- Preserve the same saved reviewer session across correction cycles and exact revision-bound approval.
- Preserve two feedback cycles, fail-closed behavior, and Plannotator as the only user approval gate.
- Visible output must not become reviewer input or a result authority.

## Process and ownership requirements

- Record and validate exact reviewer session, window, pane, process, and review identities wherever lifecycle operations require them.
- Target only exact owned reviewer resources for interruption, timeout, invalidation, shutdown, and cleanup.
- Never kill a tmux server or complete session directly.
- Do not inspect, modify, or target unrelated user-created windows in the feature session.
- Keep result transport structured and bounded. Do not parse the tmux pane transcript as the review result.
- Preserve the 64 KiB output bound, review timeout, environment isolation, repository mutation checks, and pre/post revision checks.
- A reviewer launch, output, timeout, identity, or cleanup failure blocks the review lifecycle with actionable diagnostics.
- Stale-job pruning and diagnostics must understand any new durable reviewer ownership evidence and reject malformed or mismatched identities.

## Documentation

Update the delegation manual, skill, architecture, protocol, and task evidence to describe the visible input-disabled reviewer window and its lifecycle.

## Verification

- Test reviewer window creation in the worker's exact tmux session without selecting a client.
- Test stable naming, exact identity validation, disabled pane input, visible activity, and remain-on-exit behavior.
- Test correction verification reuses the same reviewer window and reviewer session.
- Test human feedback invalidates and removes only the exact prior reviewer window.
- Test the exact 1800-second reviewer deadline and enclosing shutdown margin, malformed output, reviewer failure, shutdown, remove cleanup, retain cleanup, and unrelated-window preservation.
- Use an isolated tmux socket for automated tmux tests and verify the default server identity is unchanged.
- Run TypeScript, Python integration, formatting, ShellCheck, Nix formatting, diff checks, and full Nix checks required by repository guidance.

## Completion criteria

- A user can enter the feature tmux session and watch the independent preflight reviewer in its own window.
- User input cannot alter the reviewer.
- Findings and approvals continue through the existing automated correction and Plannotator lifecycle.
- Exact ownership and cleanup remain safe for worker, reviewer, and unrelated tmux windows.
