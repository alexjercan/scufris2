# Changelog

All notable user-facing changes to Scufris.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Releases are
immutable `vX.Y.Z` tags; see [RELEASE.md](RELEASE.md) for the process.

## [Unreleased]

## [2.8.2] - 2026-09-11

### Changed

- A message sent from the phone or a desktop panel reads as the message. It
  arrives wrapped in the widget catalogue the model is offered, which was four
  thousand characters of schema around one sentence on the screen of a terminal
  holding the conversation; Calm now draws the sentence, and names any
  attachment. The wrapper is untouched in the session, in the model's context,
  and in the conversation the service replays, and `/calm off` still shows it.

### Fixed

- A job row that was filed stays filed after a restart. A terminal takes the
  conversation after its session has started, so the work it owns arrives with
  the lease; what was filed was read before that, against no jobs at all, and
  every filed row came back to the HUD. Filing them again wrote the empty set
  over what was there, so it never held. The holder now reads what was filed
  when the work arrives.

## [2.8.1] - 2026-09-11

### Fixed

- The background service starts again where the terminal handoff is turned on.
  The unit the Home Manager module writes sets
  `SCUFRIS_SERVICE_TERMINAL_LEASE=1`, which is the documented spelling, and the
  service read only `true` and `false` there: it exited before binding
  anything, restarted every three seconds, and every client saw a missing
  socket rather than a rejected value. `1`, `true`, `yes`, and `on` are all
  read now, and so are their negatives.
- A terminal that holds the agent finds the programs the agent runs. The
  terminal launcher carried `scufris-ctl` and Pi only, so the briefing failed
  at its first import, in a terminal and nowhere else. Both launchers now take
  one list of the agent's programs, and a check asserts they carry the same
  one.

## [2.8.0] - 2026-09-11

### Added

- A terminal can hold the conversation. `scufris-terminal` starts normal
  interactive Pi on a fork of the session the background service owns, takes
  the agent from it, and gives it back on exit; every surface keeps talking to
  the same conversation while it runs. Inside a checkout that carries the
  terminal extension, plain `pi` joins the same way with catch-up.
  `/scufris status`, `release`, `attach`, and `hold` control it from the
  terminal, and `scufris-ctl state` and `scufris-ctl lineage prune` control it
  from outside. The service refuses every request until
  `programs.scufris.service.terminalLease` is on, and it is off by default.
- `programs.scufris.desktop.speech.speakTerminal` reads out an answer a
  terminal asked for. Off by default, because the person typing is usually at
  the machine and reading it.

### Changed

- Surface and agent protocol 11 adds the terminal lease, turn correlation, and
  the agent holder in surface state. Host, agent, desktop, gateway, control
  client, and iPhone must update together.
- A delegated job is owned by the conversation rather than by a Pi session, so
  work keeps its owner across a handoff. The first foreground session after the
  update adopts the jobs the previous owner held.

### Fixed

- Scheduled briefing sources can call `scufris-den` from their explicit unit
  path without depending on an interactive login profile.
- Proactive briefing delivery survives settlement before Pi starts a queued
  message and deduplicates host redelivery across an agent reconnect. The
  safety circuit distinguishes a recurring logical run from an honest backlog,
  preserves measured summaries, and lets the next user turn retry stopped
  rows; desktop and iPhone show that recovery instead of active work forever.
- Briefing retention now fits every maximal row into the shared wire bound and
  can evict the oldest stopped row before refusing new work. Publication
  validates the run before taking a bounded lock and returns its canonical
  prose on retry, while reconciliation reports a bounded set of refusals.
- Filing a delegated-job row hides only that execution generation. Steering or
  restarting the logical job makes its new active generation visible on every
  surface without restoring the completed row that was acknowledged.
- A widget summoned after the desktop has no free edge now explains the
  capacity refusal in the HUD instead of writing it only to the journal.
- Job receipts now recognize normal ancestry, squash-equivalent trees, and
  direct-base or no-content reconciliation as landed. They retain the actual
  base revision for post-cleanup receipts, measure publication from a remote
  tag instead of treating any CI run as a release, and no longer read negated
  or attributive landing words as completion claims.

## [2.7.0] - 2026-09-10

### Changed

- Surface and agent protocol 10 adds a failed briefing-delivery state. Host,
  agent, desktop, gateway, control client, and iPhone must update together.

### Fixed

- Proactive event IDs now stay on the exact Pi turn that received them. The
  service backs off consecutive proactive turns and stops before a fourth turn
  until an explicit service restart. New ingress receives the same failed state
  while that circuit remains open.
- Briefing test fixtures pin progress announcements to a fixture control client
  instead of resolving and mutating a developer's live service from `PATH`.
- Briefing publication and delivery retries are idempotent across interrupted
  artifact, conversation, and inbox writes. Queue ingress, dispatch, and
  delivery now emit structured run, event, outcome, and pending counts.

## [2.6.0] - 2026-09-10

### Added

- Desktop and iPhone now show scheduled runs in one compact `BRIEF` drawer.
  Active runs stay visible; successful deliveries disappear; failed and
  measured-partial deliveries remain until one durable dismissal hides them on
  every surface without deleting their audit, answer, or artifacts.

### Changed

- Surface and agent protocol 9 adds strict `briefing.dismiss`. Briefing
  persistence format 2 keeps a bounded dismissed-ID set beside the 128-row
  audit and migrates format 1 with no dismissals.

## [2.5.0] - 2026-09-10

### Added

- Scheduled briefings now have durable lifecycle rows on desktop and iPhone.
  Collection progress, source counts, failures, and delivery state stay visible
  without speaking or starting a model turn. The rows adapt at narrow widths
  and expose one complete accessibility label.
- The service now owns a durable proactive briefing inbox. A terminal
  generation waits for an idle model slot, carries a stable correlation ID,
  and becomes delivered only after its answer is in canonical conversation
  replay. Pending and in-progress delivery recover across service and agent
  restarts.
- An instrument hangs from a corner and the next one on that corner hangs
  below it, so a side of the desktop holds as many panels as its height has
  room for. Four was the number of corners, and the fifth panel summoned from
  the tray was refused with `no_free_slot` where nothing could say so, because
  a summon carries no request ID. Every corner still takes its first panel
  before any corner takes its second, so four panels read as four places and
  not as two piles. Two of the tallest widget that ships still fill one side;
  four claude panels now fit where two did.

### Changed

- Surface and agent protocol 8 adds `surface.briefings`,
  `control.briefing`, `control.briefing_ack`, and proactive response
  correlation. Host, agent, desktop, gateway, control client, and iPhone must
  be updated together.
- Briefing collection and delivery are independent state machines. Filesystem
  notifications are only latency hints; session startup and a one-minute
  systemd reconciliation timer authoritatively replay all retained runs.
- One predicate now decides whether a worker's summary is fit to store or
  print, and it is asked at all four doors that used to decide it differently.
  The single-byte CSI U+009B is not under 32, so it was stored and every
  reader downstream had to escape it forever, and U+2028 ended a line for a
  JavaScript reader but not for a Python one. Text a worker controls is
  refused at the door rather than escaped on the way out, so a summary that
  reaches the record is one every reader can show.

### Fixed

- A scheduled briefing can no longer stay `collecting` forever after its
  collector cgroup fails. Each source contribution is stored before completion
  is announced, and a generation-fenced `OnFailure` unit runs outside that
  cgroup to retain completed sources and mark the rest failed, including after
  an OOM kill.
- Briefing configuration, manifests, contributions, prose, and child output
  are bounded before they are consumed. Devices, FIFOs, unsafe artifact
  symlinks, and oversized files are refused. Home Manager's generated config
  symlinks remain supported after their opened target is verified as a bounded
  regular file.
