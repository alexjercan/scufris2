# Declare briefing sources for the machine in a user-level config file

- STATUS: OPEN
- PRIORITY: 90
- TAGS: workflow

## Goal

A briefing source can be declared for the machine rather than for a project.
The first one reports what Scufris did since the last run of this profile,
from facts the helper measured. It is an ordinary source with an ordinary
prompt, not a built-in kind, so nothing new has to be learned to write one and
nothing new has to be built to add the next.

Origin: task 20260908-103403, split. The last of the five, and the one that
was redesigned rather than carried over.

## Facts

- No user-level configuration exists anywhere in the stack. Everything is
  either a Nix option, a per-project `.scufris.toml`, or an environment
  variable the launcher exports.
- `XDG_STATE_HOME` is already respected for state
  (`tools/briefing/briefing.py:106`, `tools/jobs/scufris-jobs:125`). Nothing
  reads `XDG_CONFIG_HOME`.
- One reader owns project discovery and `.scufris.toml`. `declared_sources`
  asks the jobs helper (`briefing.py:269-291`), which walks git roots under
  the configured roots (`scufris-jobs:386-403`) and reads each project's file
  (`:629-661`).
- A source is asked for one envelope, and every limit it is held to is stated
  in the prompt it was given (`briefing.py:294-360`).
- Receipts are measured and stored. `receipt` writes `receipts.jsonl` beside a
  job (`scufris-jobs:3238`), and `inspect` returns the newest record
  (`:2423`).
- Nothing can answer "what landed". `all_job_records()` skips archived jobs
  (`scufris-jobs:2547-2565`) and archiving is what happens to a finished
  workflow (`:2660`).
- `programs.scufris.agent.briefing.profiles` carries timer-shaped options
  only: schedule, persistent, deadline (`nix/home-manager.nix:125-166`). That
  boundary is settled and this must not widen it.

## Direction

- Read one user-level file at `$XDG_CONFIG_HOME/scufris/config.toml`, in the
  same reader that walks projects. Its `[briefings.<profile>.<name>]` sections
  are sources not tied to any project. Names are required, because there is no
  project name to slug and no existing shape to be ambiguous with.
- A user-level source runs in `$HOME` unless it names a `root`. Nothing else
  differs: same envelope, same deadlines, same repair, same page.
- Generate the file from a typed Home Manager option with `pkgs.formats.toml`,
  so a malformed entry fails the build instead of costing a morning. The
  helper reads a TOML path and must not know Nix exists: anyone not on NixOS
  writes the file by hand.
- Add a `--config` flag and a `SCUFRIS_CONFIG` variable naming another file.
  Guidance is prose that is tuned over several runs, and the tests need the
  same seam to point the reader at a fixture.
- The file is briefings-only, and the documentation says so. A schema half
  ignored is worse than one that states what it covers. Project files keep
  `[conventions]` and `[agents.*]`: a checkout must be self-sufficient for
  someone whose machine has neither.
- Tell every source when its profile last ran, from the previous run's
  `finished`. A weekly source then reports on a week and a morning source on a
  night, with no window setting anywhere. This is the one part that changes
  the prompt every existing source already gets.
- Add the listing the jobs source needs: job records including archived ones,
  since a timestamp, with their receipts.
- Write the jobs source itself as guidance in the user file, not as code. It
  quotes receipt fields verbatim, says "claimed, not verified" for an unbacked
  claim, and says "not landed" in those words, exactly as the workflow skill
  already requires of the foreground.

## Verification

- Test: a user-level source contributes with no project declaring anything.
- Test: two named sources in one profile produce two cards with their own
  slugs, and neither collides with a project of the same name.
- Test: a user-level source with no `root` runs in `$HOME`, and one with a
  `root` runs there.
- Test: `--config` and `SCUFRIS_CONFIG` name another file, and the flag wins.
- Test: a malformed user file is one diagnostic naming it, and every project
  source still contributes.
- Test: the archived-job listing returns a landed job that
  `all_job_records()` does not.
- Test: a source prompt carries the previous run's finish time, and the first
  run of a profile says there was no previous one rather than inventing a
  window.
- Test: a malformed briefing entry in the Nix option fails the build.
- One real morning where the jobs source reports a job the receipt says was
  not pushed, and the briefing says "claimed, not verified".
- `npm run check`, Python unit tests, and `nix flake check`.
