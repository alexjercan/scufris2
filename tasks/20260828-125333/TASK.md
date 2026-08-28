# The hotkey thread and the event loop wait for each other

- STATUS: CLOSED
- PRIORITY: 95
- TAGS: desktop, bug

## What happened

Spamming the hotkey froze the companion. The tray stopped answering a right
click, the hotkey stopped working, and the windows stayed where they were. The
process was still alive: `scufris-ctl` connected, was answered, and logged the
verb it carried.

## The deadlock

Three threads, and two of them are a cycle. Read off `gdb` on a wedged process:

| Thread            | Inside                                                   | Waiting for    |
| ----------------- | -------------------------------------------------------- | -------------- |
| global-hotkey X11 | `hotkey` -> `put` -> `hide_textbox` -> `is_visible`      | the event loop |
| the event loop    | `handle_user_message` -> `GlobalHotKeyManager::register` | the X11 thread |
| the grabber       | `global_shortcut().register`                             | the event loop |

`global-hotkey` runs one X11 thread. It hands every accelerator to one handler,
and it is also the thread that carries out a `register`, which is why
`register` blocks until the handler returns. `tauri-plugin-global-shortcut`
runs that `register` on the event loop, so the event loop is what blocks.

The handler ran `App::handle` on that thread, and `App::put` asks a window
whether it is visible - a Tauri getter, which waits on the event loop with no
timeout. So the X11 thread waited for the event loop and the event loop waited
for the X11 thread. Nothing else in the process was stuck, which is why the
command socket kept answering while every window and the tray were gone.

The grabber thread is a third victim, not a cause. It has a thread of its own
so that a grab is not asked for from inside the handler, but the plugin puts
the work on the event loop regardless, so its own thread never protected it.

The pill needs a grab whenever it comes on screen or goes away, which is what
put a `register` on the event loop at the same moment as a keypress. Tapping
the hotkey moves the pill, so tap and hold made this an ordinary keypress
rather than a corner.

## Not a regression of the gesture work

`58afdb6` did not introduce it. The handler before it also ran `App::handle` on
the display's thread. What the gesture work changed is how easy it is to reach:
two handler runs per press instead of one, and a gesture that moves the pill on
every tap.

## The rule

Nothing on the display's handler thread may wait on the event loop.

`keys::Hold` decides what a key meant and posts it. One thread of the
companion's own carries the queue out. The handler now reads keys, starts the
hold timer, and returns.

Posted under the lock that decided it, because order is the meaning: the
activation that opens the microphone and the activation that closes it are the
two ends of one press, and a stop that overtook its own start would leave the
microphone open with nothing on screen saying so. That race was real before
this: the timer thread and the release ran concurrently and either could reach
the runtime first.

One queue also covers the cancel and stop keys, which had the same problem and
the same cure. Cutting the speaker stays on the handler's thread: it is the
companion's own process and waits for nothing, and stop means stop.

## Verification

- `cargo test --workspace`: 364 pass, up from 363. `keys` tests are exact
  rather than timed - the queue is a `Receiver` the test reads directly, so
  what a press asked for and in what order is an assertion.
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`
  clean.
- Live, on a real display, against an isolated companion on `Super+Y` with its
  own runtime directory and a stand-in service: see [live-run.md](live-run.md).
  The pattern that wedged the old build, twice, plus thirty fast taps. The
  event loop stayed in `poll`, 51 phase transitions were logged where the old
  build managed two before dying, and every one of them was legal and in order.