- A terminal briefing no longer races an active user turn or disappears while
  no agent is connected. User text is refused and retained while the one
  proactive slot is active, duplicate terminal ingress is idempotent, and the
  crash window between canonical replay and inbox acknowledgment cannot create
  a second visible answer.
- A held exhibit no longer keeps its hold after its backend dies. The dead
  panel stayed on the shelf and crowded out live ones. Releasing a hold now
  restarts the panel's age too, so an exhibit held for hours is not
  immediately as old as its hold, while a panel that was never held keeps
  aging across a feed and stays the right one to retire.
- The three usage backends no longer drift stale in normal operation. Each
  slept for exactly the ceiling its widget advertises, so a reading always
  arrived after the ceiling and spent its whole staleness budget on the sleep
  alone. The ceilings move below the cadence they must fit inside: 180 to 150
  seconds for claude and codex, 3 to 2 for system. The widget descriptions
  move with them, because those are what the model reads to decide when a
  number is old.
- A den value written through `normalize_*` can no longer enter the day file
  as a Markdown heading. A leading `#` passed the plain-text guard, and the
  next read parsed it as a section the writer never made, so everything after
  it landed under a heading that is not real. The den backend's `suggest` also
  reports an unreadable directory or a malformed date as a refusal instead of
  raising through the surface.
- The briefing unit has a memory bound. It had a time bound and none for
  memory, so one runaway read grew until the kernel stopped it, and because
  the collector shares a control group with its sources, systemd stopped all
  of them. `MemoryMax` stops the one process that is wrong before the machine
  notices, and `MemoryHigh` throttles it first so an honest peak is slowed
  rather than killed.

## [2.4.1] - 2026-09-09

### Fixed

- Filing a job row now stays filed. The set of filed rows lived in memory
  only, and the service starts Pi with `--continue`, so `recover` handed back
  every job that had never been stopped or landed and every row Alex had
  cleared came back at the next restart. An acknowledgement a restart forgets
  is not one. The filing is written into the session beside the wake mode, and
  restored once the recovery says which jobs still exist, so an id belonging
  to a job that has since been landed is dropped rather than carried forever.

## [2.4.0] - 2026-09-09

### Added

- An answer about a delegated job carries that job's receipt. The badges are
  drawn at the foot of the message, grouped under the job they are about and
  led by its ID, so nothing has to parse the prose to know what a fact belongs
  to. A badge is measured, refuted, claimed, or unknown. No model writes one:
  `workflow/citation.ts` reads what the jobs helper measured and maps it, so
  the four words mean the same thing on every surface and a claim the worker
  made is visibly a claim. `unknown` is a fact nobody could measure and is
  never drawn as a no.
- The next thing to do is a button. An answer may attach up to two offers to a
  job it reports on, each a short label over a prompt the model composed. The
  words never cross a socket: the extension keeps the prompt and the surface
  sends only the offer's ID, so no surface control can put a sentence into the
  conversation. Taking one runs it as a follow-up rather than as a line Alex
  did not type, and the offer stays visible and goes quiet.
- Delegated work has a list, and a row outlives its job. Every surface holds
  up to eight rows carrying the job ID, project, state, age, and summary. A job
  that finishes overnight is still on the list in the morning, and `archive` is
  what clears it - acknowledgement, not timing. `cancel` stops a job and keeps
  its unmerged branch. On the HUD the stop control arms first and asks once,
  because stopping the wrong job costs an hour of an agent's work.

### Removed

- `agent.state` and its attention notice are gone. A job that wants attention
  is a row, and the one thing the notice reported that is not a job - a failed
  event drain - is now a row too, which archiving acknowledges.
- A briefing source no longer declares a `policy`. Every source runs with every
  tool its harness has. The ladder read as a guarantee it could not make:
  `bash` was in the reader's list as well as the repairer's, so `read` never
  meant read, and the names described an intention while the flags described
  something else. A source that changes files also has nothing behind it -
  receipts are attached to jobs, which a request owns and a person can land,
  and a schedule owns a source. A `policy` still written in a `.scufris.toml`
  is an unread keyword and costs nothing.

### Changed

- Surface protocol 7. The desktop, the iPhone app, and `scufris-ctl` are
  replaced together, as every protocol change here is: a version is refused,
  never negotiated.
- The tray word is folded from the job rows on the host rather than sent as its
  own field, so `failed` holds until the row is filed instead of clearing when
  a process happens to exit.
- Scufris reviews its own day. `scufris2` declares a `nightly` briefing source
  that groups the day's commits, runs `/scufris-review` over one group at a
  time and reports what is worth fixing, without changing anything. It is the
  one source here that runs on `claude`: the night's method is a review panel,
  and a source runs with no extensions, which is where pi keeps subagents. The morning
  briefing in both reviewed projects reads the night's run and its task, and
  carries a finding that still stands into a numbered offer.
- A source is told plainly that it holds every tool, that nothing is withheld
  from it and nothing is watching, and that its guidance is the whole of its
  permission. A boundary a model can read is worth more than a flag that
  suggested one it never had.
- Every refusal code the sockets carry is named once, in
  `shared/control/src/refusal.rs` and its TypeScript mirror, instead of being
  written as a string literal at each of the twenty-one places that send or
  match one. Nothing user-facing changed; a code misspelled on one side is now
  caught by a test rather than by a match that silently stops matching.

### Fixed

- The briefing runs again from a deployed Scufris. Rendering a page needs
  markdown-it-py, and the launcher put a plain `python3` on the agent's PATH,
  so every briefing tool failed at its first import and only there. The
  interpreter the helpers run under is now written once, in `nix/python.nix`,
  and the launcher, the staging run, the packaged command and the development
  shell all take it from that one place.
- A delegated worker finds its own job again when tmux is already running.
  A tmux session takes the server's environment, and the server belongs to
  whoever started it first, so a staging run beside a developer's own tmux
  launched its workers into the wrong answer to where jobs are kept: the
  worker died on the first artifact it looked for, saying only `job artifact
is unavailable: job.json`. Every variable that says where things are is now
  pinned onto the session the worker is started in.
- A job row no longer draws its project over the state beside it. A project is
  a relative path, so a name wider than its column overlapped what came next;
  every cell in a row is now clipped to its own column, with the whole of it
  in the title.
- A briefing that is still collecting now names the sources it is waiting on.
  The manifest carried an empty `sources` until the last source returned, so an
  eight-hour night in flight read exactly like a night nothing had declared.
- A source that is cut off at its deadline keeps what it had already written.
  An eight-hour night that answered slowly used to leave nothing at all; if the
  answer was complete when the clock ran out, it is now read as the answer.

- Cancelling a Quick Review while it starts no longer wedges Scufris. The
  helper's cancel path set the flag that suppresses its own crash report, and
  the exit handler then declined to settle the promise the caller was parked
  on, so `ready` could never resolve for a process that had already gone. Pi
  awaits a tool without racing the interrupt signal, so the whole agent loop
  stopped and only killing Pi recovered it: pressing the interrupt key was
  what wedged it. A close before the agent is ready now settles as a
  cancellation.
- An answer is no longer thrown away because of what was attached to it. A
  widget call naming a widget the surface never registered, or an attachment
  id whose file had expired, refused the whole response - the prose, the
  details and everything else went with it. The turn had already ended on the
  agent's side, nothing reads that refusal and retries, and no screen said
  anything, so Alex was left looking at his own question with nothing under
  it. The offending call or attachment is dropped and the answer is recorded.
  The agent is still told what was wrong, because a mistake it can fix should
  be visible somewhere; but a widget is presentation and an attachment is not
  the answer, and neither is worth the words. This is what the documents
  already promised: both call widgets best-effort.
