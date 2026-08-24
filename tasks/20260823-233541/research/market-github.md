# Market survey: open-source Jarvis-adjacent projects

Sweep date: 2026-08-24. Stars and activity pulled live via `gh api repos/<slug>`.
Scope: projects relevant to Scufris (voice-in/out, Pi-based agent, plain-file
vault, native widget windows, NixOS deployment, local-first).

Depth is on ~16 projects across 7 categories. Each entry: what it is, storage
format, CLI/API quality, activity, what Scufris could reuse, and the design
lesson (win or failure).

---

## 1. Voice assistants

### Mycroft AI (`MycroftAI/mycroft-core`) - dead, but the reason matters

One-liner: the original open-source "Jarvis" voice assistant platform;
skills in Python, wake word + STT + intent parser + TTS pipeline.
Storage: skills as Python packages with `.dialog`/`.voc` files; no vault
concept, no PKM integration.
CLI/API: `mycroft-cli-client` for text testing; skill API is a Python class
with intent decorators.
Activity: archived. 6,610 stars, last push 2024-09-08 (dead code, kept for
history).
Reuse: none directly; superseded by OpenVoiceOS fork.
Learn (failure): Mycroft did not fail on product-market fit or code quality.
It died from a 2020 patent-troll lawsuit (Voice Tech Corporation) that
drained the company's cash reserves, on top of a Kickstarter (Mark II
hardware) it could not fulfill. Company closed February 2023, laid off ~20
staff to a skeleton crew. Lesson for Scufris: irrelevant at hobby scale, but
it's a clean example of a strong open project killed by external
non-technical forces, not by the software. The community forked within a
month (see OpenVoiceOS) -- decoupling the project's survival from the
company's survival is what actually saved the code.
Sources: https://github.com/MycroftAI/mycroft-core ,
https://www.theregister.com/2023/02/13/linux_ai_assistant_killed_off/ ,
https://www.hackster.io/news/mycroft-closes-its-crowdfunding-campaign-with-rewards-undelivered-as-it-runs-out-of-runway-00a1103e7f61

### OpenVoiceOS (`OpenVoiceOS/ovos-core`) - the fork that survived

One-liner: community continuation of Mycroft, modularized (STT/TTS/intent
parsers/PHAL hardware abstraction are separate pip packages) and boringly
maintained.
Storage: same skill-package model as Mycroft; config in `mycroft.conf`
(JSON5), no plain-file vault story.
CLI/API: skill SDK is stable but Python-class-based, heavier than a stdlib
CLI; good `ovos-*` package granularity for reuse (e.g. `ovos-plugin-manager`
for hot-swapping STT/TTS engines).
Activity: 283 stars on this repo (org is split across dozens of small
plugin repos, so stars undercount true size), pushed 2026-08-17. Active,
unglamorous, "boring installs, easy updates" is the stated design goal.
Reuse: the plugin-manager pattern (STT/TTS/wake-word engines as swappable
plugins behind one interface) is worth studying if Scufris ever needs to
swap Whisper/Piper for another engine without touching the extension.
Learn (win): OpenVoiceOS explicitly optimized for boring operability after
watching Mycroft's ambition (own hardware, own cloud, own account system)
become its undoing. It stayed software-only, avoided a company, and
distributed ownership across the community. Directly validates Scufris's
posture: no hosted service, no hardware bet, small deterministic pieces
over one monolith.
Sources: https://github.com/OpenVoiceOS/ovos-core ,
https://blog.openvoiceos.org/posts/2025-05-20-ovos-and-mycroft-a-fork-that-wasnt-meant-to-be ,
https://dev.to/goldyfruit/from-mycroft-and-ansible-to-openvoiceos-boring-installs-easy-updates-11cl

### Rhasspy / Wyoming protocol (`rhasspy/rhasspy`, `rhasspy/rhasspy3`) - archived, but its protocol won

One-liner: offline voice assistant toolkit; v3 introduced the Wyoming
protocol, a minimal newline-delimited-JSON-plus-binary-payload wire format
for chaining STT/TTS/wake-word services over TCP.
Storage: N/A (pipeline tool, not a data store).
CLI/API: Wyoming itself is the interesting artifact -- a tiny protocol
(`pypi.org/project/wyoming`), not a framework. Any STT/TTS engine can speak
it by wrapping stdin/stdout in an event loop.
Activity: both `rhasspy` (2,750 stars) and `rhasspy3` (382 stars) are
archived; last push 2025-04 and 2023-12 respectively. The GitHub org is
essentially retired, but the protocol lives on inside Home Assistant.
Reuse: not the codebase -- the pattern. Wyoming is a good reference for how
Scufris's own tool contracts (native tools wrapping `today --json`) should
look if they ever need to run out-of-process: small JSON events over a
pipe/socket, one verb per message, no framework tax. `dashboardctl`'s
Unix-socket JSON contract already follows this shape independently.
Learn (mixed): Rhasspy the _product_ stalled and was archived, but Rhasspy
the _protocol design_ was absorbed wholesale into Home Assistant's Assist
pipeline and is now the de facto standard for local voice satellites
(`wyoming-piper`, `wyoming-whisper`, `wyoming-satellite`). The lesson:
a narrow, dependency-free protocol can outlive the application that spawned
it, while the full-featured app around it does not need to survive for the
design to have won. Design the contract to be more durable than the app.
Sources: https://github.com/rhasspy/rhasspy ,
https://github.com/rhasspy/rhasspy3 ,
https://github.com/rhasspy/wyoming-satellite ,
https://grokipedia.com/page/Wyoming_Home_Assistant_integration

