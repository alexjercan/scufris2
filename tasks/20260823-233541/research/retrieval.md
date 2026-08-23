# Retrieval and indexing for the-den's library

Scope: local retrieval over a personal corpus that starts at zero and grows
to, realistically, hundreds to low thousands of items over years (the-den's
`Daily/` currently holds 1,140 files after ~3 years; a research library will
add slower than that). Canonical data is plain files: tracked manifests with
content hashes, git-ignored media blobs, transcripts, extracted text. The
index is a derived cache. Deleting it must never lose data. This file judges
options against that corpus size and that constraint, not against enterprise
search benchmarks.

## 1. Baseline: ripgrep/fd over manifests and extracted text

For hundreds to low thousands of small-to-medium text files, ripgrep is not
a "toy" baseline, it is close to the actual ceiling of what search needs to
be for one person. Ripgrep has no practical limit on file or directory count
for a corpus this size, uses a parallel recursive walk, and respects
`.gitignore` semantics natively, which lines up exactly with "tracked
manifests, ignored blobs, extracted text sitting next to both" [ripgrep.org](https://ripgrep.org/).
A `rg -t md` or `rg --json` sweep over a few thousand files completes in
tens of milliseconds on spinning rust, single-digit milliseconds on an SSD.
There is no index to build, corrupt, or rebuild: the "index" is the
filesystem itself, which is exactly the derived-and-rebuildable property the
project already committed to, taken to its logical extreme (rebuild cost is
zero because there is nothing to rebuild).

Where ripgrep actually runs out:

- **Relevance ranking.** `rg` returns every line that matches; it has no
  notion of "this document is more about X than that one." For a handful of
  results this doesn't matter (you read them all); past a couple dozen hits
  it does.
- **Fuzzy/typo tolerance and stemming.** "photograph" will not match
  "photographs" as a boosted variant, "recieve" will not match "receive." A
  personal corpus with synonyms, technical jargon, and imprecise recall of
  phrasing hits this constantly in voice-driven use where Scufris is
  reformulating the query, not the user typing it exactly.
- **Structured filtering.** Manifests carry fields (topic, modality, capture
  date, license). `rg` can grep those fields as text but cannot express
  "topic:nova-protocol AND modality:video AND after:2026-01" as a query; that
  needs either shelling out to `jq`/`yq` in a pipeline (workable, but now
  you are writing a query language) or a real index.
- **Multi-field / cross-document scoring.** No BM25-style term-frequency
  weighting, no combining a manifest hit with a transcript-body hit into one
  ranked result.

