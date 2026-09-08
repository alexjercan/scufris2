# Add a briefing scheduler: profiles on a schedule, built-in sources, offers

- STATUS: OPEN
- PRIORITY: 90
- TAGS: workflow


## Goal

Briefings become a scheduler. More than one profile runs on its own time,
sources can be built in rather than declared by a project, and a source may
offer one action that Alex starts by saying "yes". First uses: a nightly
per-project code review page, a jobs source with receipt facts, and the
seedzero offer "produce the next short".

Origin: research task 20260908-003207, candidate 2. Alex's preferred item.
Replaces the named routines idea.

## Facts

- One timer, and the scheduled profile is
  `SCUFRIS_BRIEFING_PROFILE || "morning"`
  (`agent/extensions/scufris/briefing/briefing.ts:108-121`). Nothing sets
  that variable (`nix/launcher.nix:44-48`, `nix/home-manager.nix:107-118`).
- `decide()` knows one time per day
  (`agent/extensions/scufris/briefing/schedule.ts:86-95`). Run directories
  are `briefings/<date>/` with no profile (`tools/briefing/briefing.py:123-124`),
  so a second profile on the same date finds the morning's delivered state.
- Any profile name is accepted and `scufris-briefing collect --profile X`
  works (`tools/briefing/cli.py:44-46`, `briefing.ts:245-278`).
- `briefing_sources` reads only project `.scufris.toml` files
  (`tools/jobs/scufris-jobs:623-660`). No built-in source kind exists.
- `[agents.<name>]` gives an offer a target (`scufris-jobs:513-540`).

## Direction

- Replace the one time with a profile-to-time map in the module and
  launcher, for example `briefings.schedule = { morning = "07:30", nightly = "23:00" }`.
- Key run directories by date and profile. Per-profile delivered state.
  `decide()` per profile. The wake already carries the profile.
- Add built-in sources that run without a project file. First one: `jobs`,
  listing what landed, pushed, or failed since the last run with receipt
  facts (depends on task 20260908-103402). Allow a project to opt into per-project job
  stats with one flag.
- Extend the envelope with one optional `offer`: an agent name from the
  project's `[agents.*]` and a one-line prompt. Render it on the page and in
  chat. Store offers in the run directory so "yes" binds to the latest open
  offer deterministically, not to the model's memory.
- A nightly review source is read-only and bounded, reports counts and
  candidates, and never claims a fix. Document the `nightly` profile.

## Verification

- Test: two profiles on one date produce two run directories and two
  deliveries.
- Test: a built-in jobs source contributes with no project file present.
- Test: an offer stored in the run directory resolves "yes" to one spawn
  with the stored prompt.
- `npm run check`, Python unit tests, and one staging run with a nightly
  profile.
