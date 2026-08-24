# Web capture and archival tooling for the-den library

Research date: 2026-08-24. Scope: tools for the FETCH side (what Scufris or a
helper shells out to while it browses/downloads during conversation) and the
FORMAT side (what the deterministic library CLI persists into `the-den`).
Constraint from `NOTES.md`: Scufris fetches; the library CLI only ingests an
already-downloaded local blob, stores it under a git-ignored blob directory,
and writes a git-tracked manifest (hash, source URL, capture date, modality,
topics). The CLI never talks to the network. Everything must run on NixOS,
either from nixpkgs directly or as a pinned flake input.

nixpkgs availability was checked with `nix search nixpkgs <name>` and
`nix eval nixpkgs#<attr>.version` on 2026-08-24 (channel: whatever `nixpkgs`
resolves to in this flake's inputs, effectively unstable).

## Framing: two different jobs, don't conflate them

1. **Fetch**: turn a URL (found during conversation) into a local blob or set
   of blobs on disk. This can be a full-fidelity page snapshot, an extracted
   article, a downloaded video/audio file, or a downloaded image set. Scufris
   or a small helper invoked by Scufris does this. Output lands in a scratch
   or staging directory.
2. **Format/persist**: take the blob(s) already on disk, compute a content
   hash, move/copy into the library's ignored blob store, and write a
   manifest entry. This is the library CLI's only write path. It does not
   care how the blob was produced, only that it exists locally with a known
   source URL and modality.

The tools below sort into three buckets: (a) fetch-and-snapshot tools
Scufris can shell out to, (b) format/extraction tools that turn a raw
snapshot into a cleaner artifact (can run on either side — Scufris can call
them before handing off, or the library CLI could offer a "reformat" helper
that still never touches the network), and (c) full self-hosted archiving
apps that bundle fetch+store+serve and therefore violate the split.

## Snapshot format comparison

**Single-file HTML** (monolith/SingleFile/obelisk style: one `.html` with
CSS/JS/images/fonts inlined as data URIs).

- Pro: trivially portable, one file per manifest entry, opens in any browser
  or a dashboardd webview with no server needed, human-inspectable.
- Pro: matches the-den's "plain files, git-ignored blobs" model with zero
  extra tooling — a static file server or `file://` load is enough.
- Con: not a faithful re-fetchable record (no HTTP headers, no redirect
  chain, no ability to replay multiple resources independently); large data
  URIs bloat the file and defeat text-level dedup; JS-driven pages need a
  real browser to snapshot well (monolith has no JS engine; SingleFile does
  via headless Chrome).
- Con: not indexable as text without a second extraction pass.

**WARC** (Web ARChive, the actual preservation standard; wget `--warc-file`,
browsertrix-crawler, grab-site, ArchiveBox all produce it).

- Pro: the real archival format — records full HTTP request/response,
  supports multi-resource captures, replayable with pywb/ReplayWeb.page,
  auditable provenance (headers, timestamps, digests) built in.
- Con: needs a WARC replay tool (pywb, or the browser-based
  ReplayWeb.page/WACZ viewer) to actually _view_ a snapshot — dashboardd
  would need a webview widget that can host a replay player, not just serve
  a static file. That's real extra infrastructure for a "collect resources"
  use case that mostly wants to re-read an article later.
- Con: none of the lightweight WARC-capable crawlers are packaged in
  nixpkgs (browsertrix-crawler, pywb, grab-site all absent); only `wget`
  (already in nixpkgs) can emit WARC directly, without a JS engine.
- Verdict: WARC is the right format for "this must be forensically
  reproducible," which is not the stated use case (game research, art
  references, hardware notes). Worth revisiting only if the library later
  needs legal/forensic-grade provenance or full-site captures.

