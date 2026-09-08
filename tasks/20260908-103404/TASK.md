# Add a verify agent that runs after every done job and quotes the receipt

- STATUS: OPEN
- PRIORITY: 80
- TAGS: workflow


## Goal

After every job reports `done`, a short read-only verify run checks that
the asked thing was done and whether it is landed, and the foreground quotes
its verdict. It is a quick check, not a review. The nightly review stays a
briefing profile.

Origin: research task 20260908-003207, candidate 3. Replaces the per-project
trust flag. Depends on task 20260908-103402: without receipts the verify agent reads
the same prose the worker wrote.

## Facts

- Preflight reviewers already run with restricted tools
  (`docs/src/dev/jobs.md`, reviewers limited to read, grep, find, ls, and
  `scufris_report`).
- The briefing envelope reader accepts a status, up to six facts, and a
  Markdown body (`docs/src/dev/briefings.md`) and retries a badly said
  answer once.
- `[agents.<name>]` entries carry harness, model, and thinking keywords.

## Direction

- Add `[agents.verify]` to `.scufris.toml`. When present, the `done`
  event spawns one bounded read-only child job in the same workflow with the
  diff since base, `report.md`, and `receipt.json` as inputs.
- The verify job answers one envelope: status `ok` or `attention`, facts
  such as "asked: 3 items, found: 2", and a short body. Reuse the briefing
  envelope reader.
- The foreground wake for `done` waits for the verdict when a verify agent
  exists, then quotes the receipt and the verdict. A verify job that fails
  is reported as "not verified", never as a pass.
- Keep it cheap: time bound like a briefing source, no shell writes, no
  landing.

## Verification

- Test: a done job with a verify entry produces one child job with the
  restricted tool set and one envelope in the job directory.
- Test: a worker that claims three items done with two present yields
  status `attention` and the foreground text carries the verdict.
- `npm run check` and Python unit tests.
