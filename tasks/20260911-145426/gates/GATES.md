# Phase 0: measured Pi 0.85 behaviour gates

The terminal handoff design rests on six Pi behaviours the installed 0.85
documentation does not state. Each was measured against the real `pi` binary
before any production code depended on it. `run_gates.py` runs them and
`evidence/results.json` is the run this record stands on.

Every gate ran against a disposable `PI_CODING_AGENT_DIR`, session directory,
and working tree, with `auth.json` symlinked to the deployed login the way
`scripts/scufris-staging` seeds a staging agent. Nothing deployed was touched.

```bash
python3 tasks/20260911-145426/gates/run_gates.py --out gate-run
```

`evidence/results.json` is one complete run of all six, in order, on
2026-09-11.

Pi version: 0.85.0. Model: `openai-codex/gpt-5.6-sol`, thinking off. The TUI
gates drive the real terminal UI through a pseudo-terminal; the headless gates
drive `pi --mode rpc` and `pi -p`.

| Gate | Question                                                              | Result | What it decides                      |
| ---- | --------------------------------------------------------------------- | ------ | ------------------------------------ |
| G1   | Does `--session-dir D --fork FILE` plus a typed turn append?          | pass   | session lineage by fork, both ways   |
| G2   | Does `--continue` from `$HOME` find the `$HOME`-cwd file?             | pass   | the child resumes its own fork-back  |
| G3   | Can a `session_start` dispatch run a command that switches sessions?  | pass\* | plain `pi` can get the fork path too |
| G4   | Do built-in commands, extension commands, and `!` lines fire `input`? | pass   | what the conversation records        |
| G5   | Does `session_before_switch` cancel `/new` and `/resume` in the TUI?  | pass   | the lineage stays a single chain     |
| G6   | Does a `display: false` custom message reach the model next turn?     | pass   | catch-up as one injected message     |

No gate failed, so no fallback is in force. The fallbacks stay written down in
`DESIGN.md` because they are what the design would become if a later Pi
changed one of these answers.

## G1: forking carries the branch, and the fork is writable

`pi --session-dir D --fork BASE` from a different working directory, in RPC
mode and in the TUI:

- The new file's header `cwd` is the process working directory, not the
  base's. This is the whole reason the handoff forks instead of sharing: a Pi
  session's working directory is fixed at creation and is what its tools use.
- `parentSession` names the base file exactly.
- Every base entry is copied: `model_change`, `thinking_level_change`, the
  extensions' `custom` entries, and both messages.
- The typed turn appends to the new file, and the model answered with the code
  word from the base session. Model context, not just a file copy.
- The prompt did not stall in either mode.

The design recorded an unexplained 120 s `-p` timeout in its own fork
measurement. It did not reproduce here on the fork path, but print mode did
stall once for 180 s while seeding an unrelated gate and succeeded on retry.
Recorded rather than explained: no production path uses print mode. The
service runs `pi --mode rpc` and the terminal runs the TUI.

## G2: `--continue` selects by working directory

With one session in `D` written from `$HOME` and a newer one written from the
project, `pi --session-dir D --continue` from `$HOME` grew the `$HOME` file
and created nothing. The service's child can therefore keep `--continue` after
a fork-back, and the terminal's repo-cwd sessions in the same directory are
invisible to it.

## G3: a start-time dispatch works, and must be guarded

`pi.sendUserMessage("/gateattach", { expandPromptTemplates: true })` from a
`session_start` handler dispatched the extension command, which called
`ctx.switchSession`. It completed, `withSession` ran against the replacement,
and `session_start {reason: "resume"}` fired. No deadlock, at about 2 s per
switch.

\*The condition: **the handler must not dispatch again after the switch.**
Switching starts a session, which fires `session_start`, which dispatches
again. Unguarded, the recorded run performed **3560 switches in 25 seconds**. With
one guard it performed exactly one. Production carries this as the rule that
`/scufris attach` runs only when the current session is not already the
lineage fork, and `tests/terminal.test.ts` pins it.

## G4: only prose and skill lines are the person's words

Driven through the real TUI:

| Typed         | `input` fired | Note                                         |
| ------------- | ------------- | -------------------------------------------- |
| `/gatecmd`    | no            | the extension command handler ran instead    |
| `/model`      | no            | the built-in overlay opened                  |
| `!echo …`     | no            | the shell line ran and printed its output    |
| `/skill:name` | yes           | raw, before expansion, `source: interactive` |
| plain prose   | yes           | `source: interactive`                        |

This is the slash-command policy in the design, measured. The canonical
conversation records prose and `/skill:` lines as typed and records nothing
for the rest.

## G5: a cancelled switch is cancelled, and says so

With a `session_before_switch` handler returning `{cancel: true}` and calling
`ctx.ui.notify`, `/new` and `/resume` both fired the event with their reasons
(`new`, `resume`), both were cancelled, the notice was drawn on the terminal,
and the process stayed on one session for the whole run. `/resume` reaches the
event only after a session is chosen in its picker; cancelling the picker
never gets that far, which is the same outcome.

## G6: a hidden custom message is model context

A `custom_message` with `display: false` injected at `session_start` was
written into the session file and answered from on the next turn, under both
`deliverAs: "nextTurn"` and `deliverAs: "steer"`. Catch-up can therefore be
one injected message rather than a replayed transcript. Production uses
`nextTurn`, because a joining agent must not start a turn nobody asked for.
