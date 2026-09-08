# End a briefing with a numbered list of things Alex can start

- STATUS: CLOSED
- PRIORITY: 85
- TAGS: workflow

## Goal

A briefing ends with a numbered list of things Alex can do next. He picks by
number - "do 1, 2 and 4" - or reads the list and does one himself. The numbers
are assigned by code and stored with the run, so a pick resolves from the file
and never from the model's memory.

Origin: task 20260908-103403, split. Replaces that task's single `offer` and
its "yes" binding, which bound one action and threw the rest away.

## Facts

- A contribution is a fixed envelope: title, status, headline, at most six
  facts, and a Markdown body (`tools/briefing/briefing.py:573-607`). Anything
  else is refused by name.
- Every limit a source is held to is stated in the prompt it was given
  (`briefing.py:376-425`). A field the prompt does not mention is a field a
  source will not send.
- Every source runs at once (`briefing.py:932`, one `pool.map`). No source can
  read what another found, because they are still running.
- Scufris reads every contribution when it writes the prose, and
  `publish(date, profile, prose)` is where that lands (`briefing.py:1071`).
- The manifest is an index and the body lives beside it in the contribution
  file (`briefing.py:779-798`).

## Direction

A next step is a **label** and a **detail**, and nothing else.

- Extend the envelope with `offers`: at most three per source, each a short
  label saying what could be done and a detail of a sentence or two saying
  what and why, from what that source actually read. State both limits in the
  contribution prompt, as every other field's limits are.
- No stored prompt and no agent name. A verbatim worker prompt would only make
  sense for a delegated coding job, so it would quietly restrict offers to code
  sources; a briefing about a calendar, a reading list or a house cannot fill
  one in. Keeping the shape to label and detail is what makes this generic.
- Collection merges offers across sources into one numbered list in the
  manifest. Numbers are assigned by code in source order and are stable for
  the life of the run. No model is in this seat: merging a list is
  concatenation, and a harness launch to do it would only add a way to reword
  or drop an entry.
- The page renders the list as its own block. The wake tells Scufris to end the
  briefing with the same numbered list, in the same numbers.
- A pick is ordinary conversation. Scufris resolves "do 2 and 3" against the
  stored list with `scufris_briefing_show`, which already returns the run. No
  new tool.
- When a pick is a coding job, Scufris composes the worker prompt at that
  moment from the label, the detail, and that source's body, which it has in
  front of it. Composing then beats replaying a prompt a source model wrote
  hours earlier without knowing the pick would happen.
- Policy in the tool guidelines: never act on an offer number that is not in
  the file, and never invent a step the list does not carry.

## Verification

- Test: two sources with two offers each produce one numbered list of four,
  numbered in source order, stored in the manifest.
- Test: more than three offers from one source is refused by name, like every
  other bound.
- Test: an offer missing its label or its detail is refused by name.
- Test: a source that offers nothing costs the run nothing, and a run where no
  source offered anything renders no block and says no list.
- Test: the numbers a source's contribution file carries and the numbers the
  manifest carries are the same after publish.
- `npm run check` and Python unit tests.

## Rejected

- **Offers written by Scufris at publish rather than by the sources.** Scufris
  has read every contribution, so it can see across projects, but it has not
  read any project. The source is the only thing that read the repository, the
  calendar or the journal, so it is the only thing that can propose from them.
- **A `jobs`-style machine source that collects the offers.** Every source runs
  at once, so a collector would see nothing. Even given the ordering, merging a
  list is concatenation and there is nothing in it for a model to decide.
- **A stored verbatim prompt per offer.** Makes offers coding-only, and hands a
  worker an instruction written before anyone knew it would be picked.
