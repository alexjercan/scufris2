# Make Quick Review buttons square

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: review, ui, quick-review

## Scope

Set the custom Quick Review action buttons to square corners. Keep all other
control and panel styling unchanged.

## Decisions

- Use `border-radius: 0` on `.button`. This directly implements no corner
  rounding and applies consistently to every Quick Review button variant.
- Add a focused CSS assertion to the existing Quick Review render test. Do not
  add a browser dependency for this static style change.

## Verification

- `python3 -m unittest tests.test_quick_review`: 6 tests passed.
- `ruff check tools/quick-review/quick_review.py tests/test_quick_review.py`: passed.
- `git diff --check`: passed.
- `npm run format:check -- ...`: not run because this Sprout has no installed
  `prettier` executable. The changed Python and Markdown files are outside the
  repository's TypeScript behavior surface.