- The agent no longer sends the host a message the host will refuse. The
  encoder checked length alone, while the host also rejects NUL, carriage
  return, and a value that trims to empty - and it answers an invalid
  submission by closing the agent connection without a refusal. One carriage
  return from a worker's captured output, in a state detail that was never
  validated at all, was enough to drop the answer in flight and tear down the
  channel with nothing but a journal line. The encoder now holds the host's
  rules, and a state detail is clamped to fit rather than lost, so a long
  error message still reaches the surfaces trimmed.
- A response the agent cannot send is reported instead of vanishing. The
  channel dropped it silently whenever the socket was between reconnects, and
  the attention state was never republished afterwards, so a job that failed
  while the socket was down left every surface reading clear until some other
  job changed it. State is now republished on every completed handshake.
- After an action, only `scufris_job_inspect` on the job that action named is
  blocked, rather than inspecting any job and listing the fleet. "Steer this
  one and tell me what the reviewer on that one found" was one request that
  lost its second half, and `scufris_job_list` reads memory and cannot wait,
  so refusing it only cost "start that job and show me what's running".
- Scufris starts everything a request asks for, rather than stopping after the
  first action. A successful spawn, steering, stop, landing, or review armed a
  gate that blocked every tool except the final response, so a second thing
  named in the same message - or an instruction typed while the first was still
  starting - could only be answered with an intention, and there is no later
  turn for an intention to be finished in. Asked to run a production job and
  then, moments later, to open the brief, Scufris started the job and answered
  "I still need to open today's brief in a separate action"; opening it took a
  second message. The gate now refuses `scufris_job_inspect` and
  `scufris_job_list` alone, which is the behaviour it was built to prevent: a
  foreground turn spent watching a job that reports on its own schedule and
  cannot be hurried. Shell `sleep` and `wait` are refused exactly as before, by
  a separate guard that never depended on the gate. One tool batch still holds
  one meaningful action, and a turn still ends with one acknowledgment; a turn
  may now hold as many batches as the request has things to start.
- An answer whose surface has disconnected reaches the screens that are still
  connected, rather than reaching nobody. The response association named
  whichever surface had spoken most recently and was never cleared, so one
  message from a phone owned every answer after it: a morning briefing hours
  later was attributed to that phone, found it gone, and was refused with
  `surface_unavailable` instead of being shown anywhere. The association now
  covers one turn. It opens on an accepted surface message and closes when that
  turn's answer is recorded or its abort is accepted, so an answer with no turn
  outstanding - the ordinary case for a briefing, however recently a surface
  spoke - is `unprompted`, which every screen shows and none speaks. An owner
  that left before its answer arrived is recorded the same way with its widgets
  stripped, because a widget belongs to the surface that asked and there is no
  such surface. A refusal is not an answer and leaves the turn open, so an agent
  can correct invalid widgets and still reach the owner that is waiting. An
  owner's outstanding turn still wins over a wake that lands beside it: the
  owner asked first and is waiting, and returning their own question to them
  silent on every screen would be the worse trade. `surface_unavailable` is
  gone, and `SERVICE_VERSION` is unchanged, so no surface has to move with this.
- One attachment no longer takes the whole assistant down. A single unreadable
  metadata file made every upload, every read and every send fail from then on,
  because the store loaded its index by reading every record and gave up on the
  first one it could not read. A store at its quota refused every upload rather
  than expiring the oldest, a failed write left its temporary behind to be
  counted against the quota forever, an oversized message came back as a plain
  refusal with no size in it, and a truncated upload was reported as a server
  fault. Each record is now quarantined on its own, the quota expires before it
  refuses, temporaries are removed on every path, and the gateway distinguishes
  a message that was too large from one that arrived incomplete. A
  `Content-Type` carrying a charset - which is what a browser sends - is no
  longer read as an unknown type.
- A worker event that cannot be delivered is now recoverable. The drain caught
  its own failure, remembered it, and never tried again, so every later event
  from every job was stuck behind it and the only recovery was restarting
  Scufris. The failure is reported once as an attention notice, and the turn it
  wakes is what retries. A notice is cleared when its job lands, is stopped, or
  is forgotten, and a job recovered at session start brings its notice with it,
  so the tray stops saying a job needs attention after it has been dealt with.
- The tray says something when a job is blocked or has failed. Both states fell
  through to the same grey "idle" as a companion with nothing to do, which is
  the one thing they are not. The desktop pill and its sound follow.
- A briefing that fails says so. A run where every source failed reached
  nobody: the wake refused the state, the session-start read left it out, and
  the unit exited 0, so one source failing out of five was reported and five
  out of five was silence. A failed run is now carried to the conversation and
  closes the same way a collected one does. The nightly is no longer lost
  either: it collects at 23:00, and a session opening the next morning reads
  yesterday's date as well as today's, which is the only thing that would ever
  have found it. A collection that fails after `scufris_briefing_run` has
  already answered now reports itself instead of leaving Alex waiting.
- A briefing source is told it gets exactly one turn, so a source that ended a
  turn intending to continue is no longer recorded as having failed to answer.
- One over-long worker summary no longer wedges every job. The event file was
  rejected whole, so nothing after that line could be read from any job until
  the file was edited by hand; the summary is now bounded at the door it is
  written at, in bytes, in all three places that write one. A report file at
  its ceiling keeps its history instead of being replaced by the newest entry
  alone. `scufris_job_inspect` shows the current run's events rather than every
  run's.
- A landing that Sprout refuses no longer traps the job. The cleanup intent was
  made durable before the refusal could be seen, so the job could be neither
  landed nor abandoned afterwards; the guard now runs first, and `stop` can
  re-decide a workspace removal the same way it can re-decide an abandonment. A
  subject mismatch names the subject that was recorded.
- The composer no longer eats a message it could not send. Words and every
  attached file were cleared the moment Enter was pressed, so a submission
  refused before it left - the service restarting, a message one byte over the
  bound - took them with it. They come back in the box with the reason.
- The desktop no longer refuses to start because of one unreadable file. A
  crash between creating `surface-id` and its bytes reaching disk left a
  zero-length file that made the companion exit on every launch, for good, and
  the file survives the reboot that caused it. The write is crash-safe and an
  unusable file is replaced. "Restart backend" past its budget says why instead
  of doing nothing.
- The den refuses text that would rewrite the journal. A task, idea or note
  carrying a newline wrote whatever followed it as journal structure: an
  embedded `### Habits` made a second Habits section, hid the real one from
  every later write, ticked a habit nobody made, and dropped the rest of the
  sentence, all with a success message. Ticking a habit by name no longer ticks
  a different one - `Work` matched `Deep Work` - and an ambiguous name is
  refused. A section header that is a file's last line survives the next write,
  a day written before a section existed takes a write into it, `--json` is
  honoured by every read, and a den on a full or unreadable disk refuses one
  click instead of killing the panel.
- A widget panel says what went wrong instead of going quiet. A backend handed
  a spawn payload it did not expect died before its first reading and was
  restarted into the same death; a widget that threw left a panel that rendered
  nothing forever; a dead panel that had also dimmed drew its numbers at an
  unreadable 14% opacity; a click on a panel whose backend had gone did nothing
  and said nothing; and the paperclip at eight attachments did nothing with the
  reason unreachable. Sampling intervals are held to what the panel's own
  staleness tolerance allows, so a panel no longer wears a STALE badge over
  numbers that are current. Widget window sizes are bounded, as the
  documentation always said they were. On the way out, backends that ignore
  SIGTERM are now actually killed rather than left running until the machine is
  rebooted.
