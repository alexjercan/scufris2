# Live run

Reproducing the freeze, reading it off a stopped process, and driving the same
keys through the fix.

## The rig

The companion was started under `gdb`. `/proc/sys/kernel/yama/ptrace_scope` is
`1` on this machine, so `gdb` cannot attach to a process it did not start;
starting the companion as its child is what made a backtrace possible at all.
`/proc/<pid>/task/*/syscall` is refused for the same reason, and
`/proc/<pid>/task/*/stack` needs `CAP_SYSLOG`.

`/proc/<pid>/task/*/wchan` is readable, and it is what first said where to
look: every thread in the process was asleep, and the main thread - the event
loop, which should sit in `poll` - was in `futex_do_wait`.

Otherwise as the gesture run: `native/target/debug/scufris-desktop
--foreground` on `Super+Y`, with its own `SCUFRIS_RUNTIME_DIR`,
`XDG_STATE_HOME` and `XDG_DATA_HOME`, the packaged wrapper's
`LD_LIBRARY_PATH`, a Python stand-in for the service, and `xdotool` for the
keys. Everything was stopped by recorded PID and the runtime directory removed.

## Reproducing it

Fifteen press and release pairs, held `0.22` to `0.28` seconds - straddling the
250ms threshold, so each press is a coin toss between a tap and a take, and
each outcome moves the pill.

The first take wedged it. `resting -> listening`, `listening -> transcribing`,
and then nothing. Afterwards a single clean `xdotool key super+y` did nothing
at all, while `scufris-ctl show` returned exit 0 and the companion logged the
verb arriving.

## Reading it

`thread apply all bt` on the stopped process. The three frames that matter:

```
Thread 11  scufris_desktop::hotkey (state=Released)     main.rs:718
           App::handle -> show -> Ordered::apply -> put  app.rs:924
           App::settle -> hide_textbox -> textbox::up    main.rs:191
           WebviewWindow::is_visible -> rx.recv()        <- waits for the event loop

Thread 1   tauri_runtime_wry::handle_user_message
           tauri_plugin_global_shortcut register_internal
           GlobalHotKeyManager::register -> rx.recv()    <- waits for Thread 11
           global-hotkey x11/mod.rs:48

Thread 20  scufris_desktop::keys::grabber               keys.rs:250
           global_shortcut().register -> rx.recv()      <- waits for the event loop
```

Thread 11 waits for Thread 1 and Thread 1 waits for Thread 11. Thread 20 is a
third thread caught behind the same event loop.

## After the fix

The same build steps, the same rig, three rounds without restarting:

| Round                                       | Expected              | Seen                                   |
| ------------------------------------------- | --------------------- | -------------------------------------- |
| The fifteen straddling holds that wedged it | no freeze             | took every one                         |
| Thirty taps at 0.05s                        | pill toggles, no take | no phase change, as a tap              |
| Twenty more straddling holds                | no freeze             | took every one                         |
| One clean 700ms hold after all of it        | a take                | `listening -> transcribing -> editing` |
| One clean tap after that                    | workspace             | took it                                |
| `scufris-ctl show`                          | exit 0, pill up       | exit 0, three windows up               |

The event loop was in `poll_schedule_timeout` at the end, not `futex_do_wait`.

51 phase transitions were logged, against two before the old build died. Every
one was legal and in order:

```
      1 resting->listening
     17 listening->transcribing
     16 transcribing->failed
     16 failed->listening
      1 transcribing->editing
```

No `transcribing` without a `listening` in front of it, which is the assertion
that matters for the ordering half of the fix: a take is never stopped before
it has started, however fast the key is let go.

## Not covered here

`transcribing -> failed` is the stand-in rig having no transcription endpoint.
What the takes said was not the point and was never read.

The tray's right-click menu was not clicked. It is served by the event loop,
and the event loop being back in `poll` is the same fact one step earlier.
