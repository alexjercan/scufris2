# Minor findings from the inversion review

- STATUS: OPEN
- PRIORITY: 55
- TAGS: cleanup,desktop,service

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
