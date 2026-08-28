# Live run

The gestures on a real display, against the working tree's companion.

## The rig

A staging stack was already running and was left alone, so this ran beside it
rather than through `scripts/scufris-staging`:

- `native/target/debug/scufris-desktop --foreground`, with
  `SCUFRIS_RUNTIME_DIR=/run/user/1000/scufris-live`, its own `XDG_STATE_HOME`
  and `XDG_DATA_HOME`, and `SCUFRIS_DESKTOP_HOTKEY=Super+Y` so it grabbed no key
  the deployed or staging companion holds. `LD_LIBRARY_PATH` was set to the
  packaged wrapper's, which is what the tray's appindicator needs.
- A 90-line Python stand-in for the service on that runtime directory's
  `service.sock`: it welcomes a frontend, answers `submit`, and pushes whatever
  a fifo tells it to. It is not a service. It exists because two of the checks
  are about what a widget command does on arrival, and a widget command comes
  from the agent.
- `xdotool` for the keys, so a hold is a real `keydown`, a wait, and a `keyup`
  through the display rather than a call into the handler.

Both processes were stopped by recorded PID and the runtime directory removed.

## What was checked

Windows were read with `xdotool search --onlyvisible`, which is the display's
own answer rather than the companion's.

| Gesture                              | Expected                        | Seen                                   |
| ------------------------------------ | ------------------------------- | -------------------------------------- |
| `scufris-ctl show`                   | pill up, no take                | up, no phase change                    |
| `scufris-ctl hide`                   | pill away                       | away                                   |
| `scufris-ctl show` twice             | up, and still up                | up                                     |
| Tap `Super+Y`, pill up               | pill away                       | away                                   |
| Tap `Super+Y`, pill away             | pill up, no take                | up, no phase change                    |
| Hold `Super+Y` 700ms                 | `resting -> listening` at 250ms | `phase from="resting" to="listening"`  |
| Release                              | take ends                       | `listening -> transcribing -> editing` |
| `Escape` in the textbox, empty layer | draft gone, pill away           | `editing -> resting`, pill away        |
| `Escape` in the textbox, panel up    | draft gone, pill stays          | `editing -> resting`, pill still up    |
| `Super+Escape` after it              | pill away, panel stays          | pill away, instrument still up         |
| Assistant `working`, pill dismissed  | nothing happens                 | pill stayed away                       |
| Exhibit while idle, layer down       | nothing happens                 | pill stayed away                       |
| Exhibit while working, layer down    | workspace comes up              | pill up, all three panels visible      |

The last two are the pair that matter: the same command, the same layer, and
the only difference is whether the person had asked for anything.

## Not covered here

Transcription reached a real Whisper endpoint, so what the takes said is
whatever the microphone heard for 350 milliseconds. The words were not the
point and were discarded with `Escape` both times.