**Extracted Markdown/text** (readability-cli, trafilatura, percollate,
pandoc's HTML-to-Markdown).

- Pro: this is what a manifest actually wants to _index and show_: clean
  text, real metadata fields (title, author, date, site), small, diffable,
  greppable, embeds trivially into a lexical or embedding index later. It's
  also what a "course builder" pipeline (vision pillar) wants as raw
  material — citable prose, not a DOM blob.
- Pro: trafilatura and readability-cli both accept an already-downloaded
  local HTML file (`--input-dir`, or `readable somefile.html`), so this step
  fits cleanly on either side of the fetch/persist boundary without
  violating "CLI never fetches."
- Con: lossy. Drops layout, most images (unless explicitly kept), embedded
  widgets/interactive content, and can mis-extract on unusual page
  structures (paywalls, JS-rendered SPAs need to be pre-rendered first).
- Con: not a faithful "look what I found" reference — if Scufris wants to
  show the actual page in a widget, Markdown alone loses the visual.

**Recommendation for the format side**: store both, cheaply. A single-file
HTML (or plain saved HTML if the page is simple) as the faithful visual
snapshot for the widget/browser path, plus a Markdown/JSON extraction as the
indexable, citable text for search and the course builder. Both are just
files the manifest points at; no format lock-in, no replay server. WARC is
deliberately out of scope for the first increment.

## Tool-by-tool

### Full-page single-file HTML capture (fetch-side)

**monolith** — <https://github.com/Y2Z/monolith>

- What it does: bundles a URL or piped-in HTML into one self-contained HTML5
  file, inlining CSS/JS/images/fonts as data URIs. Rust, single static
  binary, no runtime deps beyond libssl.
- Output: single `.html` (or `.mhtml` with `-m`). Adds an HTML comment with
  capture timestamp and source URL by default (suppress with `-M`) — this is
  built-in provenance, useful even though the library CLI is the real
  manifest owner.
- CLI quality: excellent — one static binary, `-o file`/`-o -` for stdout,
  domain allow/block-listing (`-d`/`-B`), cookie file support, basic-auth in
  URL, proxy via env vars, quiet/silent flags. Scriptable and predictable.
- Provenance: URL + timestamp comment (optional), nothing structured beyond
  that — fine, since the library CLI's manifest is the source of truth.
- Limitation: no JS engine. JS-rendered pages need a pre-render step (its
  own docs suggest piping `chromium --headless --dump-dom` output into it).
- Maintenance: active (last push 2026-05-25, 15.4k stars, packaged
  virtually everywhere: Homebrew, Guix, Arch, Alpine, FreeBSD, nixpkgs).
- nixpkgs: yes — `monolith` 2.10.1 (`nix search nixpkgs monolith`).
- Fit: strong fetch-side candidate for the default "just grab this page"
  path when JS rendering isn't needed. Cheapest, most portable option.

**SingleFile / single-file-cli** —
<https://github.com/gildas-lormeau/SingleFile> (WebExtension, the reference
implementation) and <https://github.com/gildas-lormeau/single-file-cli> (CLI
wrapper).

