# Nightly review

- STATUS: OPEN
- PRIORITY: 70
- TAGS: review

## Scope

Nightly review of the commits that landed on `master` on 2026-09-10.
29 commits. Read-only: no edit, no commit, no push, no release, no Sprout.
No previous nightly briefing exists on this machine, so there is no period
to measure against.

Whole day: `05feaa2~1..cfa39be`, 91 files, 12386 insertions, 775 deletions.
Above the 10000-line `/scufris-review` cap, so split into four groups.

## Groups

| Group | Range              | Lines                        | What                                                                                                                                                  |
| ----- | ------------------ | ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| G1    | `5b16f45..e045850` | 1190                         | Morning fixes: briefing profile read, jobs refusals, den values, usage backends, desktop widget column, refusal codes, nix memory bound, review skill |
| G2    | `3aad9f1..d1c1718` | 3229                         | Durable briefing lifecycle delivery: new `host/service/src/briefings.rs`, service, extension TS, nix, desktop, iOS                                    |
| G3    | `ea2ca2d..077fb9c` | 1611                         | Durable briefing drawer dismissal: service, refusal codes, desktop HUD, iOS                                                                           |
| G4    | `fa46300..cfa39be` | 5372 (1876 outside `tasks/`) | Briefing loop safeguards, isolated test announcements, Rust/Python lint fixes, v2.7.0 release                                                         |

Release-only commits (`1761adb`, `1a0dbe7`, `424e178`) and task-record
commits carry version bumps and appended task history. They fall inside
the ranges above and need no separate lane.

Review order, highest risk first: G4, G2, G3, G1.

## Baseline checks on master

Running `npm run check` and the Python helper suite before the lanes.
`nix flake check` is out of scope for this run.

## Baseline result

- `TMPDIR=/tmp npm run check`: TypeScript suite 125 pass, 0 fail. `prettier --check` flagged only this task file, which was then formatted.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 394 pass, exit 0.
- Working tree clean apart from this task directory.
- `nix flake check` not run.

## G4 `fa46300..cfa39be` - briefing loop safeguards, v2.7.0

Lanes: correctness, contracts, red team, desktop, craft.

### Wave 1 findings, adjudicated

**BLOCKER - `host/service/src/service.rs:276` - the proactive circuit counts
starts, not failures, so three successful briefing deliveries in a row stop the
fourth and put the service in `Failed`.**

Re-derived in this session. `consecutive_proactive` is incremented on every
`ProactiveStarted` (service.rs:900) and on an inferred start (service.rs:947).
It is reset in exactly two places: when `briefings.next()` returns `None`
(service.rs:270) and when a surface opens a user turn (service.rs:751). A
delivery that is answered, acknowledged and durably recorded resets nothing. So
with four or more rows queued at once - the service down while several profile
runs land, or the 1-minute reconciler replaying terminal manifests - a, b, c are
delivered correctly, then d trips `>= MAX_CONSECUTIVE_PROACTIVE_TURNS` (3),
opens the circuit, marks every remaining row `Failed`, and takes the whole
service to `ScufrisState::Failed` with "restart it from the tray". The repo's own
test pins this: `consecutive_proactive_turns_back_off_and_open_the_circuit`
(service.rs:2057) answers a, b, c successfully with `proactive_answer` +
`proactive_settled` and then asserts `generation-d` and `generation-e` are
`Failed`. `BriefingStore::open` returns `Failed` to `Pending` on restart, so a
backlog of ten needs four manual restarts. The safeguard this range exists to
add fires on correct behaviour.

**MAJOR - `agent/extensions/scufris/service/index.ts:119` - the whole new
wake/response correlation path has no test, and its failure mode wedges every
surface.**

`tests/service.test.ts` imports only `proactiveIdFromMessage`,
`proactiveMessageDetails` and `resolveSocketPath`; the default `service()` export
and the `message_end` -> `proactiveStarted` / `agent_settled` ->
`proactiveSettled` wiring are exercised by nothing. If the envelope stops
round-tripping through `pi.sendMessage` -> `message_end` (a Pi upgrade that
normalises or drops `details`, or an earlier `message_end` handler replacing the
custom message), `turnProactiveId` stays undefined, the host logs "an
uncorrelated response arrived while a proactive slot was reserved" and returns,
`active_proactive` stays `Some` with no timeout, and every `surface.message` from
every surface is refused `no_free_slot` "A briefing is finishing." until a
restart. The lane confirmed the installed Pi does carry `details` today, so this
is a missing guard rather than a live break.