### Willow (`HeyWillow/willow`, formerly `toverainc/willow`)

One-liner: ESP32-S3-BOX firmware for an Echo/Google-Home-class voice
satellite; local wake word, streams audio to a self-hosted inference server
(Willow Inference Server) or Home Assistant/openHAB.
Storage: N/A, hardware-facing firmware project.
CLI/API: web-based config UI plus a WIS (Willow Inference Server) HTTP API
for STT/TTS; not a general tool contract.
Activity: 3,095 stars, pushed 2026-08-04. Alive, moved orgs (tovera to
HeyWillow), still shipping.
Reuse: not directly applicable -- Scufris's STT/TTS already run locally via
whisper-cpp/Piper on the desktop, no satellite hardware needed. Worth a
glance only if Scufris ever grows a remote-room satellite story.
Learn: a second data point (after OpenVoiceOS) that the durable niche in
open voice assistants is "commodity hardware + local inference server +
integrate with Home Assistant," not "be the whole assistant." Nobody is
still trying to out-build a general conversational brain at the firmware
layer; they plug into one.
Sources: https://github.com/HeyWillow/willow ,
https://heywillow.io/hardware/ ,
https://blog.adafruit.com/2025/03/17/willow-an-open-source-low-cost-voice-assistant-platform/

---

## 2. LLM desktop/personal assistants ("Jarvis" attempts)

### Open Interpreter (`OpenInterpreter/open-interpreter`) - pivoted away from being an assistant

One-liner: originally "a natural-language interface for your computer" --
an LLM that writes and executes code locally to control your machine
conversationally. As of this sweep its own GitHub description reads "A
coding agent for open models like Kimi K3."
Storage: N/A, stateless CLI session.
CLI/API: `interpreter` CLI, OpenAI-compatible local server mode; simple and
well regarded early on (Simon Willison covered it favorably in 2024).
Activity: 68,122 stars (very high, mostly legacy), pushed 2026-08-20 --
technically active, but the product has moved on.
Reuse: none directly; the codebase now targets coding-agent use cases, not
personal-assistant/PC-control use cases.
Learn (failure/pivot): this is the single most important cautionary tale in
the survey for Scufris's exact ambition. Open Interpreter explicitly tried
to be "Jarvis" -- general voice-driven computer control, including a
dedicated hardware device (see next entry). It could not sustain that
positioning against the flood of coding agents (Claude Code, Cursor,
Codex) and drifted into being one more coding agent, where it now competes
in a crowded, better-funded field instead of owning a niche. The lesson:
"general assistant that controls your computer" is a starving vision unless
it stays anchored to a narrow, concrete, owned workflow (Scufris's
today/library/widget contracts are exactly that anchor -- don't lose it
chasing generality).
Sources: https://github.com/OpenInterpreter/open-interpreter ,
https://simonwillison.net/2024/Nov/24/open-interpreter/

### Open Interpreter's 01 device (`OpenInterpreter/01`) - the hardware bet that failed

One-liner: "the #1 open-source voice interface for desktop, mobile, and
ESP32 chips" -- a $99 "01 Light" wearable/desktop hardware device meant to
be a physical Jarvis, paired with an open firmware + server stack.
Storage: N/A.
CLI/API: voice-in/voice-out loop over a local or cloud LLM; ESP32 firmware
plus a Python server, similar shape to Willow but assistant-first instead
of automation-first.
Activity: 5,156 stars, last push 2024-11-01 -- effectively frozen.
Reuse: none directly.
Learn (failure): the 01 Light hardware was discontinued and all pre-orders
were refunded; the team pivoted to software-only. This is a second,
sharper version of the same lesson as the parent project: shipping physical
hardware for a still-forming software product multiplies risk (manufacturing,
fulfillment, support) faster than it validates the assistant concept. NOTES.md
already lists "give Scufris a speaker and a body" as a future library
research topic (nova-protocol/hardware) -- 01 is the cautionary precedent:
prototype the interaction pattern entirely in software first, and treat any
hardware step as a much later, much smaller bet than the software one.
Sources: https://github.com/OpenInterpreter/01 ,
https://www.geeky-gadgets.com/pocket-ai-agent/ ,
https://starlog.is/articles/developer-tools/openinterpreter-01/

### Leon AI (`leon-ai/leon`) - the closest architectural analog

One-liner: open-source personal assistant with voice/text I/O, built around
"native skills" (deterministic, hard-coded actions) and "agent skills"
(SKILL.md-backed, LLM-planned workflows) -- the same native-tool-vs-skill
split Scufris uses (native Pi tools vs delegated job workflow).
Storage: skills are packages/modules on disk; no vault/PKM concept, no
plain-file canonical store analogous to the-den.
CLI/API: modular skill SDK; supports "smart mode" (LLM chooses), "controlled
mode" (deterministic native skills only), and "agent mode" (step-by-step
planning) -- a three-way split worth comparing against Scufris's
tool-vs-job-vs-skill boundaries.
Activity: 17,455 stars, pushed 2026-08-20. Actively rebuilding toward a 2.0
developer preview.
Reuse: nothing code-level (different stack: Node/TypeScript, no `today`-style
CLI ownership model), but the three-mode design (deterministic control vs
agent-planned vs auto-routed) is a useful vocabulary check against Scufris's
own native-tool/job/skill split -- validates that the split is a recognized,
convergent pattern, not a one-off.
Learn (win, with a gap): Leon is the most direct "general Jarvis" survivor
in this survey and it survives by keeping native, deterministic skills as
first-class citizens rather than routing everything through the LLM -- the
same bet Scufris and the-den's small CLIs are making. Its gap relative to
Scufris's plan: no owned plain-file data model, so it has no equivalent of
"observation is separate from presentation" -- it's a control surface, not
a knowledge surface. That's exactly the half Scufris is filling in with
today/the-den.
Sources: https://github.com/leon-ai/leon , https://getleon.ai/