- A worker that never reached its harness now says so. `scufris-jobs launch`
  runs inside the tmux pane, and everything it could refuse before the harness
  started - a missing executable, a workspace that moved - printed into a pane
  nobody watches and exited. The job stayed `running` with nothing after
  `worker starting`, so the foreground read it as a worker still thinking and
  the only way out was noticing by hand and restarting it. Such a launch now
  publishes a `failed` event with the reason.
- A timer stays for as long as it counts. A widget Scufris opens goes about a
  minute after the conversation moves past it, which is what makes it something
  shown rather than something kept - but a countdown that has not run out has
  not moved on, so a timer set for longer than the conversation took vanished
  mid-count and the only timer worth setting was one nobody needed. A backend
  can now say it is still working, and the panel stays while it does.
- A clipboard that refuses no longer looks like it worked. Copying is what the
  pill offers for a transcript whose outcome nobody knows, and the page threw
  the refusal away: a person told the words were safe closed the pill and pasted
  something else. The refusal is said beside the words.
- A service that has stopped trying says how to start it again, instead of
  leaving the reader to know that the tray has had "Restart backend" all along.
- Scufris says when worker panes from a previous foreground session are still
  running. It could not see them and cannot stop them, so it names the tmux
  session each one can be reached by.
- A briefing asked for by hand is held to the same numbers as the one its timer
  starts. A profile's deadlines and width reached only the run the timer
  started, so a nightly that allows a source eight hours was silently cut at
  fifteen minutes and the work was lost.

## [2.3.0] - 2026-09-08

### Added

- A briefing source declares a `policy` saying what it may do: `read`, the
  default, reports and cannot change anything; `review` adds the subagents and
  the commands a review panel needs and still cannot write; `repair` adds the
  edit tools. It is a name rather than a tool list because the flags belong to
  the harness and the same guidance should run under either one. A policy the
  reader does not know is refused by name, because a source that meant `repair`
  and wrote `repairs` would otherwise run as a reader and report that it fixed
  nothing.
- A profile sets the bounds its own briefing needs, rather than inheriting one
  set written for a morning: `sourceDeadline` for how long one source may take,
  `parallel` for how many run at once, `maxOffers` and `maxBody` for how much
  one may report. The briefing-wide `keepDays` sets how many days are kept.
  A source is held to `sourceDeadline` or to whatever is left of `deadline`,
  whichever is smaller, so a profile that wants a long source raises both.
  Anything unreadable reads as the default: a briefing that refuses to run over
  a typo in a unit file is worse than one held to its own numbers.
- A briefing ends with a numbered list of things to do next. Each source may
  offer at most three, each a short label saying what to do and a detail saying
  what and why, and collection merges them across sources into one list
  numbered in source order. The numbers are assigned by code, stored with the
  run, and never reassigned, so picking one by number hours later resolves from
  the file rather than from what the model remembers saying. Only the source
  that read the project can propose from it, which is why an offer is written
  there; merging is concatenation, so no model is in that seat. An offer is a
  label and a detail and nothing else: a stored prompt for a worker would only
  fit a delegated coding job, and a briefing about a calendar or a house has a
  next step too.

## [2.2.0] - 2026-09-08

### Added

- `scufris-jobs` is an installed program. A briefing source that reports
  on jobs names it rather than a path into a checkout, and it is on the
  PATH of every briefing timer's run.
- Briefing profiles on systemd timers.
  `programs.scufris.agent.briefing.profiles` names a briefing and its
  `OnCalendar` schedule, and each one renders its own
  `scufris-briefing-<profile>` timer and oneshot service. A schedule is
  checked with `systemd-analyze calendar` while the module is built, so a
  schedule nobody can act on fails the build rather than the morning.
  `Persistent = true` collects once at the next login when the machine was off
  at the scheduled time. `{}` schedules none.
- `scufris-briefing wake` carries a gathered run to the conversation, and
  `scufris-briefing pending` lists the runs for a day that still need their
  prose. Every subcommand takes `--profile`.
- Unprompted wake ingress: `scufris-ctl wake "<text>"` delivers a proactive
  message to the foreground conversation from outside the agent process, so a
  timer or a finished job can reach it with words. The wake is not recorded as
  a user message, is not echoed to any surface, and does not change which
  surface an answer is attributed to. With no agent connected it is refused
  with `agent_unavailable` instead of being dropped. The verb lives on the
  local control socket only and is unreachable from the remote surface
  gateway.
- Briefing sources declared for the machine, in one user-level file at
  `$XDG_CONFIG_HOME/scufris/config.toml`. Its `[briefings.<profile>.<name>]`
  sections are sources that belong to no checkout, such as one reporting what
  Scufris did overnight. They are ordinary sources: the same entry, the same
  reader, the same envelope, deadlines, repair and page. An optional `root`
  says where one runs; without it, the home directory. The file is
  briefings-only, and a project keeps declaring its own briefing in its own
  `.scufris.toml`. `programs.scufris.agent.briefing.sources` generates the file
  from a typed option, so a malformed entry fails the build; the helper reads a
  TOML path and anyone else writes the same file by hand.
- `scufris-briefing --config` and `SCUFRIS_CONFIG` name another user-level
  file, the flag winning. A file either of them names and is not there is a
  refusal; the default path being absent is a machine with no sources of its
  own. A malformed file costs itself only: one diagnostic naming it, and every
  project still contributes.
- `scufris-jobs history --since <moment>` lists job records including archived
  ones, each with the receipt measured about it. Every other listing skips the
  archive and archiving is what happens to a workflow that finished, so nothing
  could answer what landed.
- Job receipts: measured git, remote, and CI facts for one job, appended to
  `receipts.jsonl` and returned by `land`, `stop`, and `inspect`. Scufris
  measures on every terminal event, so a completion claim about landing,
  pushing, tagging, or releasing is checked instead of repeated. A fact that
  could not be measured is reported as unknown with its reason, never as a no.
  An unbacked worker claim is said as "claimed, not verified", and unlanded
  work is said as "not landed".

### Changed

- Briefing runs are keyed by date and profile:
  `$XDG_STATE_HOME/scufris/briefings/<date>/<profile>/`. Two profiles on one
  date used to be one directory, and the second collection wrote over the
  first. Old single-profile runs are not migrated; they age out with the last
  thirty dates.
- A briefing tool call takes a profile, and the wake names the one it was
  collected for. Publishing without a profile resolves only to a run that was
  gathered and never written up, so a wake can never put one briefing's prose
  on another's page.
- A session no longer holds a briefing timer. It reads once, at session start,
  for a run that was gathered while nothing was connected, and asks for the
  writing. A wake refused with `agent_unavailable` therefore leaves the run
  gathered instead of losing it.
- Service protocol version 6 replaces 5 without negotiation, adding the
  `control.wake` and `agent.wake` messages. Service, agent, gateway, and every
  surface must be updated together.
- Stopping a job with `remove_workspace` now keeps a branch that was never
  merged. Deleting one needs an explicit `abandon`, so unlanded work is no
  longer lost to a cleanup.
- Every briefing source is told when its own profile last began a run, read
  from the runs on disk. A weekly source reports on a week and a morning source
  on a night, with no window setting anywhere. The start and not the finish:
  the last run's sources were asked at about its start, so measuring from its
  finish would leave everything that happened during a collection reported by
  neither briefing. It is a fact and not an instruction: guidance that names
  its own window keeps it, and the first run of a profile says there was no
  previous one rather than leaving a model to invent a period.

### Removed

- `programs.scufris.agent.briefing.time` and `SCUFRIS_BRIEFING_TIME`. The
  schedule is a systemd timer now, so it is removed with a message rather than
  renamed: the option's type is an attribute set of profiles and cannot hold a
  time of day. `SCUFRIS_BRIEFING_PROFILE` is gone with it; the unit names the
  profile on the command line.

