# Scufris does what is asked, not a workflow

- STATUS: OPEN
- PRIORITY: 75
- TAGS: identity, workflow

## Goal

Scufris stops running a fixed pipeline and starts delegating literally.
"Implement feature X in project Y" means work, and only work. A review
happens when the user asks for a review. Nothing else is added because
the project could do it.

## The problem

Today Scufris reads `.scufris.toml` and treats it as a plan to carry
out: it finds the project can review, so it reviews; it finds a landing
gate, so it gates. The file describes what a project supports, and
Scufris reads it as what a request means. The result is that one verb
from the user turns into a pipeline the user did not ask for.

Alex, 2026-08-26: "the `.scufris.toml` just explains how things can be
used/done but scufris should do as I say".

## Scope

- `.scufris.toml` declares agent types rather than a workflow. Each one
  names what it is for and how it is run: work, review, quick-review,
  and whatever a project adds. The file is a menu, not a program.
- Scufris delegates to the agents the request names, and to no others.
  One verb is one job. "Implement X" is the work agent. "Implement X and
  review it" is the work agent and then the review agent.
- Conventions are inferred, overrides are obeyed. Whether to open a
  Tatr task, whether to work in a Sprout worktree or on `master`, and
  which harness to use are inferred from the project's preferences
  unless the user says otherwise in the request. "Do it directly on
  master" wins over the file.
- One job, archived, reported back. Scufris finishes what was asked,
  records it, and says what it did. It does not queue follow-on work of
  its own.
- The identity and the delegation prompt change with the file. Both
  currently describe the pipeline.

## Verification

- "Implement X" runs work and nothing else, and the transcript shows no
  review agent started.
- "Implement X, then review it" runs both, in that order.
- "Review the last change" runs review alone against existing work.
- A request naming `master` works on `master` in a project whose
  preferences ask for a Sprout worktree.
- A project whose `.scufris.toml` declares an agent Scufris has never
  seen can still be delegated to by name.

## Notes

Queued by Alex on 2026-08-26 while the widget runtime was being built.
Not to be started until the widget runtime increments and the i3 binding
mode task are done.
