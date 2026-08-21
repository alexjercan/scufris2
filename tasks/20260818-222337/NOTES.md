# Launcher composition note

## Accepted 2026-08-21

Scufris composes with the user's Pi configuration without changing normal Pi sessions.

Launcher precedence:

1. Use `pi` from the caller's `PATH` when present.
2. Otherwise use the Pi package pinned by the Scufris flake or configured through the Home Manager module.
3. Add only the enabled Scufris extensions and skills to that Pi invocation.

This makes system Pi extensions and themes, such as Plannotator and Gruber Darker, available in Scufris. Normal `pi` does not load delegation or widgets. Ambient composition is intentionally environment-dependent. The pinned fallback keeps the flake app usable on systems without Pi.