## [2.1.7] - 2026-09-07

### Changed

- Desktop and iPhone final-response `details` now render full selectable
  Markdown with Gruber styling. Required `text` stays literal, with only safe
  bare HTTP(S) URLs autolinked. Raw HTML, unsafe URL schemes, and other unsafe
  content remain inert.

## [2.1.6] - 2026-09-06

### Fixed

- The latest 200 canonical conversation messages now survive a background
  service restart or Home Manager switch. Scufris stores one private atomic
  replay snapshot under its XDG data directory and safely isolates malformed
  or incompatible snapshots instead of failing startup.

## [2.1.5] - 2026-09-01

### Changed

- Desktop and iPhone conversations keep message bodies in a consistent layout,
  separate message runs clearly, and follow new messages only while the reader
  remains at the end. When the reader scrolls up, an accessible down-arrow
  control reports waiting messages and returns to the latest one.

## [2.1.4] - 2026-09-01

### Fixed

- Desktop widget scrollbars no longer overlap row actions in long agenda,
  notes, food, or workout lists.

## [2.1.3] - 2026-09-01

### Fixed

- Delegated Pi workers can start from packaged Scufris resources. The launcher
  now finds the dedicated `scufris_report` extension in both packaged and
  source-tree layouts instead of leaving a dead pane with a working job.

## [2.1.2] - 2026-08-31

### Added

- A source that answered badly is asked once more. It has already read its
  project by then, so the second asking hands back its own words and the one
  reason they could not be used, with no tools at all, and it may change only
  that. A real 4507 character answer lost to a stray quotation mark came back
  in 26 seconds against the 197 the first run cost. Bounded by
  `SCUFRIS_BRIEFING_REPAIR_DEADLINE` and never past what the source has left.
  A source that never answered is not asked again.

### Fixed

- Nothing a source does can end a run. A malformed answer is refused by name
  rather than raised, including one nested past the decoder's stack or written
  in bytes that are not text; a source that finds a way past that is caught and
  named; a contribution that cannot be written costs one source and not the
  morning; and a page that cannot be laid out leaves the collected run
  standing, with the reason kept in the manifest.

## [2.1.1] - 2026-08-31

### Fixed

- A briefing source can run the commands its own guidance names. `claude`
  sources ran sandboxed, so a project told to read CI or refresh its numbers
  reported that `gh` or `python3` had been denied instead of reporting itself.
  Both harnesses now answer without asking, and what a source may reach is
  decided by its tool list and its guidance.
- A contribution whose body fences code of its own is read whole. The closing
  fence was matched against the first ``` inside the answer, which cut the
  envelope off mid-string and lost the whole contribution.
- A source is told the limits it is held to. Title, headline, fact, and body
  lengths are in the prompt, so an answer is no longer dropped for a limit it
  was never given.
- A session no longer waits for the morning before the surfaces can reach it.
  The briefing was collected inside session startup, and pi runs the startup
  listeners one after another, so every surface was left with an agent that
  could not answer for as long as the sources took. The collection now starts
  with the session instead of holding it open.

## [2.1.0] - 2026-08-31

### Changed

- An answer nobody asked for is displayed instead of refused. A morning
  briefing or a finished job that speaks before the owner has used any surface
  is shown on every surface, against a reserved name that none of them holds,
  so it is never spoken aloud and never runs a live widget call. It used to be
  rejected, which meant the first briefing after a restart reached nothing and
  was not kept in the conversation either.

### Added

- An unprompted morning briefing. A project declares `[briefings.morning]` in
  its own `.scufris.toml` and is asked once a day for one bounded, evidence
  based contribution. Scufris writes them up in its own voice, says it in chat,
  and keeps the run under `$XDG_STATE_HOME/scufris/briefings/`.
- `scufris-briefing`, the same collection and rendering from a terminal, and a
  read-only page for the day beside the prose. The page is rendered from the
  finished run rather than generated a second time, so it cannot say anything
  the briefing did not. It is written as soon as the sources answer, so the day
  has one whether or not it is written up. It opens when it is asked for and
  never by itself.
- `programs.scufris.agent.briefing.time`, the local morning the briefing is
  assembled, or `off`. A session that opens later in the day catches up once.
- The-den journal is read and written inside Scufris. `tools/den/den.py` is the
  whole format - days, backlog, and the food database - compiled into the
  desktop's `den` backend and run by the new `scufris-den` command.
- `scufris-den`, a non-interactive command line over the journal, plus a `den`
  agent skill, so the agent reads and writes the day by the same rules a panel
  does.
- A backlog: ideas with no day yet, kept in `Backlog.md` at the top of the den.
  The agenda panel keeps one and pulls one onto the day that is showing.
- Restant on the agenda: what was left undone on earlier days, bounded by a
  horizon of 60 days by default and marked on the month with everything else.
- A workout log. A `### Workout` section names the day's split on one line and
  holds `exercise,weight,reps` rows under it. The macros panel shows the split
  as its heading and the sets grouped by movement with the day's volume. One
  exercise is one question: the sets go in as `60x8 60x8 60x6` and the split is
  asked for once a day. A movement is edited the way it is drawn - clicking one
  asks for its sets back, and what is typed replaces them, an empty answer
  removing the movement. An entry written before the section existed gains it on
  the first write.
- Removing and correcting from the panels. A task, an idea, a note, a food row
  and a movement each carry a red `x`; a food row and a movement open the box
  that wrote them when clicked. Hovering a line shows the whole of it, which a
  panel three hundred pixels wide would otherwise cut. The same correction is
  `scufris-den macros edit` from the command line.
- Two databases in the den itself. `Exercises.csv` holds `split,exercise` rows
  and is what the panel offers under the exercise field before a movement has
  ever been trained - what was trained recently comes first, then what the
  database knows, and the day's split puts its own movements at the top.
  `Foods.csv` is the food database, which the den now answers for before
  Neovim's copy.
- A widget backend may name libraries in a `prelude` file, which `build.rs`
  compiles in ahead of it.

### Changed

- The agenda, macros, and notes panels no longer run the `today` command. They
  read the journal in their own process, so a panel no longer depends on what
  is on the path and a click costs a read rather than a process.
- The macros panel shows one whole day - eaten, weighed, and lifted - and is
  taller for it. The agenda holds its two new lists in the frame it already
  had, which is the tallest a pair of panels on one edge can be.
- `MACROS_DATABASE` and `desktop.widgets.macrosDatabase` are unchanged, but the
  default moved: the den's own `Foods.csv` answers first, and Neovim's file is
  read only when the den holds none.

### Removed

- **(breaking)** `programs.scufris.desktop.widgets.todayCommand` and its
  `desktop.todayCommand` alias, with `SCUFRIS_TODAY_COMMAND`. There is no
  command to point at. `desktop.widgets.denPath` and `DEN_PATH` are unchanged
  and still say where the journal is.

## [2.0.0] - 2026-08-30

### Added

- Managed attachments across the service, agent, desktop, and iOS. Messages
  carry bounded opaque references while the service owns private durable bytes,
  canonical metadata, quotas, retention, and replay.
- Authenticated upload, download, HEAD, and single-range attachment transfer on
  the existing private surface gateway.
- Native document and photo selection, protected downloads, Save to Files, and
  inline image and video thumbnails on iOS.
- Native desktop file selection, atomic private saves, inline raster images,
  extracted video thumbnails, and safe media preview behavior.
- An orchestrator-only `store_attachment(path)` tool for delivering generated
  files through canonical responses.
- Staging-only offline Swagger and OpenAPI documentation for the remote gateway.