- What it does: same single-file-HTML goal as monolith, but drives a real
  headless Chrome/Chromium via CDP (through Deno), so it captures
  JS-rendered content and generally produces the most faithful snapshot of
  the three single-file tools (per obelisk's own README comparison).
- Output: single HTML, or a self-extracting ZIP/WACZ-like archive with
  `--compress-content`/`--crawl-save-archive`. Filename templating supports
  `{url}`/`{date-iso}` placeholders useful for staging filenames before the
  library CLI ingests them.
- CLI quality: good, many options (batch via `--urls-file`, crawl depth,
  URL rewriting), but requires a working Chrome/Chromium install and Deno —
  heavier dependency footprint than monolith.
- Provenance: no explicit in-file metadata block by default; relies on
  filename templating instead.
- Maintenance: very active (SingleFile extension pushed 2026-08-05,
  single-file-cli pushed 2026-08-16, 22.2k/1.5k stars respectively, AGPL-3.0
  with a commercial license option).
- nixpkgs: yes — `single-file-cli` 1.1.49.
- Fit: the fetch-side upgrade path when a page needs real JS rendering
  (most modern SPAs, paywalled-but-visible articles, art/game reference
  sites with dynamic galleries). Costs a Chromium dependency.

**obelisk** — <https://github.com/go-shiori/obelisk>

- What it does: Go equivalent of monolith; built as shiori's archiving
  engine. Inlines JS/CSS directly (not always base64) for smaller files,
  disables external requests via CSP by default, concurrent asset fetch
  (faster), handles non-HTML content and cookie-based auth.
- Output: single HTML file, usable as a library (Go) or CLI.
- CLI quality: decent but far less documented than monolith; primarily
  consumed as a library by shiori rather than standalone.
- Maintenance: moderate — last push 2026-02-01, 320 stars, small
  maintainer base (single author, built for shiori's own needs).
- nixpkgs: not packaged standalone (only reachable today as a dependency
  inside shiori's build; no top-level `obelisk` attribute — nixpkgs
  `obelisk` is an unrelated OCaml/typst tool).
- Fit: interesting but redundant with monolith/SingleFile and not
  independently packaged. Skip unless shiori itself becomes relevant.

### Readable-text / Markdown extraction (either side)

**trafilatura** — <https://github.com/adbar/trafilatura>

- What it does: state-of-the-art boilerplate removal and main-content +
  metadata extraction from HTML. Python package and CLI.
- Output formats: `txt` (default), `csv`, `json`, `markdown`, `html`, `xml`,
  `xmltei` (`--output-format` or shorthand flags `--markdown`/`--json`/...).
- Metadata: with `--with-metadata`, extracts a structured record with
  fields `title, author, url, hostname, description, sitename, date,
categories, tags, fingerprint, id, license, image, pagetype` (from the
  `Document` class, `trafilatura/settings.py` in the source tree) — this
  maps almost one-to-one onto the manifest fields NOTES.md wants (source
  URL, capture date, topics via categories/tags). `--only-with-metadata`
  can gate on having title+url+date present.
- Local-file support: `--input-dir`/`-o --output-dir` processes
  already-downloaded HTML with no network access — exactly the "CLI never
  fetches" shape, so this can safely live inside the library CLI itself as
  an ingest-time enrichment step, not just on Scufris's side.
- CLI quality: excellent, mature, well-documented (`docs/usage-cli.rst`),
  parallel/batch processing, blacklist files, precision/recall tuning.
- Maintenance: very active (pushed 2026-08-21, 6.7k stars), used by
  HuggingFace/Microsoft Research/Stanford per its own docs; widely
  benchmarked as a top open-source extractor.
- nixpkgs: yes — `python313Packages.trafilatura` / `python314Packages...`
  2.1.0 (Python library + `trafilatura` console script).
- Fit: the strongest single tool for the format side. Feed it a downloaded
  HTML file, get Markdown + a metadata JSON sidecar for the manifest.

**readability-cli** — <https://gitlab.com/gardenappl/readability-cli>
(binary name `readable`, npm package `readability-cli`, nixpkgs
`readability-cli`)

- What it does: wraps Mozilla's actual Readability library (the same code
  Firefox Reader View uses) via Node.js or Deno. `SOURCE` can be a URL, a
  file, or `-` for stdin — so it _can_ fetch on its own, but works equally
  well purely on a local file (`readable index.html`).
- Output: HTML article body by default; `-p title,excerpt,byline,...`
  prints individual properties; `-j/--json` dumps all known properties
  (`title, byline, excerpt, length, dir, text-content, html-content`) as
  JSON. No native Markdown output — pipe through pandoc for that.
- CLI quality: solid, man-page documented, sane sysexits-style exit codes,
  proxy env var support, `--low-confidence` policy for uncertain
  extraction (`keep`/`force`/`exit`).
- Maintenance: actively packaged — nixpkgs pins
  `2.4.5-unstable-2026-01-07`, i.e. tracked this year.
- nixpkgs: yes — `readability-cli` 2.4.5-unstable-2026-01-07.
- Fit: good, lighter-weight alternative to trafilatura when only
  title/byline/excerpt/content is needed and Mozilla's exact Reader View
  behavior is preferred over trafilatura's own heuristics. Slightly weaker
  metadata (no explicit date/tags field) than trafilatura.

**percollate** — <https://github.com/danburzo/percollate>

- What it does: fetches URLs (via Puppeteer/headless Chrome, prefers AMP)
  and bundles one or more pages into PDF, EPUB, HTML, or Markdown, with
  cover page and table-of-contents generation.
- Output: PDF/EPUB/HTML/Markdown, single bundled doc by default or
  `--individual` per source.
- CLI quality: good, `--title`/`--author` metadata flags, `--inline` for
  base64 images, `--css`/`--template` customization.
- Maintenance: last push 2025-08-29 (about a year old at research time),
  4.7k stars, 17 open issues — slower-moving than trafilatura/monolith but
  not abandoned.
- nixpkgs: yes — `percollate` 4.3.0.
- Fit: nice-to-have for "compile several saved pages into one EPUB/PDF for
  reading later" (maps to the course-builder vision pillar), but overlaps
  with trafilatura+pandoc for the base extraction job. Not needed for the
  first increment.

**pandoc** — <https://pandoc.org>, <https://github.com/jgm/pandoc>

- What it does: universal document converter. Relevant here: `pandoc -f
html -t markdown` for HTML-to-Markdown, `--extract-media DIR` to pull
  embedded/linked images out into files during conversion, `-M` to inject
  metadata as YAML frontmatter (does not auto-extract HTML metadata itself
  — that has to come from trafilatura/readability-cli output or manual
  flags).
- CLI quality: extremely mature, the de facto standard for markup
  conversion; also useful downstream for turning saved Markdown into PDF/
  EPUB for the course builder.
- Maintenance: very active (pushed 2026-08-23, 46k stars).
- nixpkgs: yes — `pandoc` (top-level package; not surfaced by fuzzy
  `nix search`, confirmed via `nix eval nixpkgs#pandoc.version` = 3.7.0.2).
- Fit: glue tool, not a primary capture tool. Useful for format
  normalization (e.g., EPUB assembly for the course builder) and as a
  fallback HTML-to-Markdown converter when trafilatura's extraction is
  intentionally bypassed (e.g., converting a whole saved page rather than
  just the extracted article).

### WARC / high-fidelity crawling (fetch-side, deliberately not first-increment)

**wget** (`--warc-file`) — already in nixpkgs (`wget` 1.25.0), also
`wget2` 2.2.1 (successor, fewer WARC options currently).

- Supports `--warc-file`, `--warc-header`, `--warc-cdx`, `--warc-dedup`,
  `--no-warc-compression`, SHA1 digests — genuine WARC output with zero
  extra packaging since wget is already a base NixOS tool. No JS
  rendering, so SPA-heavy pages capture incompletely.
- Fit: the cheapest possible path to WARC if/when the library needs it;
  no new dependency. Not needed while single-file HTML + Markdown covers
  the stated use cases.

**browsertrix-crawler** — <https://github.com/webrecorder/browsertrix-crawler>

- Docker-first, Puppeteer/Brave-driven high-fidelity crawler producing
  WARC and WACZ. Handles JS-heavy sites and multi-page crawls well; the
  closest thing to "Archive.org quality" capture.
- Maintenance: active (pushed 2026-08-21, 1.1k stars, AGPLv3).
- nixpkgs: **not packaged** (no `browsertrix-crawler` attribute); would
  need a Docker image or a manually pinned npm/Playwright build — heavy
  lift for NixOS packaging (Playwright's browser binaries are notoriously
  painful to pin in Nix).
- Fit: skip for now. Revisit only if full-site or forensic-grade capture
  becomes a real requirement.

**pywb** — <https://github.com/webrecorder/pywb>: WARC replay/record
toolkit. Not packaged in nixpkgs (searched `pywb`, only `pywbem` — an
unrelated systems-management package — matches). Needed only if WARC
becomes the chosen format; skip for now given the Markdown/single-file
recommendation above.

**grab-site** — <https://github.com/ArchiveTeam/grab-site> (formerly
ludios/grab-site): wpull-based WARC crawler with a live dashboard.
Maintenance is stale (last push 2025-05-23) and not in nixpkgs. Skip.

