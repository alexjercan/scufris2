# Research: reusable tools and patterns

Judgment roll-up of five component sweeps. Full evidence with citations
and nixpkgs checks lives under `research/`. All availability claims were
verified live against nixpkgs on 2026-08-24.

## Web capture (`research/capture.md`)

Shortlist for the first library increment, all in nixpkgs:

- `monolith` - single-file HTML snapshots of static pages, no browser
  dependency.
- `single-file-cli` - headless-Chrome snapshots when JS rendering is
  required.
- `yt-dlp` and `gallery-dl` - video and image references, with sidecar
  metadata (`--write-info-json`, `--write-metadata`) feeding provenance.
- `trafilatura` - extraction of clean Markdown/JSON from an
  already-downloaded local HTML file, with metadata fields (title,
  author, url, date, tags) that map almost directly onto the manifest
  schema. Fits the "CLI never fetches" rule exactly.
- `pandoc` - format glue, later useful for the course pillar.

Store two artifacts per item: the single-file HTML snapshot (faithful
visual display) and the extracted Markdown (indexable, citable text).
Skip WARC: replay tooling (pywb, browsertrix) is not in nixpkgs and no
stated use case needs forensic fidelity; `wget --warc-file` remains a
zero-cost fallback.

Rejected as wrong-shaped: ArchiveBox, shiori, linkding, wallabag. All
couple fetch, store, and serve (usually with their own database), which
violates the Scufris-fetches / CLI-persists split.

## Multimodal ingestion (`research/ingestion.md`)

The video-to-learning-material pipeline is cheaper than expected:

- Transcription: the existing `whisper-server` already returns
  word-level timestamps via `response_format=verbose_json`. No new
  service. WhisperX (in nixpkgs) is a documented precision upgrade
  (~50 ms alignment vs ~500 ms), not needed for v1.
- Keyframes: ffmpeg scene detection emits frame timestamps on the same
  clock as the transcript; joining is time-window arithmetic.
  PySceneDetect (in nixpkgs) upgrades slide-heavy screen recordings.
  Local VLM frame pruning (ollama or llama.cpp, packaged with Vulkan)
  is a real but deferred lever, applied only to scene-change candidates.
- Documents: `docling` (in nixpkgs) for PDF and image to Markdown with
  page and bounding-box provenance; `tesseract` and `ocrmypdf` as
  fallbacks. `marker` is NOT in nixpkgs (the attribute with that name
  is an unrelated GTK app).
- Cost: transcription is the only GPU-heavy step (minutes per hour of
  video); everything else is cheap CPU. No cloud step is justified for
  v1 - the local-first default holds without compromise.

## Retrieval (`research/retrieval.md`)

Staged, evidence-gated:

1. v1: `rg`/`fd` over manifests and extracted text. Legitimate for
   hundreds to low thousands of items, not a placeholder. Queries come
   from Scufris as reformulated text, so typo tolerance and live-typing
   latency do not apply.
2. v2: SQLite FTS5 embedded inside the library CLI (external-content
   tables). In nixpkgs' default sqlite; no daemon, no new packaging.
   Adds ranking and phrase queries.
3. v3, only on evidence: sqlite-vec plus fastembed (nomic-embed-text)
   in the same SQLite file, with RRF fusion in the CLI query path. The
   gate is a concrete observed signal: repeated real queries where FTS5
   misses an item the user can name.

Rejected: meilisearch and typesense (real but a ~500 MB resident daemon
to babysit, unjustified at this scale); Khoj-style architecture (needs
Postgres and pgvector - validates the desire, not the design).
Operational: hash-keyed incremental reindex is rename-proof; full
rebuild costs seconds to minutes; citations are file line ranges and
transcript timestamp ranges. The index is derived and disposable by
construction.

## Reference display (`research/reference-display.md`)

Grounded in the dashboardd source. Split by content type:

- Saved and extracted content (articles, notes, images): a generic
  viewer widget following the pattern `widgets/tatr-tasks` already
  ships - a typed artifact reference as input, `markdown|html|text|
image` content rendered through `DOMPurify.sanitize()` with a strict
  forbid-list. No dashboardd core changes; Scufris drives it through
  the existing typed open/update/focus/close flow.
- Live pages, full-fidelity snapshots, PDFs: the real browser plus i3
  window rules and a small helper. Best isolation, zero dashboardd
  work.
- Structurally blocked today: iframes and external URLs inside
  dashboardd. The global CSP sets `frame-src 'none'` and `object-src
'none'`, and `WebviewUrl::External` is unused. A generic URL window
  or a standalone wry viewer stays a later option with known Tauri
  rough edges (tauri#8476, tauri#12740).

Sanitized-viewer rendering doubles as the untrusted-content boundary:
captured web content displays without script execution.

## Calendar, parked (`research/calendar.md`)

Presumptive answer when the need appears: khal plus vdirsyncer. The
vdir format is exactly one standard `.ics` file per event, git-friendly
and readable by other tools; an assistant agenda query is `khal list
now 7days --json`. Both in nixpkgs. Caveats: vdirsyncer is entering
maintenance mode; reminders need a small systemd timer with
notify-send. Radicale, calcurse, and gcalcli do not fit the pattern.

## Cross-cutting conclusions

- The dual-artifact pattern (snapshot for display, extraction for
  retrieval) mirrors the observation-versus-presentation principle at
  the data layer.
- Everything on the shortlists is in nixpkgs; the deployment gate adds
  little friction here. Known gaps: marker, pywb, browsertrix - none
  needed.
- No new daemons anywhere. The only index is embedded SQLite inside
  the library CLI.
- Native-tool surface should stay well under the ~30-tool ceiling that
  degrades tool selection (see MARKET.md); grouped tools beat
  one-tool-per-subcommand.
