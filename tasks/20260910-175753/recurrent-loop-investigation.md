# Recurrent briefing loop investigation

Captured read-only on 2026-09-10 after the foreground recurrence. The bounded,
exact event sequence is in `recurrent-loop-events.json`. It excludes encrypted
model reasoning and unrelated conversation.

## Finding

The producer was this job's Python briefing test suite, not systemd
reconciliation and not a real briefing profile.

`tools/briefing/briefing.py` calls `announce()` at collection start, after each
source, and at terminal completion. `announce()` resolves `SCUFRIS_CTL` or the
bare command `scufris-ctl`. The `Run` and command-line fixtures changed `HOME`,
`XDG_STATE_HOME`, `XDG_CONFIG_HOME`, and `PATH`, but did not create or pin a
fixture `scufris-ctl`. Their temporary `bin` directories contained only model
and opener stand-ins. Resolution therefore continued through `PATH` to
`/home/alex/.nix-profile/bin/scufris-ctl`, which sent every synthetic lifecycle
update to the live service.

The temporary manifests were correctly isolated under directories named
`/tmp/scufris-briefing-*`, then removed by `TemporaryDirectory` cleanup. The
live run root `$HOME/.local/state/scufris/briefings` is absent. The foreground
therefore received valid control messages that referred to already deleted
test manifests. Its repeated refusal to invent or publish prose was correct.

## Exact timeline

The worker's durable harness session records these test invocations. The Pi
session groups the resulting wake messages at the shown times.

| Invocation                                | Local call time |             Wake interval | Wakes |
| ----------------------------------------- | --------------: | ------------------------: | ----: |
| Focused publication tests, first attempt  |    18:16:48.750 | 18:16:56.274-18:17:09.647 |     2 |
| Focused publication tests, second attempt |    18:17:16.369 | 18:17:19.635-18:18:09.873 |     5 |
| Full Python suite                         |    18:39:10.165 | 18:39:11.101-18:45:06.210 |    66 |
| Focused publication tests                 |    18:53:14.346 | 18:53:14.706-18:53:31.900 |     5 |
| Focused concurrent-publication test       |    18:56:42.887 |              18:56:43.205 |     1 |
| Full Python suite                         |    18:57:00.291 | 18:57:00.902-19:02:18.131 |    66 |

The 145 starts split as follows:

- 2026-08-31 morning: 113 (87 collected, 26 failed).
- 2026-08-31 evening: 10 collected.
- 2026-09-01 morning: 10 (8 collected, 2 failed).
- 2026-09-10 morning: 4 collected.
- 2026-09-10 nightly: 8 (6 collected, 2 failed).

All 145 generations and run IDs are unique because collection creates a fresh
random generation in each temporary fixture. Exact event-ID deduplication
cannot merge distinct generations.

The service store retained its 128-row maximum: 125 wake-bearing rows plus
three zero-source rows. It evicted the oldest 20 wake rows. Every retained row
is delivered; `pending` and `dismissed` are empty. The canonical conversation
has 145 briefing delivery IDs at sequences 7 through 151. They match the 145
computed Pi event IDs in exact order.

The deployed service has been active since 17:45:44 and uses Scufris 2.6.0,
agent protocol 9, and the mutable `AgentClient.activeProactive` implementation.
The Pi custom messages contain date, profile, generation, and source counts but
not the proactive ID. The client took the ID from each host socket wake, held
it as mutable client state, and added it to the next atomic response. There was
no competing proactive event in these spans, so that unsafe mechanism happened
to correlate every response correctly. The canonical order proves no observed
misacknowledgement in this recurrence.

The two-minute reconciliation service repeatedly reported `sent: 0` around the
incident. Its times do not match the immediate post-command starts. It was not
the producer.

Bounded journal evidence was copied before rotation. The live service journal
records `Started Scufris background service` at 17:45:44, then PID 1106165
records `attachment store opened`, `the service is listening`, `conversation
history opened`, and `the agent is starting`; it records `agent connected` at
17:45:45. `systemctl status` identifies invocation
`51a5b15287f4454cb013f87948697ab4`; `systemctl show` identifies the wrapped
2.6.0 executable listed below and the same uninterrupted active timestamp.

These nearby reconciler process records each emitted exactly
`{"finalized": 0, "refused": 0, "sent": 0}`:

| Timestamp       | Journal invocation ID              |     PID |
| --------------- | ---------------------------------- | ------: |
| 18:16:46.649164 | `34d78006574547dd82219bdf493eae6d` | 1139220 |
| 18:17:44.462369 | `71ef5006abe04972a1f8b73f9f2b53c6` | 1139538 |
| 18:18:44.174586 | `9f13e7ca7f5c472eaa3a52570ca62996` | 1139721 |
| 18:38:31.100984 | `4bd218f2650846a4ab88336c62d5c116` | 1153760 |
| 18:39:32.323310 | `d3e116c74ca84428bc4e9c4c4102af9b` | 1156771 |
| 18:52:31.121701 | `0b1035e52e5c42ab9dad1f127d4f160b` | 1165895 |
| 18:54:31.187976 | `b60ba4de8f7e40909328fa50a5fec6d7` | 1186279 |
| 18:56:31.099370 | `39a1a99439cd4337ad29fcab9d15c8fb` | 1188876 |
| 18:58:35.099231 | `5a9b6f3c6e0e48ccbadc1db55b34ea6a` | 1212207 |
| 19:00:31.122858 | `ca60877b18f34f92b397f19e7e2ddcd3` | 1225415 |
| 19:02:31.089799 | `fd8cece477ae4859b7a0485d368b9268` | 1227807 |

## Comparison with commit 593a045

The unlanded circuit would have stopped model execution after three actual
proactive starts, with 2-, 4-, and 8-second bounded spacing, before a fourth
turn. Exact Pi-message correlation would also remain safe if another follow-up
interleaved. Publication idempotence would not deduplicate this incident,
because the producer made 145 distinct generations and the foreground
published none.

Commit `593a045` therefore bounded the visible/model loop but did not remove
its concrete producer. It also marked only rows already queued when the circuit
opened. A continuous producer could add later rows as ordinary pending work
while the circuit remained open.

The follow-up correction:

- Pins both Python fixture families to a temporary `scufris-ctl`, including an
  explicit `SCUFRIS_CTL`, and records fixture-local calls for regression checks.
- Marks terminal ingress that arrives after an open circuit as failed with the
  same restart guidance. It cannot accumulate as ordinary pending work.

A full 394-test Python run after the correction left both the SHA-256 and mtime
of the live briefing store unchanged. No live files, services, timers, queues,
or run artifacts were changed during this investigation.

## Preserved source snapshots

At extraction time:

- `briefings.json`: SHA-256
  `3754fdc04f33228e34421404838ebc8fc7968192b034784c40c90cb385ffa6a3`,
  29,203 bytes, mtime 19:02:23.660025467 +03:00.
- `conversation.json`: SHA-256
  `d831da3ed79673834b5f03060b5f6da7874848aa867e23e60ca141aa19b70126`,
  35,494 bytes at extraction.
- Pi session `01a08b94-9398-70b8-9660-42164ce8d0ae`: SHA-256
  `8f717a3575d84a729dae75bb134584b7fb3b516ee7983c00c2d7fab1c88d6777`,
  743,576 bytes at extraction.

The source files remain in live state. Only their bounded facts and hashes were
copied into the task evidence.