---

## 3. AI + PKM over plain files

### Khoj (`khoj-ai/khoj`)

One-liner: "AI second brain" -- chat, semantic search, and scheduled
automations over your own docs (Markdown, org-mode, PDF, Notion) plus the
web; Obsidian/Emacs/desktop clients sync into it.
Storage: not the-den's model. Files are the _input_, but Khoj ingests them
into its own server-side index/DB (hybrid local vector index) for
retrieval; canonical truth moves off the plain files once synced. Real-time
sync keeps a vault mirrored, but the assistant reasons over the derived
index, not the files directly.
CLI/API: REST API and MCP server; strong docs; Obsidian plugin is the
most-used integration point.
Activity: 36,682 stars, pushed 2026-08-02. Actively developed, has a hosted
paid tier alongside the open self-hosted path (server-based revenue,
similar shape to Zylon/PrivateGPT).
Reuse: the retrieval architecture (embeddings + hybrid local vector index
over a synced vault) is a solid reference if/when the-den's library gets an
embedding-based index -- Khoj's docs describe the local-vector-index tradeoffs
well as a design precedent, even though Scufris would keep manifests+search
index separate from a hosted service.
Learn (design tension): Khoj started local-first and file-centric, then grew
server/cloud infrastructure to scale to many users. That drift is exactly
what NOTES.md's rejected-directions list is guarding against (no database
as canonical store, derived index must be safely deletable). Scufris should
resist Khoj's trajectory: keep the index strictly derived/rebuildable and
never let it become the thing that must survive.
Sources: https://github.com/khoj-ai/khoj , https://docs.khoj.dev/clients/obsidian/ ,
https://blog.khoj.dev/posts/obsidian-ux-revamp/

### Reor (`reorproject/reor`) - archived, worth reading as an autopsy

