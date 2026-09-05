# Investigate history survival across Home Manager switch

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: investigation, home-manager, sessions

## Result

The investigation found that Pi JSONL normally survived a switch while the
service-owned surface replay did not. See
[INVESTIGATION.md](INVESTIGATION.md).

The approved persistence design is now implemented. The service atomically
stores its versioned canonical latest-200-message replay under XDG data,
restores it before registration replay, and isolates malformed or incompatible
state without blocking startup. Deterministic restart, truncation,
deduplication, recovery, reconnect, privacy, and staging-isolation tests pass.
See [IMPLEMENTATION.md](IMPLEMENTATION.md).