Concrete judgment: `rg` over manifests plus `fd` for filename/path discovery
is not a placeholder to be embarrassed about, it is the correct v1. It
covers "find me the thing with this word in it" and "list items under this
topic" (if topics live in predictably-shaped manifest fields), which is most
of what a library at hundreds of items actually needs. It stops being enough
the moment Scufris needs to rank candidates before citing one, or the user
starts asking meaning-based questions ("that video about compressing normal
maps" without recalling the word "compress").

`ripgrep-all` (`rga`) is worth flagging separately: it layers rg's engine
over PDF text extraction, archives, and other binary formats via adapters,
which matters once the library holds fetched PDFs alongside Markdown
[ripgrep-all issue tracker context](https://github.com/phiresky/ripgrep-all/issues/56).
For this project extracted text already lives in tracked files per the
manifest design, so `rga`'s adapter layer is a nice-to-have, not load
bearing — the extraction happens once at capture time, not at query time.

## 2. Lexical indexes

### SQLite FTS5

FTS5 is a virtual table module built into SQLite; nixpkgs' `sqlite` builds
with FTS5 enabled by default, so there is no extra package, just a schema.
Benchmarks against naive `LIKE` scans show FTS5 delivering roughly 16-30x
speedups (16x on 7,677 documents in a 3.5MB database, ~30x in a separate
demo) [TheLinuxCode](https://thelinuxcode.com/sqlite-full-text-search-fts5-in-practice-fast-search-ranking-and-real-world-patterns/) [Ppang0405/sqlite_fts5_demo](https://github.com/Ppang0405/sqlite_fts5_demo).
At the scale this project cares about (low thousands of documents), FTS5
queries return in single-digit milliseconds regardless; the speedup number
matters more as a signal that FTS5 was designed for exactly this shape of
problem than as a number to hit.

The operationally important detail is **external content tables**: FTS5
can index text that lives in a separate table (or is synthesized from files
on read) rather than duplicating the corpus inside the FTS index, which
keeps the derived index small and keeps the plain files authoritative
[sqlite.org fts5.html](https://www.sqlite.org/fts5.html). The tradeoff is
that FTS5 never writes to the content table and the caller is responsible
for keeping the index in sync — normally via `AFTER INSERT/UPDATE/DELETE`
triggers on the content table, or, for this project's shape (content lives
in files, not in SQLite rows), via an explicit reindex step in the library
CLI's write path rather than triggers at all. That reindex step is cheap:
`INSERT INTO fts(rowid, body) VALUES (...)` per changed manifest, no full
rebuild required.

Verdict: FTS5 is the natural second step after `rg`. It is not a daemon, it
is a library linked into whatever process runs the query (the library CLI
itself), it ships in nixpkgs' default sqlite build, and the index file is
just another derived artifact that can be deleted and rebuilt from
manifests. It adds ranking (BM25 is built in), phrase/prefix/NEAR queries,
and column-scoped search (search only titles, only transcripts) that `rg`
cannot express cleanly. This is the first place where "index" stops meaning
"the filesystem" and starts meaning an actual data structure — but it is
still embedded, not a service, and rebuild is `rm index.db && reindex`.

### tantivy / tantivy-cli

Tantivy is a Lucene-inspired full-text library written in Rust, not a
turnkey server — "a crate to build a search engine with," closer to Lucene
than to Elasticsearch [fulmicoton.com](https://fulmicoton.com/posts/behold-tantivy/) [docs.rs/tantivy](https://docs.rs/tantivy/).
The historical `tantivy-cli` wrapped it into an actual command-line indexer
usable without writing Rust, but verified against nixpkgs directly (`nix
search nixpkgs`), **there is no `tantivy-cli` package in nixpkgs** as of
this research — only `python313Packages.tantivy` (Python bindings to the
library) and `lnx`, a REST-server deployment of tantivy that is packaged
(`legacyPackages.x86_64-linux.lnx`, "ultra-fast, adaptable deployment of the
tantivy search engine via REST"). Using bare tantivy on NixOS therefore
means one of: packaging `tantivy-cli` from crates.io yourself (buildable,
low effort, but a from-scratch derivation to own), using the Python
bindings inside a small script, or running `lnx` as a daemon (see below,
same cost profile as meilisearch/typesense).

Judgment: tantivy is the right engine when the library CLI is willing to be
a Rust program embedding a real search library directly, matching the
"small deterministic helper" ethos more than SQLite FTS5 does not — because
SQLite FTS5 already gets you 90% of tantivy's benefit for a personal corpus
with an index that's already in nixpkgs and zero extra packaging. Tantivy
only pulls ahead at result-set sizes and query-complexity levels (faceting,
custom scoring, very large corpora) this project will not reach. Skip it
unless the library CLI is rewritten in Rust for other reasons; FTS5 covers
the lexical-index need at this scale with less packaging surface.

### meilisearch / typesense (daemon cost)

Both are real, packaged nixpkgs options: `meilisearch` (1.50.0) and
`typesense` (29.0) both resolved directly via `nix search nixpkgs`, so
NixOS packaging is a non-issue for either. Both are also genuinely
pleasant to use — typo tolerance, faceting, instant-search-grade latency
out of the box, no query language to learn.

The cost that matters here is the daemon, not the software quality.
Meilisearch is LMDB-backed and memory-mapped; run standalone it will use as
much RAM as the OS lets it, though community reports put steady-state usage
around ~500MB after indexing a modest dataset, and there's an experimental
flag (`--experimental-reduce-indexing-memory-usage`) specifically for
memory-constrained hosts [meilisearch storage docs](https://www.meilisearch.com/docs/learn/engine/storage) [meilisearch memory-leak postmortem](https://www.meilisearch.com/blog/memory-leak-investigation).
Typesense has a comparable footprint profile (in-memory index, RocksDB
persistence). Both want a systemd unit, a port, a data directory outside
the-den, and independent lifecycle from the library CLI — i.e. a service to
keep alive, monitor, and restart on a personal desktop that otherwise runs
nothing server-shaped for this use case.

For hundreds to low thousands of documents, a resident several-hundred-MB
daemon buys typo tolerance and instant-as-you-type latency that SQLite
FTS5 does not natively give you, in exchange for a process that must be
running before Scufris can query it, a second thing that can crash or drift
from the manifests, and a NixOS module to write and maintain. That is a bad
trade at this corpus size: FTS5's query latency is already invisible to a
voice interaction (tens of milliseconds either way), so the daemon's only
real edge — typo tolerance during interactive typing — doesn't apply to a
tool that receives already-transcribed, already-reformulated natural
language queries from an LLM, not raw keystrokes. Reserve
meilisearch/typesense for a future where the library is browsed through a
web/typing UI directly (not through Scufris) and instant-search UX actually
matters.

## 3. Local embeddings

### Inference: llama.cpp, ollama, fastembed

All three are viable and all three are NixOS-packagable, verified directly:
`llama-cpp` (10121, plus `-vulkan`/`-rocm` variants) and `ollama` (0.32.3,
plus GPU-accelerated variants) both resolve in nixpkgs; `fastembed` resolves
as `python313Packages.fastembed` (0.8.0).

- **llama.cpp** (`llama-server --embeddings`) exposes an OpenAI-compatible
  `/v1/embeddings` endpoint and runs GGUF embedding models such as
  `nomic-embed-text-v1.5` — the same binary family that already backs
  Whisper STT in nix.dotfiles via `whisper-cpp-vulkan`, so this is the
  option with the least new operational surface: one more llama.cpp process
  (or one more model loaded by the existing one) rather than a new runtime
  [llama.cpp embeddings discussion](https://github.com/ggml-org/llama.cpp/discussions/7712) [nomic-embed-text-v1.5-GGUF](https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF).
  Nomic's model needs task-instruction prefixes (`search_query:` /
  `search_document:`) baked into the client code, a small but real
  integration detail.
- **ollama** wraps the same llama.cpp core behind a friendlier model-pull
  UX and its own daemon; for CPU-only machines, `nomic-embed-text` (274MB)
  is the practical default and `qwen3-embedding:0.6b` (639MB) the strongest
  sub-1GB alternative [morphllm ollama embedding models 2026](https://www.morphllm.com/ollama-embedding-models).
  Functionally near-identical to llama.cpp for this purpose; the main
  reason to prefer it is convenience of model management, the main reason
  to prefer llama.cpp is not running a second inference daemon alongside
  the Whisper one already pinned in nix.dotfiles.
- **fastembed** (Qdrant's library) runs ONNX-quantized embedding models
  in-process from Python with no server at all — call a function, get
  vectors back, memory-efficient generator API for batch embedding a
  library's worth of manifests. For a library CLI that already leans
  Python/stdlib in the `today` mold, this is the lowest-ceremony inference
  option: no daemon, no port, embed-at-index-time as a subprocess step, and
  it disappears from the process list when done.

Judgment: fastembed for the indexing/reindexing path (invoked by the
library CLI's reindex command, batch job, no daemon) and, if query-time
embedding needs to be fast and is already warm, llama.cpp's server (since
it is already precedented via Whisper) rather than adding ollama as a third
model-runtime alongside llama.cpp and Piper. Avoid running three different
local-inference daemons (whisper-cpp, ollama, piper) when llama.cpp already
covers two of the shapes needed.

### Vector stores: sqlite-vec, LanceDB, chroma

All three resolve in nixpkgs, verified directly: `sqlite-vec` (0.1.9, a
top-level package — the loadable SQLite extension itself), and
`python313Packages.lancedb` (0.32.0) and `python313Packages.chromadb`
(1.5.9) as Python libraries.

- **sqlite-vec** is a loadable SQLite extension providing a `vec0` virtual
  table for vector search inside an ordinary SQLite file — no separate
  process, vectors live next to (or in the same file as) the FTS5 index,
  backup is "copy the file." A direct comparison against ChromaDB and
  Pinecone for personal RAG concludes explicitly that for a single-developer
  project with a corpus of a few thousand to tens of thousands of chunks,
  sqlite-vec is the right choice, framing the whole deployment story as
  "build it locally, scp the file, bounce the API," and stating the design
  principle plainly: "the part of a system you most want to be boring is
  the part that holds your data" [prommer.net sqlite-vec vs ChromaDB vs Pinecone](https://prommer.net/en/tech/sqlite-vec-vs-chromadb-pinecone/).
  That is close to a direct match for this project's constraints.
- **LanceDB** is an embedded, disk-backed columnar store (the Lance
  format) built for larger-than-memory, multimodal data, with native Rust,
  Python, and TypeScript SDKs — a legitimate embedded alternative to
  sqlite-vec, but it introduces a second storage format and file layout
  (Lance datasets) alongside SQLite/FTS5, which is unjustified complexity
  at this corpus size. Its edge (disk-efficient, larger-than-memory,
  strong multimodal handling) is a scale property this project won't hit.
- **Chroma** is the "accept a resident process" option: purpose-built
  collections and metadata filtering, genuinely good developer ergonomics,
  but it wants a running process (in-memory index backed by persistent
  storage) — the same daemon-cost argument as meilisearch/typesense above,
  now for vectors instead of text.

Judgment: sqlite-vec, in the same SQLite file as FTS5 (or a sibling file),
is the correct embedded default if/when embeddings are justified. It keeps
the "derived index = a file you can `rm`" property, avoids a second daemon,
and is already in nixpkgs with no packaging work.

### Model choice for English personal notes + technical content

`nomic-embed-text-v1.5` (via llama.cpp/ollama/fastembed, all can load
compatible weights) is the reasonable default: strong general-English MTEB
performance, small enough to run CPU-only, explicit prefix conventions for
query vs. document embedding that map cleanly onto "embed the query" vs.
"embed the manifest/transcript chunk at index time." For technical/code-ish
content specifically, `nomic-embed-code` exists as a heavier alternative if
the library starts accumulating code snippets or API docs, but that's a
later, concrete-need decision, not a v1 default.

## 4. Hybrid lexical + semantic: when embeddings actually beat FTS

The honest framing from current practice: BM25/FTS is precise on exact
terms and fails on synonyms/paraphrase; embeddings are the reverse — they
generalize over meaning but blur precision, and can surface topically
related but wrong results when the user actually wanted an exact match
[Zilliz semantic vs lexical vs full-text](https://zilliz.com/blog/semantic-search-vs-lexical-search-vs-full-text-search) [Weaviate hybrid search explained](https://weaviate.io/blog/hybrid-search-explained).
Hybrid search (BM25 + dense retrieval fused, typically via Reciprocal Rank
Fusion, optionally cross-encoder reranked) is the production answer when an
application must serve arbitrary users with arbitrary vocabulary against a
corpus the system doesn't get to assume familiarity with. A one-person
library assistant is a different regime: the person asking the question
often *is* the person who wrote or captured the material, so their
vocabulary drift from the original text is bounded, not adversarial.

The clearest counter-signal to "just add embeddings" comes from the
grep-vs-RAG debate in coding-agent contexts, which generalizes directly to
this project's plain-text, git-tracked, small-corpus shape: for small,
plain-text corpora (a docs folder, a handful of markdown notes), lexical
search is fast, predictable, and gives exactly the precision needed, with
zero infrastructure to maintain and zero risk of returning a stale or
renamed match that a poorly-maintained embedding index would still surface
confidently [LlamaIndex: is grep all you need](https://www.llamaindex.ai/blog/is-grep-all-you-need-lexical-vs-sematic-search-for-agents) [Creative Tinkering: agentic, semantic, or both](https://wowelec.wordpress.com/2026/05/18/agentic-semantic-or-both-notes-from-the-code-search-debate/).
The same source is explicit that evaluation, not intuition, is what
justifies the added complexity of embeddings: without a query set and a
relevance baseline (NDCG/MRR-style, even informally — "did the top result
actually answer this question I actually asked") on your own corpus, there
is no way to tell whether an embedding layer improved anything at all
[digitalapplied hybrid search reference 2026](https://www.digitalapplied.com/blog/hybrid-search-bm25-vector-reranking-reference-2026).

Khoj is the closest thing to a real "personal knowledge assistant" case
study, but its current production architecture (per its own docs and
self-hosting instructions, verified) is **not** a lightweight
lexical-first-then-embeddings-later story: self-hosting Khoj requires
Postgres with the `pgvector` extension as a hard dependency (the
docker-compose ships `pgvector/pgvector:pg16`, or an embedded `pgserver`
for the pip install path), and its retrieval is bi-encoder embedding search
in pgvector with optional cross-encoder reranking, with configurable
backends (local sentence-transformers, HuggingFace endpoints, OpenAI)
[docs.khoj.dev self-host setup](https://docs.khoj.dev/get-started/setup/) [khoj-ai/khoj docker-compose.yml](https://github.com/khoj-ai/khoj/blob/master/docker-compose.yml).
That is a heavier stack than anything this project's rejected-directions
list allows (a database as canonical-adjacent infrastructure, a server
process, a second DB engine) and is aimed at a multi-user hosted product,
not a single person's derived index. The lesson to take from Khoj is
narrower than "embeddings win": it validates that semantic search over
personal notes is a real, wanted feature (that's the whole product), while
also demonstrating that doing it "properly" pulls in a database server —
which is exactly the operational weight sqlite-vec exists to avoid, and
exactly why this project should not copy Khoj's architecture even while
taking its feature (search-by-meaning) seriously as an eventual target.

Net judgment: hybrid retrieval earns its cost when (a) the corpus is large
or heterogeneous enough that exact-term recall genuinely misses things the
person remembers differently than they were captured, and (b) there's a
concrete failure mode observed in practice — not hypothesized. For a
library that starts at zero items, the correct sequencing is lexical only
until the corpus and the observed query failures justify semantic search,
then add it as a second signal (fused, not a replacement) rather than
rebuilding retrieval around it.

## 5. Operational concerns

**Incremental reindexing on file change.** Because manifests are the
canonical source of truth and already carry content hashes (per the
library's decided storage design), the natural reindex trigger is the
library CLI's own write path, not a filesystem watcher: every manifest
write (capture, edit, delete) is a known event the CLI controls, so it can
update FTS5/sqlite-vec rows synchronously as part of that same write,
using external-content-table semantics — the manifest stays the row of
record, the FTS/vector rows are derived and keyed by content hash + rowid
[sqlite.org fts5.html](https://www.sqlite.org/fts5.html). This avoids
needing `inotify`/watcher machinery at all for the common case (Scufris/the
library CLI is always the writer). A watcher only becomes necessary if
manifests get hand-edited outside the CLI (e.g. in Neovim, matching the
project's "human-editable" principle) — in which case a cheap
stat-based reconciliation pass (size+mtime prefilter, content hash on the
remainder, matching the pattern used by incremental-index tools generally)
run on demand or on a timer is sufficient; no need for a live watcher
daemon at this corpus size [content-hash reconciliation pattern](https://gist.github.com/tuandinh0801/7a6c6e81ab41576e11dc4d41a6676602).

**Deletion/rename handling.** Because retrieval rows are keyed by content
hash (already the manifest's identity per the decided design) rather than
by file path, a rename is a no-op for the index (same hash, same row,
manifest's path field just changes) and a deletion is a straightforward
`DELETE FROM fts WHERE rowid = ...` triggered by the manifest's own
deletion. This is a direct benefit of choosing content-hash identity for
the library over path identity: git already tracks renames on the plain
files; the derived index doesn't need to detect renames at all, it needs
the CLI to tell it "this hash's manifest changed," which it already knows
because it just performed that write.

**Index rebuild cost.** At hundreds to low thousands of items, a full
rebuild (drop the SQLite file, re-run "for every manifest, insert into
FTS + embed into vec0") is a batch job measured in seconds to at most a
couple of minutes (embedding is the only slow step; FTS5 insertion of a
few thousand rows is sub-second). This is cheap enough that "just rebuild
from manifests" is a legitimate recovery path for any index corruption or
schema change, which is the whole point of "derived and rebuildable."

**Provenance (citing file + line/range back to source).** The general RAG
literature is explicit that most systems lose exact source location at
chunk time and can only cite "this document," not "this document, line 47"
[Tensorlake: citation-aware RAG](https://www.tensorlake.ai/blog/rag-citations) [Index-RAG: storing text location in vector databases](https://medium.com/@praneeth.v/index-rag-citation-first-approach-to-rag-0e948b9e12c1).
The fix generalizes cleanly to this project because the canonical data is
already plain files with stable structure: store byte or line offsets (and,
for FTS5 in particular, `snippet()`/`highlight()` output which already
returns match-anchored context) as part of what gets indexed, not as an
afterthought computed post-hoc from a vector's nearest-chunk. Concretely,
each row in the derived index should carry: manifest content hash, source
file path, and a line range (or timestamp range for transcripts, see
below) — enough for Scufris to both answer and open the exact widget/region
being cited, matching the project's "widgets are for showing, not the only
source of a fact" principle.

**Multimodal entries.** Transcripts and image captions are just text with
extra structural metadata, not a different retrieval problem. whisper.cpp
already supports word-level timestamps and a `--output-format json` mode
that emits per-segment (and with `--word-timestamps`, per-word) start/end
times [openai-whisper word-timestamps guide](https://openai-whisper.mintlify.app/guides/word-timestamps) [whisper.cpp word-timestamp JSON discussion](https://github.com/ggml-org/whisper.cpp/issues/701) —
which is exactly the "word:timestamp transcript" format the vision
document already commits to for video ingestion. Indexing a transcript
means indexing it at a chunk granularity (e.g. per-segment, a few sentences
per FTS row) with the segment's timestamp range stored alongside, so a
citation becomes "this manifest, timestamp 4:12-4:38" and the reference
widget can seek a locally-stored keyframe or open the transcript at that
offset. Image captions are simpler still: one short text field per image,
indexed like any other manifest field, with the image's local path as the
citation target instead of a line range.

## Staged recommendation

**v1 search (ships with the first library increment, no new daemons, no
embeddings):** `rg`/`fd` over tracked manifests and extracted text,
exactly as the vision document already proposes as the interim. This is
not a stopgap to feel bad about — for a library starting at zero and
growing to low hundreds over the first year, it is close to sufficient on
its own. Add a thin `library search <query>` subcommand to the library CLI
now, backed by `rg --json` under the hood, so Scufris and widgets get a
stable JSON contract from day one even though the implementation underneath
it can change without breaking callers. This satisfies "derived and
rebuildable" trivially — the filesystem *is* the index.

**v1.1, first real index (ship as soon as the `rg` implementation starts
feeling slow or unranked, likely low hundreds of items):** swap the
`library search` implementation to SQLite FTS5, same CLI contract. Index
file lives outside git (a `.cache`-style path, or alongside blobs), keyed
by manifest content hash, updated synchronously in the library CLI's write
path (capture, edit, delete), with an on-demand reconcile pass for
out-of-band manifest edits. No new NixOS packaging: nixpkgs' default
`sqlite` already builds FTS5 in. This is the lexical ceiling for a corpus
this size and should be treated as the default state for a long time, not
a waypoint to rush past.

**Measurable signal that justifies adding embeddings:** do not add
semantic search speculatively. Add it only after collecting a small,
real query log from actual use (Scufris already logs what it searched for
and what it returned, per the audit principle) and observing a recurring,
named failure mode: FTS5 returns nothing or returns the wrong item for a
query where the person can point at a specific manifest and say "that one,
I just didn't use its words." A rough trigger: three or more distinct,
real instances of that failure, or a standing frustration ("I know I saved
something about X but can't find it") repeated across sessions. This
mirrors the explicit lesson from current RAG practice that hybrid search
should be justified by an evaluation set on your own corpus, not by
default-adding a vector layer because it's available
[digitalapplied hybrid search reference 2026](https://www.digitalapplied.com/blog/hybrid-search-bm25-vector-reranking-reference-2026).

**Concrete stack when justified:** `sqlite-vec` as a loadable extension in
the same (or a sibling) SQLite file as FTS5 — verified present in nixpkgs,
zero packaging cost. Embed at index time with `fastembed`
(`python313Packages.fastembed`, verified in nixpkgs) run as a batch step
inside the library CLI's reindex command, no daemon; `nomic-embed-text-v1.5`
as the default model, loaded either via fastembed directly (ONNX, in
process) or via the already-precedented `llama-cpp` runtime if a resident
embedding server turns out to be worth it for query-time latency. Fuse
FTS5's BM25 ranking with `sqlite-vec`'s cosine/L2 distance via a simple
Reciprocal Rank Fusion in the library CLI's query path — no external
fusion library needed, RRF is a few lines of arithmetic over two ranked
lists. Explicitly do not adopt meilisearch, typesense, tantivy(-cli/lnx),
LanceDB, ChromaDB, or Khoj's Postgres/pgvector architecture: each solves a
scale or multi-tenant problem this project does not have, and each adds a
resident daemon or a second storage format where SQLite (FTS5 + sqlite-vec,
one file, `rm`-and-rebuild) already covers the need end to end.
