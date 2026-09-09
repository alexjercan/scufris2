# Receipts, offers, and job rows in the HUD

Settled with Alex on 2026-09-09. `design.html` beside this file is the
document with working mockups; open it in a browser. It is also published at
<https://claude.ai/code/artifact/f1baecc6-86de-4779-bc61-ca7ad174210f>, which
wraps it in the head and body tags the file leaves out.

## The message

Badges go at the foot of the message, grouped by job. One wrapping rank per
job in the body column, where `.message-attachments` already sits, led by the
job's full 12-character id in quartz.

The id does the binding. The extension never parses the model's prose and
never breaks it, and the label is the handle Alex already uses to name the
job. A single-job message carries the label too. No special case.

A receipt is flat and bracketed. An offer carries the `>` caret from the
typing line and lights on hover, because an offer runs and a receipt does not.

## Offers

An offer is a control. A click sends `offer.take { id }`; the host submits the
prompt the extension composed when it built the offer, in the form
`for job <id>: <thing>`. A surface never writes that string.

No user line appears in the conversation. The badge marks itself spent, dim
and inert, and that mark is the only thing in the window that says what the
reply below answers. Spent is recorded against the offer id with the
conversation entry, so a reconnect does not re-arm a button for finished work.
This is the one store change the design needs.

The offer is the only badge whose words the model writes. Every receipt comes
from the helper's own `receipt.facts`, `receipt.claims`, and
`receipt.unavailable`, in the vocabulary `receipt_sentences` already uses
(`tools/jobs/scufris-jobs:3467`). Four states, and not a fifth:

- `measured`: the fact is true. Quartz.
- `refuted`: measured and false. Red.
- `claimed`: the worker said it and no fact backs it. Yellow.
- `unknown`: in `unavailable` with a reason. Muted, and never drawn as a no.

## Job rows

The job list is the last item in the conversation flow. It scrolls away like
any other line. No sticky floor.

A row outlives its job. Finishing does not remove it, so an overnight run is
still there in the morning; filing it is the acknowledgement and is what
clears it. One control per row, and which control it is says what the row is:

- Live rows show a red `x`. It arms first. One click reads `sure?` for three
  seconds, a second stops the job. Stopping the wrong job costs an hour of an
  agent's work and the guard costs a click.
- Terminal rows show `clear`. When two or more rows are terminal the list ends
  with `clear the N finished`.

The list holds 8 rows. Live rows never drop; the oldest terminal rows fall off
the top at the cap. They are still in `scufris-jobs`.

The tray keeps its one aggregate word, folded on the host from the rows rather
than sent as its own field. A failed row holds, so the tray stays red until
Alex files it and not until the process exits. Acknowledgement, not timing.

## Protocol 7

`read_exact` (`shared/control/src/service.rs:422`) rejects any version but its
own, so there is no negotiation and no ignoring unknown fields across
versions. Desktop, iOS, and `scufris-ctl` move together, and that costs the
same for one field or four.

```
agent.response
  text, details?, widgets?, attachments?
+ receipts?: Citation[]       // at most 4, one per job

Citation { job_id: string,
           badges: Receipt[],   // at most 6
           offers: Offer[] }    // at most 2

Receipt  { label, value,
           state: "measured" | "refuted" | "claimed" | "unknown" }
Offer    { id, label }          // the prompt stays host-side

agent.jobs                      // replaces agent.state
  jobs: JobRow[]                // at most 8, replayed on connect

JobRow   { id, project, since, summary,
           state: "working" | "blocked" | "done" | "failed" }

job.command                     // surface -> host -> extension
  { id, action: "cancel" | "archive" }
offer.take
  { id }                        // the host submits the stored prompt
```

Offers sit inside the citation because every offer follows from a receipt and
the strip is where they are drawn. An offer with no job has nowhere to go.

`agent.state` goes. The tray word is derived from the rows.

## iOS

The same shapes SwiftUI already has. A strip is a wrapping run of capsules
under the bubble, one run per job, with the same four state colours. The job
list is a section above the composer. The row control is a swipe: swipe a live
row to stop it, a terminal row to file it. The `blocked` notice at
`surfaces/ios/Sources/ContentView.swift:1134` goes; it was the aggregate.

## Rejected

- **Inline badges** in the sentence, like a link. Closest to the claim, but
  four marks in a paragraph is a paragraph nobody reads, and a badge can land
  on a line break and split the sentence.
- **Strips spliced into the prose**, breaking the message after the sentence
  that names each job so position did the binding and no label was needed. It
  cost a byte offset in the protocol, a sentence-boundary search over model
  prose, a fallback for when the model did not name the job, and four blocks
  of height for two sentences. The job id binds for one token.
- **A sticky job floor** pinned to the bottom of the scroller. About 46px of a
  560px window, permanently, and the first chrome this window would own that
  is not a message.
