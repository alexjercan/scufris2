# Ideas, curated

Curated from the 53-idea brainstorm in `research/ideas-raw.md` (numbers
below reference it). Organized by the roadmap stage that unlocks each
idea, then by value. Value and complexity tags are kept from the raw
file where they held up under review.

## Promoted to stage requirements

Four ideas are cheap, load-bearing, and belong inside their stage
rather than after it:

- Provenance ledger (#8): fetched-by, fetched-at, source on every
  manifest - already in the stage 2 manifest format; keep it
  non-optional.
- Quiet-hours registry (#17) and escalation ladder (#19): these ARE
  the proactive policy mechanism; stage 6 ships them, not offers them.
- Stale-index detector (#40): mtime comparison flagging a stale FTS5
  index - part of stage 4, enforcing "derived is disposable" as
  behavior.
- Tool-gap friction log (#43): a plain-file append whenever Scufris
  hand-rolls a workaround. Ship with stage 1; it costs nothing and
  starts the self-improvement backlog immediately.

## Unlocked by stage 1 (Scufris x today)

- Voice quick-macro capture (#1, high/small): "log lunch: chicken and
  rice" straight to `macros add`. The single biggest friction kill in
  daily logging.
- Read-back confirmation for mutations (#23 with #4, high/small): the
  approval-before-mutation constraint made concrete for voice; text
  review widget as the visual variant.
- Voice macro shortcuts (#22, high/small): fixed phrases mapping to
  canned CLI calls, skipping general parsing for daily commands.
- End-of-day recap (#3, high/small): three fixed questions written
  verbatim to the day's Notes. Works on request from day one; the
  timed prompt waits for stage 6.
- Backfill assistant (#5, medium/medium): "I forgot to log yesterday"
  writes correctly dated entries, not today's file.

## Unlocked by stage 2 (library)

- Ambient "note:" capture (#24, medium/small): a spoken fragment
  appended to an inbox file; the cheapest possible thought capture.
- Screenshot and sketch capture (#6, #10, high/medium): images and
  whiteboard photos as first-class library items; doubles as a
  game-dev input.
- Asset inspiration capture (#30, medium/small): captures pre-tagged
  to the active project; kills "lost the tab".
- PDF distiller (#7, high/medium): manifest metadata at ingest;
  full docling extraction arrives with stage 5.

## Unlocked by stage 3 (viewer widget and generic widgets)

- Scratch-answer widget (#14, high/small): a generic markdown surface
  for any answer too structured to speak. High leverage, tiny build;
  a strong candidate to ship alongside the viewer.
- Design-reference board (#28, high/medium): the planned `board`
  variant, populated by topic - the nova-protocol moodboard.
- Comparison-table widget (#11, high/small) and timeline widget (#12,
  medium/medium): generic JSON-driven widgets, many callers, zero
  coupling.
- Backlink surfacer (#38, high/medium): viewer shows what mentions
  this item, from a disposable index.

## Unlocked by stage 4 (search)

- "What did I already know about X" (#39, high/medium): one query
  across journal and library, one narrated, cited summary. The
  flagship retrieval use case.
- Citation trail widget (#41, low/small): which retrieval stage
  produced the evidence; nice for trust and debugging.

## Unlocked by stage 5 (ingestion)

- Timestamp clip notes (#9, medium/large): "capture this point" in a
  video becomes a manifest note anchored to a timestamp.
- Playtest diarization (#26, medium/large): user-triggered session
  recordings with speaker attribution; explicitly bounded, never
  always-on.

## Unlocked by stage 6 (proactive policy)

- Weekly review offer (#18, high/medium): the briefing pattern at
  week scale.
- Habit nudge and journal check-in, single-shot (#2, #21,
  medium/small-medium): one offer, no repeats, budget-counted.
- Focus-mode suppression file (#20, medium/small): a plain file the
  user flips; all offers respect it.
- Time-capsule notes (#51, medium/small): "surface this on a future
  date, and say why past-me wrote it" - the most charming legitimate
  use of proactive contact in the list.

## Game-dev thread (spans stages 2-3)

- Playtest note capture (#27, high/medium): explicit start/stop voice
  session producing a structured session file.
- Bug-repro capture (#29, high/medium): voice-described bug becomes a
  structured repro note in project files; pushing to any tracker stays
  a separate approved step.
- Design-doc diff narrator (#31) and RTS balance log (#32,
  medium/medium): plain-file experiment and iteration memory.

## Course thread (after stages 2-5)

- Reference-to-lesson drafts (#33, high/large): cluster tagged items,
  draft the lesson skeleton, human approves - the course builder's
  first real step.
- Explain-back check (#35, medium/small) and plain-text spaced
  repetition (#34, medium/medium): learning-science support with no
  external app.
- Course progress widget (#37, low/small).

## Self-improvement thread

- Draft tool-spec proposer (#44, high/medium): friction-log entries
  become narrow CLI specs for review.
- Release-gate checklist assistant (#45) and self-test check (#46,
  medium/small): readiness reporting, never acting.
- Sandboxed tool tryout in a Sprout worktree (#47, medium/medium)
  [spicy]: demo real behavior before the gate; composes two existing
  mechanisms.

## Evidence-gated or fun

- Contradiction flagger (#42) [spicy] and reference graph widget
  (#13) [spicy]: wait for embeddings evidence and real library scale.
- Ambient status widget (#48) [spicy]: idle/thinking/waiting badge -
  most of the embodiment payoff, zero hardware.
- Rubber-duck mode (#49), lore persona (#50), streak celebration
  (#52), devil's-advocate pass (#53): low-stakes, small, genuinely
  fun; build any of them on a slow afternoon.

## Deliberately absent

Nothing here proposes always-on capture, autonomous mutation, a second
canonical store, dashboardd coupling, or hardware - the raw list was
generated under those constraints and survived curation under them.
