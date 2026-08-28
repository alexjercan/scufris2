# Fix the widget open deadlock between the link thread and the GTK main loop

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: desktop, widgets

## Report

Reported 2026-08-28. Asked for "both codex and claude usage". One gray square
came up, and the companion froze. The square could be dragged, and dragging it
over the pill and the textbox left them gray as well. Nothing recovered. An
earlier report the same day: two widgets up, a third one asked for, frozen.

Both are the same fault.

## The deadlock

Two threads take what the other holds.

The link's single reader thread handles `LinkEvent::Widget` inline
(`native/scufris-desktop/src/main.rs:471`). `Widgets::open` takes `turn`
(`widgets/mod.rs:415`) and holds it through `settle` and `place`
(`widgets/mod.rs:663`), which does three things in order:

1. `windows::fit` - messages to the event loop, and does not wait.
2. `windows::raise` - `show()` is a message, then `display::came_up` polls the
   X server for up to 500 ms over its own connection (`display.rs:45`,
   `display.rs:283`). The first `up()` calls `window_handle()`, which is a
   getter.
3. `windows::monitor` - `current_monitor()` (`widgets/mod.rs:680`), a getter.

Every getter in `tauri-runtime-wry` is `window_getter!` -> `rx.recv()` with no
timeout (`tauri-runtime-wry-2.11.4/src/lib.rs:197`). From a thread that is not
the main thread it waits for the GTK main loop and nothing else.

The main thread takes the same `turn` in `decide` (`widgets/mod.rs:451`),
reached from `run_on_main_thread` two ways:

- the aging sweep, every second, unconditional (`widgets/mod.rs:151`);
- the beat, every 250 ms, when the backends produced news (`widgets/mod.rs:197`).

So the link thread holds `turn` and waits for the main loop, while the main loop
holds nothing but waits for `turn`. Neither moves again. `rx.recv()` has no
timeout and there is no watchdog, so the companion stays wedged until it is
killed.

## Why the symptoms are these symptoms

The window mapped, so `fit` and `show` were carried out before the loop stopped.
The page never painted, because the loop stopped before it could, which is a
gray rectangle at the widget's size.

Dragging works because i3 moves the window itself and does not ask the client.
The smearing over the pill and the textbox is the proof it is the whole process:
those are other windows on the same main loop, and a loop that answers no expose
event leaves whatever was last on those pixels.

The trigger is not a widget count. It is the first open that runs while a
backend is already feeding: with one widget up and writing, the main thread
takes `turn` five times a second instead of once, and the open loses the race.
That is why "codex and claude usage" fails on the second of the two, and why a
third widget failed the first time.

The vulnerable window is the polling in step 2 while `turn` is held. A shell
that maps on the first ask never enters the loop, which is why opening widgets
one at a time from a warm pool does not reproduce it.

## Fix

The rule to restore: the main loop must never block on a lock that a worker can
hold while the worker needs the main loop.

Preferred: `decide` uses `try_lock` when it runs on the event loop and re-posts
itself when `turn` is busy. The sweep and the beat are both periodic, so
deferring one costs nothing, and the main loop stays free to answer the getter
the link thread is waiting on. `display::on_the_event_loop` already knows which
thread it is.

Rejected without measurement, but worth stating: moving `LinkEvent::Widget` off
the link thread the way `LinkEvent::Conversation` was moved does not fix this.
Any thread that holds `turn` and then calls a getter deadlocks the same way.

## Second path, same family

`Backends::send` writes to a backend's stdin under the backends lock
(`widgets/backends.rs:292`), reached from `perform` while `turn` is held. A
backend that stops reading its stdin wedges the same way once the pipe fills.
Much less likely, and the same rule covers it.

## Verification

- A test that holds `turn` from a worker and asserts the event loop is not
  blocked by it.
- Live: ask for two usage widgets in one turn, which is the reported case, and
  then a third while both feed.
- `cargo test --workspace`, `cargo clippy`, `nix flake check`.

## Outcome

The event loop no longer waits for the turn. It takes the turn if the turn is
free, and hands what it cannot decide now to a thread that is allowed to wait.
The deadlock needed both sides of a cycle and one side is gone.

### What was built

1. `native/scufris-desktop/src/widgets/turn.rs`. `Turn<T>` is the turn and the
   queue for whoever cannot wait for it: `wait` for a thread that may, `free`
   for the one that may not, `later` to hand a decision over, and `staff` to
   give the queue a thread. Its module documentation is where the deadlock is
   written down, because that is what the type is for.
2. `Widgets::decide` asks `display::on_the_event_loop` first. On that loop it
   takes the turn only if `free` answers, and otherwise queues the command. Off
   it, nothing changed: the thread waits as it always did.
3. `Widgets::open` splits. `open` routes and `opening` does the work. An open
   asked for on the event loop always queues rather than trying the turn first:
   it waits twice, once up to three seconds for a warm shell and again for the
   placement the loop itself has to carry out, and a free turn shortens neither.
   The tray's summon is the open that arrives that way, and it used to be able
   to hold the loop for three seconds on its own.
4. `Turn` counts what is queued, and `free` refuses while anything is. Without
   the count a caller could hand a reading over, find the turn free a moment
   later, and put a newer reading of the same panel up before the older one -
   leaving the stale one showing until something replaced it.
5. `display::on_the_event_loop` is public. It was already the module's answer to
   "can this thread wait"; waiting for the turn is the second thing that
   question decides.

### What was not changed, and why

`LinkEvent::Widget` still runs on the link's single reader thread. Handing it
off the way `LinkEvent::Conversation` was handed off does not fix this - any
thread that holds the turn and then asks the toolkit a question deadlocks the
same way, which is the whole finding. It is worth its own task for its own
reason: while that thread is inside an open, the frontend reads no transcript,
no state and no answer, which is exactly what the comment on the `Conversation`
branch says no branch should do.

`Backends::send` still writes to a backend's stdin under the backends lock. A
backend that stops reading its stdin still wedges whoever is holding the turn
once the pipe fills. What changed is the blast radius: the event loop no longer
joins it, so the companion keeps its windows, its pill and its keyboard and
loses only its widget decisions. The write itself wants to be non-blocking, and
that is a separate change with its own failure mode - a partial write of a JSON
line is worse than a slow one.

### Verification

`tasks/20260828-101057/live-run.md` has the live run: the reported request
verbatim, the two panels it opened at their real sizes rather than a
placeholder, a third and fourth opened while the first two fed, four rounds of
close-and-reopen, and the tray summon driven over the tray's own dbusmenu -
four in a row - to exercise the queue's open arm. Seven widget windows up, the
event loop still answering `hud`, and the claude panel still moving. Nothing
from the widget runtime in the log.

The run cannot catch the old deadlock in the act: the losing side of the race no
longer exists, so there is no arrangement of the stack that still shows it. What
it proves is the other half - that every path still opens widgets, including the
two that now always go through the queue.

Six tests in `widgets::turn`, run twelve times over for the two with timing in
them. Green: `cargo test --workspace` (348), `cargo clippy --workspace
--all-targets -D warnings`, `cargo fmt`, `nix flake check`.
