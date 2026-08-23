# Scufris Future Ideas - Raw Brainstorm

Raw material for a lead to curate. Not a roadmap. Every idea below respects
the hard constraints: local-first; canonical data is plain files; derived
indexes are disposable; deterministic narrow CLIs are the contract for
mutations; dashboardd only gets generic widgets, never Scufris-specific
hooks; any mutation or external effect needs explicit approval; anything
deployed goes through the NixOS release gate; proactivity is calm and
graduated (offer, don't act); no always-on screen recording.

Tags: Value = high/medium/low. Complexity = small/medium/large.
`[spicy]` = novel angle, still inside every constraint.

---

## 1. Daily journal and capture friction

1. **Voice quick-macro capture** - Say "log lunch: chicken and rice" and
   Scufris parses it and calls the `today` CLI to add the macro entry
   directly, skipping any manual typing. Removes the single biggest reason
   logging gets skipped: friction at the moment of the act.
   Value: high. Complexity: small.

2. **Habit nudge offer** - If a habit hasn't been logged by a configured
   time, Scufris surfaces one low-key offer to log it (not a repeated
   nag). Keeps streak data honest without turning into a scold.
   Value: medium. Complexity: medium.

3. **End-of-day recap prompt** - At a chosen time, Scufris asks three fixed
   questions (wins, blockers, tomorrow) and writes the verbatim answers into
   the day's journal file via `today notes add`. Cheap ritual that produces
   real prose in the vault instead of nothing.
   Value: high. Complexity: small.

4. **Correction-friendly capture** - Before calling a mutating `today`
   command from a voice parse, Scufris shows the pending entry in a review
   widget and waits for confirmation. Directly implements the
   approval-before-mutation constraint for the highest-frequency mutation
   path.
   Value: high. Complexity: small.

5. **Backfill assistant** - "I forgot to log yesterday" triggers a
   structured backfill dialog that writes correctly dated entries into the
   right plain files instead of today's file. Prevents silent data
   corruption from lazy backfills.
   Value: medium. Complexity: medium.

## 2. The library and multimodal ingestion

6. **Screenshot-to-reference capture** - Point Scufris at an image (paste
   or file path); it fetches/normalizes and hands off to the deterministic
   capture CLI, which persists the blob and writes a manifest entry with
   title, source, and tags. Closes the largest current capture gap:
   anything that isn't a URL.
   Value: high. Complexity: medium.

7. **PDF/paper distiller** - On ingest of a PDF, a narrow deterministic CLI
   extracts text and metadata (title, authors, page count) into the
   tracked manifest; Scufris's summary is a derived, disposable artifact,
   never the canonical record. Keeps papers searchable without inventing a
   second source of truth.
   Value: high. Complexity: medium.

8. **Source provenance ledger** - Every manifest entry carries a plain-text
   line: fetched-by, fetched-at, original URL/path. Cheap insurance
   against "what even is this blob" six months later.
   Value: high. Complexity: small.

9. **YouTube timestamp clip notes** - For video references, a local
   transcript (Whisper) lets the user say "capture this point" and get a
   manifest note anchored to a timestamp instead of "the whole video."
   Turns hour-long videos into addressable references.
   Value: medium. Complexity: large.

10. **Multimodal sketch capture** - Photograph a whiteboard sketch (a game
    design doodle), run local OCR/description, persist as a tracked
    manifest entry plus blob under the library. Bridges paper thinking and
    the plain-file vault; doubles as a game-dev workflow input.
    Value: high. Complexity: medium.

## 3. Widgets and visual answers

11. **Generic comparison-table widget** - A reusable table widget that
    renders from any JSON payload Scufris hands it: macro comparisons,
    recipe options, design-decision matrices. One widget, many callers,
    zero Scufris-specific coupling in dashboardd.
    Value: high. Complexity: small.

12. **Generic timeline widget** - A horizontal timeline widget driven by a
    plain JSON schema (label, date, note). Useful for nova-protocol dev
    history, playtest session logs, or course-builder module sequencing.
    Value: medium. Complexity: medium.

13. **Reference graph widget** `[spicy]` - A generic force-graph widget
    that renders a node/edge JSON payload; Scufris feeds it a snapshot of
    linked references and notes computed from a disposable index. Makes
    the "connections between notes" lens visible instead of only
    queryable.
    Value: medium. Complexity: large.

14. **Scratch-answer widget** - A freeform markdown/text widget Scufris
    opens for any answer too long or too structured to speak comfortably.
    Lower latency to build than any specialized widget and covers most
    "just show me" moments.
    Value: high. Complexity: small.

15. **Opt-in macro/weight dashboard** - A standing widget assembled from
    `today`-derived summaries, opened on request and closed when done -
    explicitly not a persistent always-on display, keeping clear of the
    Rewind resource-cost lesson.
    Value: high. Complexity: medium.

## 4. Proactive contact and briefings

16. **Morning brief offer** - Scufris prepares a draft brief (tasks due,
    habits pending, weight trend, all local) and offers to deliver it; it
    never speaks unprompted at a fixed time. The offer is the whole
    feature - a spoken brief without consent would violate the calm
    proactivity constraint.
    Value: high. Complexity: medium.

17. **Quiet-hours registry** - A plain-file schedule of do-not-disturb
    windows that every proactive path consults before making an offer.
    Small, boring, and load-bearing infrastructure for every other
    proactive idea in this list.
    Value: high. Complexity: small.

18. **Weekly review offer** - On a chosen day, Scufris offers to compile a
    week-in-review from `today` data (streaks, tasks closed); it only
    writes or displays it after confirmation. Turns scattered daily data
    into a reflection point without any autonomous write.
    Value: high. Complexity: medium.

19. **Escalation ladder** - Proactive offers start as a silent dashboard
    badge and only escalate toward voice after being ignored a configured
    number of times, per an explicit ladder in config. Makes "graduated
    proactivity" a concrete, tunable mechanism instead of a vibe.
    Value: medium. Complexity: medium.

20. **Context-aware suppression** - Scufris reads an explicit user-set
    status file ("focus mode: on") and suppresses all proactive offers
    while it's set. Deliberately not screen-recording-based - a plain file
    the user flips, matching the local-first, no-monitoring constraint.
    Value: medium. Complexity: small.

21. **Threshold check-in, single-shot** - If no journal entry exists by a
    late configured hour, one offer fires and does not repeat that day.
    Cheap safety net against silently losing a day.
    Value: medium. Complexity: small.

## 5. Voice interaction

22. **Voice macro shortcuts** - Short fixed phrases ("log water") map
    directly to canned deterministic CLI calls, skipping general parsing
    for the highest-frequency commands. Lower latency and higher
    reliability than free-form parsing for the same handful of actions
    every day.
    Value: high. Complexity: small.

23. **Read-back confirmation for mutations** - Before calling any mutating
    tool from a voice command, Scufris speaks a one-line confirmation and
    waits for explicit yes. The voice-native version of the
    approval-before-mutation constraint.
    Value: high. Complexity: small.

24. **Ambient "note:" capture prefix** - Anytime, saying "note: <fragment>"
    appends the raw transcript to a capture inbox file, fully local and
    deterministic, with no pipeline beyond a file append. Lowest possible
    friction for capturing a fleeting thought while working.
    Value: medium. Complexity: small.

25. **Whisper-mode text-only replies** - A mode where Scufris answers in
    text only, suppressing Piper output, for moments voice output would be
    disruptive (late night, deep focus). Same assistant, quieter channel.
    Value: medium. Complexity: small.

26. **Local diarization for playtest sessions** - For a user-triggered
    (not always-on) recording of a playtest session with others present,
    a local diarization pass distinguishes speakers in the transcript.
    Turns a chaotic multi-voice session into attributable notes.
    Value: medium. Complexity: large.

## 6. Game-dev workflows (nova-protocol, horror, RTS)

27. **Playtest note capture** - User explicitly starts/stops a session;
    Scufris timestamps voice-tagged notes into a structured session file
    for later review. Explicitly not always-on recording - a bounded,
    consented capture window.
    Value: high. Complexity: medium.

28. **Design-reference board widget** - A generic dashboardd grid/gallery
    widget populated from library manifest entries tagged to a project
    (e.g. "nova-protocol", "horror-ref"). Turns scattered reference
    captures into an actual moodboard on demand.
    Value: high. Complexity: medium.

29. **Bug-repro capture** - During a playtest, voice-describe a bug and
    Scufris drafts a structured repro note (steps, expected, actual) into
    the project's plain files; pushing it to any external tracker is a
    separate, approved step. Keeps the fast local capture and the slow
    external mutation cleanly separated.
    Value: high. Complexity: medium.

30. **Asset inspiration crawler** - While browsing, point Scufris at art or
    sound references; the capture CLI persists them pre-tagged to the
    active project. Removes the "I saw something great and lost the tab"
    failure mode.
    Value: medium. Complexity: small.

31. **Design-doc diff narrator** - On request, Scufris summarizes what
    changed in a design doc plain file since it was last read. Useful
    across long game-dev iteration cycles where docs drift silently.
    Value: medium. Complexity: medium.

32. **RTS balance sandbox log** - A structured plain-text table of
    balance experiments (values tried, outcome) that Scufris helps append
    to and query. Gives an RTS project a lightweight experiment log
    without a database.
    Value: medium. Complexity: medium.

## 7. Learning and the course builder

33. **Reference-to-lesson drafts** - The course builder clusters tagged
    library references and drafts an interactive HTML lesson skeleton
    (headings seeded from manifest titles); the user edits and approves
    before anything is published. Turns raw captures into course material
    without ever auto-publishing.
    Value: high. Complexity: large.

34. **Spaced-repetition prompts** - Plain-text flashcard files generated
    from journal/library highlights, no external app or account. Keeps
    the whole learning loop inside the plain-file vault.
    Value: medium. Complexity: medium.

35. **"Explain what you captured" check** - Right after ingesting a
    reference, Scufris asks the user to explain it back before marking it
    "learned" in the manifest. Cheap comprehension check that also
    improves the summary quality for later retrieval.
    Value: medium. Complexity: small.

36. **Concept-linking suggestions** - While drafting a lesson, Scufris
    surfaces other library references sharing tags or triggering
    evidence-based embeddings matches, for possible inclusion. Uses the
    staged retrieval ladder for a concrete authoring task instead of just
    Q&A.
    Value: medium. Complexity: medium.

37. **Course progress widget** - A generic checklist/progress-bar widget
    showing course-builder module completion. Small but satisfying visual
    feedback for a long-running project.
    Value: low. Complexity: small.

## 8. Retrieval and connections between notes/references

38. **Backlink surfacer** - Opening a reference in the sanitized viewer
    widget also shows plain-text-derived backlinks (other notes that
    mention it), computed from a disposable index rebuilt on demand.
    Makes the vault feel connected without a permanent graph database.
    Value: high. Complexity: medium.

39. **"What did I already know about X"** - A single query that runs the
    full staged ladder (rg, then FTS5, then embeddings only on evidence)
    across both journal and library and returns one narrated summary.
    The flagship use case that justifies building the staged ladder at
    all.
    Value: high. Complexity: medium.

40. **Stale-index detector** - A small deterministic check that compares
    index mtimes against source file mtimes and flags when FTS5 or
    embeddings are stale, prompting a rebuild instead of silently trusting
    old data. Directly enforces "derived indexes are disposable" as a
    real behavior, not just a principle.
    Value: medium. Complexity: small.

41. **Citation trail widget** - A generic widget that shows which stage of
    the retrieval ladder (rg, FTS5, embeddings) actually produced an
    answer's evidence. Builds trust in staged retrieval and helps debug
    surprising results.
    Value: low. Complexity: small.

42. **Contradiction flagger** `[spicy]` - When an evidence-triggered
    embeddings search surfaces two references that appear to disagree,
    Scufris notes the tension for the user's own judgment - it never
    resolves or deletes anything. Turns a large library from a pile into
    something that can push back gently.
    Value: medium. Complexity: large.

## 9. Self-improvement loop (tools through the release gate)

43. **Tool-gap friction log** - Whenever Scufris has to hand-roll a
    multi-step workaround, it appends a plain-text entry proposing a
    narrow CLI for it, for later human review. Turns recurring pain into
    a visible backlog instead of repeated silent toil.
    Value: high. Complexity: small.

44. **Draft tool-spec proposer** - On request, Scufris turns friction-log
    entries into a draft spec (name, inputs, outputs, one narrow
    responsibility) for the user to implement or reject. Scufris proposes,
    it never writes code into a deployed path unprompted.
    Value: high. Complexity: medium.

45. **Release-gate checklist assistant** - For a proposed new tool, Scufris
    walks the NixOS release-gate checklist (tests present, docs present,
    `nix flake check` clean) and reports readiness rather than acting.
    Value: medium. Complexity: small.

46. **Self-test nag** - After a new tool is merged, Scufris checks that a
    focused integration test actually exists before treating the loop as
    closed. Cheap guardrail against "shipped but untested" drift.
    Value: medium. Complexity: small.

47. **Sandboxed tool tryout** `[spicy]` - A proposed tool can be dry-run
    inside a Sprout worktree before it goes anywhere near the release
    gate, so Scufris can demo real behavior without touching the deployed
    package. Combines two already-decided pieces (Sprout, release gate)
    into one loop instead of inventing a third mechanism.
    Value: medium. Complexity: medium.

## 10. Fun/weird-but-plausible

48. **Ambient status widget, not hardware** `[spicy]` - A small dashboardd
    widget (not a physical light - embodiment stays research-only) that
    reflects Scufris's current state: idle, thinking, waiting-for-
    approval. Gets most of the embodiment payoff with zero hardware risk.
    Value: low. Complexity: small.

49. **Rubber-duck mode** `[spicy]` - Scufris stays silent and only reflects
    back what was just said, with no tool calls at all, for thinking out
    loud about a design problem. The value is in doing nothing, which is
    an unusual mode for an assistant to offer on purpose.
    Value: low. Complexity: small.

50. **Nova-protocol lore persona** `[spicy]` - A clearly-toggled voice
    persona that answers in-character for brainstorming flavor, with an
    unambiguous switch back to the normal assistant for real work. Purely
    a Piper voice/prompt change, no architecture impact.
    Value: low. Complexity: small.

51. **Time-capsule notes** - Schedule a plain-text note to "surface" on a
    future date, delivered as an offer (not an auto-read) - "here's why
    you made this call three months ago." A small, honest use of
    proactive contact for a genuinely useful moment.
    Value: medium. Complexity: small.

52. **Streak celebration widget** - A small generic badge/confetti widget
    that fires when a `today` habit streak crosses a threshold.
    Deliberately low-stakes fun, cheap to build, easy to ignore.
    Value: low. Complexity: small.

53. **Devil's-advocate review pass** `[spicy]` - On request, Scufris
    re-reads a design doc and argues the opposite position as a plain
    text response, purely to stress-test a decision before it is locked
    in. No mutation, no proactivity - just a sharper reading mode.
    Value: medium. Complexity: small.
