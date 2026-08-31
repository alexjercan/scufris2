---
name: scufris-den
description: Read and write the-den journal - the day's tasks, habits, notes, food, weight, and workout, plus the backlog of undated ideas. Use whenever the person asks about their day, their week, what is late, what they ate, what they weigh, or what they trained.
---

# The den

`scufris-den` is the only way to read or write the journal. Do not read or edit
the Markdown directly: the command holds the lock and owns the format.

Add `--json` to any read for a machine-readable answer. `--date YYYY-MM-DD` or
`-N DAYS` selects a day other than the one it is.

## Reading

```bash
scufris-den show --json          # the whole day
scufris-den restant --json       # what was left undone before the day
scufris-den upcoming --json      # what is dated after the day
scufris-den backlog list --json  # ideas with no day yet
scufris-den weight --json        # the day's weight and a month of trend
scufris-den gym history --json   # what was trained, newest first
```

A read never creates an entry. Prefer one `show` over four narrow reads.

## Writing

```bash
scufris-den task add "call the dentist"
scufris-den task done 2
scufris-den habit toggle Gym
scufris-den note add "what was decided" --title standup
scufris-den weight 81.4
scufris-den macros log "chicken breast:g" 150
scufris-den gym add Push "bench press" 60 8
scufris-den backlog add "learn to weld"
scufris-den backlog promote 1
```

- A task with no day goes in the backlog. `promote` moves it onto a day.
- Log food by database id. `scufris-den macros query WORDS --json` finds the id;
  words matching exactly one food are taken as that food.
- One set is one `gym add`. Three sets of the same movement are three calls.
- A weight is kilograms. Reps are whole numbers. A load of `0` is bodyweight.

## Rules

- The journal is personal. Report what was asked for and nothing more, and
  never copy the day into another tool or message unless the person asked.
- Never invent an entry. If a day has nothing, say it has nothing.
- A refusal comes back on standard error with a reason. Report the reason
  rather than retrying.