**MAJOR - `docs/src/guide/using.md:74` and `docs/src/dev/desktop.md:89` - both
pages promise a dismiss control on a failed briefing row that does not exist.**

Verified in this session. `BriefingRow::dismissible()`
(`shared/control/src/service.rs:276`) requires `delivery == Delivered`;
`requires_attention()` (service.rs:269) is true for `delivery == Failed`. So a
circuit-stopped row is drawn with attention styling and the word `halt`, and
`surfaces/desktop/ui/hud.ts:567` withholds the button (`tests/desktop-ui.test.ts`
pins 4 children, not 5). The owner guide says a failed briefing "stays until you
dismiss it" and never mentions the service restart that is the only recovery.

**MAJOR - `docs/src/dev/surfaces.md:92` - the client contract page tells an
author to send `"v": 9` three lines above its own `"v":10` samples.**

Verified in this session. This range bumped `SERVICE_VERSION` 9 -> 10 in all
three mirrors and rewrote the JSON block, but not the prose. A wrong version is
logged and the connection closes with no response, so a third-party surface
author gets a silent close from the page they just read.

**MINOR - `tools/briefing/briefing.py:1806` - publication permanently refuses
different prose for a generation, on the retry path this range makes routine.**
Redelivery of one generation is now an ordinary outcome (settled without
response, failed acknowledgement persist, agent disconnect, restart). On the
second wake the model regenerates prose, `scufris_briefing_publish` raises
`Refused("this briefing generation already has different prose")`, and the
prompt guidelines still say to tell the user the same briefing that was
published, with nothing about reading the fixed prose back.

**MINOR - `host/service/src/briefings.rs:374` - `fail_runs` overwrites the row's
real summary with the delivery-stopped sentence, and `BriefingStore::open`
restores the state but not the summary.** After a restart the drawer shows a
`ready` row whose text is "delivery stopped after repeated proactive turns;
restart the Scufris service" until the next reconcile pass, and permanently for a
run past `KEEP_DAYS`. Separately, an already-`Failed` row still counts as
`changed`, so every reconciler ingress while the circuit is open rewrites
`briefings.json` and logs a `circuit_failed` count that claims rows newly failed
when none changed.

**MINOR - `host/service/src/service.rs:133` - a circuit-stopped briefing masks
the tray detail of a failed job.** `attention()` returns the briefing summary
before scanning `self.jobs`, so a job awaiting filing is never named until a
restart clears the briefing rows. Colour is still right; detail is lost.

**MINOR - `host/service/src/service.rs:311` and `:1035` - two circuit-open paths
leave their rows saying `ready`.** The reservation-persist failure and the
"event was absent from its briefing inbox" branch set `proactive_circuit_open`
without calling `queued_delivery_failed()`, so the tray goes red while the drawer
row still reads `ready` with no restart guidance. `docs/src/dev/service.md` says
every retained queued row becomes `failed`. The next reconciler ingress
converges it.

**MINOR - stale protocol-version comments.** `shared/control/src/lib.rs:4` still
says "version 9 service channels" twelve lines above `SERVICE_VERSION = 10`, and
`surfaces/desktop/src/widgets/runtime.rs:284` still says "Protocol v9 currently
constructs only `Open`". Both verified in this session; they are the only
non-`tasks/` source left naming v9.

Not raised higher: the docs findings are MAJOR rather than BLOCKER because they
mislead rather than break a build. The circuit finding is raised to BLOCKER
because it ships in v2.7.0 and turns correct behaviour into a red service state
that only a manual restart clears.

### Correction to the wave 1 adjudication

Re-derived against `tasks/20260910-175753/recurrent-loop-investigation.md`,
which records the design intent: "The unlanded circuit would have stopped model
execution after three actual proactive starts, with 2-, 4-, and 8-second bounded
spacing, before a fourth turn." Counting starts rather than failures is
deliberate, and resetting on an acknowledged delivery would have defeated the
guard for the incident it was written for - in that recurrence the foreground
answered every one of the 145 wakes and published none.

So the circuit finding is ranked down from BLOCKER to MAJOR, and restated:

**MAJOR - `host/service/src/service.rs:276` - the loop guard is a raw
consecutive count, so it cannot tell a runaway producer from an honest backlog,
and the only recovery drains exactly three more rows.**

