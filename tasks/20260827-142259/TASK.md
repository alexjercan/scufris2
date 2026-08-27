# A panel joining a shared backend gets nothing until the next beat

- STATUS: OPEN
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

## Verification

Not started.