### Full self-hosted archiving apps (rejected for this split, noted for completeness)

**ArchiveBox** — <https://github.com/ArchiveBox/ArchiveBox>

- What it does: the closest thing to an all-in-one answer — per URL it
  runs wget (mirror + WARC), a headless Chrome for screenshot/PDF/DOM
  dump, SingleFile, readability/Mercury extraction, yt-dlp for media, and
  git-clones for repo URLs, indexing everything in a SQLite index plus
  per-snapshot `index.json`/`index.html`.
- Why it's a poor fit here: it _is_ fetch+store+index bundled together,
  which is exactly the coupling NOTES.md's division of labor rejects
  ("Scufris fetches ... CLI never fetches from the network"). It also
  wants to own its own SQLite index as canonical, conflicting with the
  "plain files, tracked manifests" decision. Bare-metal install needs
  Python 3.13+, Chrome, wget, yt-dlp, git, supervisord — a lot of surface
  for a piece that would only be used for its individual extractors.
- nixpkgs: **not packaged** (only a helper, `readability-extractor`, is).
  No top-level `archivebox` attribute — would mean packaging by hand or a
  Docker/uv install, on top of it being the wrong shape anyway.
- Useful anyway: as a reference for which extractors to shell out to
  individually (wget WARC, SingleFile, yt-dlp, readability) — its
  extractor list is a good checklist, just not its architecture.

