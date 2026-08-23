# Append status-linked report entries

- STATUS: OPEN
- PRIORITY: 70
- TAGS: workflow

## Goal

Turn `report.md` into an append-only, status-linked conversation between a
worker and foreground Scufris.

## Format

Each report operation appends one entry whose heading is the exact status line
and whose body is the detailed Markdown evidence:

```markdown
# working: implementing filesystem notifications

Updated the watcher lifecycle and added focused tests.

# blocked: shutdown ownership is unclear

The current shutdown contract conflicts with...
```

## Scope

- Append report entries instead of replacing the complete file.
- Write each status heading and report body through one bounded reporting
  operation so evidence is unambiguously linked to its event.
- Preserve ordering: durable report entry first, then the matching status
  notification.
- Include runtime-generated failure entries through the same internal format.
- Define bounded file and entry behavior without losing the newest linked
  evidence.
- Make inspection return the chronological report without reconstructing links
  from separate files.
- Update the Pi report tool, Claude adapter, helper validation, prompts,
  documentation, and integration tests.

## Acceptance

- Multiple worker updates remain visible in chronological order.
- Every status event has one matching report heading and body.
- Status text in the heading exactly matches the appended event line.
- Partial writes cannot expose an event without its durable report evidence.
- Bounds, file modes, symlink refusal, and owned-job validation remain intact.

## Dependencies

Run after `20260823-153415`, and therefore after the status simplification.
