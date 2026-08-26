---
name: scufris-widgets
description: Show something on the desktop beside the pill. Use when a reply is easier to look at than to listen to, or when the user asks to keep information visible.
---

# Scufris widgets

A widget is a small panel on the user's desktop. The installed widgets and what
each one takes are listed in the `scufris_widget_open` schema. They are read,
not operated: the user watches a widget while the conversation continues.

## When to open one

- Open an exhibit when the answer is easier to look at than to listen to. Keep
  speaking; the panel is beside the pill, not instead of the reply.
- Open an instrument only when the user asks to keep something visible. There
  are four edge slots. When all four are taken the open fails, and the user
  decides what goes.
- Do not narrate the open. The panel is on screen; saying so repeats it.

## Working with an open surface

- `scufris_widget_open` returns a surface identifier. Keep it for as long as the
  panel is worth updating.
- `scufris_widget_update` replaces what one surface shows. Update rather than
  open a second panel for the same subject.
- An exhibit retires on its own once newer ones crowd it out. Call
  `scufris_widget_close` only when the user asks for it to go.
- `scufris_widget_clear` takes down everything Scufris opened. Panels the user
  kept stay. Use it when the user asks to clear the screen.

## What the desktop reports back

- `surface_not_found` means that panel is already gone. Forget the identifier
  and do not reopen it unless the user asks.
- A closed-surface notice means the user closed it themselves. Forget it, and
  do not remark on it.
- `companion_unavailable` means the desktop is not running. Answer in words
  instead, and do not retry.
