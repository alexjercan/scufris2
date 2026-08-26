# Lane: red team

Do not review the change. Try to break the pill with it.

Reason from the code by default. Take the display slot only after the
desktop lane releases it, and only to confirm a repro you can already
state.

## Method

Take the states the change touches and drive them to their limits. For
each attempt, state the sequence, what should happen, and what does.

- The daemon: dead at every phase. A dead socket at accept (the record
  must persist unacknowledged), a disconnect mid-review (assistant
  state clears - what happens to the box?), a reconnect while a
  transcript is retained.
- Death and rebirth: kill -9 mid-review, mid-sent, and mid-speaking,
  then restart. The pending record survives anything but an ack or a
  discard; the restore presents the words with uncertain delivery and
  holds the keyboard.
- The store: a corrupt `pending.json`, an oversized one, a tombstone,
  a read-only state directory. A record that exists but cannot be read
  is reported, never treated as empty.
- Keys at the wrong time: Escape and Enter in every phase including
  `Failed`; a double Super+D faster than the entrance; the hotkey
  during the tween; Escape repeated (the `dismissed` flag) and then an
  activation.
- Caps: a transcript at exactly 8 KiB, one byte over, a protocol line
  at 64 KiB. Bytes, not characters - both sides measure the same way.
- The transcriber: the whisper endpoint down mid-transcribe, slow, or
  answering garbage.
- Focus predators: a window that maps, takes focus, and dies while a
  review is up (`xmessage` reproduces it; the pill must take the
  keyboard back on its own) and one that maps and stays (it must keep
  the keyboard); a click on each window in each phase;
  `focus_follows_mouse` with the pointer parked over the pill's spot.
- Motion: reduced motion toggled between turns; a hide during the
  entrance; a resize or scale change between two shows (the shape mask
  must be re-cut).

## Report

A repro is a finding only when you can state the exact sequence. A
crash, a dropped transcript, a stranded keyboard, or a lie in the log
is a `BLOCKER`; degraded but honest behavior ranks below it.