### Changed

- Surface protocol v5 replaces v4 without negotiation. Service, gateway, agent,
  desktop, and iOS must be updated together.
- Desktop and iOS attachment presentation uses prominent Save actions and
  thumbnail-driven native previews. iOS also has larger accented controls and
  interactive keyboard dismissal.

### Fixed

- Empty attachment arrays omitted by canonical serialization now decode as
  empty arrays at the agent boundary.
- Bot-generated videos retain recognized media types. Existing opaque video
  descriptors use a conservative filename-extension fallback.
- Desktop video thumbnails avoid WebKit custom-scheme media playback and its
  blank conversation-window failure.

## [1.1.1] - 2026-08-30

### Fixed

- Starting or restarting the background service now also starts its enabled
  remote-surface gateway, so Home Manager activation cannot leave the private
  API route without a loopback backend.

## [1.1.0] - 2026-08-30

### Added

- A native SwiftUI iOS surface delivered through TestFlight. It stores its
  stable identity, WSS URL, and bearer token in Keychain, reconnects with
  canonical replay, and submits text to the shared conversation.
- An optional loopback-only `scufris-surface-gateway` serves an authenticated
  HTTP and WebSocket API. It bridges strict protocol-v4 surfaces and forwards
  bounded private audio transcription to host `ai-tools-api`. Its Home Manager
  option also owns a declarative Tailscale Serve route that provides private TLS
  without exposing agent, control, or inference listeners.
- iOS hold-to-dictate records up to 60 seconds locally, transcribes on the
  private host, and returns text to the editable composer for explicit send or
  discard.

### Changed

- Home Manager now treats `programs.scufris.agent` as the core interactive
  launcher that the optional background service also runs. API process
  ownership and the shared machine endpoint are top-level under `aiToolsApi`,
  while the desktop can override its consumer base URL.
- Desktop speech exposes model and voice. Transcription exposes model and
  language and always derives its route from the desktop API base URL.
- Desktop controls are named `popupKey`, `backgroundKey`, and `abortKey`.
  Terminal integration is `terminalCommand`, and journal-backed settings are
  grouped under `desktop.widgets`.
- `staging up` and `staging backend` now start an isolated external-surface
  gateway. They can own an exact temporary Tailscale Serve path and remove only
  that path during teardown.
- The iOS surface now follows the dark terminal interaction study:
  route and state header, speaker-column transcript, inline details, and a
  compact bottom composer. Its next TestFlight marketing version is 1.1.0.
- Desktop and iOS conversation views show one transient `thinking...` row while
  the service reports working. It clears on the final response or another state
  and never enters canonical replay.

### Removed

- The Home Manager transcription endpoint override. Compatible routes are
  derived from `desktop.aiToolsApi.baseUrl` instead.

## [0.6.0] - 2026-08-30

### Added

- Strict protocol v4 with separate surface, agent, and control sockets. Several
  desktop surfaces can share one canonical 200-message conversation, reconnect
  with stable identities, and receive replay without repeating speech or widget
  effects.
- Named split staging commands: `nix run .#staging -- backend` and
  `nix run .#staging -- frontend NAME`. Each frontend has private state,
  identity, data, command socket, lock, and teardown.
- Structured backend and desktop logs. INFO records major lifecycle events;
  DEBUG adds typed payload and routing detail for local diagnosis.
- A pinned `ai-tools-api` v0.1.1 package and app. The Home Manager module reuses
  an enabled shared provider, can consume an explicit external endpoint, or can
  run one hardened fallback service.

### Changed

- Protocol v4 replaces protocol v3 outright. The service owns canonical
  conversation history, the agent emits atomic final responses, and only the
  live originating frontend performs speech and widget presentation.
- Speech inference now uses the bounded OpenAI-compatible transcription and
  speech routes on `ai-tools-api` port 10300. Scufris retains recording,
  validated WAV playback, mute, and cancellation.
- Home Manager now groups the supervised agent under `service.agent`, desktop
  speech under `desktop.speech`, and speech-to-text under
  `desktop.transcription`. The former option paths remain deprecated aliases
  for one release.
- Source and package layout now follows agent, host, shared protocol, and
  surface ownership boundaries.
- The isolated staging stack still runs with `nix run .#staging -- up`; it uses
  a deployed shared API by default and supports explicit managed API mode.

### Removed

- Direct Scufris ownership of Whisper, Piper, their models, patches, packages,
  and user services. One `ai-tools-api` deployment now owns inference per
  machine.
- Protocol v3, RPC prompt ingress, dynamic model-defined widgets, spoken-event
  routing, response detail artifacts, and their compatibility paths.

## [0.5.0] - 2026-08-29

### Added

- Ambient tray notices for unattended jobs. A blocked job paints the tray
  wisteria and a failed job paints it red until that job reports progress or
  completion. Notices are kept independently by job identifier in the service,
  so one job cannot clear another and a companion that connects later receives
  everything still waiting.
- The conversation window. The pill says what Scufris is doing and could never
  say what was said; this draws it, and gives you a line to type on. Click the
  pill to put the window up, and click it again to put it away - the pill's one
  pointer gesture. Bind `scufris-ctl hud` to a key for the same thing.
  `Enter` sends, `Shift+Enter` starts a new line, `Escape` closes it. The tray
  shows it too, on a left click and from the menu. It holds the same last two
  hundred lines the service keeps, so everything said is there whoever said it
  and however it was sent. `scufris-ctl debug` in a terminal is still the
  deeper tool and is not a fallback for this: it is a whole Pi session, and the
  window is the last few lines and a place to answer them.
- `scufris-service`, the headless half of Scufris. It supervises one
  `pi --mode rpc` agent, owns the session directory, and serves one socket that
  every surface connects to. It builds and runs with no graphical dependency,
  so a machine with no display keeps the conversation. Off by default;
  `programs.scufris.service.enable` gives it a systemd user unit of its own.
  See the [background service](docs/src/dev/service.md) chapter.
- `scufris-ctl send`, `state`, `watch`, `abort` and `debug`, which reach the
  background service from any terminal. `debug` takes the agent away and opens
  its session where you are; closing the terminal gives it back, so there is no
  way to be left detached with nothing to put it back. `scufris-ctl` is now its
  own package, installed by whichever half of Scufris you enable.
- The service says so when the agent it started never connects back to it. Such
  an agent still holds a conversation, because the service reads Pi's own
  events, but it can report nothing it said, nothing to speak, and no widget,
  which looks exactly like a broken speaker. The usual cause is a `scufris` from
  somewhere else on `PATH`, built without the service extension, so the warning
  names the binary it started.

- Native widgets. Scufris can open a small panel on the desktop while it
  answers: an exhibit on a shelf above the pill, which ages out on its own, or
  an instrument in one of four edge slots when you ask to keep it. A widget
  window never takes the keyboard. The `widgets` extension registers
  `scufris_widget_open`, `scufris_widget_update`, `scufris_widget_close`, and
  `scufris_widget_clear`, typed from what the companion says it has installed.
  Four widgets ship with it: `cpu`, `timer`, `claude`, and `codex`. See the
  [widgets](docs/src/dev/widgets.md) chapter.
- Exhibits age out on their own. A panel the conversation has moved past dims
  and retires a minute later, and an update or the pointer over it brings it
  back. Clocks stop while Scufris speaks and while you are reading the panel.
  Instruments and pinned panels are yours and never age.
- Widgets go away with the pill and come back with it. Putting the pill down
  takes the whole layer off the screen with their state and their remaining
  time intact. Panels you pinned and instruments stay where they are, because
  they are yours rather than the runtime's.