One-liner: Electron desktop app, "private & local AI PKM for high entropy
people" -- auto-linked notes, semantic search, and Q&A entirely on-device,
Obsidian-like markdown editor.
Storage: plain Markdown notes on disk plus a local vector DB (LanceDB) for
embeddings -- closer to Scufris's plain-file-canonical + derived-index model
than Khoj is.
CLI/API: none meaningful; GUI-only Electron app, no scriptable contract.
Activity: archived 2026-03-07 (per GitHub), 8,568 stars, last push
2025-05-13. Dead.
Reuse: none directly (Electron app, not a library or CLI).
Learn (failure): Reor got the storage model closer to right (files
canonical, vector index derived and local) than most of this survey, yet it
still died. Likely causes visible from the outside: it was a closed-loop
GUI app competing against Obsidian-plugin ecosystems (Smart Connections,
Copilot) that ride on Obsidian's existing install base for free, and it had
no CLI/API surface for anyone to build on top of or extend it -- when the
maintainer's interest waned, there was no ecosystem left holding it up.
Lesson: a good storage design is necessary but not sufficient; a personal
tool without either a CLI contract (today's model) or a huge host-app
ecosystem (Obsidian's model) has only one maintainer's attention keeping it
alive.
Sources: https://github.com/reorproject/reor ,
https://alternativeto.net/software/reor-1/about/

### Obsidian Smart Connections (`brianpetro/obsidian-smart-connections`)

One-liner: Obsidian plugin, local embedding model for "related notes" and
semantic search inside the vault; explicitly "zero setup, no API key."
Storage: reads the vault's own Markdown files directly; stores its
embedding index inside the vault's `.smart-env`/plugin data folder,
derived and rebuildable.
CLI/API: N/A -- it's a plugin, not a service; all interaction is inside
Obsidian's UI.
Activity: 5,389 stars, pushed 2026-08-22. Active, single-maintainer,
sustained via Obsidian's plugin ecosystem rather than its own
infrastructure.
Reuse: the "index lives next to the data, fully local embedding model,
zero external API" posture is precisely what a the-den library embedding
index should look like if one is ever built -- proof this is viable at
personal-vault scale without a server.
Learn (win): Smart Connections survives specifically _because_ it declined
to be a whole app -- it's a thin layer over an existing plain-file host
(Obsidian) instead of reimplementing a note editor. Reor tried to
out-build Obsidian and died; Smart Connections rode on top of it and
thrives. For Scufris this reinforces "don't build a widget window manager,
don't restructure the-den" -- plug a thin, local, derived-index layer onto
what already exists rather than replacing it.
Sources: https://github.com/brianpetro/obsidian-smart-connections

### org-ai (`rksm/org-ai`)

One-liner: Emacs package that turns org-mode buffers into an AI chat/agent
surface -- inline completions, image gen, speech input/output, all inside
plain `.org` text files.
Storage: none of its own -- the org-mode buffer _is_ the storage; AI
responses are inserted as plain text blocks in the same file the human
edits, versioned by whatever the user already uses for their org files.
CLI/API: N/A, Emacs Lisp package, editor-native only.
Activity: 822 stars, pushed 2026-01-07. Small, steady, single ecosystem.
Reuse: nothing code-level (Emacs-specific), but it is the cleanest existing
example of "AI output lives in the same plain file the human edits, with no
separate database, no separate app" -- directly validates the-den's
Notes/Daily model where CLIs splice known regions and preserve human edits
byte-for-byte.
Learn (win): org-ai never tried to own storage or presentation -- it is
purely a generation layer over a format (org-mode) that already had a
mature plain-text ecosystem (version control, other tooling, no lock-in).
The smaller the AI layer's footprint on the file format, the longer it
survives changes in both the AI landscape and the editor. Directly
reinforces `today`'s "preserve everything else byte for byte" design.
Sources: https://github.com/rksm/org-ai

---

## 4. Local RAG stacks

### PrivateGPT / Zylon (`zylon-ai/private-gpt`)

One-liner: "interact with your documents privately" -- was the original
breakout local-RAG project (hit #1 across all GitHub categories twice in
2023), now "complete API layer for private AI applications: RAG, skills,
tools, MCP, text-to-sql."
Storage: ingests documents into a local vector store (Qdrant/Chroma
depending on config); documents are not the canonical store, the index is.
CLI/API: REST API, OpenAI-compatible surface, now explicitly MCP-capable.
Activity: 57,458 stars, pushed 2026-08-21. Active but now maintained as the
open core beneath a separate commercial product.
Reuse: as a reference for RAG API shape (ingest/query/MCP-expose) if the
library's retrieval layer ever needs an embedding index; not something to
adopt wholesale (heavier than the-den needs -- full document-management
service, not a thin CLI).
Learn (pattern, not failure): PrivateGPT didn't die, it split. The org
renamed to `zylon-ai`, and the team built a separate enterprise product
(Zylon: deployment, governance, multi-user, auditability) on top of the
still-open PrivateGPT core, rather than closing the open project or forcing
a rename fight. Clean precedent for "open core stays open, monetization
layer stays separate and clearly labeled" if the library or any Scufris
component ever needed a sustainability model -- not urgent for a personal
tool, but a clean pattern if scope ever grows.
Sources: https://github.com/zylon-ai/private-gpt , https://www.zylon.ai/

### AnythingLLM (`Mintplex-Labs/anything-llm`)

One-liner: "everything you need for a powerful local-first agent
experience" -- desktop app + server, document ingestion, agents, multi-model
support, workspace-based RAG.
Storage: SQLite by default (Prisma), documents ingested into a vector DB
(LanceDB/Chroma/Pinecone/etc, pluggable); again index-owns-truth, not
plain-file canonical.
CLI/API: full REST API, well documented, plus a desktop Electron app;
strongest "batteries included" option of the RAG stacks surveyed.
Activity: 65,108 stars, pushed 2026-08-22. Very active, broad plugin
ecosystem (agent "skills" as a marketplace concept).
Reuse: its agent-skill marketplace concept (small declarative capability
packages, sandboxed) is worth a glance as prior art for a
skills/SKILL.md-driven capability catalog, but Scufris already has a
narrower, working answer to this (Pi skills + native tools) and doesn't
need AnythingLLM's heavier document-workspace model.
Learn: another data point that the winning shape for "personal AI over your
data" at scale is a full server+DB app, not a CLI. That's a deliberate
non-goal for Scufris (NOTES.md rejects a database as canonical store) -- the
market default and Scufris's design are pulling in different directions on
purpose, and that's fine given Scufris optimizes for one user, one machine,
plain-file durability, not multi-user/multi-model breadth.
Sources: https://github.com/Mintplex-Labs/anything-llm

### Onyx, formerly Danswer (`onyx-dot-app/onyx`)

One-liner: open-source "AI platform" / enterprise search -- connectors to
many data sources (Slack, Confluence, Google Drive, etc.), RAG chat, agents.
Storage: Postgres + vector DB (Vespa), fully server-side; no plain-file
model at all, built for team/org data, not a personal vault.
CLI/API: REST API, connector framework is its main reusable idea -- a
declarative "connector" abstraction per source type (pull, normalize,
index) that's conceptually similar to what a library CLI's ingestion
adapters could look like per content type (article, video, PDF).
Activity: 31,731 stars, pushed 2026-08-23. Very active; GitHub API reports
license as `NOASSERTION` (monorepo mixes an MIT-licensed Community Edition
with a separately licensed Enterprise Edition path -- worth re-checking the
LICENSE file directly before assuming full MIT if ever vendoring code).
Reuse: the connector-per-source-type abstraction is a useful shape to
borrow conceptually for the library CLI's ingestion adapters (one adapter
per modality: web article, video, PDF), even though Onyx itself is far too
heavy (Postgres+Vespa+multi-service) to run for a single-user vault.
Learn: renamed from Danswer to Onyx during a broader repositioning toward
enterprise search/agents -- search results didn't surface a stated reason,
but the pattern (rename away from a "-answer" branded name toward a
neutral one while chasing enterprise) mirrors Zylon/PrivateGPT: consumer/
hobbyist-sounding open-source brand, enterprise-sounding commercial brand,
kept deliberately separate.
Sources: https://github.com/onyx-dot-app/onyx ,
https://techcrunch.com/2025/03/12/why-onyx-thinks-its-open-source-solution-will-win-enterprise-search/

---

## 5. Read-later / archival

### ArchiveBox (`ArchiveBox/ArchiveBox`)

One-liner: self-hosted web archiving -- takes URLs (or browser history,
bookmarks, Pocket/Pinboard exports) and saves HTML, PDF, screenshots, WARC,
media, git repos, per URL.
Storage: exactly the pattern the-den's library wants. One folder per
snapshot under `data/archive/<timestamp>/`, each containing the raw
extractor outputs (`.html`, `.pdf`, `warc/`, `media/`, `screenshot.png`,
etc.) plus a redundant `index.json` and `index.html` describing that
snapshot. A single `data/index.sqlite3` is a _rebuildable_ aggregate index
over all the folders -- delete it and `archivebox update` regenerates it
from the folders, which are the actual source of truth. Newer dev versions
reorganize by user/date/domain/UUID but keep the same folder-is-truth,
index-is-derived split.
CLI/API: `archivebox add/list/update/schedule` CLI is mature and scriptable;
REST API and admin UI on top of the same data.
Activity: 28,165 stars, pushed 2026-08-19. Very active, long-running (since
2017), one of the most stable projects in this survey.
Reuse: this is the single strongest direct precedent for the library CLI's
manifest design. Its snapshot-folder-plus-JSON-manifest-plus-derived-index
model answers the open question in NOTES.md almost exactly: one manifest
per captured item, content-addressable/timestamped folder for blobs, a
rebuildable SQLite (or in Scufris's case, `rg`-searchable manifests) index
on top. ArchiveBox's `index.json` schema (url, timestamp, title, tags,
extractor outputs list, history) is worth reading directly as a starting
schema before inventing one from scratch.
Learn (win): explicitly designs for its own obsolescence -- "stores data in
standard formats that remain readable for decades without requiring
ArchiveBox itself." That's the right target for the library: manifests and
blobs must stay useful even if the library CLI is rewritten or abandoned.
Sources: https://github.com/ArchiveBox/ArchiveBox ,
https://docs.archivebox.io/dev/README.html , https://deepwiki.com/ArchiveBox/ArchiveBox

### Omnivore (`omnivore-app/omnivore`) - shut down, the sharpest lesson in this survey

One-liner: was a full-featured open-source read-it-later app (clean reader
view, highlights, labels, newsletter capture, offline mobile apps) -- widely
considered the best Pocket alternative before it closed.
Storage: Postgres + object storage (self-hostable via docker-compose);
not a plain-file model.
CLI/API: GraphQL API, browser extensions, mobile apps; strong integration
surface while it lived.
Activity: shut down for good. Acquired (acquihired) by ElevenLabs,
announced 2024-10, service and all cloud data deleted 2024-11-15 with a
14-day export window. Repo remains on GitHub (~16,226 stars currently,
still gaining stars post-mortem) and is not archived, but has no
maintaining company; last push shown 2026-08-22 is likely dependency/CI
housekeeping from a community fork process, not product work. A community
fork ("omnivore.work") exists for self-hosting but cannot recover deleted
cloud accounts.
Reuse: nothing to run directly, but its data model (clean-reader
extraction + highlights + labels + newsletter-to-read-later) is a good
checklist for what the-den's library manifest should be able to represent
per web-article item: title, source URL, capture date, extracted clean
text, highlights, tags.
Learn (failure, the clearest one here): Omnivore did not fail on product or
adoption -- it was well-loved and actively used. It failed because it was
venture-funded with no independent revenue model, so when a better exit
(acquihire) appeared, the team took it and the _hosted service_ died
overnight, taking self-hosters' comfort in "we'll always be able to
self-host" along with it for anyone who hadn't set up their own instance
already. The org's data-deletion timeline (14 days) is a stark reminder
that "open source" alone does not protect a hosted user's data if they
never ran their own copy. Directly validates Scufris's local-first,
no-hosted-service, plain-file-canonical stance: the library must never
depend on Scufris (or any vendor) staying in business, and manifests+blobs
living in a private git repo the user controls is the correct answer, not
an incidental one.
Sources: https://www.creativerly.com/the-exit-us-of-omnivore-from-open-source-to-ai-vc-money/ ,
https://molodtsov.me/2024/10/omnivore-is-dead-where-to-go-next/ ,
https://gleamr.io/blog/omnivore-shut-down-alternatives

### Wallabag (`wallabag/wallabag`)

One-liner: long-running self-hostable read-it-later app (PHP/Symfony),
the "boring, mature" alternative that predates and outlived Omnivore.
Storage: MySQL/Postgres/SQLite backend; articles stored as extracted
HTML/text in the DB, not plain files.
CLI/API: REST API (OAuth2), official CLI client (`wallabag-cli`) for
scripted add/list/tag; browser extensions, mobile apps, Kobo/Kindle export.
Activity: 12,928 stars, pushed 2026-08-21. Steady, unglamorous, very long
track record (since 2013).
Reuse: not directly (DB-backed, heavier deployment than the-den wants), but
its longevity is itself the data point -- see below.
Learn (win by boringness): Wallabag is the project that both Omnivore's and
Karakeep's own postmortem blog posts point people toward first, precisely
because it never chased VC money, never had a flashy AI pivot, and is still
exactly what it was in 2013. For a personal tool meant to outlive its
author's enthusiasm cycles, "boring and self-funded" beats "exciting and
funded" on the only metric that matters here: still running in ten years.
Sources: https://github.com/wallabag/wallabag ,
https://www.readless.app/blog/omnivore-alternatives-2026

### Karakeep, formerly Hoarder (`karakeep-app/karakeep`)

One-liner: self-hosted "bookmark everything" app (links, notes, images)
with AI auto-tagging and full-text/semantic search; the project the
community converged on as Omnivore's actual successor.
Storage: SQLite (not Postgres) for metadata plus a plain asset directory
organized by user-id/asset-id -- lighter-weight than Omnivore's or
Wallabag's stack, closer to a personal-scale footprint.
CLI/API: three official access surfaces -- a CLI (built on the same tRPC
procedures as the web app), a TypeScript SDK generated from an OpenAPI
spec, and a native MCP server. This is the most complete "assistant-ready"
tool contract of any project in this survey: an agent can drive it exactly
as easily as a human web-UI user, through a protocol built for the purpose.
Activity: 28,562 stars, pushed 2026-08-22. Very active, fast-growing since
the Omnivore shutdown pulled in refugees.
Reuse: the CLI+SDK+MCP-server triad is the concrete template for what the
library CLI should eventually offer once it has enough surface: a stable
core API, a thin CLI over it, and an MCP server over the same procedures so
Scufris (as an MCP-capable Pi tool caller) gets the contract for free
instead of Scufris needing bespoke wrapper tools. Worth reading its MCP
server source directly when the library CLI's read surface (list, show,
search, resolve) is being designed.
Learn (naming lesson, small but real): Hoarder renamed to Karakeep in early
2025 specifically over a trademark conflict -- same repo, same
community, pure rebrand cost (docs, container names, icon packs, third-party
integration configs all had to churn). Cheap, avoidable lesson: check
trademark availability before a name sticks in READMEs, package registries,
and other projects' integration code.
Sources: https://github.com/karakeep-app/karakeep ,
https://deepwiki.com/karakeep-app/karakeep/4.4-cli-sdk-and-mcp-server ,
https://jameskilby.co.uk/2025/01/how-i-migrated-from-pocket-to-hoarder-and-introduced-some-ai-along-the-way/

