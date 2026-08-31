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
scufris-den gym known --json     # the movements the database knows
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
scufris-den gym split push
scufris-den gym add "bench press" 60x8 60x8 60x6
scufris-den gym edit "bench press" 60x8 60x8 60x7
scufris-den gym learn push "bench press"
scufris-den backlog add "learn to weld"
scufris-den backlog promote 1
```

- A task with no day goes in the backlog. `promote` moves it onto a day.
- Log food by database id. `scufris-den macros query WORDS --json` finds the id;
  words matching exactly one food are taken as that food. Both databases live in
  the den: `Foods.csv` and `Exercises.csv`.
- A day is one split. Name it once with `gym split`; every set of that day
  belongs to it. `gym add` is one movement and all of its sets, written
  `weight x reps`.
- `gym edit` writes over every set of one movement at once, so a set added and
  a set dropped are the same call. No sets removes the movement, and `--rename`
  keeps them under another name. `gym rm` removes one set by its number.
- A weight is kilograms. Reps are whole numbers. A load of `0` is bodyweight.
- `gym learn` adds a movement to the den's exercise database, which is what
  the panel offers before a movement has ever been trained. Adding one is worth
  doing; it is not required to log a set.

## Rules

- The journal is personal. Report what was asked for and nothing more, and
  never copy the day into another tool or message unless the person asked.
- Never invent an entry. If a day has nothing, say it has nothing.
- A refusal comes back on standard error with a reason. Report the reason
  rather than retrying.
