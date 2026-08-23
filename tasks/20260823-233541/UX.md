# UX: end-to-end flows

Each flow shows what Alex says or does, what Scufris calls, what is
spoken, what opens, and how it fails. Voice arrives through the popup
today and through the Super+D HUD once task `20260822-132001` ships;
the flows are identical either way - the HUD only changes how the
transcript enters and which states (listening, transcribing, working,
speaking, attention) are visible. Widgets referenced are the six today
variants (deployed) and the planned den viewer variants (item, inbox,
board).

Guiding rule, codified in the skills: answer from data first; open a
widget when the user asks to see something, when a trend or list is
easier shown than spoken, or as confirmation after a mutation. One
spoken sentence plus at most one relevant surface.

## 1. Tomorrow's plan

- Alex: "what do I have tomorrow?"
- Scufris: `scufris_den_read {domain: today, query: show, date: +1}`
  and `{query: upcoming}`.
- Spoken: "Two tasks tomorrow: finish the nova-protocol enemy spawner,
  and call the dentist. Nothing else until Friday."
- No widget unless asked; "show me" opens `today.upcoming` focused on
  the range discussed.
- Failure: today CLI absent - "I cannot read the journal right now"
  plus the reason in the transcript.

## 2. Capture a task by voice

- Alex (HUD, Enter-to-send): "add a task for Saturday: playtest the
  new build with sound off."
- Scufris: `scufris_den_write {action: task_add, date: 2026-08-29,
  payload: "Playtest the new build with sound off"}`.
- Spoken: "Added to Saturday." Optionally `today.tasks` for that date
  opens as visual confirmation when the mutation was ambiguous.
- Failure: revision conflict (file edited in Neovim meanwhile) - the
  tool error instructs a re-read; Scufris retries once and confirms,
  or reports the conflict plainly.

## 3. Weight trend

- Alex: "how is my weight trending this month?"
- Scufris: `scufris_den_read {query: weight_history, days: 30}`.
- Spoken: "Down 0.6 kilos over 30 days, 70.8 this morning. Four days
  missing last week." Opens `today.weight` (the graph earns its
  window; the numbers alone undersell the shape).
- The missing-data remark is observation, not nagging: stated once,
  not repeated daily (proactivity budget applies when this becomes a
  briefing item).

## 4. Collect resources (the library capture flow)

- Alex: "collect some references on roguelike deckbuilders for
  nova-protocol - a few good articles and a video or two."
- Scufris spawns a worker job (existing workflow engine): fetch
  helpers into staging, extraction, then `den add` per item with
  status inbox and topic nova-protocol. Job events stream progress;
  the HUD shows working.
- Spoken on completion: "Captured five references: three articles, two
  videos. Two look strong. They are in the library inbox."
- Opens `den.inbox` for triage. Keep and discard are direct widget
  actions (widget backend calls `den set`); no model round-trip per
  click.
- Failure: a fetch fails - the item is reported as failed with its
  URL, nothing partial enters the library; staging is discarded.

## 5. "Look what I found" (single reference)

- During any conversation Scufris captures one URL the same way, then:
- Spoken: "Saved. This one benchmarks Slay the Spire's energy economy
  - look at the second chart."
- Opens `den.item` at that reference, extract view, cited passage
  highlighted. "Open the real page" runs `scufris-browse` (real
  browser, floated by i3 rule).
- The viewer always shows provenance: source, capture date, trust
  label, and which conversation captured it.

## 6. Research recall with citations

- Alex, weeks later: "what did we save about deckbuilder energy
  systems?"
- Scufris: `den search` (v1 rg, v2 FTS5), reads the top manifests.
- Spoken: "Three items. The strongest is the energy-economy benchmark
  from August; you marked it kept and noted the mana-curve section."
- Answer text carries citations (den ids); "show it" opens `den.item`
  at the cited range. If retrieval confidence is poor, Scufris says
  what it searched and offers the inbox list instead of guessing -
  and these misses are the recorded signal that later justifies
  embeddings (RESEARCH.md gate).

## 7. Video consumed as transcript plus frames

- Alex: "summarize that GDC talk we saved yesterday."
- Scufris reads transcript.json via `den show`; answers with timestamp
  citations ("the fail-faster argument lands at 14:20").
- "Show me that part" opens `den.item` in video mode: keyframe strip
  synced to transcript, jump-to-timestamp. Watching the actual video
  stays a `scufris-browse` action.

## 8. Morning briefing (proactive v1, timer-triggered)

- 09:00 systemd user timer starts a turn with a fixed briefing prompt.
- Scufris reads today, upcoming, habits, weight; checks the library
  inbox count; composes three sentences maximum.
- Delivery follows the graduated path: transcript entry always; HUD
  attention state (or dunst until the HUD ships); spoken only if wake
  or speech mode allows. Opens the briefing layout only on request or
  when configured to.
- Every briefing states its trigger ("morning briefing, 09:00 timer").
  Per-topic mute ("stop telling me about habits") is honored and
  audited. Quiet hours and the daily interruption budget cap it.

## 9. Proactive finding (later, policy-gated)

- A watcher or job notices something relevant (a captured page updates,
  a topic connects two items). The policy layer scores it against the
  budget; most findings become transcript notes, not interruptions.
- When it does surface: "While indexing I noticed the pathfinding
  article you kept contradicts your enemy-spawner note - want the
  comparison?" - offer, not action. Show opens the two items side by
  side (two viewer surfaces, tiled presentation).
- The user can always ask "why did you tell me this?" and get the
  trigger, the rule, and the mute option.

## 10. Course from the library (future pillar)

- Alex: "build me a short course on A* from what we have."
- Scufris compiles kept items tagged pathfinding into an interactive
  HTML page (designs/course-concept.html is the concept): hook, ladder
  of abstraction, sandbox, one check, citations back to den ids.
- The artifact is itself persisted as a library item (modality:
  course), viewable in `den.item` or the browser, and improvable in
  later sessions ("add a section on jump point search").

## Interaction invariants

- Every number, date, or quote Scufris states traces to a CLI read in
  the same turn; every reference it shows carries provenance.
- Widgets never substitute for answers; answers never require reading
  a widget.
- Direct manipulation (checkbox, keep/discard) goes widget-backend to
  CLI without a model round-trip; Scufris notices state changes on the
  next read, not by watching clicks.
- Corrections are conversational ("no, Saturday not Friday") and map
  to a compensating write plus confirmation.
- Anything destructive (task rm, discard-all, blob deletion) is named
  explicitly before it happens.
