# Look at this: ask the program what it is showing

- STATUS: OPEN
- PRIORITY: 60
- TAGS: voice, desktop, service

## Goal

Rung 2 of the capture ladder. When the pointed window belongs to a
program that can be asked directly, ask it, and hand the agent the real
artifact rather than a description of it.

Two probes are worth building, and they are the same shape: the window
resolves to a process, the process is listening, and one call gets the
truth. Rung 0 and rung 1 are `20260825-153756`; the picture is
`20260828-224226`.

## Why this rung is the one that pays

For a file-backed program the best capture is not content at all - it is
**the identity of the thing**. Given a path, Scufris reads the whole file
rather than the visible part, greps it, and can change it. A screenshot
of an editor is worse in every dimension than the path it is editing.

## Neovim

Every running nvim already listens on a socket named
`$XDG_RUNTIME_DIR/nvim.<pid>.0`. No configuration change. Read-only, one
call, proven against the three instances running while this was written:

```
nvim --server "$sock" --remote-expr 'expand("%:p") . " @ " . line(".") . " cwd=" . getcwd()'

nvim.2021417.0 -> /home/alex/personal/scufris2/tasks/20260828-220328/TASK.md @ 1 cwd=/home/alex/personal/scufris2
nvim.2046889.0 -> /home/alex/personal/nova-protocol/crates/.../standard.rs @ 333 cwd=/home/alex/personal/nova-protocol
```

Absolute path, cursor line, working directory. Take the visual selection
too when there is one - `getpos("'<")` and `getpos("'>")` - because a
selected range is a better "this" than a cursor.

The window is a terminal, so the socket is found from the terminal's
child process rather than from `_NET_WM_PID`. Walk the process tree, or
take it from `kitty @ ls`, which reports the foreground process of each
window.

Guard it: a bounded timeout, and a failed probe falls to the next rung
rather than failing the capture. An editor in a modal state must not hang
the verb.

## Kitty

`kitty @ get-text` for the scrollback and `kitty @ ls` for the working
directory and the foreground process.

**One prerequisite, one line, in a repo you own.**
`allow_remote_control = true` is already set at
`nix.dotfiles/home/modules/kitty/default.nix:17`, but that permits
control only from a process running _inside_ kitty, over its tty. The
companion is outside it, so the same module needs `listen_on` and the
call needs `kitty @ --to`. Add that before building this.

Scrollback is the terminal case that matters: what you copy-paste out of
a terminal is command output, a stack trace, a failing test. Text can be
quoted, diffed and grepped; a picture of text cannot.

Bound it to the last screenful or few rather than the whole history.
"Look at this" means the screen, and a screenful is what the
demonstrative refers to.

## Order inside the rung

The two are independent. Neovim first: it needs no configuration change,
the payoff is the highest of anything on this desktop, and it proves the
probe registry with the cheapest possible case.

## Shape

A small registry of probes, each one a matcher on the window or its
process and a bounded command that returns a block of facts. A probe that
does not match, times out, or fails contributes nothing and the ladder
continues. Adding an application later must be adding one probe, not
touching the capture path.

## Scope

- The probe registry and the fall-through rule.
- The neovim probe: path, cursor, visual selection, cwd.
- The kitty probe: bounded scrollback, cwd, foreground process.
- The `skills/look/SKILL.md` paragraph for each: prefer the path.

## Verification

- "Look at this" over nvim hands over `path:line` and Scufris answers
  from the file, not from a description of it.
- A visual selection narrows what Scufris reads.
- "Look at this" over a Kitty window running a failed command answers
  from the output and quotes it.
- A probe that times out does not fail the capture.
- No capture happens without the explicit verb.
