# Session HUD widget over session mirroring

- STATUS: OPEN
- PRIORITY: 55
- TAGS: voice, desktop, hud

## Goal

The v2 increment of task 20260822-132001, as promoted at design review:
the HUD widget becomes the primary way to read the session, and kitty +
pi demotes to the debug view.

## Scope

- Extend the control protocol with session mirroring. V1 messages stay
  valid unchanged. The daemon remains the only writer of session files.
- The HUD is the session surface: scrollback, an editable input, and the
  same submit path the pill uses. Super+S or a pill click spawns it.
  Escape hides it; the session lives on.
- The popup is itself a widget: the session surface is spawned through
  the same spawn interface as any other surface. Escalation is spawning
  surfaces with increasing prominence: a line in the pill, then exhibits
  beside it, then the session surface raised.
- Transition gradually: the Kitty popup stays authoritative until the
  mirror earns trust. Do not remove the Kitty path in this task.

## Verification

- Pill submissions, popup submissions, and HUD submissions appear in one
  conversation, in order.
- A HUD crash loses nothing from the session.
- The mirror shows a turn that ran while the HUD was closed.

Decisions: `tasks/20260822-132001/RESEARCH.md` design review section
(session HUD promoted; escalation unification).