An ordinary backlog of four or more queued rows - the service down over a
weekend with a daily profile, several profiles landing near the same minute, a
reinstalled store replaying terminal manifests within `KEEP_DAYS = 30` - delivers
a, b and c correctly, then trips the limit on d, marks every remaining row
`Failed`, and takes the whole service to `ScufrisState::Failed` with "restart it
from the tray". `BriefingStore::open` returns `Failed` to `Pending`, so a backlog
of ten needs four manual restarts, each one presenting as a service failure. A
rate over time, or a distinction between rows queued before the burst and rows
that arrived during it, would stop the runaway without failing an honest
backlog. Recorded as MAJOR rather than BLOCKER because the hard stop is
deliberate and a recovery does exist.

### Found in adjudication, not by a lane

**MAJOR - `host/service/src/briefings.rs:255` - a `failed` row is never
evictable, so an open circuit can fill the 128-row store and then refuse every
briefing ingress.**

Derived in this session. `upsert` evicts only rows with
`delivery == Delivered` (briefings.rs:255-264); anything else returns
`StoreError::Full`, which `control_briefing` (service.rs:1291) turns into
`refusal::NO_FREE_SLOT` for the ingress. Before this range a burst of
generations ended up `Delivered` and therefore evictable - the incident record
says exactly that: "The service store retained its 128-row maximum: 125
wake-bearing rows plus three zero-source rows. It evicted the oldest 20 wake
rows. Every retained row is delivered." Under v2.7.0 the same burst delivers
three, opens the circuit, and marks every later row `Failed`, and `Failed` rows
are both `active()` and `requires_attention()`, so none of them can be evicted.
At 128 the store refuses all briefing ingress, including a legitimate profile's.
A restart returns them to `Pending`, which is also non-evictable, and each
restart converts only three rows to `Delivered`, so the store drains at three
rows per manual restart. The correction traded a visible loop for a store that
fills.

**Checked and clean:** every `thread::sleep` in `host/service/src/service.rs`
(HELLO_GRACE at :1424, RESTART_DELAY at :1543, the proactive backoff at :458)
runs after `drop(inner)`, so no lock is held across a wait.
`nix develop --command cargo clippy --workspace --all-targets -- -D warnings`
exits 0 with no warnings.

## Cross-cutting, found in adjudication

**MAJOR - `RELEASE.md:14` - the Python release gate never reads the
extensionless helpers, so it passed over code it did not look at.**

`RELEASE.md:14-15` and `docs/src/dev/maintenance.md:166` name the gate as
`ruff check .` and `ruff format --check .`. Both are green on master right now:
"All checks passed!" and "261 files already formatted". Ruff's directory
discovery only picks up `*.py`, so the five Python helpers that carry a shebang
instead of an extension are never in that 261:

- `scripts/scufris-jobs`
- `tools/jobs/scufris-jobs`
- `tools/jobs/scufris-report`
- `tools/quick-review-agent/scufris-quick-review-agent`
- `tools/voice/scufris-speak`

Naming them explicitly, `ruff check` reports 6 errors and `ruff format --check`
reports one file to reformat:

```
tools/jobs/scufris-jobs:471:9   TRY004 Prefer `TypeError` exception for invalid type
tools/jobs/scufris-jobs:737:13  TRY004
tools/jobs/scufris-jobs:740:13  TRY004
tools/jobs/scufris-jobs:793:17  TRY004
tools/jobs/scufris-jobs:1784:35 RUF100 Unused `noqa` directive (unused: BLE001)
tools/jobs/scufris-jobs:2272:40 UP012 Unnecessary UTF-8 encoding argument
```

The formatting one landed today: `228ea0b` put `displayable()` inside the
constants block at `tools/jobs/scufris-jobs:63`, and
`MAX_EVENT_BATCH = 1024 * 1024` follows its `return` on the next line with no
blank line between them. A reader sees a constant that looks like part of the
function. Ranked MAJOR because a gate that reports a pass over files it never
opened is a verdict that can lie, not because any of the six errors is a defect.

## G1 `5b16f45..e045850` - morning fixes

Read in this session ahead of the lanes. The substantive changes are sound:
`read_profile_bounds` (tools/briefing/briefing.py:1197) opens with `O_NOFOLLOW`,
settles the file type from the descriptor with `fstat`, uses `O_NONBLOCK` so the
FIFO guard is not itself a hang, and reads one byte past `MAX_PROFILE_BOUNDS` so
a file that grew is refused rather than truncated into something that parses.
`displayable()` unifies four doors that previously disagreed by seven code
points. Both are correct answers to the incidents in
`tasks/20260909-230022/TASK.md`.

