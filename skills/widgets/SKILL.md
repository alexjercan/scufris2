---
name: widgets
description: Open and control live native dashboard widgets for telemetry, projects, tasks, and daily information.
---

# Widgets

Use dashboard widgets when the user asks to show or keep visible live information.
The available widget and variant choices are discovered when the Pi session starts
and appear in the `scufris_widget_open` schema.

- Select the smallest variant that satisfies the request. Use a detailed variant
  when the user asks to show a widget without a size preference.
- Use `focus` presentation unless the user asks to tile or keep the widget in the
  dashboard layout.
- Retain returned surface IDs in conversation context.
- Update, focus, or close only surfaces opened by this Pi session.
- Treat an open result as independently running. Do not query widget data through
  dashboardd.
- When Scufris reports an external close, forget that surface ID. Never reopen it
  unless the user asks.