- A widget Scufris opened follows you between workspaces. Pinning it brings it
  down onto the workspace you are on and parks it in a screen-edge slot of its
  own, so nothing the shelf does afterwards lands on top of it. A pin with no
  free slot says so on the panel instead of doing nothing.
- Widgets that show live numbers, and the `cpu` widget on the first of them. It
  draws the last minute of processor load as a graph, with the package
  temperature beside it - in the warning colour once it is hot - and the memory
  in use and the load average under it. A widget can name a backend, a small
  program that reports readings, and two panels asking for the same numbers
  share one process. A backend that goes quiet says so on the panel; one that
  dies turns the frame red and offers a restart tick, rather than leaving a
  frozen number that looks live. Nothing is left running when the last panel
  closes or when the companion exits.
- Widgets you can act on, and the `timer` widget on the first of them. A panel's
  own buttons write back to its backend, which answers with the refreshed
  reading, so what the panel shows is always what the backend knows. Ask for a
  timer and it counts down on the desktop, with ticks to pause, resume, add a
  minute, and start over. Two timers of the same length are two timers.
- Put a widget up yourself. The tray menu offers the widgets that fill
  themselves, and one you open from there is yours: it goes in an edge slot,
  stays until you close it, and Scufris is not told about it.
- The `claude` and `codex` widgets, which say how much of each subscription is
  spent. Every usage window is a meter, the one closest to its limit is the
  headline, and the panel says how long until it starts over. They read the
  token the vendor's own CLI already keeps on this machine, so there is nothing
  to sign in to and nothing to configure, and a machine that never signed in
  gets a panel that says so rather than a stale number. The `rfr` tick asks
  again without waiting out the poll.
- Panels are bigger and their type is larger. A widget sits on the desktop
  beside the work rather than in it, and is read from across the room; the type
  scale went up a step and every panel went up with it, because type that grew
  inside a window that did not would only have less room to say the same thing.
- `SCUFRIS_WIDGET_PATH` names extra widget roots, separated the way `PATH` is,
  so another project can ship a widget for the desktop. Widgets that shipped
  with Scufris always win, and one that will not install is reported in the log
  rather than stopping the companion.
- `scufris-ctl open`, which puts the pill up from outside its window, so a
  window manager binding can be the thing that opens it. See
  [Using Scufris](docs/src/guide/using.md).
- The pill answers `Super+Escape` while it is on screen, built from whatever
  modifier your activation hotkey uses. It cancels a take, and it puts a
  resting pill away without opening the microphone on the way.
- `scufris_conversation`, so Scufris can show the conversation window itself.
  Ask to see the conversation and it opens the window; ask it to put the window
  away and it does. It shows and closes rather than toggling, because it cannot
  see your screen and a toggle would leave it unable to say which of the two it
  had just done.
- `Super+Delete` stops Scufris, built the same way and grabbed on the same
  terms. It cuts what is being spoken and ends the run, and it changes nothing
  else: a transcript you are still editing stays where it is, and the
  conversation keeps everything said so far. With nothing running it does
  nothing. `scufris-ctl abort` is the same verb from a terminal.
- `programs.scufris.desktop.cancelKey` and `stopKey` name those two keys
  yourself. Deriving them from the hotkey is what ships, so most deployments
  set neither; `"none"` takes a key off the companion entirely, which is the
  answer where your desktop already means something by it.
- `packages.scufris-speak`, the synthesiser the companion runs. It binds the
  pinned Piper package, model, and configuration, so the voice is a property of
  the package rather than a run-time setting.
- `SCUFRIS_RUNTIME_DIR` names the socket directory outright, used as named with
  no `scufris` below it. The service, the companion, `scufris-ctl` and the
  agent's own service extension resolve their sockets through it, so one export
  moves a whole stack together and none of them can end up in another Scufris's
  conversation. A socket named outright still outranks it, and nothing sets it
  in an ordinary session.
- `nix run .#staging -- up` runs this source tree's Scufris beside the deployed
  one: its own sockets, state, sessions, and `Super+G`, against a disposable
  root under `/tmp`. It stays in the foreground and Ctrl+C stops both halves.
  It speaks with the packaged `scufris-speak`, so staging has the voice the
  deployment would have, and says on start when it could find no synthesiser
  rather than leaving a missing voice to look like a broken one. See the
  [staging](docs/src/dev/staging.md) chapter.
- Your journal on the desktop, as three panels over the `today` command:
  `agenda` is a month to pick a day from and then that day's habits, tasks and
  what follows it; `macros` is the day's calories and food with a month of
  weight behind them; `notes` is the day's notes. They write as well as read.
  Tick a habit or a task to mark it done, click a weight to log one, click a
  note to rewrite it, and use the `+` ticks to add a task, a food or a note.
  Logging a food offers your database as you type the name. Everything lands on
  the day the panel is showing, and goes through `today`, so a habit ticked
  here and one ticked in your editor are the same habit. They need
  `programs.scufris.desktop.todayCommand`, and a food needs
  `programs.scufris.desktop.macrosDatabase` unless your database is where
  `today` looks by default.
- A panel that needs words asks for them in a small box of its own, over the
  panel that asked. `Enter` saves, `Escape` closes it with nothing written, and
  the keyboard goes back where it was. A panel still never takes the keyboard
  itself.

### Changed

- Two panels on one screen side no longer stand on each other. The second place
  on a side used to be measured halfway down the screen, whatever was already
  there, so any panel taller than a quarter of the screen overlapped the one
  above it. The two places now hang from opposite ends of the side. The shelf
  above the pill holds a lane per panel for the same reason, wide enough for
  the widest one that ships.

- Scufris delegates literally. `.scufris.toml` is a menu of agent types, not a
  workflow: one `conventions` table for what Scufris infers when you do not
  say, and one `agents.<name>` table per agent, each with a `description` of
  what it is for and `keywords` for how it is run. Ask to implement something
  and Scufris runs the work agent and stops. Ask to implement and then review
  and it runs both, in that order. It starts no agent because the project
  declares one, and it queues no follow-on work of its own. An explicit
  instruction such as "do it directly on master" wins over a convention, and an
  agent name Scufris has never seen is delegated to like any other. A later
  round of an agent already running steers that job rather than starting a
  second one, so a reviewer keeps what it already accepted instead of finding
  new fault every round. The retired `preferences` shape is refused with a
  diagnostic rather than half-read; see the
  [jobs chapter](docs/src/dev/jobs.md) for the file to write instead.
- Scufris is a background service with clients now, which is the whole shape of
  this release. `scufris-service` owns the conversation, the session, and the
  socket; the Pi agent, the desktop companion, and `scufris-ctl` are all
  clients of it. There is no terminal that owns the conversation any more, so
  putting the pill away, closing the terminal, or a companion crash leave the
  conversation exactly where it was, and a machine with no display still has
  one.
- `programs.scufris.desktop.enable` requires `programs.scufris.service.enable`.
  The tray's restart hook restarts `scufris-service.service`.
- The tray's "Open chat" is now "Open in terminal", under the new "Show
  conversation" entry, and a left click on the tray icon shows the conversation
  window rather than opening a terminal. `chatCommand` is unchanged and still
  optional; the terminal is a different tool, not a fallback.
- Control protocol version 3, which replaces version 2 outright. It adds the
  `agent` role, so the Pi process reports what it said, the paragraph it wants
  spoken, and the widgets it asks for, and it carries stable refusal codes a
  caller branches on. There is no conversion from version 2: the companion, the
  service, and the Scufris package must be updated together.