**MAJOR - `tools/briefing/briefing.py:2097` - the reconciler is the designated
backstop for a refused briefing ingress, and it counts refusals without ever
saying why.**

Derived in this session. `announce()` returns `(ok, reason)`. Three of its five
call sites discard both: `briefing.py:1384` (collection started),
`briefing.py:1411` (a source finished), and `briefing.py:1500` (terminal). Those
three are documented as a "best-effort latency hint backed by later full
reconciliation", so dropping them is deliberate. The reconciliation that backs
them is `reconcile()` at `briefing.py:2097`, which writes
`ok, _reason = announce(manifest, ctl=ctl)` and reports only
`{"sent": N, "refused": N, "finalized": N}`. The systemd unit runs
`exec scufris-briefing reconcile --json`, so the journal gets a count and never
a cause. When the service refuses every ingress - a full store returning
`NO_FREE_SLOT`, or "Briefing delivery is stopped, but its safety state could not
be stored" - the last reporter in the chain says `refused: 1` every minute and
nothing anywhere names the reason. This is the "failure that reports to nobody"
shape: the reason is computed, returned, and thrown away one line from the
journal.

## G2 `3aad9f1..d1c1718` - durable briefing lifecycle delivery

Read in this session ahead of the lanes.

**MINOR - `host/service/src/briefings.rs:85` - a rejected briefing store is
reported only to the journal, and every surface sees an empty drawer.**

`BriefingStore::open` logs one `warn!("briefing state was rejected; starting
empty")` and calls `reject()`, which renames the file to
`briefings.json.corrupt` (`briefings.rs:577`, overwriting any previous one) and
starts with no rows. The data is kept on disk and the 1-minute reconciler
re-announces terminal manifests within `KEEP_DAYS`, so a retained day heals
itself. What does not heal is the report: `failed_delivery_summary()` returns
`None` for an empty store, so the tray stays green, the drawer is empty, and
nothing outside `journalctl` says a store was rejected. The red-team brief's own
law is "A record that exists but cannot be read is reported, never treated as
empty". Ranked MINOR because the bytes survive and the reconciler repopulates.

**Checked and clean in G2:** `BriefingStore::load` (`briefings.rs:455-520`)
bounds the file at `MAX_FILE_BYTES = 8 MiB`, refuses a non-regular file, reads
one byte past the bound, rejects an unsupported format version, and validates
every row, wake and dismissed ID for duplicates and for referential integrity
against the rows. `MAX_BRIEFING_ROWS = 128` matches the "128 audit rows" claim
at `docs/src/dev/briefings.md:479`.

## Rust and lint baseline

- `nix develop --command cargo test --workspace`: 443 tests, 0 failed
  (26 + 336 + 72 + 9, two suites empty).
- `nix develop --command cargo clippy --workspace --all-targets -- -D warnings`:
  exit 0, no warnings.
- `nix develop --command ruff check .` and `ruff format --check .`: clean, but
  see the gate hole above.

### G4 wave 2 - desktop lane, adjudicated

**MAJOR - `host/service/src/service.rs:131` - once the proactive circuit opens,
the whole service state is pinned to `Failed` for the life of the process, so
the assistant never says it is working again.**

Re-derived in this session. `attention()` returns the briefing failure summary
before it looks at any job, and `state()` returns `Failed` from either that or
`proactive_circuit_open`. `proactive_circuit_open` is assigned `true` at
service.rs:277, 311, 347, 540, 843 and 1035 and is never set back to `false`
outside the struct initializer at service.rs:409. So after the circuit opens: a
job that later fails or blocks never reaches the tray tooltip, and the HUD's
thinking line never comes on again for a typed turn, because
`Lifecycle::Working` can no longer reach `state()`. A person keeps talking to
Scufris and gets no working indicator at all. This supersedes and raises the
MINOR recorded earlier about `attention()` masking a job summary.

**MAJOR - `surfaces/desktop/ui/hud.ts:499` - a circuit-stopped row counts as
active, so the compact drawer cannot be collapsed and grows one permanent row
per briefing run.**

`briefingActive` is true for any `delivery !== "delivered"`, which now includes
`failed`. The collapsed row set is every active row plus the newest attention
row, so every stopped row is drawn in both states and the expand toggle changes
nothing but `aria-expanded`. Nothing clears the rows: `BriefingStore::open`
returns `Failed` to `Pending` (still not `Delivered`), retention at
briefings.rs:255 refuses to evict anything that is not `Delivered`, and ingress
while the circuit is open is marked failed at service.rs:1297. Two profiles a
day add two permanent rows a day, up to 128. `#briefings` is pinned at the end
of the conversation flow by `tail()`, so the list sits between the reader and
the newest line.

