# Make landing receipts reflect repository truth

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: bug, workflow

Investigate false `not landed` and `claimed, not verified` receipts using jobs 0af28213d372 and 3dea8799b1ad. Define landing semantics for normal ancestry, squash-equivalent trees, direct-base work, and no-content reconciliation. Fix receipt measurement and claim checking narrowly, add regression tests, and run focused checks, npm run check, and nix flake check. Do not land or release.

## Evidence and root cause

- Job `3dea8799b1ad` ended on Sprout revision `72fafc0`. Manual `sprout land` created squash commit `0187c9f`; both commits have tree `6747820`, but neither is an ancestor of the other. The done receipt used ancestry only and returned `landed=false`.
- The later `scufris_job_land` correctly used Sprout's tree-equivalence rule and returned `landed=true`. It then retained the original source revision in durable cleanup state. Its nested receipt tested that source revision by ancestry after removing the branch and worktree, so it again returned `landed=false`.
- Job `0af28213d372` performed the authorized release on `master`. Its Sprout tip `01d581c` had the same release tree as tagged master commit `424e178`; merging the Sprout content into later master `cfa39be` produced the master tree unchanged. Receipt code had no content-equivalence rule, so it returned `landed=false`, measured push/tags/CI against the synthetic Sprout merge commit, and missed the measurable base and remote facts.
- Claim matching treats negated text such as `not landed or released` and attributive text such as `the landed safeguards` as positive completion claims. That caused false `claimed, not verified` verdicts independent of Git measurement.

## Semantics

`landed` means that the job has no committed content left to apply to its recorded base:

1. A normal commit is landed when it is an ancestor of the base. Its own commit is the landed revision.
2. A squash is landed when combining its source revision with the base leaves a base revision unchanged. The first such revision on the base's first-parent history is the landed revision.
3. Direct-base work is landed when the nominal workspace has no content left to apply to the base. Base and remote facts, not the workspace branch identity, describe its reconciliation.
4. A no-content reconciliation is landed even when the source and base trees differ because the base has later unrelated commits. The same unchanged-merge measurement applies.

A failed measurement remains `null` with a reason. A measured divergent merge remains false. Positive claims exclude explicit negation and attributive `landed`/`merged` wording. Release claims are backed by a published GitHub release for a remote tag on the measured landed revision, not by an arbitrary CI run.

## Implementation

- `tools/jobs/scufris-jobs` now uses one `measured_landing` rule for receipt generation and official landing. It first preserves normal ancestry. Otherwise it uses `git merge-tree --write-tree` against bounded first-parent base history and selects the oldest consecutive base revision that already contains the source content.
- Receipts use that base revision as `commit` and `landed_revision`, so push, tag, CI, and release facts refer to the commit that exists on the base. `land` persists the same base revision before cleanup when a squash or no-content reconciliation makes the Sprout command a no-op.
- `release_run` remains the generic CI fact. New `release` evidence comes from one bounded `gh release list` query and matches a published, non-draft release to a remote tag on the landed revision. Release claims now use that evidence.
- Claim matching rejects negated completion wording and attributive `landed` or `merged` wording.
- Regression tests reproduce the two incidents: a direct-master tagged release whose Sprout content is already present before a later task-close commit, and a manual squash followed by official no-op landing and removal. Existing ancestry and honest-unknown cases remain covered.

## Verification

- Historical object probe against the cited repository: `01d581c -> 424e178`, `72fbaae -> cfa39be`, and `72fafc0 -> 0187c9f`, all measured landed.
- Focused receipt and landing regressions: passed.
- `python3 -m unittest tests.test_scufris_jobs`: 57 passed.
- `npm ci`: completed from the lockfile with no vulnerabilities.
- `env -u PI_PACKAGE_DIR npm run check`: passed version check, strict TypeScript, 125 Node tests, and Prettier.
- `env -u PI_PACKAGE_DIR nix flake check -L`: all 54 checks passed; helper tests reported 396 tests with 4 skipped.
- `nix develop -c ruff format --check tools/jobs/scufris-jobs tests/test_scufris_jobs.py`: passed.
- `nix develop -c ruff check --ignore TRY004,RUF100,UP012 tools/jobs/scufris-jobs tests/test_scufris_jobs.py`: passed. Those three ignored rule families report six unchanged baseline findings in `tools/jobs/scufris-jobs`; the base file was checked separately to confirm them.
- `git diff --check`: passed.

No work was landed, pushed, tagged, or released.
