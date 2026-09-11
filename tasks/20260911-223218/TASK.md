# Draw a surface message as the words that were sent

- STATUS: CLOSED
- PRIORITY: 90
- TAGS: pi, presentation

## Purpose

A message sent from the phone or a desktop panel reads as four thousand
characters of widget schema around one sentence in a terminal that holds the
conversation.

## Investigation

- `service/protocol.ts` `surfacePrompt` wraps every `agent.message` in
  `<scufris_surface_message>` with the whole widget catalogue, the attachment
  descriptors, and the person's words, each JSON encoded and XML escaped.
  `service/client.ts` submits it with `pi.sendUserMessage`. The managed RPC
  child is given the same block; a terminal is the first holder with a screen.
- Typed input is not wrapped. `service/index.ts` observes `pi.on("input")` to
  mirror the turn and never rewrites it. Catch-up is a custom message with
  `display: false`, and wakes are custom messages Calm already hides.
- Measured on the live session: one such message was 4827 characters carrying
  100 characters of words.
- `pi.registerMarkdownTransformer` is display only, chains in load order, and
  runs for user messages, restored messages, and width changes. Pi keeps the
  original in the session and in the model's context. It arrived in Pi 0.84.0;
  the development dependency is 0.84.2 and the deployed Pi is 0.85.0.
  `response.ts` already registers one, and only when not in a terminal, so a
  terminal has no chain to conflict with.
- `sendUserMessage` has no display text, and a custom message with
  `registerMessageRenderer` would change what is written into the session and
  the canonical conversation for the managed child too.

## Scope

- Calm draws a surface message as its words, with attachments named, default
  on, `/calm off` for the raw block. Display only, no protocol change.

## Implementation

- `calm.ts` gains `calmUserMessage`, one pure function: it matches the exact
  envelope, parses the attachment descriptors and the words back out of their
  JSON, and returns the words with `_attached: <names>_` when something was
  sent. Anything that is not the envelope, and any envelope that does not
  parse, is returned as it came.
- The factory registers it through `pi.registerMarkdownTransformer` for
  `messageType === "user"` while Calm is on. Calm defaults on, so a terminal
  gets it from its first message. The state is read inside the transformer
  rather than captured, so `/calm off` applies from the next draw.
- Nothing else moved. The service protocol, the conversation the service
  replays, and what the model is given are all unchanged.

## Verification

- `tests/calm.test.ts` builds its fixtures with the real `surfacePrompt`, so
  the two files cannot drift apart silently: a plain message draws as its
  words, one carrying `</user_message>` and `&` round-trips exactly, an
  attachment is named, an empty message with an attachment is the name alone,
  typed text and assistant text are untouched, a broken envelope is returned
  raw, and `/calm off` shows the block again.
- `nix develop -c npm run check`: TypeScript, 158 tests, and Prettier passed.
- `nix flake check`: all checks passed on x86_64 Linux, including the mdBook.
- Not deployed and not released. A terminal loads the extension from this
  checkout, so `scufris-terminal` here already draws it this way; the managed
  child reads the packaged resources and is unchanged until a release.
