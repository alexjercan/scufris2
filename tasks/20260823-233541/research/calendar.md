# Local-First Calendar Tooling: .ics Files for ~/personal/the-den

Research on five calendar solutions for git-tracked local-first event storage.

## khal + vdirsyncer

**Storage Format:**
vdir format stores exactly one VEVENT/VTODO per .ics file, with filename matching UID.
Fully compatible with CalDAV/CardDAV standards. Each event is a standalone .ics file readable by any standard tool.

**CLI/JSON Quality:**
khal list supports --json flag to output structured data for programmatic queries.
Example: `khal list now 7days --json title start-time` outputs JSON array of events.
ikhal interactive mode available but less scriptable.

**nixpkgs:**
khal 0.14.0 and vdirsyncer 0.20.0 both present. Actively maintained packages.

**Maintenance:**
khal: active, recent issues in 2026. Small community (~10 contributors).
vdirsyncer: entering maintenance phase. Latest 0.16.8 (June 2026) final release.
Successor pimsync in development but incomplete feature parity.

## ikhal (Islamic Calendar)

Interactive mode of khal; covered above. Separate Islamic calendar utility exists but unrelated to this use case.

## calcurse

**Storage Format:**
Text-based format, proprietary binary or custom text serialization.
Can export to ical/pcal formats but not primarily one-event-per-file.

**CLI/JSON Quality:**
No native JSON output. Format strings for non-interactive output.
Export via --export-format ical possible but not designed for programmatic agenda queries.

**nixpkgs:**
calcurse 4.8.2 present. Active maintenance.

**Maintenance:**
Stable, focused on local-only calendar/todos. Not designed for server sync.

## gcalcli

**Storage Format:**
Google Calendar cloud-native. Requires authentication, no local .ics files.
Local caching exists but not primary use case.

**CLI/JSON Quality:**
Limited JSON support. TSV output available for scripting.
Primarily text-formatted output for human reading.

**nixpkgs:**
gcalcli 4.5.1 present.

**Maintenance:**
Active. Cloud-dependent, not suitable for local-first vault.

## radicale

**Storage Format:**
CalDAV/CardDAV server. Stores on filesystem but splits collections into multiple .ics files per calendar rather than one-event-per-file.
Storage backend does not produce standard single-event .ics files suitable for git.

**CLI/JSON Quality:**
No CLI. Server-only, accessed via CalDAV clients (like khal/vdirsyncer).

**nixpkgs:**
radicale 3.7.7 present.

**Maintenance:**
Active. Designed as sync server, not local vault storage.

## Desktop Notifications

Khal + vdirsyncer stack can integrate with:

- **libnotify + notify-send:** Desktop notifications via DBUS.
- **dunst:** Simple notification daemon.
- **systemd timers:** Schedule khal agenda queries via systemd.startAt.
- **cron:** Poll khal periodically.

Reminders require external daemon (not built into khal).

## Git Compatibility

**Standard .ics files in git:**
khal+vdirsyncer stores each event as a valid RFC 5545 .ics file, one per file.
Fully readable by other tools (Calendar.app, Evolution, caldav clients).
Git-friendly: one-event-per-file means clean diffs and merge-friendly changes.
Recommend: vdir (khal+vdirsyncer) for this exact use case.

## Verdict

khal + vdirsyncer is the presumptive answer for ~/personal/the-den/.
vdir format produces standard .ics per event, directly readable by other tools.
Agenda query: `khal list now 7days --json title start-time end-time`.
vdirsyncer entering maintenance; monitor pimsync progress for future migration path.
Notification layer requires systemd timer + notify-send wrapper, not included out-of-box.

## Sources

- [The Vdir Storage Format — vdirsyncer documentation](https://vdirsyncer.pimutils.org/en/latest/vdir.html)
- [Vdirsyncer and Khal — Srivathsan Murali](https://srivathsan.me/posts/2020/07/09/vdirsyncer-and-khal.html)
- [khal — GitHub pimutils/khal](https://github.com/pimutils/khal)
- [vdirsyncer — GitHub pimutils/vdirsyncer](https://github.com/pimutils/vdirsyncer)
- [pimsync: a successor to vdirsyncer — pimutils blog](https://pimutils.org/blog/2025-01-08-pimsync-a-successor-to-vdirsyncer/)
- [calcurse Documentation](https://calcurse.org/files/manual.html)
- [Radicale Documentation](https://radicale.org/v2.html)
- [khal Usage — Documentation](https://khal.readthedocs.io/en/latest/usage.html)
- [Desktop notifications - ArchWiki](https://wiki.archlinux.org/title/Desktop_notifications)
- [Systemd/timers - NixOS Wiki](https://wiki.nixos.org/wiki/Systemd/timers)