---

## 6. Personal dashboards / widget systems

Framing: all three below are _browser-tab_ dashboards (one page, many
tiles/feeds), fundamentally different from dashboardd's model of
independent native windows (Tauri/SPA-over-SSE) that i3 places and manages
individually. They're still useful as config-format and
integration-breadth precedent.

### Glance (`glanceapp/glance`)

One-liner: single static Go binary, single YAML config, widget-first
feed/status dashboard (RSS, weather, stocks, server stats, bookmarks) --
"an RSS reader crossed with a status board."
Storage: one YAML file is the entire configuration; no DB, no runtime
state beyond that file plus whatever it polls live.
CLI/API: no real API -- config-file-driven only, restart to apply. That's a
deliberate simplicity trade, not an oversight.
Activity: 36,555 stars, pushed 2026-08-21. Currently the most-starred
project in this whole category, explicitly prized for being "the lightest"
option (runs on a Pi Zero 2W).
Reuse: nothing code-level useful to dashboardd (different problem: single
page vs many native windows), but its "one YAML file, zero database, single
binary" posture is a good sanity check on config-format minimalism if
dashboardd's own widget-catalog/config story ever needs a comparison point.
Learn: the market strongly rewards minimal-footprint, git-trackable,
single-file config over "smart" auto-discovering dashboards (see Homarr
below) -- directly consistent with dashboardd's existing typed-widget,
explicit-catalog approach rather than an auto-discovery model.

