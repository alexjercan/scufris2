# Lane: correctness and tests

Judge whether the change is right at its edges, and whether its tests
would catch it being wrong.

## Look for

- The edge the change does not handle: an empty transcript, a
  transcript at `MAX_TRANSCRIPT_TEXT_BYTES`, a line at
  `MAX_MESSAGE_BYTES`, a zero-length name, a state the `(Phase, Event)`
  table in `state.rs` drops on the floor or handles twice. Take the
  numbers from the constants rather than from memory; both have moved.
- Durability invariants around `pending.rs`: the record lives from
  accept until an acknowledgment or a discard retires it, a tombstone
  stops restoration, a failed save stops the submission, and a store
  error is reported rather than mistaken for an empty store.
- Traps this repository has already hit:
  - A test that can never fail. `Object.assign` flattened the stub
    DOM's `activeElement` accessor and every focus assertion passed
    forever. Prove a new test fails against the unfixed code, or
    against the fix deliberately removed.
  - Suite results that lie. `npm test` without `TMPDIR=/tmp` fakes
    about 48 socket failures under a nested nix-shell.
  - A page that acts on its own optimism. `hud_submit` answers whether
    the line was taken and the field is cleared on that answer, because
    a page that cleared first threw away a sentence the host refused.
    Any new send path owes the same round trip.
  - A timer keyed on something two attempts share. A retry reuses the
    submission identifier, so an acknowledgement timeout needs a
    generation as well - `App::submissions` and
    `App::capture_generation` are the pattern.
  - No async runtime. The companion is `std::thread` everywhere; the
    `Executor` spawns a thread per task, and locks recover from
    poisoning rather than propagating it.
- A bug fixed with no test that fails without the fix. When the layer
  cannot host one (a live X behavior), the evidence in the task record
  must say so and say what was proven instead.
- A test that asserts the implementation instead of the behavior. Test
  names here read as behavior statements
  (`the_frame_is_the_size_the_page_lays_out`); a name that does not is
  a smell worth raising.
- A test that passes for the wrong reason. `QueueExecutor::expire`
  fires every pending timeout, so a test about which of two timers may
  act has to fire them one at a time (`expire_oldest`) or the live one
  settles the state and the assertion holds with the defect present.

## Running

```bash
nix develop --command cargo test -p scufris-desktop
TMPDIR=/tmp npm test
npm run typecheck
```

Run what the change touches. Never `nix flake check`.
