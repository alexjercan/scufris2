# Minor findings from the inversion review

- STATUS: CLOSED
- PRIORITY: 55
- TAGS: cleanup, desktop, service

## Source

Review round 1 of `20260827-081702`, range `185034a..a13cb38`. The minor
findings that no other queued task carries: m1, m3, m4, m5, m7, m8, m13
and m15. Full record: `tasks/20260827-081702/REVIEW.md`.

These are independent. Take them in any order, or singly.

## m1. `bounded()` cuts on a UTF-16 index and can make a lone surrogate

`client.ts:498` shrinks by `slice(0, floor(length * 0.9))`, which can
land between the halves of an astral character. `encodeClientMessage`'s
`wellFormed` replacer then throws `not_well_formed` (`protocol.ts:160`),
and `tell()` swallows it at `"info"` (`client.ts:326`), which
`service/index.ts:69` does not surface. The line the function exists to
preserve is dropped instead.

Latent today, and the one point the panel split on. Both emit sites pass
`entry.spoken` through `plainProseParagraph`, which returns `undefined`
above 1000 UTF-8 bytes (`response.ts:62`), or through a `maxLength: 1000`
schema. A thousand code points cannot exceed 4000 bytes, so the loop
cannot run. It becomes live the moment anything else calls `said` or
`speak`, which is one line away.

Cut on code points, and raise the catch for `not_well_formed` to `warn`.

## m3. The companion's command socket is taken and released on the opposite policy to the service's

`command.rs:76` removes whatever is at the path with no liveness probe,
where `server.rs:51` refuses with `AddrInUse` when the socket still
answers. `unbind` (`command.rs:130`) then removes the path
unconditionally, so a second companion stopping takes the first one's
socket file with it, and `scufris-ctl open` reports nothing listening
while a companion runs.

Needs two companions, which L1 forbids. The asymmetry is the finding,
not the removal.

## m4. The pending record is fsynced but the directory entry is not

`pending.rs:215` does `sync_all` then `rename` and returns. The rename
that gives the file its name stays in the page cache, so a power loss
after `save()` returned can lose an accepted transcript - against a
module header that says nothing is submitted until a save is known to
have landed. Only a machine-level loss reaches it; process death is
already covered.

## m5. `begin_debug` checks the lease before the role

`service.rs:791` answers `debug_held` to a frontend that could never hold
the lease; with no lease held the same client correctly gets
`wrong_role`. No shipped caller reaches it - `scufris-ctl debug` always
connects as control. Swap the two blocks.

## m7. A configured key equal to the hotkey silently removes activation

`PillKeys::new` (`keys.rs:89`) checks neither key against `hotkey`, and
the handler matches `cancels` then `stops` before falling through to
`Event::Activate` (`main.rs:305`). `SCUFRIS_DESKTOP_CANCEL_KEY=Super+D`
beside the default hotkey makes `Super+D` mean Escape, with no warning.
`chosen()` already refuses and logs an unparseable accelerator; this is
the same class of answer.

## m8. `hud` is missing from the verb check

`nix/checks/service.nix:47` loops `send state watch abort debug open`.
`Spoken::Hud` exists (`scufris-ctl.rs:90`) and `scufris-ctl hud` is the
only way a window manager reaches the conversation window (D-HUD-4).
Renaming or dropping it passes `nix flake check`.

## m13. `tests/*.py` are run by no gate

`npm test` globs `tests/*.test.ts` and no derivation under `nix/checks/`
runs Python. This range adds a fourth Python file,
`test_usage_backends.py`, covering 444 new lines of parsing in the two
new backends. All 45 pass when run by hand. The gap was opened by the
three earlier files, not this one. `dev/maintenance.md:154` documents the
runner; AGENTS.md does not.

Add a check derivation that runs them, and name the runner in AGENTS.md.

## m15. Duplication the build forces, recorded only in a comment

`native/widgets/claude/widget.ts` and `codex/widget.ts` are 224 lines
identical apart from a six-line header. `deaf()` is the same nine lines
in four backends, and the whole driver is duplicated between the two new
Python ones. `build.rs` `include_str!`s each module whole, so a relative
import would not resolve: the copies are a consequence, not
carelessness. Nothing stops a shared prelude being concatenated at build
time. If the constraint is permanent, say so in `build.rs`.

