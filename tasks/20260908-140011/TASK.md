# Dogfood the nightly profile with a read-only review in nova-protocol

- STATUS: CLOSED
- PRIORITY: 60
- TAGS: workflow

## Goal

Run the scheduler against a real project. nova-protocol declares a nightly
review profile, and the nightly briefing reports what it found. This is the
dogfood item that proves the profiles, the schedule, and the offers work
outside a test.

Origin: task 20260908-103403, split. Last of the five: it is a
`.scufris.toml` entry in another repository plus a paragraph here, and it
should not be built inside the feature it tests.

## Facts

- A source declares itself in its own `.scufris.toml` and the project owns the
  guidance, the paths, and the meaning (`docs/src/dev/briefings.md`).
- A source runs with the edit tools off. That is an intent boundary and not a
  sandbox: what a source may reach is decided by its tool list and its own
  guidance (`tools/briefing/briefing.py:75-79`, `:305-330`).
- A source is bounded at 900 seconds and the run at 1800
  (`briefing.py:60-61`).

## Direction

- Add `[briefings.nightly]` to nova-protocol with guidance that is read-only
  and bounded: name what to read, cap what it costs, and report counts and
  candidates.
- It reports and never claims a fix. No branch, no commit, no landing. A
  candidate is a path and a reason, not a patch.
- Document the `nightly` profile here: what it is for, what it may cost, and
  why a review source reports rather than repairs.
- Verify by watching real nights, not by asserting on a fixture.

## Verification

- One real nightly run that produces a contribution with measured counts.
- One night where the source has nothing to report and the briefing says so
  without inventing a candidate.
- The run stays inside its deadline.

## Superseded

Replaced by `20260908-184224`, which does the review as a job and lets the
morning report it.

A briefing source cannot do this. It is bounded at 900 seconds and it reports
rather than repairs, so a nightly review declared as a source could only ever
name candidates it was not allowed to act on. The night's work belongs to a
job that can run for hours and commit; the morning belongs to a source that
reads what the job wrote.
