# Investigate interactive Pi as a Scufris terminal surface

- STATUS: CLOSED
- PRIORITY: 70
- TAGS: research, architecture, pi, surfaces

Investigate an interactive Pi 0.85 terminal process as a first-class Scufris surface. Compare using it as the foreground agent plus terminal UI against retaining the headless foreground RPC agent and attaching a terminal surface client. Do not modify production code.

Deliverables:

- Read repository architecture and all relevant installed Pi documentation/examples.
- Record exact path/API evidence.
- Analyze lifecycle, ownership, correlation, replay, proactive wakes, jobs, briefings, attachments, widgets, reconnect, tools, and project-local extensions.
- Produce a capability matrix, recommendation, staged plan, tests, security boundaries, migration, and rollback.
- Trace the focused Pi 0.85 extension-only handoff path, project-local `.pi` cwd behavior, historical lease revisions, token/process fencing, and upstream API boundary.
