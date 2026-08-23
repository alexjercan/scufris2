# Final recommendation

Answers to the seven questions in NOTES.md, resting on INVENTORY.md,
RESEARCH.md, MARKET.md, ARCHITECTURE.md, UX.md, and ROADMAP.md.

## 1. The Scufris x today surface

Two grouped native tools - `scufris_den_read` and `scufris_den_write` -
behind a private helper shelling to `today --json`, with a den skill
carrying the usage rules (ARCHITECTURE.md option 1B). The read and
write split encodes the safety model, stays far under the tool-count
ceiling, and absorbs the future library domain without new tools.
Answer-plus-widget as one response is a skill rule, not new plumbing:
answer from data first, then at most one relevant surface - opened when
asked, when shape beats speech, or as mutation confirmation.

## 2. The reference-showing surface

Split by content type. Saved and extracted content: a generic den
viewer widget (item, inbox, board variants) shipped from the den repo,
rendered sanitize-and-forbid on the tatr-tasks precedent - no
dashboardd core changes, no Scufris hooks in dashboardd. Live pages and
PDFs: the real browser through a `scufris-browse` helper with i3 window
rules. The external-URL webview widget is explicitly deferred;
dashboardd's CSP makes it expensive and the browser makes it
unnecessary.

## 3. The library's first increment

`Library/items/<id>/` in the-den: tracked manifest.md (frontmatter:
source, hashes, modality, topics, status, trust, provenance) plus
tracked extract.md and transcript.json, git-ignored blobs. A new `den`
repository in the today mold - narrow deterministic writes (`add`,
`set`, `note`; never fetches), rich reads (`list`, `show`, `path`,
`search`, `verify`). Scufris fetches via monolith, single-file-cli,
yt-dlp, gallery-dl into staging; trafilatura and docling extract; only
`den add` persists. Triage (inbox to kept) is first-class from day one.

## 4. Calendar

Waits for a concrete timed-event need. When it comes: khal plus
vdirsyncer vdir storage - standard one-event-per-file `.ics` in the-den,
`khal list --json` for agendas, no custom CLI. Designed, parked.

## 5. Proactive contact

One policy layer in the Scufris daemon; sources (timers first, worker
events, watcher much later) never reach the user directly. Offer, do
not act; graduated delivery (transcript, attention state, spoken);
stated triggers; per-topic mute; quiet hours; interruption budget;
audit entries. The only hook the first slices need is starting a turn
from a timer with a fixed prompt. V1 is the morning briefing, stage 6.

## 6. Not built

No custom calendar or workout CLIs; no canonical database; no Scufris
hooks in dashboardd; no external-URL widget; no always-on capture; no
speculative embeddings, knowledge graph, or sync service; no repo
merging; no hardware (library research topic only); wake word stays in
the HUD task.

## 7. First demonstrable prototype

Stage 1: Scufris x today. One session must demonstrate: "what do I
have tomorrow" answered from data; a task added by voice for a future
date surviving a Neovim-concurrent edit via revision retry; "how is my
weight trending" answered with the today.weight widget opened
alongside. Verified by extension tests over a fixture den plus that
live session; released and pinned through the normal gate. It is the
smallest slice that makes Scufris feel like Jarvis - it knows your
day - and every later stage (library, viewer, search, ingestion,
briefing) builds on its plumbing.
