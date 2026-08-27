# A panel joining a shared backend gets nothing until the next beat

- STATUS: CLOSED
- PRIORITY: 80
- TAGS: widgets,desktop,bug

## Goal

A second panel reading a backend that is already running shows nothing until
that backend's next reading. `Backends::subscribe` adds the surface to the
running process's refs and returns; nothing replays the last reading to it
(`native/scufris-desktop/src/widgets/backends.rs:205`). Two CPU graphs on
screen are meant to be two panels reading one sampler, and the second one is
blank for a whole cadence.

## Evidence

Found by a flaky `nix flake check` on 2026-08-27, while landing increment 5 of
`20260827-081702`, which touches nothing in the widget runtime.

`widgets::backends::tests::two_widgets_asking_the_same_question_share_one_process`
fails in the Nix sandbox and passes on the developer machine, and it fails on
some sandbox runs and not others. Its backend script is
`echo '{"load":1}'; sleep 30`: one reading, immediately, then nothing for
thirty seconds. `widget-1` subscribes and the process starts. Whether
`widget-2` is ever handed that reading depends on whether it subscribes before
the process writes the line. The test asserts it always is.

The test also has a second, smaller problem. Its `hear` helper returns on the
first non-empty drain, so even a correct fan-out can be split across two
drains and read as a miss. Accumulating over the poll window instead was tried
and did not fix the failure, which is how the real cause above was found.

## Scope

- Decide the behaviour first. Either a late subscriber is replayed the last
  reading the shared process produced, or it is not and a panel says it is
  waiting rather than showing a blank. The first is what the sharing claim
  implies.
- Fix `hear` to accumulate over its window either way. First-non-empty is
  wrong for any assertion about more than one piece of news.
- The test must then be deterministic in the sandbox: run
  `nix flake check` several times, not once.

## Decision

A late subscriber is replayed the last reading the shared process produced.
That is what the sharing claim implies: a backend answers a question, not a
panel, and what it last said is true of whoever asks next. Waiting for the
next line costs a beat on a sampler and five minutes on a backend that asks
the network, which the `claude` and `codex` widgets now do.

## Change

`Running` keeps `last`, the line it wrote most recently. `subscribe` hands it
to a surface joining a process that is already up, along with the health
marker if it is not fresh. `hear` in the tests now drains until the news stops
rather than until it starts: two pieces of news that belong to one moment can
land in two drains, and returning on the first read the second as a miss.

## Verification

- `cargo test -p scufris-desktop widgets::backends`, six consecutive runs, all
  16 green.
- `cargo test -p scufris-desktop`, 241 green.
- `nix flake check --offline` twice and `nix build .#scufris-desktop --rebuild`
  once, all green - three runs of the suite in the sandbox where it used to
  fail.
- New test `a_panel_that_joins_late_is_handed_what_the_backend_last_said`
  covers the behaviour directly, rather than leaving it to the timing of
  `two_widgets_asking_the_same_question_share_one_process`.