**shiori** — <https://github.com/go-shiori/shiori>

- Go bookmark manager, CLI or web-app modes, SQLite/Postgres/MySQL
  backend, uses obelisk internally to snapshot pages. Actively maintained
  (pushed 2026-07-10, 11.6k stars) and in nixpkgs (`shiori` 1.8.0).
- Poor fit: it's fetch-oriented (give it a URL, it downloads and stores in
  its own DB-backed structure), which again couples fetch and persist
  the way NOTES.md explicitly avoids, and stores state in a database
  rather than plain tracked files.

**linkding** — <https://github.com/sissbruecker/linkding>: Django web app,
Docker-first, REST API, no meaningful standalone CLI. Self-hosted service,
not a personal CLI tool; wrong shape entirely for this project. In
nixpkgs (`linkding` 1.45.0) but not a fit.

**wallabag** — <https://github.com/wallabag/wallabag>: Symfony/PHP web
app needing a database (MySQL/Postgres/SQLite) and full web server stack.
Same problem as linkding, heavier. In nixpkgs (`wallabag` 2.6.14) but not
a fit; note nixpkgs also has a separate lightweight `read-it-later` GTK
client for it, irrelevant here.

### Media (video/audio/galleries) — fetch-side

**yt-dlp** — <https://github.com/yt-dlp/yt-dlp>

- What it does: downloads video/audio from thousands of sites (YouTube
  plus a generic extractor for many others). Directly relevant to the
  "video becomes transcript + keyframes" multimodal ingestion idea in
  NOTES.md.
- Metadata/sidecars: `--write-info-json` (full metadata as JSON),
  `--write-thumbnail`, `--write-description`, `--write-subs`/
  `--write-auto-subs` (existing or auto-generated captions/transcript),
  `--embed-metadata`/`--embed-chapters`/`--embed-thumbnail` to bake
  metadata into the container. `--dump-json`/`--simulate` gets metadata
  without downloading, useful for a "let me check what this is before
  grabbing it" step.
- CLI quality: excellent, huge flag surface, extremely well documented,
  the de facto standard.
- Maintenance: extremely active (pushed 2026-08-20, 186k stars, ~24k
  commits) — fork of youtube-dl, now the more actively maintained project.
- nixpkgs: yes — `python313Packages.yt-dlp` (and a `-light` variant with
  fewer deps).
- Fit: clear fetch-side tool for video/audio. `--write-info-json` output
  maps directly onto manifest fields (source URL, title, upload date,
  tags). Auto-subs (`--write-auto-subs`) is a cheap first pass toward the
  "word:timestamp transcript" pipeline described in NOTES.md, ahead of
  running local Whisper for higher-quality transcripts.

**gallery-dl** — <https://github.com/mikf/gallery-dl>

- What it does: downloads image galleries/collections from many sites
  (Twitter/X, Reddit, Pixiv, Instagram, DeviantArt, ArtStation-adjacent
  sites relevant to "art for inspiration" use case) with a huge
  site-extractor list, mirroring yt-dlp's architecture.
- Metadata/sidecars: `--write-metadata` (per-file JSON), `--write-tags`,
  `--write-info-json` (gallery-level), `-j/--dump-json` and `-s/--simulate`
  for metadata-only, `--zip`/`--cbz` archive packing, `--mtime` to set
  file times from metadata, powerful filename/directory templating.
- CLI quality: excellent, same tier as yt-dlp (shared authorship
  conventions), heavily configurable via JSON config files.
