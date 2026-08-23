# Market sweep: non-GitHub sources on personal AI assistants and local-first knowledge systems

Scope: products, postmortems, essays, and community discussion outside GitHub.
Goal: lessons and stealable ideas for Scufris (Jarvis-style assistant, local
voice, plain-file vault, domain CLIs, native widget windows, future course
builder). Six themes swept via web search and fetch, ASCII punctuation
throughout, HN links included where useful.

## 1. Commercial products and their fates

**Rewind.ai -> Limitless pivot.** Rewind built a local, encrypted, always-on
screen/audio recorder with local search over your own history. It won
Product Hunt 2022 and raised at a $350m valuation, then pivoted in April 2024
to Limitless: cloud-based meeting transcription plus a hardware pendant.

- Lesson: the most common reason users churned was not privacy or features
  but raw performance cost. Rewind "turned MacBooks into a toaster" and drained
  battery; continuous local capture has a real, user-visible resource tax that
  has to be engineered down, not hand-waved away. An early adopter built a
  competing local diff-based capture engine ("TopSecret") that got 2.5x lower
  battery draw than Rewind through smarter screenshot diffing, proving the
  problem was solvable, not fundamental.
- Lesson: local-first, privacy-first positioning is not sufficient on its own.
  Rewind's AI features (summaries, "Ask Rewind") stayed "impressive prototype"
  quality and never crossed into reliable-enough-to-trust, so users churned
  before the privacy story could pay off.
- Lesson: the pivot swapped user-owned local storage for cloud upload to
  "third-party transcription partners," a straight privacy downgrade in
  exchange for quality. This is the classic tension: local-first is a
  discipline that must be actively defended against the pull of "the cloud
  version just works better."
- Idea worth stealing: continuous or frequent local capture needs an explicit
  performance budget from day one (CPU/battery ceiling, diffing instead of
  brute-force snapshotting), because that budget is what determines whether
  people keep the feature on.