## Proof

Whichever are taken: `cd native && TMPDIR=/tmp nix develop --offline -c
cargo test --workspace`, `TMPDIR=/tmp npm test`, and `TMPDIR=/tmp nix
flake check` for m8 and m13.

## Outcome (2026-08-27)

All eight, in one commit. Each is small; what follows is only where the
fix was not the one the finding named, or reached further than it.

### m1

Cut on code points, as written. The level is `"error"` and not `"warn"`:
the sink is `(message, level: "info" | "error")` and `service/index.ts`
surfaces only `"error"`, so `"warn"` is not in the vocabulary and would
have been the same silence under another name.

The test that proves it needed a second line to wait for, because with
the bug the first one is never sent. Waiting for a total of three lines
hung instead of failing - the hello counts - so `FakeService` gained
`untilLine`, which waits for the marker rather than for a count. Against
the UTF-16 cut the test now fails on `expected 2, actual 1`.

### m3

The liveness probe, mirroring `server::bind`. That settled `unbind` as
well: the file is only ever removed by the companion that bound it,
because `CommandSocket` is managed only when `listen` returned `Ok`.

`a_socket_an_earlier_run_left_behind_is_replaced_rather_than_refused`
asserted the old policy with a live listener, which is the case that
changed. It now builds a genuinely stale socket - bind and drop, which
keeps the name and closes the socket, exactly as a killed process leaves
it - and a second test covers the live one.

### m4

The directory is opened and fsynced after the rename. Not covered by a
test: what it prevents is a machine losing power between the rename and
the writeback, and nothing here can produce that.

### m7

`chosen` refuses a key equal to the hotkey, on both roads in: a
deployment can name it, and a hotkey of `Super+Escape` derives to
itself. The hotkey is parsed with `.ok()` rather than through `parse`,
which logs, so a hotkey that will not parse is not reported twice by a
function that is not about it.

Beyond the finding: cancel and stop can also be each other, with the
same shape - the handler matches cancel first, so stop is what silently
goes. The two keys are now settled together in `arrange`, which is what
`PillKeys::new` calls and what the tests can reach without an
`AppHandle`.

### m8

`hud` added to the loop.

### m13

`nix/checks/helpers.nix` runs `python3 -m unittest discover`, and
AGENTS.md names the runner beside `npm run check`. Three things the
sandbox needed:

- `git` and `tmux`, which the job runner shells out to.
- `patchShebangs`, because every helper is `#!/usr/bin/env python3` and
  the sandbox has no `/usr/bin/env`. Linking one in is not open: the
  chroot root is read-only.
- The same rewrite by `sed` over `tests/*.py`, because the fake agents
  are written into a scratch directory during the run and are past every
  patch. This is the one way the check differs from running the suite by
  hand.

Two tests make a real Sprout workspace, and Sprout is on the person's
own path rather than in this flake. They now skip when it is absent,
which is also the right answer for a machine without it. 46 tests, 2
skipped, in about 17 seconds.

### m15

Recorded in `build.rs` on both tables. The constraint is one file per
widget and per backend with no directory behind it, so an import has
nothing to resolve against. Noted as the current shape rather than a law:
a prelude concatenated in at build time would lift it.

### Beside the findings

`ruff check .` and `ruff format --check .` were failing on three things
the documented local contract covers: two nested `with` statements in
`test_usage_backends.py` from the reviewed range, and the missing
`check=False` and formatting in the `check-briefs.py` this session
added. Fixed. `dev/maintenance.md` also had no entry for
`test_usage_backends.py` in its test-ownership list, and now names it
and the one field rule it asserts.

### Proof

- `cargo test --workspace`: 341 passed, up from 336. `cargo clippy
--workspace --all-targets -- -D warnings`: clean.
- `npm run check`: typecheck, 80 tests, format, clean.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 46 passed.
- `ruff check .` and `ruff format --check .`: clean.
- `nix flake check`: all checks passed, `helper-tests` among them.
- Both new test groups were run against the unfixed code first: m1 fails
  on the line count, m7 on both of its tests.