**MAJOR - `surfaces/desktop/ui/hud.ts:596` - the drawer header and the toggle's
aria-label report rows hidden while every row is on screen.**

Re-derived in this session. `hidden = Math.max(0, attention.length -
(latestAttention ? 1 : 0))` was correct only while active and attention were
disjoint. A `failed` row is in both. With two stopped rows: `active.length` 2,
`attention.length` 2, `shown` 2 of 2, and the header reads
`2 active - 2 need attention - 1 hidden`. Pressing expand reveals nothing. The
lane transcribed hud.ts:499-607 into a bounded scratch script outside the repo
and measured the same. `hidden = ordered.length - shown.length` is the correct
form. `tests/desktop-ui.test.ts:1421` asserts `2 hidden` and passes only because
its stopped row happens to be the newest attention row.

**MAJOR - `docs/src/dev/desktop.md:84` - three claims in the drawer contract are
stale.** "collapsed state shows every active briefing" now conflicts with "one
compact drawer"; "The header reports active, attention, and hidden counts"
describes a count that can lie; and the `dismiss` control claim is false for a
stopped row. `docs/src/dev/surfaces.md:155` has the matching gap for all
surfaces.

**MINOR, provisional - `surfaces/desktop/ui/hud.ts:627` - `tail()` detaches the
`<li>` holding the briefing toggle on every redraw, so keyboard focus drops to
`<body>`.** `briefings.remove()` runs unconditionally and `tail()` is called
from `drawBriefings`, `drawRows` and `append`, so any unrelated job tick can
bounce a user tabbing toward a `dismiss` button. Pre-existing, but this range
makes the drawer permanently non-empty and therefore the toggle permanently
focusable. Provisional: the lane reasoned from the DOM focus-fixup rule and did
not observe it in WebKitGTK, and the test page does not model
`document.activeElement`.

**MINOR - `surfaces/desktop/ui/hud.ts:522` - a collection-failed row and a
circuit-stopped row share `data-state="failed"` and the same red, separated only
by the words `fail` and `halt`, and one has a dismiss control while the other has
none.** The CSS comment above `.briefing-row` still says "Delivered failures and
measured partials alone have one explicit dismissal control".

The desktop lane's judgement is unharnessed: no display slot was taken, so
nothing about windows, mapping, `WM_HINTS.input`, focus capture and restore, the
X Shape re-cut or stacking was exercised. The range does not touch those paths,
but that is reasoning, not a pass.

**The hidden-count defect is on both surfaces, not just desktop.**
Derived in this session. `surfaces/ios/Sources/Protocol.swift:233` computes
`hiddenAttentionCount = max(0, attention.count - (latestAttention == nil ? 0 : 1))`
while `rows` keeps every `isActive` row, and `isActive` (Protocol.swift:175) is
`collection == .collecting || delivery != .delivered`, so a `failed` row is in
both sets exactly as on desktop. Two stopped rows give the iPhone drawer
`2 active, 2 need attention, 1 hidden` with both rows on screen and nothing
behind the expand control. Any fix has to land on both surfaces together.

Checked and clean: the dismissal path itself. `BriefingStore::dismiss`
(briefings.rs:435) rolls the dismissed set back when `persist()` fails, and
`host/service/src/service.rs:1171-1200` answers all three failures with the
named refusal codes `briefing_unavailable`, `briefing_not_dismissible` and
`briefing_dismissal_failed`, which `surfaces/desktop/ui/hud.ts:511` renders as a
`trouble`-toned notice. iOS mirrors the word `halt`, the red, and the restart
sentence, and puts the sentence in the accessibility label.

**MINOR - `nix/home-manager.nix:681` - only the briefing collector unit got the
memory bound.** `MemoryMax = "4G"` and `MemoryHigh = "3G"` are set on the
collector alone. The reconciler, the finalizer, the service, and the gateway
units carry `Restart = "on-failure"` and no memory bound at all
(nix/home-manager.nix:781, 824, 841, 894, 913). The reconciler runs the same
Python readers on the same manifests every minute, so the second wall the
collector now has is the one thing standing between a future runaway read and
the whole user control group. The incident this answered was in the collector,
so this is an observation about coverage rather than a known defect.

### Re-derived in adjudication

