# Launcher composition note

## Accepted 2026-08-21

Scufris composes with the user's Pi configuration without changing normal Pi sessions.

Launcher precedence:

1. Use `pi` from the caller's `PATH` when present.
2. Otherwise use the Pi package pinned by the Scufris flake or configured through the Home Manager module.
3. Add only the enabled Scufris extensions and skills to that Pi invocation.

This makes system Pi extensions and themes, such as Plannotator and Gruber Darker, available in Scufris. Normal `pi` does not load delegation or widgets. Ambient composition is intentionally environment-dependent. The pinned fallback keeps the flake app usable on systems without Pi.

## Calm presentation accepted 2026-08-21

Scufris starts in Calm mode. `/calm` toggles it for the current Scufris process. The state survives session replacement and reload, then resets to on for a new process. Normal Pi sessions are unaffected.

Calm shows genuine user prompts, final assistant replies, the standard working indicator, Scufris footer status and notifications, and final model, abort, or truncation errors. It hides thinking, intermediate assistant text before tool calls, tool call and result rows, and Scufris job and widget event transcript rows.

Calm changes presentation only. Session storage, model context, resume, compaction, and exports retain the complete content. Keep the required Pi renderer patches isolated in one Scufris-only extension and fail checks when those renderer seams become incompatible.

## Cross-project delegation accepted 2026-08-21

Scufris can run from any directory and delegate into a different discovered Git repository. The model uses an opaque project ID, not a filesystem path. Project IDs are repository paths relative to configured discovery roots, such as `personal/nix.dotfiles`.

`scufris_agent_projects` lists valid IDs. `scufris_agent_spawn.project` selects one. When omitted, spawn uses the current repository if the session is inside one. Outside a repository, an explicit discovered project is required. Unknown, duplicate, non-Git, and escaping targets fail before Sprout runs. Worktree isolation and review guarantees remain unchanged.
