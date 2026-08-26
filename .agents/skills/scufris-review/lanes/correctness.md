# Lane: correctness and tests

Judge whether the change is right at its edges, and whether its tests
would catch it being wrong.

## Look for

- The edge the change does not handle: an empty transcript, a
  transcript at the 8 KiB submission cap, a line at the 64 KiB cap, a
  zero-length name, a state the `(Phase, Event)` table in `state.rs`
  drops on the floor or handles twice.
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
  - Geometry measured from `getClientRects` is visual, not layout:
    while a transform animates, divide by the current scale or the
    marks freeze at a mid-animation frame.
  - The draft mirror (`scufris://draft`) fires only while the field is
    neither hidden nor read-only; an edit path that skips its triggers
    desyncs the review box silently.
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

## Running

```bash
nix develop --command cargo test -p scufris-desktop
TMPDIR=/tmp npm test
npm run typecheck
```

Run what the change touches. Never `nix flake check`.
