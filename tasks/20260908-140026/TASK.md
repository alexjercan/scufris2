# End a briefing with a numbered list of offers Alex can start

- STATUS: OPEN
- PRIORITY: 85
- TAGS: workflow

## Goal

A briefing ends with a numbered list of things Alex can start. He picks by
number - "do 1, 2 and 4" - or copies a step and runs it himself. The numbers
are assigned by code and stored with the run, so a pick resolves from the file
and never from the model's memory.

Origin: task 20260908-103403, split. Replaces that task's single `offer` and
its "yes" binding, which bound one action and threw the rest away.

## Facts

- A contribution is a fixed envelope: title, status, headline, at most six
  facts, and a Markdown body (`tools/briefing/briefing.py:388-425`). Anything
  else is refused by name.
- Every limit a source is held to is stated in the prompt it was given
  (`briefing.py:167-232`). A field the prompt does not mention is a field a
  source will not send.
- `[agents.<name>]` gives a target with a harness, a model, and thinking
  keywords (`tools/jobs/scufris-jobs:513-540`), rendered into the delegation
  menu.
- The briefing extension cannot spawn jobs. Spawning is the workflow
  extension's (`agent/extensions/scufris/workflow/orchestration.ts:868`), and
  the two do not share a module.
- The manifest is an index and the body lives beside it in the contribution
  file (`briefing.py:578-596`).

## Direction

- Extend the envelope with `offers`: at most three per source, each a label, a
  verbatim prompt, and an optional agent name from that project's
  `[agents.*]`. State every limit in the contribution prompt, as the other
  fields are.
- Collection merges offers across sources into one numbered list in the
  manifest. Numbers are assigned by code in source order and are stable for
  the life of the run.
- The page renders the list as its own section. The wake tells Scufris to end
  the briefing with the same numbered list, in the same numbers.
- Add `scufris_briefing_offers`, which reads the stored offers for a run and
  returns them with their numbers, projects, agents, and prompts. Scufris
  starts one by passing the stored prompt verbatim to `scufris_job_spawn`.
  Two extensions, one spawner, and a prompt that came from the file.
- An offer with no agent is a step, not a delegation. Scufris may do it in the
  foreground or say what it would run; it never invents an agent for it.
- Policy in the tool guidelines: quote the stored prompt verbatim, never
  compose one, and never act on an offer number that is not in the file.

## Verification

- Test: two sources with two offers each produce one numbered list of four,
  numbered in source order, stored in the manifest.
- Test: an offer naming an agent the project does not declare is refused at
  collection and the rest of the contribution survives.
- Test: `scufris_briefing_offers` returns the stored prompt for a number, and
  refuses a number the run does not have.
- Test: more than three offers from one source is refused by name, like every
  other bound.
- `npm run check` and Python unit tests.