- Maintenance: active (pushed 2026-08-01, 19.3k stars); note the GitHub
  mirror itself flags that primary development has moved to Codeberg
  (<https://codeberg.org/mikf/gallery-dl>) — worth pinning the flake input
  to Codeberg or verifying the GitHub mirror stays in sync before treating
  GitHub as canonical.
- nixpkgs: yes — `gallery-dl` 1.32.6.
- Fit: clear fetch-side tool for the "art for inspiration" / image
  reference use case named in NOTES.md. `--write-metadata` output again
  maps onto manifest fields directly.

## nixpkgs availability summary

| Tool                | nixpkgs attribute               | Version (checked 2026-08-24) | Notes                                     |
| ------------------- | ------------------------------- | ---------------------------- | ----------------------------------------- |
| monolith            | `monolith`                      | 2.10.1                       | yes                                       |
| single-file-cli     | `single-file-cli`               | 1.1.49                       | yes, needs Chromium at runtime            |
| readability-cli     | `readability-cli`               | 2.4.5-unstable-2026-01-07    | yes                                       |
| trafilatura         | `python313Packages.trafilatura` | 2.1.0                        | yes                                       |
| percollate          | `percollate`                    | 4.3.0                        | yes, needs Chromium (Puppeteer)           |
| pandoc              | `pandoc`                        | 3.7.0.2                      | yes (top-level, not fuzzy-search-ranked)  |
| yt-dlp              | `python313Packages.yt-dlp`      | 2026.07.04                   | yes                                       |
| gallery-dl          | `gallery-dl`                    | 1.32.6                       | yes                                       |
| wget                | `wget`                          | 1.25.0                       | yes, has native `--warc-file`             |
| wget2               | `wget2`                         | 2.2.1                        | yes                                       |
| warcio              | `python313Packages.warcio`      | 1.7.5                        | yes (WARC read/write lib, if ever needed) |
| shiori              | `shiori`                        | 1.8.0                        | yes, wrong shape (see above)              |
| linkding            | `linkding`                      | 1.45.0                       | yes, wrong shape                          |
| wallabag            | `wallabag`                      | 2.6.14                       | yes, wrong shape                          |
| ArchiveBox          | none                            | -                            | not packaged; wrong shape anyway          |
| obelisk             | none (top-level)                | -                            | not packaged standalone                   |
| browsertrix-crawler | none                            | -                            | not packaged; Docker-first                |
| pywb                | none                            | -                            | not packaged                              |
| grab-site           | none                            | -                            | not packaged; stale upstream              |

## Shortlist for the first library increment

Keep the increment to tools that are (a) already in nixpkgs, (b) genuinely
single-purpose, and (c) respect the fetch/persist split.

**Fetch-side, invoked by Scufris or a thin helper it shells out to:**

- `monolith` for the default "snapshot this page" case (static pages,
  articles, docs) — no headless-browser dependency, cheapest to pin.
- `single-file-cli` as the fallback when a page needs JS rendering
  (accept the Chromium dependency only when this path is actually used).
- `yt-dlp` for video/audio references, with `--write-info-json
--write-auto-subs` as the default flags — this both captures the blob
  and gives the library CLI ready-made manifest fields plus a first-pass
  transcript.
- `gallery-dl` for image/gallery references, with `--write-metadata`.

**Format-side, run either by Scufris before handoff or as an optional
enrichment step inside the library CLI itself (still purely local, no
network):**

- `trafilatura --input-dir ... --json --with-metadata --markdown` to turn a
  saved HTML snapshot into clean Markdown plus a metadata JSON sidecar.
  This is the single highest-leverage tool in the whole survey: its
  `Document` fields (`title, author, url, hostname, date, categories, tags,
license, ...`) map almost directly onto the manifest schema NOTES.md
  wants, and it explicitly supports operating on already-downloaded files,
  so it never breaks the "CLI never fetches" rule even if invoked from
  inside the library CLI.
- `pandoc` kept in reserve as the general-purpose converter for anything
  trafilatura doesn't cover (e.g., turning a Markdown extract into EPUB for
  the course-builder vision pillar later).

**Explicitly deferred, not in the first increment:**

- WARC/replay tooling (`wget --warc-file`, browsertrix-crawler, pywb) —
  right tool only if forensic/full-site fidelity becomes a real
  requirement; adds a replay-player dependency to the widget path that
  nothing in NOTES.md currently needs.
- `readability-cli` and `percollate` — real alternatives to trafilatura,
  worth a second look if trafilatura's extraction quality disappoints on a
  concrete page, but redundant to add both now.
- ArchiveBox, shiori, linkding, wallabag — all couple fetch and persist or
  require a database/server, which the decided architecture rejects; keep
  ArchiveBox's extractor list as a reference checklist only.
- `obelisk`, `browsertrix-crawler`, `pywb`, `grab-site` — not in nixpkgs;
  no current requirement justifies the packaging effort.

**Manifest field mapping this shortlist enables directly:** source URL
(from Scufris's fetch invocation or trafilatura's/yt-dlp's/gallery-dl's
own metadata extraction), capture date (`date`/`filedate` from trafilatura,
or capture-time timestamp), modality (which tool produced the blob:
html-snapshot / article-markdown / video / image), topics (trafilatura's
`categories`/`tags` as a starting point, refined by Scufris's own judgment
at capture time).