### Homepage (`gethomepage/homepage`)

One-liner: YAML-configured homepage/startpage with deep service-integration
API widgets (pulls live data from ~100 self-hosted services: \*arr apps,
Docker, Proxmox, Pi-hole, etc.).
Storage: YAML config files (services.yaml, widgets.yaml, settings.yaml);
config is git-trackable, matches the-den's plain-file philosophy at the
config layer.
CLI/API: no control API of its own; it's a read-only aggregation surface
over other services' APIs.
Activity: 32,196 stars, pushed 2026-08-23. Very active; "no real competitor"
on integration depth per community comparisons.
Reuse: nothing to run, but its per-service "widget" plugin format (a small
adapter that knows how to poll one external API and render one card) is
conceptually close to how dashboardd widgets already work (typed
inputs/outputs per widget) -- confirms that's the right granularity, no new
idea to import.
Learn: none critical; a healthy, mature project. Notable only as the
"maximizes integration breadth" end of a spectrum that Glance anchors at
the "maximizes simplicity" end -- dashboardd already sits where it should
(typed widgets over a specific curated set, native windows instead of one
page), no repositioning suggested by this comparison.

### Homarr (`homarr-labs/homarr`)

One-liner: dashboard configured entirely through a drag-and-drop web UI
(no YAML authoring) with ~40+ live service integrations.
Storage: moved from YAML to a database-backed model in its 1.0 rewrite --
config now lives in the app's own DB rather than a plain file.
CLI/API: web UI only; no config-as-code path anymore post-rewrite.
Activity: 4,595 stars, pushed 2026-08-23. Active, smallest star count of
the three dashboard projects surveyed.
Reuse: none.
Learn (small cautionary note): Homarr's move from YAML-in-git to
DB-backed-only config is called out repeatedly in community comparisons as
a regression for "config I can version and inspect," and it correlates
with a smaller, less enthusiastic following than Glance/Homepage despite
comparable feature depth. Reinforces: for a tool aimed at people who value
inspectable, diffable state, moving canonical config into a database is a
retention risk, not just a philosophical purity issue -- direct support for
NOTES.md's ban on database-as-canonical-store.
Sources: https://github.com/glanceapp/glance , https://github.com/gethomepage/homepage ,
https://github.com/homarr-labs/homarr ,
https://digitalbiztalk.com/article/why-i-switched-from-homarr-to-glance-for-my-dashboard ,
https://homelabcompass.com/alternatives/self-hosted-dashboard