Confirmed the extension-wedge mechanism end to end.
`agent/extensions/scufris/service/index.ts:126` sends `proactiveStarted` only
from `message_end`, and `index.ts:128` returns early from `agent_settled` when
`turnProactiveId` is undefined, so a lost correlation sends neither marker.
`host/service/src/service.rs:481` then returns early from `proactive_settled`
because `!active_proactive_started`, and nothing else releases the slot except
an agent disconnect. There is no timeout. So the correctness lane's MAJOR holds:
a correlation that stops round-tripping wedges every surface behind
`no_free_slot` until the agent or the service restarts.

Also confirmed the lane's MINOR about `fail_runs`. `queued_delivery_failed`
(briefings.rs:359) collects run IDs from the whole `self.pending` Vec, which is
never pruned - `pending_len()` and `next()` filter by row state instead
(briefings.rs:136-156). `fail_runs` re-enters its branch for a row that is
already `Failed`, because the guard is `!= Delivered`, so `changed` is nonzero
and `persist()` rewrites `briefings.json` on every reconciler ingress while the
circuit is open, and the `circuit_failed` count in the log claims rows were
newly failed when none changed.

One more, small: `host/service/src/service.rs:481` drops a settled marker that
arrives without a start marker with no log line at all, so the one event that
would name the correlation gap is silent.

### G1, found in adjudication

**MAJOR - `surfaces/desktop/src/widgets/runtime.rs:1277` - a widget summoned
from the tray that the runtime refuses says nothing to the person who clicked
it.**