- Speech is the companion's, all of it. It owns the speaker, so it owns the
  mute: "Mute Scufris" in the tray silences Scufris without touching the
  conversation. Nothing in the agent's process tree makes sound and nothing in
  it decides to; every answer is one prose paragraph whatever is listening,
  which is the shape of the assistant rather than a speech setting. A session
  with no companion is silent, and so is a companion with no synthesiser, which
  is the one thing enabling `voice` now does.
- `Super+D` is the one key of the take. Press it once and the pill rises and
  the microphone opens; press it again and the take stops and what you said
  arrives in a textbox above the pill. The textbox is an ordinary focused
  window, so `Enter`, `Escape`, and every editing key are its own and work
  wherever you are. The pill is an indicator and never takes the keyboard.

### Removed

- The Kitty popup. `programs.scufris.voice.popup.*` and the
  `scufris-popup.service` unit are gone; use `programs.scufris.service.enable`
  and reach the conversation with `scufris-ctl debug`. Nothing is migrated: a
  configuration that set the popup options fails to evaluate.
- Control protocol version 2, the `desktop` Pi extension that served it,
  `SCUFRIS_DAEMON`, and `tools/desktop/scufris-socket-lock`.
- Dashboardd widget control. The `dashboard` extension, its skill, the
  `scufris-dashboard` helper, the `dashboardd` flake input, and the
  `programs.scufris.dashboard.*` options are gone. Widgets return as a native
  runtime inside the desktop companion.
- The window manager binding mode, and with it the `accept` and `cancel` verbs
  of `scufris-ctl` and `programs.scufris.desktop.modeCommand`. The textbox holds
  the keyboard itself, so there is nothing left for a binding mode to route. A
  configuration that sets `modeCommand` fails to evaluate; nothing is migrated.
- `Enter` while the microphone is open. One take is one key: `Super+D` stops
  it, and the words are sent from the textbox.
- The speech mode, and with it `/speech`, `SCUFRIS_SPEECH`, and
  `SCUFRIS_VOICE_AVAILABLE`. Whether Scufris makes a sound was a switch kept in
  the session and seeded from a variable on a process that owns no speaker. It
  is the tray's now. A configuration that sets either variable is ignored;
  nothing is migrated, so a session that recorded `/speech off` is audible
  again until the tray is told otherwise.
- The voice build variants: the `scufris-voice` package and app, the
  `voice-resources` package, and `npm run dev:voice`. They existed to ship a
  speech module and set a variable for it, and both are gone. There is one
  launcher, and it is the one that was always silent.

## [0.4.0] - 2026-08-25

### Added

- `scufris-desktop`, the voice pill and tray companion. `Super+D` opens a
  bottom-center pill and starts recording. `Enter` transcribes and sends,
  `Escape` discards, and the accelerator again opens the transcript for
  editing. The tray shows the assistant state and can open the chat, start
  voice input, restart the backend, and quit. See the
  [desktop companion](docs/src/dev/desktop.md) chapter.
- `packages.scufris-desktop`, a separate Linux flake output. Nothing else
  pulls Tauri or WebKitGTK into its closure, which a closure check enforces.
- `programs.scufris.desktop` Home Manager options, among them `enable`,
  `hotkey`, `chatCommand`, and `stt`. The module defines the
  `scufris-desktop.service` user service and a generated backend restart hook.
- A bundled loopback `whisper-server` with a pinned model on
  `127.0.0.1:10302`, used when `desktop.stt.endpoint` is not set, so voice
  input works on any Nix system.
- Control protocol v1 on `$XDG_RUNTIME_DIR/scufris/daemon.sock`. The popup Pi
  process serves it. Submissions are acknowledged against the session, and an
  unacknowledged transcript is kept and reported as uncertain instead of being
  sent again.
- The `desktop` Pi extension, which serves that socket in the daemon role and
  reports one assistant state: idle, working, speaking, attention, or error.

### Changed

- Independent review honors the harness and model configured for the project.
- Quick Review runs as a separate Pi RPC agent that loads the standalone npm
  extension. The in-repository walkthrough implementation is removed.
- The default Pi package comes from the `llm-agents.nix` input.
- `nix/checks/` replaces the single `nix/checks.nix` file, with one group per
  check concern.

## [0.3.0] - 2026-08-24

### Added

- Project workflow preferences in `.scufris.toml`: task tracking, isolated
  Sprout worktrees, the implementation harness and model, review, and the
  landing gate.
- Complete job inspection with `scripts/scufris-jobs`.
- Explicit `/wake` and `/calm` controls, restored with the session.
- The user and developer guides in the mdBook manual.

### Changed

- Delegated jobs never block the foreground conversation. Workers report
  `working`, `blocked`, `done`, and `failed` events.
- Scufris answers as a prose-only orchestrator. Optional detail is a private
  artifact opened with `/detail <id>`.
- Quick Review follows pull request review semantics.

### Fixed

- Foreground workflow acknowledgments, response termination, delegation
  routing, and speech ordering for prose-only responses.

## [0.2.0] - 2026-08-22

### Added

- Independent preflight review before landing.
- The mdBook manual and its generated option reference.
- Tagged release automation with `release.yml`.

## [0.1.0] - 2026-08-22

### Added

- The Scufris Pi package: foreground identity, the delegated job loop, and the
  Nix flake with the Home Manager module.

[Unreleased]: https://github.com/alexjercan/scufris2/compare/v2.8.2...HEAD
[2.8.2]: https://github.com/alexjercan/scufris2/compare/v2.8.1...v2.8.2
[2.8.1]: https://github.com/alexjercan/scufris2/compare/v2.8.0...v2.8.1
[2.8.0]: https://github.com/alexjercan/scufris2/compare/v2.7.0...v2.8.0
[2.7.0]: https://github.com/alexjercan/scufris2/compare/v2.6.0...v2.7.0
[2.6.0]: https://github.com/alexjercan/scufris2/compare/v2.5.0...v2.6.0
[2.5.0]: https://github.com/alexjercan/scufris2/compare/v2.4.1...v2.5.0
[2.4.1]: https://github.com/alexjercan/scufris2/compare/v2.4.0...v2.4.1
[2.4.0]: https://github.com/alexjercan/scufris2/compare/v2.3.0...v2.4.0
[2.3.0]: https://github.com/alexjercan/scufris2/compare/v2.2.0...v2.3.0
[2.2.0]: https://github.com/alexjercan/scufris2/compare/v2.1.7...v2.2.0
[2.1.7]: https://github.com/alexjercan/scufris2/compare/v2.1.6...v2.1.7
[2.1.6]: https://github.com/alexjercan/scufris2/compare/v2.1.5...v2.1.6
[2.1.5]: https://github.com/alexjercan/scufris2/compare/v2.1.4...v2.1.5
[2.1.4]: https://github.com/alexjercan/scufris2/compare/v2.1.3...v2.1.4
[2.1.3]: https://github.com/alexjercan/scufris2/compare/v2.1.2...v2.1.3
[2.1.2]: https://github.com/alexjercan/scufris2/compare/v2.1.1...v2.1.2
[2.1.1]: https://github.com/alexjercan/scufris2/compare/v2.1.0...v2.1.1
[2.1.0]: https://github.com/alexjercan/scufris2/compare/v2.0.0...v2.1.0
[2.0.0]: https://github.com/alexjercan/scufris2/compare/v1.1.1...v2.0.0
[1.1.1]: https://github.com/alexjercan/scufris2/compare/v1.1.0...v1.1.1
[1.1.0]: https://github.com/alexjercan/scufris2/compare/v0.6.0...v1.1.0
[0.6.0]: https://github.com/alexjercan/scufris2/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/alexjercan/scufris2/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/alexjercan/scufris2/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/alexjercan/scufris2/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/alexjercan/scufris2/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/alexjercan/scufris2/releases/tag/v0.1.0
