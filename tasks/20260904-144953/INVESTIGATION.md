# Conversation history across a Home Manager switch

> This report records the pre-implementation behavior at revision `483ae587`.
> The approved fix is recorded in [IMPLEMENTATION.md](IMPLEMENTATION.md).

## Conclusion

Scufris has two different forms of conversation history, with different survival behavior.

1. The Pi session normally survives. Pi stores completed session entries as JSONL under `programs.scufris.service.sessionDirectory`. The service starts Pi with `--session-dir ... --continue --mode rpc`, so a new Pi process resumes the latest matching session when the directory and working directory stay the same.
2. The surface-visible conversation does not survive a restart of `scufris-service`. Its canonical last-200-message replay is only an in-memory `VecDeque`. Every new service process initializes that ring empty. Desktop and iOS surfaces clear their own copy when reconnect replay starts, so they replace the old visible history with the new service's empty replay.

Therefore, the user-facing answer is no for a Home Manager switch that changes and restarts the Scufris service unit. The model-context answer is normally yes for completed turns in the same session directory.

This is conditional. A switch that leaves `scufris-service.service` unchanged does not restart it, so its in-memory replay remains. A desktop-only restart also preserves visible history because the still-running service can replay it.

## The switch path

The Scufris Home Manager module defines an active user service with:

- `ExecStart` set to the Nix store path of `service.package`;
- `SCUFRIS_SERVICE_AGENT` set to the Nix store path of `agent.package`;
- `SCUFRIS_SERVICE_SESSION_DIR` set to the stable data path; and
- no `Unit.X-SwitchMethod` override.

See `nix/home-manager.nix:369-395`.

The pinned Home Manager module defaults `systemd.user.startServices` to true and runs `sd-switch` after linking the new generation. In the pinned `sd-switch` 0.6.4 source:

- unchanged units receive no action;
- changes only to `Description` or `Documentation` are ignored for restart comparison;
- a changed active non-target unit with no explicit switch method defaults to `StopStart`; and
- stop happens before daemon reload, then start happens after reload.

Relevant source:

- Home Manager `modules/systemd.nix:335-357,490-540` at locked revision `c53d643b3737e2fcd04e6cb3b3580ef50b2087a0`;
- `sd-switch/src/unit_file.rs:88-105,142-149`;
- `sd-switch/src/lib.rs:239-320,413-444,527-541`.

`Restart=on-failure` is not what causes this switch restart. `sd-switch` explicitly stops and starts the unit.

A typical Scufris update changes the unit text:

- service code changes alter the `ExecStart` store path;
- agent extensions, skills, launcher settings, project roots, briefing time, or Pi package changes alter the `SCUFRIS_SERVICE_AGENT` store path; and
- the shared Rust source fileset includes `host`, `shared`, `surfaces`, and `tools/den`, and both service and desktop packages consume it. A desktop Rust source change can therefore also produce a new service package path.

See `nix/scufris.nix:11-22,43-51`, `nix/service.nix:14`, and `nix/desktop.nix:19`.

Exceptions include:

- an unrelated or docs-only switch whose generated service unit is equal;
- a configuration that overrides `systemd.user.startServices` to false;
- a switch when the user systemd manager is unavailable, which Home Manager skips; or
- a desktop configuration change that changes only `scufris-desktop.service`.

## What survives a service stop

### Pi session and model context

`host/service/src/config.rs:136-151` constructs:

```text
--session-dir <stable data path> --continue --mode rpc
```

Pi's startup path calls `SessionManager.continueRecent`. It opens the latest session matching the working directory when Scufris supplies its non-default flat session directory. Pi then rebuilds the active branch and assigns those messages to the new agent state.

Pi persists regular messages on `message_end` and persists extension custom entries as they are appended. The Scufris response extension writes each atomic user-visible assistant response as a `scufris-response-v5` custom entry before emitting it to the service. Relevant code:

- installed Pi `dist/main.js:278-338`;
- installed Pi `dist/core/session-manager.js:616-768,1217-1227`;
- installed Pi `dist/core/agent-session.js:375-414`;
- installed Pi `dist/core/sdk.js:82-91,239-245`;
- `agent/extensions/scufris/response.ts:38-42`.

The service handles SIGTERM deliberately. It takes the child, closes Pi's RPC stdin, waits up to five seconds, then escalates to SIGTERM and SIGKILL if needed. Pi treats RPC stdin EOF as shutdown and runs session shutdown hooks. See `host/service/src/main.rs:10-17,136-154`, `host/service/src/agent.rs:104-148`, and installed Pi `dist/modes/rpc/rpc-mode.js:579-646`.

This supports orderly continuation of already persisted entries. It is not a transactional handoff of an active turn:

- an assistant message is not persisted until its `message_end`;
- RPC shutdown calls runtime disposal and does not explicitly await turn settlement before exit; and
- for a brand-new session, Pi deliberately does not create the JSONL file until the first assistant message exists. A stop before the first assistant completes can leave no session file to resume.

For an established session, the current user message is normally appended before the assistant response. Partial assistant output is not durable conversation history.

Changing `service.sessionDirectory` is another exception: Pi then looks in a different directory and cannot resume the old session through `--continue`.

### Surface-visible canonical replay

The service owns a separate projection for surfaces:

- `Inner.conversation` is `VecDeque<ConversationMessage>` at `host/service/src/service.rs:76`;
- `Service::new` sets it to `VecDeque::new()` at line 199;
- `record` retains the last 200 in memory at lines 160-165; and
- registration replays only that deque at lines 245-256.

There is no file path, serializer, load step, or persistence call for this deque. The `session_file` field learned from Pi is used as session state, but the service never reads it to rebuild surface replay.