`refused(id, code, detail)` with `id == None` - which is every tray summon,
because a summon carries no request ID - logs `warn!("a summoned widget was
refused: {detail}")` and returns an empty `Vec<Act>`. The person clicks the tray
item, no panel appears, and nothing anywhere on the desktop says why. `e045850`
names this exact gap in its own message ("the refusal was a log line and nothing
else: the tray tick did nothing and said nothing about it") and answers the
capacity half of it: a corner is now the head of a column, so a side holds as
many panels as its height allows. The reporting half is untouched. The refusal
is now rarer and still silent, and `free_edge` still returns `None` when the two
columns growing toward each other would touch, so `no_free_slot` at
`runtime.rs:640` is still reachable. Verified at HEAD, not only in the diff.

**Checked and clean in G1:** the column geometry itself. `cargo test -p
scufris-desktop` passes 336 tests including
`every_instrument_the_runtime_admits_stands_clear_of_the_others`, which opens
panels of four heights until the runtime refuses and asserts no two rectangles
meet.

## Verified in adjudication

Every load-bearing claim above was re-derived from the tree in this session,
not taken from a lane report:

- The circuit's reset sites and increment sites (`service.rs:270, 751, 900,
947`) and the design intent recorded in
  `tasks/20260910-175753/recurrent-loop-investigation.md`.
- `state()` and `attention()` pinning to `Failed`, and the six places
  `proactive_circuit_open` is set true against the one place it is set false
  (`service.rs:409`, the struct initializer).
- The drawer's `hidden` arithmetic on both surfaces
  (`hud.ts:596`, `Protocol.swift:233`) against `briefingActive`/`isActive`.
- `dismissible()` requiring `Delivered` against the two doc pages that promise
  a dismiss control.
- The eviction guard at `briefings.rs:255` and `MAX_BRIEFING_ROWS = 128`.
- `fail_runs` overwriting the summary and re-counting an already-failed row.
- `refused(None, ...)` at `runtime.rs:1277` returning an empty `Vec<Act>`.
- `ruff format --check .` passing while `ruff format --check
tools/jobs/scufris-jobs` fails.
- Every `thread::sleep` in the service running after `drop(inner)`.

Checks run against master in this session, all green:
`npm run check` (125 TS tests; Prettier clean apart from this task file, since
formatted), `python3 -m unittest discover` (394), `cargo test --workspace`
(443), `cargo clippy --workspace --all-targets -- -D warnings` (0 warnings),
`ruff check .` and `ruff format --check .`, `shellcheck` on the three scripts,
`git diff --check` over the whole day. `nix flake check` was not run; the
v2.7.0 release record says it passed at `424e178`.

## Lanes not dispatched

Named as skips, not passes:

- Craft, on every group.
- Correctness, contracts, red team and desktop on G2 and G3. Those two ranges
  were covered only by the direct reading recorded above, which is one reader
  and not a panel.
- G1 had no panel at all; the reading above is all it got.

\*\*MINOR - `shared/control/src/lib.rs:28` and `shared/control/src/service.rs:58`

- the row cap and the frame cap are not reconciled, so a full store of maximal
  rows cannot be published at all.\*\*

Measured in this session. A `BriefingRow` at the protocol's own maxima - 64-byte
`id`, 64-byte `profile`, 256-byte `summary`, `u32::MAX` counters, validated by
`isProtocolValid` at `surfaces/ios/Sources/Protocol.swift:189` and its Rust
twin - encodes to 565 bytes. `MAX_BRIEFING_ROWS` is 128, so a full
`surface.briefings` frame is about 72,500 bytes against
`MAX_MESSAGE_BYTES = 64 * 1024`. The realistic circuit-stopped row is 297 bytes
and 128 of those come to about 38,200, so this needs long IDs and profiles as
well as a full store. Ranked MINOR for that reason, but the two bounds should
agree: nothing today stops the store from reaching a state whose own publish
frame is over the wire limit.

**Escalation - `host/service/src/service.rs:1297` with
`host/service/src/briefings.rs:368` - an open circuit turns the one-minute
reconciler into a continuous rewrite of the whole briefing store.**

Measured in this session, and this raises the earlier `fail_runs` MINOR.
`nix/checks/briefing.nix` pins the reconcile timer at `OnUnitActiveSec=1m`, and
`reconcile()` (`tools/briefing/briefing.py:2070`) announces every manifest for
every profile across `KEEP_DAYS = 30` days on every pass - roughly 60
`scufris-ctl briefing` calls a minute for two profiles. `control_briefing` calls
`queued_delivery_failed()` on every one of those while the circuit is open, and
`fail_runs` re-enters its branch for rows that are already `Failed` because the
guard is `!= Delivered`, so `changed` is nonzero and `persist()` rewrites
`briefings.json` in full. That is about 60 whole-file rewrites a minute, for as
long as the circuit stays open, each one logging a `circuit_failed` count that
claims rows were newly failed when none changed. `pending` is never pruned - the
comment at briefings.rs:357 says the queue is kept deliberately so a restart can
return the rows to pending - so nothing stops the loop short of the restart.

**MINOR - `surfaces/ios/Sources/ContentView.swift:1117` - the only recovery
instruction a stopped briefing carries is the tail of a one-line truncated
summary.** `DELIVERY_FAILED_SUMMARY` (`host/service/src/briefings.rs:22`) is 76
characters, and the words that say what to do - "restart the Scufris service" -
are the last 27 of them. The iOS row draws `Text(row.summary)` with
`.lineLimit(1)` and `.truncationMode(.tail)` at 10pt monospaced, so on a phone
those last words are the first thing to go. `accessibilitySummary`
(ContentView.swift:1136) carries the whole sentence, so VoiceOver is fine and a
sighted reader may not be. I could not measure the rendered width without a
device, so this is bounded by that: the string length, the truncation mode and
the word order are grounded; the actual clipping point is not. Desktop looks
safe - `.briefing-summary` (hud.css:511) sets no `text-overflow` and moves to
its own grid row at narrow widths. Worth noting that this is the first time a
recovery instruction has been carried in a summary field rather than in state
detail.

### G4 wave 2 - red team lane, adjudicated

**BLOCKER - `host/service/src/service.rs:480` - a wake that Pi never delivers
holds the single proactive slot forever, and while it is held every surface
message is refused with a reason that is false.**

Re-derived in this session. `dispatch_briefing_now` sets
`active_proactive = Some(A)` with `active_proactive_started = false`
(service.rs:336). If `message_end` never fires for the queued custom message -
the run rejects before the loop starts, or aborts before the pending queue
drains - `agent/extensions/scufris/service/index.ts:119` never sets
`turnProactiveId`, so neither `proactive_started` nor `proactive_settled` is
ever sent, and `proactive_settled` is additionally gated on
`active_proactive_started` at service.rs:480 so even a settle marker could not
release it. There is no timer, no deadline and no control verb that clears the
slot. The range's own test
`an_unrelated_settled_event_cannot_retry_a_proactive_message_still_queued_in_pi`
(service.rs:1951) asserts exactly this state and asserts it persists. While it
is held, `surface_message` (service.rs:693) refuses every submission from every
surface with `no_free_slot` and the detail "A briefing is finishing. Send this
again when it is done." - nothing is finishing - and every uncorrelated
assistant answer is dropped at service.rs:1050 with only a `warn!`, so anything
typed into Pi's own TUI is answered there and never enters canonical replay. The
row reads `in_progress` and `state()` reports `Idle`, so nothing says the system
is wedged. Under v9 this self-healed: the client set `activeProactive` when the
wake crossed the socket, so the next atomic response released the slot; v10
removed that (`client.ts`) without adding a bound. Caveat: the service half is
certain from the code and the test; the Pi-side triggers were read from the
vendored `pi-agent-core` dist and not reproduced.

**Circuit finding restored to BLOCKER.** Two lanes independently ranked
`service.rs:276` a BLOCKER, and the earlier down-rank in this task leaned on an
intent record that describes stopping a runaway, not failing an honest backlog.
The backlog path is the one `collected_runs()` is written to support: the
service or the machine unavailable for a day or two, several profiles collect,
the reconciler announces them all in one burst. Three deliver correctly, the
fourth trips the limit, every remaining row is marked `Failed` with its measured
summary destroyed, and the tray goes red with no user-reachable action. Draining
a twenty-item backlog costs seven restarts. It ships in v2.7.0.

**MAJOR - `host/service/src/service.rs:880` - the breaker is blind to the
duplicate proactive turns it exists to bound.** If the agent socket closes and
the same Pi process reconnects still holding its queued follow-up,
`unregister_agent` retries the event and it is dispatched again, so Pi holds two
copies. The second turn's `proactive_started` finds `active_proactive == None`,
is logged `outcome = "ignored"`, and does not increment `consecutive_proactive`;
its answer is dropped at service.rs:937. A full model turn is spent, its answer
is never seen, and the breaker never sees it - the exact runaway shape the
investigation record documents. Caveat: the consequence is grounded in the code;
the lane could not name a certain cause of a socket close that leaves the Pi
process alive with its queue intact, so the premise is inferred from
`unregister_agent`'s own handling.

**MAJOR - `tools/briefing/briefing.py:1769` - publishing into a run that is not
there now blames its own lock file instead of the missing run.**

Re-derived in this session. `publish()` opens
`run_dir(date, profile) / ".publish.lock"` with `O_CREAT` before anything reads
the manifest. With no run directory the caller gets "the morning briefing
publication lock is unavailable: [Errno 2] No such file or directory:
.../.publish.lock" instead of "no morning run for 2026-08-31". That is precisely
the message the model was getting over and over during the incident under
review, and it now names a lock rather than the missing run. `collect()` does it
the other way round (briefing.py:1250): the unsafe-link check, then the
directory, then the lock. `test_publishing_a_run_that_is_not_there_is_refused`
only asserts that `Refused` is raised, so nothing catches the change.

**MINOR - `tools/briefing/briefing.py:1769` - the publish lock is created before
the unsafe-symlink check and with a blocking `flock`.** `collect()` refuses a
symlinked run directory before it creates `.collect.lock`; `publish()` has no
such check, so a symlinked run directory gets a `.publish.lock` written inside
the symlink target. `collect()` also takes `LOCK_EX | LOCK_NB` and refuses with
"the {profile} briefing is already collecting" (briefing.py:1269); `publish()`
takes a plain blocking `LOCK_EX`, so a stuck holder blocks the model's tool call
with no bound and no message.

Live-machine note from the lane, worth the morning knowing: the service was up
with all three sockets present, and a nightly briefing was collecting at 23:00
while this review ran. The lane therefore did not run the Python briefing suite,
because running it is the action that caused the incident under review and the
fix under review is what would make it safe. It verified the fixture isolation
statically instead. That skip is the one worth a second pass on an idle machine.

## Verdict

29 commits landed on 2026-09-10, including three releases (v2.5.0, v2.6.0,
v2.7.0). Every check I can run is green on master: 125 TypeScript tests, 394
Python tests, 443 Rust tests, Clippy with `-D warnings`, Ruff, ShellCheck,
`git diff --check`. Nothing is red; the findings are behaviour and reporting,
not a broken build.

25 findings after merging duplicates across lanes: 2 BLOCKER, 11 MAJOR,
12 MINOR. Both BLOCKERs are in the v2.7.0 briefing-loop safeguard, and both are
the same shape - a safety mechanism that stops more than the thing it was built
to stop, with a manual service restart as the only way back.

Coverage: G4 got a four-lane panel (correctness, contracts, red team, desktop);
craft was not dispatched. G1, G2 and G3 got the direct reading recorded above
and no panel at all. Every skip is named in "Lanes not dispatched".
