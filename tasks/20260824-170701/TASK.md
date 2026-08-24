# Remove legacy Scufris Quick Review

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: quick-review

Remove Scufris's built-in Quick Review generator, page, bridge, job helper
operations, workflow tool, documentation, packaging, and focused tests. Keep
Plannotator behavior intact. Integration with the standalone Quick Review
package and its separate Pi RPC agent is follow-up work.

The follow-up now adds a non-blocking, separately owned Pi RPC agent that loads
only `npm:@alexjercan/quick-review`, opens the exact Sprout range, handles page
questions, and returns its durable terminal outcome to foreground Scufris.

## Result

Removed the legacy walkthrough generator, parser, page, bridge, helper
operations, orchestration state, tests, packaging assertions, and documentation.
Replaced them with a narrow `quick-review-target`, a separately owned Pi RPC
adapter, and `scufris_job_quick_review`. The child uses the direct Pi core entry
point, read-only built-in tools, and only the pinned
`npm:@alexjercan/quick-review@0.1.1` package. It returns readiness immediately,
handles walkthrough and page questions outside foreground Scufris, validates
the versioned completion, relays approval, and restarts the implementation job
with requested changes. Workflow and session cleanup stop only recorded child
processes.

Split review policy into ordered `independent-review` and `quick-review`
preference entries in all discovered project configurations: `scufris2`,
`quick-review`, and `nova-protocol`. Only the second entry selects Pi RPC and
the npm extension, so those settings cannot be mistaken for the independent
review adapter.

## Verification

- `npm run check`: passed 45 Node tests, typecheck, and Prettier.
- `python3 -m unittest tests.test_quick_review_agent tests.test_scufris_jobs`:
  passed 31 tests.
- Ruff check and format checks passed for all changed Python files.
- A real Pi RPC package smoke test loaded `/quick-review` from
  `npm:@alexjercan/quick-review@0.1.1` with discovered extensions disabled.
- Python TOML checks proved all three project files have distinct ordered review
  entries and reserve `mode = "rpc"` for Quick Review.
- `nix flake check -L`: passed all 26 checks after staging only new files for
  flake source visibility, then resetting the index.
- `git diff --check` passed in Scufris and Nova Protocol.