---

## 7. Agent-tool contracts (MCP servers)

### Official filesystem server (`modelcontextprotocol/servers`, `src/filesystem`)

One-liner: the reference implementation for exposing a scoped filesystem to
an LLM agent as tools.
Contract: 14 tools split cleanly into read-only (`read_text_file`,
`read_multiple_files`, `list_directory`, `directory_tree`, `search_files`,
`get_file_info`, `list_allowed_directories`, ...) and write-capable
(`create_directory`, `write_file`, `edit_file`, `move_file`), each taking a
`path` (or `paths` array). Every write tool supports a `dryRun` preview
mode. Access is scoped by an explicit allowlist of directories passed at
startup, with the MCP Roots protocol allowed to narrow/replace that
allowlist per client session; `list_allowed_directories` exists purely so
the model (and a human debugging it) can introspect its own scope. Tools
are annotated `destructiveHint`/idempotency so a client can treat writes
differently from reads without parsing tool names.
Activity: part of `modelcontextprotocol/servers`, 89,806 stars, pushed
2026-08-20. This is the canonical reference every other MCP filesystem
server imitates.
Reuse: directly relevant as a contract template for any future Scufris
native tool that touches the-den or the library on disk. The
allowlist-plus-dryRun-plus-introspection pattern (explicit scope, preview
before destructive action, a tool to ask "what am I allowed to touch") is
worth adopting verbatim for library-write tools, even though Scufris's
current today-wrapping tools go through `today --json` rather than raw
filesystem access.
Learn (win): the split between read-only and write-capable tools, each
separately annotated, is exactly the shape Scufris should keep as its
native-tool surface grows past `today` into the library -- cheap to audit,
cheap for a human to reason about what an agent could do wrong.
Sources: https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem

### Official memory server (`modelcontextprotocol/servers`, `src/memory`)

One-liner: reference "persistent memory" MCP server -- a local knowledge
graph of entities, relations, and observations.
Contract: storage is a JSONL file (one entity/relation per line, trivially
diffable and git-trackable). Tools: `create_entities`, `create_relations`,
`add_observations`, `delete_entities`/`delete_observations`/
`delete_relations`, `read_graph`, `search_nodes`, `open_nodes`. Also
exposes a `memory://knowledge-graph` MCP resource with live update
notifications for subscribed clients.
Reuse: the entity/relation/observation shape and the JSONL storage choice
are a reasonable, lightweight reference if Scufris ever wants a durable
cross-session memory store distinct from the-den's daily notes and the
library's manifests -- notably it is _not_ a vector index, it's a plain
structured-text graph, which fits the local-first/plain-file bar better
than most "AI memory" products.
Learn: a second confirmation (after ArchiveBox and org-ai) that the
projects most aligned with Scufris's constraints default to
line-oriented, git-friendly plain text as the storage format, and treat
anything richer (embeddings, DBs) as optional and derived.
Sources: https://github.com/modelcontextprotocol/servers/tree/main/src/memory

### Community Obsidian MCP servers (`MarkusPfundstein/mcp-obsidian` and ~9 forks/variants)

One-liner: the dominant pattern for exposing a plain-file Markdown vault to
an LLM agent is a thin MCP server that either shells out to Obsidian's
"Local REST API" community plugin, or reads the vault files directly.
Landscape: `MarkusPfundstein/mcp-obsidian` (4,326 stars, the de facto
standard, wraps the Local REST API plugin) versus smaller direct-filesystem
variants like `Piotr1215/mcp-obsidian` (23 stars, "simple mcp server for
interacting with local obsidian notes") and semantic-search variants like
`tcsavage/mcp-obsidian-index`. All expose broadly the same verbs: list
notes, read note, search, create/update note, list tags/backlinks.
Reuse: confirms the shape a hypothetical "the-den MCP surface" would take
if Scufris's native tools were ever exposed as MCP instead of Pi-native
tools -- but also confirms Scufris's actual choice (native Pi tools wrapping
`today --json` directly, no filesystem-level MCP indirection, no dependency
on a REST-API plugin analog) is the tighter, more deterministic contract
for a single-owner CLI-per-domain model. The proliferation of
near-duplicate community servers for the same vault is itself informative.
Learn: when there is no single blessed contract owner, the ecosystem
fragments into a dozen slightly-different reimplementations of "list/read/
search/write a note." `today` already avoids this fate by being the one
owning contract for daily notes; the same discipline (one canonical CLI per
structured area, everything else reads through it) should extend to the
library CLI so it does not end up needing "13 competing mcp-library
servers" down the line.
Sources: gh search results for `mcp obsidian`, https://github.com/MarkusPfundstein/mcp-obsidian

### Community Google Calendar MCP servers (`markelaugust74/mcp-google-calendar` and ~9 variants)

