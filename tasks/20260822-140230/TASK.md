# Keep delegated workers on managed development Pi

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: bug, development

## Outcome

- `scripts/scufris-dev` exports the exact PATH used to select managed Pi.
- Sanitization removes duplicate, trailing-slash, relative, and symlink forms of the repository npm bin.
- PATH ordering and empty entries remain unchanged. Normal and voice composition remain unchanged.
- The integration fixture proves foreground and descendant ambient Pi resolve the same managed executable while a hostile repository-local Pi cannot win.

## Decision

Keep the fix in the development launcher. Do not add theme ownership, worker flags, settings changes, or a new executable-identity protocol.

## Verification

- Initial `npm run check` - unavailable before local `npm ci`; TypeScript could not find the Node type definitions.
- `npm ci` - passed; installed the locked development dependencies.
- `node --experimental-strip-types --test --test-concurrency=1 tests/dev_helper.test.ts` - passed, 2 tests.
- `npm run check` - passed, 38 tests plus type and format checks.
- `python3 -m unittest discover -s tests -p 'test_*.py'` - passed, 26 tests.
- `ruff check .` and `ruff format --check .` - passed.
- `shellcheck scripts/scufris-dev` - passed.
- `nix fmt -- --check .` - passed.
- `git diff --check` - passed.
- `nix flake check -L` - passed, 25 checks on x86_64-linux.
- `sprout sync fix-worker-pi-path` - passed after implementation; already up to date.
- Post-sync rerun - all focused, npm, Python, Ruff, ShellCheck, Nix format, diff, and flake checks passed.
- Manual-review sync with master `3a45327` - passed as merge `3183b37` with no conflicts. The visible reviewer, prose-only orchestration, response extension, and matching development composition are present.
- Manual-review checks - focused helper 2 tests; npm 47 tests plus type and format; Python 29 tests; Ruff, ShellCheck, Nix format, diff, and all 25 x86_64-linux flake checks passed.

## Revisions

- Starting master: `ea20df6`.
- Implementation: `808d6e9`.
- Initial evidence: `5f4b7cf`.
- Manual-review base: `3a45327`.
- Manual-review merge: `3183b37`.
- This record's commit contains final manual-review evidence.
