# Delegated work and review

Scufris handles narrow project work directly when it should take seconds. This includes reading one named file, inspecting a small task record, and answering a focused repository question. It delegates work expected to take minutes, such as broad codebase review, substantial research, implementation, full checks, releases, and deployment. Routing uses expected scope and latency, not the presence of project tools.

A delegated Pi or Claude worker runs in a Sprout worktree. The worker receives the request and must inspect applicable repository instructions, context, code, history, and checks. The foreground session owns the job, mediates decisions and blockers, and keeps landing local.

Every job declares one review policy at spawn:

- `code`: implementation correctness and maintainability.
- `consumer`: documentation, setup, and user outcomes.
- `operations`: deployment, reliability, diagnostics, and rollback.
- `interface`: APIs, schemas, protocols, and caller contracts.
- `none`: a non-landable result.

A landable policy includes a concise accepted-outcome and audience brief. A `none` job finishes with `done` and cannot enter review-ready.

## Landable review sequence

1. The worker commits, synchronizes, checks, and reports `review-ready`.
2. Scufris verifies the exact clean base and feature revisions.
3. A fresh Pi reviewer runs in an input-disabled `preflight-<review_id>` window in the worker's feature tmux session. Scufris announces the window without selecting a client. The reviewer has a separate cold session and does not receive the worker transcript, report, reasoning, or claims.
4. The actual reviewer output and read-only tool activity remain visible in that window. Fix-worthy BLOCKER, MAJOR, or MINOR findings return through bounded structured files, not pane scraping. The same window and reviewer session verify correction commits. A third change request stops for Pair mediation.
5. Exact preflight approval opens the Plannotator since-base review.
6. Plannotator feedback invalidates preflight approval. Scufris removes only the exact owned reviewer window and session. The next worker revision starts a fresh reviewer window and session.
7. Human Plannotator approval remains the only landing approval. Scufris lands only the exact approved revisions after the worker's required acknowledgment.

One reviewer run has an exact 1800-second execution deadline. When the exact reviewer child starts in the pane, the helper sends a private readiness signal that resets the enclosing process deadline to 1810 seconds. This gives a fixed 10-second margin to stop the child and return the fail-closed result. Review failure, timeout, malformed output, identity drift, repository mutation, or revision drift fails closed. The finished pane remains visible with `remain-on-exit`. Scufris records exact reviewer window, pane, launcher, child, session, and review identities. Cancellation and shutdown remove only that owned reviewer window. Retain cleanup keeps reviewer evidence; remove cleanup lets Sprout remove the complete feature session.
