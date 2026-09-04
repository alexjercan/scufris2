# Durable canonical conversation replay

## Result

Scufris now keeps the service-owned latest-200-message surface replay across a service restart and a Home Manager switch. This is separate from Pi's JSONL model session.

The service restores the replay before any surface can register. Existing registration behavior is unchanged: retained messages are queued in order, followed by state and ready, while the service lock excludes live broadcasts. Desktop and iOS can therefore continue to clear their local copy at replay start without duplicating messages.

This implementation cannot preserve the in-memory replay during the one upgrade that first introduces persistence. The old service process has no code that writes the snapshot. Messages recorded by this and later builds are durable for subsequent restarts.

## Design

`host/service/src/conversation.rs` owns a JSON snapshot with:

- format version 1;
- ordered entries that contain an internal sequence and the complete canonical `ConversationMessage`;
- the protocol's existing 200-message count bound;
- a 16 MiB defensive file bound; and
- the same field and size validation that a live surface message uses.

The canonical message remains unchanged. Surface identity, role, text, optional details, optional widget calls, and attachment descriptors round-trip without reconstruction from Pi JSONL.

Exact duplicate records with the same sequence and payload are collapsed during recovery. Equal message content with different sequences remains as separate turns. A repeated sequence with different content is malformed and is not guessed at.

Each update serializes the complete bounded snapshot to a mode-0600 temporary file, calls `sync_all`, renames it over the prior snapshot, and synchronizes the parent directory. The parent is mode 0700. An interrupted write leaves the prior complete file; an incomplete temporary file is removed at the next open.

The default path is `$XDG_DATA_HOME/scufris/conversation.json`, with the standard `$HOME/.local/share` fallback. It is exposed as:

- `scufris-service --conversation-file`;
- `SCUFRIS_SERVICE_CONVERSATION_FILE`; and
- Home Manager `programs.scufris.service.conversationFile`.

The generated user unit sets the stable XDG path explicitly. Staging sets `$SCUFRIS_STAGING_ROOT/data/scufris/conversation.json`, so it cannot read or write the deployed replay.

## Recovery

A malformed snapshot is moved to `conversation.json.corrupt`. An unsupported version is moved to `conversation.json.incompatible`. Both cases start with an empty in-memory replay and do not prevent service startup. The next canonical message creates a current version snapshot.

A storage I/O error is logged. The live service keeps its current bounded replay and retries the complete snapshot on the next canonical message. Partial state is never exposed at the final path.

## Verification

All checks passed without starting Scufris, touching production runtime state, or activating Home Manager.

- `TMPDIR=/tmp cargo test -p scufris-service --no-fail-fast`: 36 service tests and 9 gateway tests passed.
- `TMPDIR=/tmp cargo test -p scufris-control`: 16 tests passed.
- `cargo clippy -p scufris-service --all-targets -- -D warnings`: passed.
- `cargo clippy -p scufris-control --all-targets -- -D warnings`: passed.
- `python3 -m unittest tests.test_scufris_staging`: 11 tests passed.
- `ruff check tests/test_scufris_staging.py`: passed.
- `ruff format --check tests/test_scufris_staging.py`: passed.
- `shellcheck scripts/scufris-staging`: passed.
- `env -u PI_PACKAGE_DIR npm run check`: 99 TypeScript/JavaScript tests, type checking, version validation, and Prettier passed. The foreground worker's injected `PI_PACKAGE_DIR` was removed so the test used its pinned local package, as intended.
- Focused Nix builds for `service-closure`, `service-home`, `service-interface`, `helper-tests`, and `docs`: passed.
- `nix fmt -- --check .`: passed.
- `nix flake check -L`: all compatible-system checks passed.
- `git diff --cached --check` and `git diff --check`: passed.
