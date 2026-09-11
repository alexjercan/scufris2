# Resurface restarted jobs after HUD clear

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: bug

Investigate the HUD job-row dismissal bug where a cleared logical job stays hidden after restart. Trace generation semantics, service snapshots, persisted filing, desktop HUD, shared iPhone presentation, and tests. Reproduce clear -> restart/new generation -> active row visible. Keep completed historical generations dismissed while surfacing the new active generation. Implement the narrow fix and run focused checks, npm run check, and nix flake check. Do not land.

## Investigation

- Reproduced on the unmodified tree with `publishedRows`: generation 1 appeared as `done`, filing its logical job ID produced an empty list, and changing the same job to generation 2 `working` still produced an empty list.
- Root cause: `workflow/orchestration.ts` stores filed rows as `Set<string>` keyed only by the 12-character logical job ID. `scufris_job_send` and Quick Review feedback update the owned job generation but never invalidate that broad tombstone. Session recovery restores the same broad ID from `scufris-filed-rows-v1`, so the bug also survives Pi and service restarts.
- The service owns only the latest whole `agent.jobs` snapshot. It does not persist job filing. Desktop and iPhone replace their complete job lists and hold no local dismissal. They will both redraw the same logical ID when the extension publishes its new active generation.
- The intended scope is one execution generation. A filed terminal generation stays acknowledged. Steering starts a distinct generation that must appear as active. If that generation later completes, its terminal row remains until it is filed separately.
- Baseline focused tests for existing generation restart, filing persistence, service job lists, and desktop rows passed after `npm ci`. The direct reproduction returned `{"appeared":["done"],"cleared":[],"restarted":[]}`.

## Implementation

- Replaced the broad logical-ID filing set with `scufris-filed-rows-v2`, which stores `{id, generation}` for each filed execution. Publication suppresses only an exact generation match. A terminal generation stays hidden, but steering the same logical ID to a later generation makes it visible.
- Filing now accepts only a current terminal job row. The synthetic failed drain row keeps its existing filing behavior.
- Legacy v1 migration reconstructs a filed generation from the terminal worker messages that preceded each ID's first appearance in the whole filing set. Later v1 snapshots preserve that fence even if the hidden job reports new work. Ambiguous legacy records never become a tombstone for a later current generation. The migrated state is rewritten as v2.
- The service and both surfaces keep protocol v10 unchanged. The extension applies the generation fence before it sends the whole snapshot. The service broadcasts replacement snapshots; desktop and iPhone replace their local row arrays.
- Added the exact sequence to agent tests, service snapshot coverage, desktop redraw coverage, and iPhone wire coverage. Updated the developer documentation for generation-scoped filing.

## Verification

- Direct post-fix reproduction: `{"appeared":["done"],"cleared":[],"restarted":["working"],"migratedFence":[["812dd3f9cef9",1]]}`.
- Focused agent tests: 3 passed, including generation restart, exact filing, persistence, and legacy migration.
- Focused desktop test: 1 passed.
- `nix develop -c cargo test -p scufris-service`: 85 passed across the service and gateway binaries.
- `env -u PI_PACKAGE_DIR npm run check`: passed; TypeScript, 128 tests, and Prettier all passed. A first unsanitized run inherited this foreground Pi's package override and failed only because that Pi store path has no `dark.json`; removing the unrelated override uses the locked development dependency.
- `nix flake check`: all checks passed on x86_64 Linux.
- `cargo fmt --all --check` and `git diff --check`: passed.
- iPhone protocol coverage was added in `surfaces/ios/Tests/ProtocolTests.swift`. It was not executed here because this Linux host has no `xcodebuild`, `swift`, or `xcodegen`.
- The initial pass did not land the Sprout.

## Authorized sync and landing

Foreground guidance later identified the generation-scoped filing behavior as approved and explicitly authorized sync, landing, and Sprout removal. It prohibited push, release, deploy, and live service mutation.

- `sprout sync restarted-job-visibility`: already up to date with `master` at `768b035`; no conflicts or source changes.
- Re-ran the three focused orchestration regressions, the desktop regression, and the service regression after sync: all passed.
- Re-ran `env -u PI_PACKAGE_DIR npm run check`: TypeScript, all 128 tests, and Prettier passed.
- Re-ran `nix flake check`: all x86_64 Linux checks passed.
- The exact-revision Quick Review graph was expanded with helper generation authority, worker-message migration evidence, the protocol generation boundary, and filing retention details before the authorized landing step.