- Sources: [TechCrunch on the pivot](https://techcrunch.com/2024/04/17/a16z-backed-rewind-pivots-to-build-ai-powered-pendant-to-record-your-conversations/), [An early adopter's thoughts on Rewind.ai's pivot](https://andrewschreiber.substack.com/p/an-early-adopters-thoughts-on-rewindais), [HN: rewind.ai pivot to cloud-only](https://news.ycombinator.com/item?id=40107443)

**Humane AI Pin.** $699 wearable plus $24/month, positioned as an "iPhone
killer" that did strictly less than a phone.

- Lesson: the core failures (heat, battery life of 2-4 hours, an outdoor-
  unreadable laser display) were physics problems, not software problems, and
  no firmware update could patch them away. Do not ship hardware whose failure
  mode is thermodynamics.
- Lesson: no app store, no third-party integrations, and a device that had to
  replace rather than extend an existing phone/workflow. It asked users to
  give something up (their phone) rather than adding to what already worked.
- Lesson: internal dissent was punished (an engineer was fired for flagging
  the product wasn't ready), which drove a talent drain right when the
  product needed the most scrutiny. Organizational signal: silencing internal
  "this isn't ready yet" feedback is a leading indicator of a launch failure.
- Outcome: shut down February 28, 2025; HP acquired the IP and most of the
  team for roughly $116m (an acquihire, not a product win).
- Sources: [What Went Wrong With the Humane AI Pin - Unite.AI](https://www.unite.ai/what-went-wrong-with-the-humane-ai-pin/), [Humane AI Pin postmortem - failure.museum](https://failure.museum/humane-ai-pin/), [Humane AI post-mortem](https://futurelabconsulting.com/index.php/blog/169-humane-ai-post-mortem)

**Rabbit R1.** $199 "Large Action Model" device demoed at CES 2024 doing
autonomous app actions; shipped unable to do most of what was demoed.

- Lesson: demo-driven launches create a promise gap that reviewers will
  immediately test and expose. Do not demo capability you have not shipped.
- Lesson: heavy dependence on a handful of third-party APIs (Spotify, Uber,
  DoorDash) made the device's usefulness hostage to partners' API stability,
  and analysis suggested most of the functionality could have run as a plain
  Android app -- the dedicated hardware added cost and fragility without a
  proportional benefit.
- Lesson: "even when AI systems are powerful, they are useless if they are
  not reliable" -- consistency beats raw capability for anything positioned
  as an assistant a person depends on.
- Sources: [The Rabbit R1 Failure: A Product Development Post-Mortem - Cubix](https://www.cubix.co/blog/the-rabbit-r1-failure/), [Engadget review](https://www.engadget.com/rabbit-r1-review-a-199-ai-toy-that-fails-at-almost-everything-161043050.html), [HN: Rabbit R1 and Humane Pin skepticism](https://news.ycombinator.com/item?id=40224041)

**Mycroft AI bankruptcy.** Open-source local voice assistant company, ceased
operations February 2023.

- Lesson: a patent-troll lawsuit (Voice Tech Corporation), even one that was
  eventually dropped, was enough to drain a small company's cash and staff
  down to two developers before the litigation even resolved. Legal risk is
  existential risk at small scale.
- Lesson: hardware partnerships were a persistent weak point -- reliance on
  generic off-the-shelf components caused compatibility and performance
  problems, worsened by COVID-era supply chain disruption. Voice hardware
  is a harder business than voice software.
- Lesson (positive): the open-source community and codebase outlived the
  company. OpenVoiceOS (OVOS) is a direct continuation, still active, still
  privacy-focused. For local-first tools, the license and community structure
  is itself a durability feature independent of the founding company's
  survival.
- Sources: [Patent troll kills Mycroft AI voice assistant](https://home-assistant-guide.com/news/2023/02/16/patent-troll-kills-mycroft-ai-voice-assistant/), [From Mycroft and Ansible to OpenVoiceOS](https://medium.com/@goldyfruit/from-mycroft-and-ansible-to-openvoiceos-making-open-voice-assistants-boring-066caae846d4), [Wikipedia: Mycroft (software)](https://en.wikipedia.org/wiki/Mycroft_(software))

**Omnivore shutdown.** Open-source read-it-later app, acquired by ElevenLabs
November 1 2024, shut down November 15 -- two weeks' notice to export data.

- Lesson: this was an acquihire, not a product acquisition -- "the service
  had no value to ElevenLabs, so it was shut down." The team was hired to
  work on ElevenReader (TTS), not to keep Omnivore running.
- Lesson: donation-only monetization for a niche, largely-free-riding
  user base ("most people were drawn to the app because it was free") is not
  a durable business model, however good the open-source ethics are. A
  personal tool you depend on that you don't operate yourself carries this
  risk permanently, regardless of how open its source is.
- Idea confirmed for Scufris: this is exactly the argument for capture into
  a local, self-owned vault rather than dependency on a hosted read-it-later
  service -- the shutdown notice window (two weeks) is the whole point of
  local-first: you should never be racing an export deadline for your own
  data.
- Sources: [The exit(us) of Omnivore](https://www.creativerly.com/the-exit-us-of-omnivore-from-open-source-to-ai-vc-money/), [TechCrunch: ElevenLabs hires Omnivore team](https://techcrunch.com/2024/10/29/elevenlabs-has-hired-the-team-behind-omnivore-a-reader-app/), [Omnivore is Dead: Where to Go Next](https://molodtsov.me/2024/10/omnivore-is-dead-where-to-go-next/)

**Evernote's decline.** Peaked as the default "second brain," collapsed from
9.6m yearly downloads (2017) to 1.7m (2023); acquired by Bending Spoons in
Nov 2023 to be stripped back down.

- Lesson: feature bloat (business-card scanning, a sock-and-backpack
  marketplace, presentation mode) diluted the core promise. "Going wide and
  shallow instead of narrow and deep" is named explicitly as the failure
  mode. Directly relevant to Scufris's "keep orchestration narrow" principle.
- Lesson: degraded free tier (device sync capped at two devices in 2016) plus
  rising premium price pushed users toward simpler, cheaper competitors
  (Apple Notes, Obsidian, Joplin) exactly when those alternatives matured.
- Lesson: performance/reliability decayed as the product grew in scope --
  the same pattern as Rewind: unmanaged resource cost erodes trust well
  before the business model does.
- Sources: [What went wrong with Evernote - MakeUseOf](https://www.makeuseof.com/what-happened-to-evernote/), [What went wrong with Evernote - Medium](https://medium.com/@vladcampos/what-went-wrong-with-evernote-how-did-we-get-here-dbe1d9303a65)

## 2. Voice assistants people actually keep using

**Home Assistant Voice / Assist (community practitioner experience).** A
detailed practitioner writeup ("My Journey to a reliable and enjoyable
locally hosted voice assistant") documents building a fully local pipeline:
llama.cpp on a Beelink mini-PC with an eGPU, HA Voice Preview Edition
satellites, Qwen3-ASR for STT, Kokoro/OmniVoice for TTS.

- Lesson: default quantized models (Ollama defaults) were the single biggest
  quality problem -- moving to higher-quantization GGUF builds from Hugging
  Face "immediately" improved reliability. Off-the-shelf model defaults
  undersell local voice; the ceiling is much higher than the out-of-box
  experience suggests.
- Lesson: the system prompt matters more than model choice for tool-call
  reliability -- structured per-service sections and explicit output examples
  produced a large jump in correctness. Prompt engineering is a first-class,
  ongoing maintenance task, not a one-time setup step.
- Lesson: pure LLM tool-calling was not reliable enough for some flows (music
  control, weather); the practitioner fell back to deterministic sentence-
  trigger automations layered under the LLM. This validates Scufris's
  "domain CLIs + deterministic helpers, LLM as router/narrator" design over
  LLM-does-everything.
- Lesson: context budget matters concretely -- limiting exposed entities to
  about 32 devices avoided token-budget-induced confusion. Keep the tool/
  entity surface small and curated, not exhaustive.
- Independent reviews note HA Voice Preview Edition, out of the box, still
  trails cloud assistants (Alexa) on wake-word accuracy and answer breadth;
  a comparison found cloud LLM passed 11/19 test prompts (58%) versus 7/19
  for local voice. The gap is closable (per the practitioner writeup above)
  but is not closed by default.
- Idea worth stealing: treat "expose fewer, better-curated tools/entities"
  and "deterministic automation as a fallback under the LLM" as load-bearing
  architecture decisions, not just performance optimizations -- they are also
  reliability and trust features.
- Sources: [My Journey to a reliable and enjoyable locally hosted voice assistant](https://community.home-assistant.io/t/my-journey-to-a-reliable-and-enjoyable-locally-hosted-voice-assistant/944860), [Set up a fully local voice assistant - Home Assistant docs](https://www.home-assistant.io/voice_control/voice_remote_local_assistant/), [Home Assistant Voice Preview Edition](https://www.home-assistant.io/voice-pe/)

**Alexa / Google Assistant stagnation.** Both platforms plateaued years ago
and are now being formally wound down (Google Assistant being replaced by
Gemini on Android from March 2026).

- Lesson: "most users rely on these assistants for only one or two tasks:
  playing music or setting a timer" -- ambient assistants that promised
  general capability converged on a narrow, boring actual-use core. Breadth
  of ambition did not translate into breadth of adopted use.
- Lesson: both platforms were run as loss-leading hardware/ecosystem plays
  (Alexa division reportedly on track to lose up to $10bn in 2022); when
  corporate priorities shifted, investment and quality both stalled. A
  personal assistant that depends on a company's ongoing willingness to
  subsidize it is fragile in a way a locally-run one is not.
- Lesson: Google's own 2026 "Proactive Intelligence" trial -- an app that
  analyzed life patterns to suggest actions before the user asked -- was
  pulled from the Play Store within weeks of leaking, an unforced signal that
  even Google is unsure proactive suggestion crosses the creepy/useful line
  successfully. Directly relevant to Scufris's proactive-contact design:
  approach with caution and be ready to retreat.
- Sources: [Alexa at 10: a winner and a failure](https://www.getrecall.ai/summary/ai/alexa-at-10-amazons-assistant-is-a-winner-and-a-failure-or-the-vergecast), [8 Years Ago, Google Beat Alexa. Then It Just Let the Assistant Waste Away](https://www.inverse.com/tech/google-assistant-8-year-anniversary), [Voice assistants serve their makers not their users - The Register](https://www.theregister.com/2022/12/14/voice_assistants_failed/), [Google pulls mysterious proactive AI assistant trial](https://www.voiceofemirates.com/en/science-and-tech/ai/2026/05/03/the-deleted-app-mystery-google-pulls-the-plug-on-a-mysterious-mind-reading-ai-assistant-trialthe-mystery-of-googles-deleted-proactive-ai-assistant-app/)

## 3. Local-first movement

**Ink & Switch, "Local-first Software: You Own Your Data, in Spite of the
Cloud" (2019), plus the follow-on community.**

- The seven ideals, as durable design targets rather than a strict checklist:
  no spinners (instant response, data is local); work is available across
  devices; the network is optional (core function works fully offline);
  collaboration works without a central server (conflict handling is local);
  data has long-term viability (works in 10 years without the vendor);
  security and privacy by default; and the user retains ultimate ownership
  and control (data in formats you can read without the app).
- CRDTs are proposed as the technical bridge that lets local-first software
  still support multi-writer collaboration without a central server of
  record -- relevant if Scufris's vault or manifests ever need multi-device
  sync, less relevant for a single-desktop deployment today.
- The 2025-era community explicitly reframes the seven ideals as "a gradient,
  not a checklist" -- partial local-first is still worth doing, and the open
  challenges people are actively wrestling with are sync between offline
  devices, conflict resolution producing "technically consistent but
  semantically unexpected" results, and schema migration across versions.
  Practical guidance converging on SQLite (or RocksDB) as the default local
  store, with SQLite-in-WASM plus the Origin Private File System for browser
  contexts.
- A framing worth stealing directly: "local-first software that still works
  after the acquihire" (QCon London 2025, Ink & Switch's Alex Good) -- the
  test for durability is not "does this work today" but "does this keep
  working if the vendor disappears tomorrow." That is a clean one-line
  design review question to apply to every Scufris dependency.
- Sources: [Local-first software - Ink & Switch](https://www.inkandswitch.com/local-first-software/), [Local-first software - Ink & Switch essay](https://www.inkandswitch.com/essay/local-first/), [awesome-local-first](https://github.com/alexanderop/awesome-local-first), [QCon London 2025: Local First - How To Build Software Which Still Works After the Acquihire](https://qconlondon.com/presentation/apr2025/local-first-how-build-software-which-still-works-after-acquihire), [InfoQ summary](https://www.infoq.com/presentations/local-first-build-software/)

## 4. Personal RAG / "second brain" practitioner reality checks

**Retrieval quality reality check (HN: "So you wanna build a local RAG").**

- Lesson: lexical search (grep/BM25) frequently outperforms embeddings in
  practice for well-structured personal corpora -- one commenter: "I'd be
  looking right at the page that contained the literal words in my query and
  embeddings would fail." Hybrid (embeddings + BM25, then rerank) is the
  consensus best approach when semantic fuzziness is actually needed.
- Lesson: recall measured during development is not recall in production --
  one team saw 90% recall in dev collapse to 30% with real users, because
  real users "don't know the exact terminology used in the articles."
  Developers testing with queries they already know the answer to is a
  systematic blind spot.
- Lesson: Anthropic reportedly avoids vector embeddings in Claude Code
  specifically, prioritizing latency and operational simplicity; large
  context windows are shrinking the cases where RAG is even necessary versus
  just handing the model a directory tree.
- **"We replaced RAG with a virtual filesystem" (HN, re: Mintlify's
  ChromaFs)** -- reinforces the same point from a production system:
  "the agent doesn't need a real filesystem, it just needs the illusion of
  one." Translating grep/ls/find/cd onto an index cut session creation from
  ~46s to ~100ms and matched what LLMs are already trained to do well (shell
  interaction), versus a bespoke retrieval API the model has to be taught.
  Vector embeddings were criticized for "destroying information" by
  collapsing keywords/acronyms into floats; the group consensus reframed
  this as "rediscovering" classic information-retrieval ideas (inverted
  indexes, Boolean search) that got skipped over during the RAG hype cycle.
- **"This is just RAG" (HN thread on an LLM-maintained wiki)** -- useful
  distinction: static-corpus RAG (retrieve from a fixed set of documents) is
  different in kind from a system where the LLM actively files, links, and
  edits its own knowledge base over time (closer to a zettelkasten the agent
  maintains than a search engine it queries). Worth naming explicitly if
  Scufris's vault interactions grow beyond pure retrieval into agent-driven
  organization.
- **Practitioner "abandon after the honeymoon" pattern** -- multiple
  independent essays ("I Deleted My Second Brain" by Joan Westenberg; "I
  rage quit my second brain"; a developer's two-year Obsidian vault of 2,137
  notes / 15,742 backlinks abandoned) converge on the same failure: capture-
  everything systems become "a mausoleum -- a dusty collection of old selves,
  old interests, old compulsions" rather than a usable tool, because nothing
  forces curation or forgetting. Practitioner blog consensus elsewhere:
  "a focused second brain with 200 high-quality captured items consistently
  produces better AI responses than a hoarder's archive of 2,000 items" --
  quality/curation beats volume for both human recall and AI retrieval.
- Idea worth stealing: Scufris's "capture before retrieval" design (Scufris
  fetches, a deterministic CLI persists blobs and manifests) is already
  aligned with the winning pattern here -- structured, deliberate capture
  with a manifest, not indiscriminate hoarding. The retrieval-quality
  lessons argue for defaulting to lexical/filesystem-shaped access over the
  vault (grep-able manifests, directory structure as the primary index) and
  treating embeddings as an optional enhancement, not the backbone.
- Sources: [HN: So you wanna build a local RAG?](https://news.ycombinator.com/item?id=46080364), [HN: We replaced RAG with a virtual filesystem](https://news.ycombinator.com/item?id=47618223), [HN: This is just RAG](https://news.ycombinator.com/item?id=47644949), [I Deleted My Second Brain - Joan Westenberg](https://medium.com/westenberg/i-deleted-my-second-brain-b7a65bce3717), [HN: Show HN plugin inspired by "I deleted my second brain"](https://news.ycombinator.com/item?id=44507956)

## 5. Proactive assistants and attention

- Calm technology (Amber Case): the durable framing is "technology should
  require the smallest possible amount of attention" and should be able to
  communicate without needing to speak -- push information through ambient
  channels (a light, a glance-able widget) before escalating to an
  interruption. Directly applicable to how Scufris's widgets should present
  information versus when voice should interrupt.
- Notification research: unread notifications alone measurably degrade
  attention and task focus even when not acted on; users respond to
  low-quality notifications by disabling the channel entirely rather than by
  tuning it, and repeated poor suggestions make a user "notification-blind"
  to the whole surface, not just the bad suggestions. The failure mode is
  binary (all off) more often than graduated.
- CHI 2024 research on proactive voice assistants in smart homes ("Better to
  Ask Than Assume") frames the fix as a communication-strategy problem: an
  assistant that proactively acts (e.g., silently adjusting devices) reads as
  presumptuous and erodes trust, versus one that proactively offers and asks.
  Acceptance of proactive suggestions varies a lot person to person, arguing
  for a tunable/quiet-hours-style threshold rather than one global default.
- Google's own abandoned "Proactive Intelligence" trial (see section 2) is
  a live, recent (2026) case of a well-resourced team pulling back from
  full pattern-based proactivity after limited exposure -- a caution flag
  specifically for the "predict what to surface before being asked" end of
  the design space.
- Idea worth stealing: model proactive contact as "offer, don't act" plus an
  ambient/low-attention default channel (widget appears silently) with an
  explicit escalation path to voice interruption only above a deliberate
  threshold, and treat any disable action as high-signal, not noise --
  matches the ROADMAP task's requirement that proactive contact ship with
  quiet rules and an audit trail, deferred to a later stage.
- Sources: [Calm Technology - calmtech.com](https://calmtech.com/), [Better to Ask Than Assume - CHI 2024](https://dl.acm.org/doi/full/10.1145/3613904.3642193), [Notifications' Effect on Attention](https://aleynadogan.medium.com/notifications-effect-on-attention-470ee8706a41)

## 6. Learning-tools angle (course-builder pillar prior art)

**Nicky Case, explorable explanations.**

- Design pattern, from "How I Make an Explorable Explanation": start with a
  question framed as a story or game to create emotional engagement, not an
  abstract definition; climb a ladder of abstraction from concrete
  interaction (drag, play) up to the general idea, using narrative
  connectors ("therefore", "but") to create plot-twist-style reveals of
  counter-intuitive results; end in an open sandbox that lets the learner
  explore past what the lesson covered.
- Explicit principle: ground before abstracting -- never open with jargon,
  and treat "elevate understanding" as different from "dumb down."
- Toolchain is deliberately lightweight: static site generator (11ty),
  GitHub Pages hosting, a reusable "Nutshell" component for expandable
  inline explanations, "Orbit" for spaced repetition, MathJax for notation.
  Nothing exotic -- reinforces that interactive courses do not require a
  bespoke platform, just composable small tools.
- **Execute Program's spaced-repetition course design** is the sharpest
  contrast with generic Anki-style tools: instead of content-neutral spaced
  repetition (Anki, where the user supplies all content and self-grades),
  Execute Program couples spaced repetition tightly to hand-authored,
  interactive, dependency-ordered lessons -- a later lesson is only unlocked
  once prerequisite concepts are reviewed and passed, reviews are spaced
  less aggressively than typical SRS defaults because the interactive
  lesson itself is the primary teaching event, and the system grades
  automatically rather than asking users to self-assess. The core idea:
  spaced repetition should serve the curriculum's dependency graph, not run
  as a bag of independent flashcards.
- Idea worth stealing directly for the future course-builder pillar: (1)
  question-first framing plus a sandbox at the end, borrowed from explorable
  explanations, for the "interactive HTML learning course" shape; (2)
  dependency-ordered unlocking plus auto-graded review from Execute Program,
  so a course compiled from captured references is not just a linear
  slideshow but has prerequisite structure and spaced re-exposure to the
  material actually captured; (3) keep the toolchain composable and static
  (small components, static hosting, no bespoke LMS), matching Scufris's
  preference for small deterministic helpers over a heavyweight framework.
- Sources: [How I Make Explorable Explanations - Nicky Case](https://blog.ncase.me/how-i-make-an-explorable-explanation/), [Explorable Explanations - Nicky's Blog](https://blog.ncase.me/explorable-explanations/), [awesome-explorables](https://github.com/blob42/awesome-explorables), [Execute Program: Spaced Repetition](https://www.executeprogram.com/spaced-repetition), [Spaced repetition, Anki and Execute Program - mike.place](https://mike.place/2020/executeprogram/)

## Durable lessons (ranked)

1. **Resource cost is a trust feature, not a footnote.** Rewind, Evernote,
   and (in reverse) the HA Voice practitioner's model-quantization fix all
   show that unmanaged CPU/battery/latency cost is what actually gets local
   or continuous-capture features turned off, independent of privacy or
   feature merits. Budget it explicitly.
2. **Curated and deterministic beats exhaustive and model-driven, at the
   edges.** HA Voice's fallback automations under the LLM, the RAG-vs-
   filesystem HN consensus (grep/ls beats embeddings for well-structured
   corpora), and "200 curated items beat 2,000 hoarded ones" are the same
   lesson from three independent angles: keep the tool/data surface small,
   structured, and lexically accessible; treat semantic/model layers as an
   enhancement over that, not the foundation.
3. **Proactive and ambient behavior fails by being presumptuous, not by
   being wrong.** Alexa/Google's plateau, Google's abandoned Proactive
   Intelligence trial, and the CHI research on "ask, don't assume" all point
   the same way: offer through a low-attention channel first, escalate to
   interruption only past a real threshold, and treat "user disabled this"
   as a hard stop, not noise to route around.
4. **Local-first is a discipline that erodes under business pressure unless
   it is actively defended.** Rewind's pivot away from local storage,
   Omnivore's shutdown as an acquihire, and Mycroft's collapse under legal
   costs all show that "local-first" and "small/independent" projects are
   structurally fragile unless the data format and license outlive the
   maintaining entity (as OpenVoiceOS did for Mycroft). The "does it still
   work after the acquihire" test is a good one-line design review question.
5. **Demoed capability must already be shipped capability.** Rabbit R1 and
   Humane both damaged trust immediately at launch by showing more than the
   product could do; for a personal assistant that a single user will
   depend on daily, credibility lost early is expensive to rebuild.
6. **A course/learning layer works best when it has a dependency graph and
   an open sandbox, not just linear content.** Execute Program and Nicky
   Case converge on this from opposite ends (drilling vs. exploring) --
   both outperform passive content by giving the learner either structure
   (prerequisites, auto-graded review) or agency (sandbox past the lesson).

## Ideas worth stealing

- **Explicit performance budget for any always-on/continuous local capture**
  (screen watching, transcript capture, indexing): define a CPU/battery
  ceiling up front and prefer diffing/incremental work over brute-force
  reprocessing, following the Rewind vs. "TopSecret" contrast.
- **Grep-first, embeddings-optional retrieval over the vault.** Make the
  manifest and directory layout the primary index (lexically searchable,
  agent-navigable like a filesystem), and treat semantic search as an
  additive layer for fuzzy recall, not the default path -- matches both the
  HN RAG-reality-check consensus and Scufris's own CLI-contract design.
  Consider a "virtual filesystem" style tool surface (ls/find/grep against
  the vault) for the agent rather than a bespoke retrieval API.
- **"Offer, don't act" plus ambient-first escalation** for the proactive
  contact design: default to a silent widget surfacing information; only
  escalate to voice/interruption past a deliberate, user-tunable threshold;
  log and hard-respect any disable signal per source, not globally.
- **"Still works after the acquihire" as a dependency review question.**
  Apply it to every third-party service or model Scufris leans on: if it
  disappeared or got acquihired tomorrow, does the vault and the CLI
  contract still work? This is a cheap, concrete gut-check pulled straight
  from the local-first community's own framing.
- **Curated capture over exhaustive capture.** Bias the library's capture
  flow toward deliberate, manifest-backed acquisition (a person or agent
  decided this was worth keeping) rather than indiscriminate background
  hoarding -- directly supported by both the "second brain" abandonment
  postmortems and the retrieval-quality research.
- **Course-builder shape borrowed from two traditions at once:** question-
  first hook and end-of-lesson sandbox (explorable explanations) wrapping a
  dependency-ordered, auto-graded spaced-review core (Execute Program) built
  from whatever was actually captured into the vault -- keep the toolchain
  itself composable and static rather than a bespoke LMS.
- **Small, curated tool/entity exposure to the LLM.** The HA Voice
  practitioner's ~32-entity cap for avoiding context-budget confusion is a
  concrete, transferable number-order-of-magnitude lesson for how many
  native tools/domain CLIs Scufris should expose to the agent at once.