A Pi child crash while `scufris-service` remains alive does not clear this deque. A service process restart does.

### Surface copies

The desktop intentionally has no durable conversation copy. `surfaces/desktop/src/conversation.rs:17` says so. On `ReplayStarted`, `surfaces/desktop/src/main.rs:627` calls `Hud::reconnected`, which calls `Conversation::restart`; that clears all lines at `surfaces/desktop/src/conversation.rs:121-132`. It then accepts whatever the service sends before `surface.ready`.

This is correct when only the surface reconnects to the same service. It is destructive when the service is new and its ring is empty.

The iOS surface has the same contract. `ConversationStore.startConnection` and `connectAndReceive` clear `conversation` before replay at `surfaces/ios/Sources/ConversationStore.swift:507-552`.

The gateway forwards surface traffic and has no durable replay cache.

## Outcome matrix

| Event                                                    | Pi model context                   | Visible last-200 history   |
| -------------------------------------------------------- | ---------------------------------- | -------------------------- |
| No-op or unrelated Home Manager switch                   | stays in the running process       | survives                   |
| Desktop service restart only                             | unaffected                         | survives by service replay |
| Gateway restart only                                     | unaffected                         | survives by service replay |
| Pi child restart while service stays up                  | resumes persisted entries          | survives in service memory |
| Changed `scufris-service.service` during switch          | normally resumes persisted entries | lost                       |
| Manual backend restart, service crash, reboot, or logout | normally resumes persisted entries | lost                       |
| Switch during the first unfinished turn of a new session | not guaranteed                     | lost                       |
| Switch that changes `sessionDirectory`                   | old context is not selected        | lost                       |

## Why the Pi file is not already the replay store

The stores overlap but are not equivalent.

- Surface user messages contain plain text, a surface ID, and canonical attachment descriptors. Pi receives an XML-wrapped prompt with widget and attachment context but no originating surface ID.
- The response extension persists the exact atomic assistant response as a custom Pi entry, including details, widgets, and attachment IDs, but it does not persist the selected surface association.
- The service can accept and broadcast a canonical user message before Pi has persisted it.
- Pi history also contains tool calls, tool results, custom state, compactions, and branches that are not surface messages.

Rebuilding the service projection from Pi JSONL would require Scufris-specific parsing and still would not recover every canonical field losslessly. It would also couple the service to Pi's session schema.

## Repository history

The volatility is longstanding, but the terminology became broader than the implementation.

- `5c550e1` (2026-08-27) introduced the service. Its ring comment called the 200 entries "A screenful, not a history" and said deeper data belonged to the Pi session file.
- `e43620d` inverted ownership so the service, agent, and desktop became separate processes. The service still initialized the transcript deque empty.
- `a9d65bf` added the desktop conversation window. It explicitly documented "No durable copy" and clearing local lines before full service replay.
- `c122ba8` introduced protocol v4 and renamed the projection the canonical 200-message conversation history. It retained the in-memory deque. The old deep debug/get-entries route is not present in the current protocol.
- `0122f47` fixed iOS to clear before replay, matching the desktop behavior.
- protocol v5 and attachment work retained the same replay ownership.
- `bd2311b` explicitly describes a "fresh service" with no prior surface association and an empty window after restart, but fixes only routing of a new unprompted response.

No later commit adds a durable canonical replay or a special Home Manager switch policy.

Current documentation is accurate when it distinguishes the stores, for example `docs/src/dev/architecture.md` names the data directory "canonical Pi conversation" and `docs/src/dev/service.md` describes the 200-message replay. Other phrases use "conversation" for both. In particular, the comment at `nix/home-manager.nix:387-388` says "the conversation is on disk either way". That is true of the Pi session, but not of the surface projection.

## Existing test boundary

Current tests cover:

- replay from one live `Service` instance;
- exact retention of the latest 200 messages;
- desktop clearing followed by replay; and
- replay causing no speech or widget side effects.

They do not create a second service process or service instance backed by shared durable replay state. There is no persistent canonical state dependency to supply to such a test. The Home Manager checks assert the unit and `Restart=on-failure`, but do not assert continuity across unit replacement.

The existing tests therefore confirm same-process reconnect behavior, not restart survival.

## Recommendation

If "history survives a Home Manager switch" means the visible canonical conversation, treat this as a real persistence gap.

The architecture-consistent fix is a service-owned, bounded, versioned canonical replay store under `$XDG_DATA_HOME/scufris`, separate from Pi JSONL. The service should restore it before accepting surfaces and persist each accepted canonical user or assistant message before acknowledging it as durable. A bounded atomic snapshot is simpler than an unbounded log because the contract is exactly 200 messages.

A design must also decide and test:

- whether latest-surface response association is durable or reset intentionally;
- corruption and schema migration behavior;
- mode 0700 directories and mode 0600 files;
- attachment reference lifetime after restart;
- ordering between agent delivery, durable canonical record, broadcast, and message acknowledgment; and
- active-turn shutdown behavior.

Reconstructing from Pi JSONL is a weaker alternative because surface identity is missing and it couples independent layers. Setting `X-SwitchMethod=keep-old` would only defer deployment of new code. A reload method would require a custom live handoff and is more complex than persisting 200 bounded messages.

If the intended guarantee is only that Pi keeps model context, no runtime change is required. The documentation should then say explicitly that the surface window is process-lifetime replay and becomes empty after a service restart.

## Investigation limits

This was a static investigation at repository revision `483ae5872aea97f4a432583a9a58ca0f8540d12c`.

No product code was changed. No Home Manager generation was activated. No service was restarted or signaled. No production process, socket, journal, or session content was inspected. Evidence came from repository source and history, pinned Home Manager and `sd-switch` source, and the installed Pi package documentation and source.