One-liner: same fragmentation pattern as Obsidian, applied to calendar:
a dozen small, mostly-abandoned or barely-maintained MCP wrappers around
the Google Calendar API (create/list/update events, check availability),
none with meaningful adoption (largest is 33 stars).
Reuse: not directly reusable (all thin, low-quality, Google-API-specific,
and NOTES.md already rejects a custom calendar CLI in favor of
`.ics`/`khal`/`vdirsyncer`). Worth noting only that no serious MCP
calendar contract exists yet for the `.ics`-plus-`khal` local model
Scufris is aimed at -- if calendar support is ever added as a native tool,
it will likely need to be written fresh (wrapping `khal`'s own CLI/JSON
output), since nothing in this space targets a local `.ics` vault.
Learn: low-star, high-count clusters like this are a signal of unmet
demand without a clear technical center of gravity yet -- nobody has done
for local calendar tooling what `today` already did for daily notes. No
existing project to adopt; this is a gap, not a lesson.
Sources: gh search results for `mcp google calendar`

---

## Reusable now

Concrete tools/patterns worth adopting into Scufris/the-den/library work,
ordered roughly by how directly applicable they are:

1. **ArchiveBox's snapshot-folder + manifest + rebuildable-index model**
   (`data/archive/<id>/` holding raw outputs and an `index.json`, with a
   derived aggregate index on top) is the closest existing answer to the
   library's open manifest-format question. Read its `index.json` schema
   directly as a starting point before inventing one.
2. **The official MCP filesystem server's contract shape**: read-only vs
   write-capable tools cleanly separated, `dryRun` on writes, an explicit
   allowlist, and a self-introspection tool (`list_allowed_directories`).
   Apply this shape to any future library-write native tools.
3. **The official MCP memory server's JSONL entity/relation/observation
   format** as a lightweight, git-friendly reference if Scufris ever needs
   structured cross-session memory distinct from daily notes.
4. **Karakeep's CLI + SDK + MCP-server triad built on one shared procedure
   layer** as the template for the library CLI's eventual read surface
   (list, show, search, resolve) -- one core API, thin CLI on top, MCP
   server as a third view of the same procedures, not a separate
   reimplementation.
5. **OpenVoiceOS's plugin-manager pattern** for swappable STT/TTS/wake-word
   engines behind one interface, if Whisper/Piper ever need a hot-swap
   story.
6. **Wyoming protocol's minimalism** (tiny newline-JSON-plus-binary wire
   format, one verb per message) as a style reference for any future
   out-of-process Scufris tool contract, alongside dashboardd's own
   Unix-socket JSON model which already follows the same instinct.

## Mistakes to avoid

Recurring failure modes seen across this survey, mapped to what they mean
for Scufris:

1. **Hosted-service dependency kills self-hosters too.** Omnivore was
   well-loved and still vanished in 15 days because its team took an
   acquihire and canonical data lived in a company's Postgres, not in each
   user's own files. Never let Scufris or the library depend on any
   vendor's continued existence; canonical data stays in the user's own
   git repo, always.
2. **Config/state moving from plain files into a database is a quiet
   regression users notice.** Homarr's YAML-to-DB rewrite is cited
   repeatedly as a reason people left for Glance/Homepage. Reinforces
   NOTES.md's ban on a database as canonical store -- the market
   independently arrived at the same preference.
3. **A good storage model isn't enough without either a CLI/API contract or
   riding on a bigger host ecosystem.** Reor had local-first plain files
   plus a local vector index -- a genuinely good design -- and still died,
   because it was a closed GUI app with no scriptable surface and no host
   ecosystem propping it up. Every domain tool in the today/library mold
   needs a real CLI contract from day one, not just good file formats.
   Alternatively (Smart Connections model): ride on an existing host
   ecosystem instead of rebuilding one.
4. **Chasing generality dilutes a working niche.** Open Interpreter's drift
   from "general Jarvis" to "one more coding agent" happened because the
   general-assistant positioning couldn't compete once every serious coding
   agent existed. Scufris's anchor to owned, narrow contracts (today,
   later the library) is what should keep it from the same drift -- resist
   expanding into general computer-control before the owned contracts are
   solid.
5. **Physical hardware multiplies risk before the software is proven.** The
   01 device's discontinuation and refunded pre-orders is the sharpest
   version of this. Any future "give Scufris a body/speaker" work should
   stay software-prototype-first, hardware last and small.
6. **Fragmentation without a blessed contract owner wastes effort.** The
   MCP Obsidian and Google Calendar server landscapes are each a dozen
   near-duplicate low-adoption reimplementations of the same handful of
   verbs, because no single canonical tool claimed the contract the way
   `today` claims daily notes. Keep extending that discipline: one owning
   CLI per structured area in the-den, so nobody (including future-Scufris)
   needs to reinvent "list/read/search/write" for the same data twice.
7. **External non-technical shocks can end even excellent projects.**
   Mycroft's patent-troll lawsuit had nothing to do with code quality. Low
   relevance at hobby scale, but the mitigation that worked -- a fast
   community fork (OpenVoiceOS) because the code and its contract were
   already decoupled from any one company or server -- is a generally good
   property to keep: nothing in the Scufris stack should require a company,
   account, or hosted backend to keep functioning.
8. **A cheap, avoidable cost: not checking trademark/name availability
   early.** Hoarder's rename to Karakeep was pure overhead (docs, package
   names, icons, third-party integrations all had to update) for a problem
   a five-minute search could have caught before the name spread.
