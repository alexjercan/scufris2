# End-to-end scenarios

The design lists nine manual scenarios. This file says, for each one, what
pins its claims automatically and what a person still has to do by hand. It is
the honest boundary of the implementation work: an automated suite proves the
behaviour of the parts, and a person at a screen proves the deployment.

The manual steps are documented for a person in
[Hold the conversation from a terminal](../../docs/src/dev/staging.md), which
carries the commands and a table of cases. This file does not repeat them.

## What was run in this task

| Suite                                                  | Result            |
| ------------------------------------------------------ | ----------------- |
| `cargo test --workspace`                               | pass              |
| `cargo test -p scufris-service --test lease`           | 3 pass            |
| `cargo test -p scufris-desktop`                        | 340 pass          |
| `npm run check` (typecheck, 157 tests, prettier)       | pass              |
| `python3 -m unittest discover -s tests -p 'test_*.py'` | pass              |
| `nix flake check -L`                                   | all checks passed |

## What was not run, and why

Three things in the scenarios need something this task did not have:

- A screen. The desktop companion is a GTK/WebKit window and no display was
  available, so the HUD was never watched by eye. `tests/desktop-ui.test.ts`
  and the `scufris-desktop` unit tests drive the same page and the same
  presentation rule instead.
- A phone. The iPhone surface needs Xcode and a device. The gateway speaks the
  same surface protocol and the checks exercise it
  (`authenticated_websocket_still_bridges_strict_surface_v11`), and the Swift
  suite runs in the `iOS` workflow, which is its only gate.
- A model. A real turn needs the machine's inference service. The scenarios
  that turn on what the model says (S5's "the model repeats the fact") cannot
  be asserted without spending the user's inference on a non-deterministic
  answer. What is asserted instead is the mechanism: which file was forked,
  and what the catch-up message contained.

So S1 to S9 are documented for a person to run on the staging harness against
the release build, which is what the design's go/no-go asks for. They are not
claimed as run.

## The scenarios

### S1 Attach and type

Automated: `a_lease_stops_the_child_and_fences_the_agent_channel` and
`a_terminal_takes_the_agent_and_gives_it_back` (the grant arrives after the
child is reaped, `waitpid` observed);
`a_terminal_turn_and_its_answer_are_recorded_under_the_terminal_name`;
`an_answer_closes_only_the_turn_it_names`; `a typed turn is recorded and its
answer names it` and `an accepted turn is the one the answer names` in
`tests/terminal.test.ts`.

By hand: the HUD showing holder `terminal`.

### S2 Phone while attached

Automated: `a_leased_terminal_reports_its_own_activity_and_still_hears_everyone`;
`a_cross_surface_steer_moves_the_response_association`;
`a_leased_terminal_reports_its_own_activity` in `tests/terminal.test.ts`.

By hand: the phone itself.

### S3 Wake and briefing while attached

Automated: `a_wake_owns_its_answer_only_when_no_owner_turn_is_open`;
`only_the_matching_proactive_turn_can_acknowledge_a_delivery`;
`proactive_response_is_safe_for_both_settled_event_orderings`;
`external_input_resets_the_proactive_turn_counter`.

By hand: a real briefing run against a model.

### S4 Release and crash

Automated: `a_terminal_takes_the_agent_and_gives_it_back` (release, then the
child starts again); `a_holder_that_stops_answering_loses_the_lease_and_a_late_ping_says_so`
(three missed pings, then `lease_ping_stale`); `a connection that ends while
held is reported once as lost` and `a lost lease is retried with a backoff, and
hold stops it` in `tests/terminal.test.ts`.

By hand: `kill -9` and `kill -STOP` against a real Pi, with the timing watched.

### S5 Lineage

Automated: `the_lineage_is_what_the_next_holder_forks_from`;
`a_terminal_that_did_not_fork_is_told_what_it_missed`;
`a_catch_up_page_is_bounded_and_says_when_more_follows`; `a leased terminal
keeps the lineage a single chain`, `attach forks the lineage once, and never
asks twice`, `a joining agent is caught up in one hidden message`, and `the
words of a catch-up page say who said them` in `tests/terminal.test.ts`.

By hand: whether the model actually repeats the fact. Gate G1 measured that
`pi --session-dir D --fork FILE` carries the branch and writes `parentSession`;
see `gates/`.

### S6 Jobs

Automated: `the owner of delegated work outlives the session that started it`
and `stray worker panes are named by whose they are` in `tests/agents.test.ts`;
`JobOwnershipTest` in `tests/test_scufris_jobs.py` (recover adopts once,
migrate-owner moves work back, `orphans` names the kind of owner).

By hand: a real delegated worker moving between holders.

### S7 Speech and state

Automated: `a_terminals_answer_is_spoken_only_where_the_deployment_asked_for_it`
and `a_terminals_answer_is_read_out_only_when_the_deployment_asked` in the
desktop crate; `who_holds_the_agent_is_part_of_what_the_page_is_told`; "the
strip says where the conversation is being answered" in
`tests/desktop-ui.test.ts`; the `desktop-configuration` Nix check pins
`speak_terminal` in both `--print-config` tables.

By hand: hearing it.

### S8 Two terminals and a service restart

Automated: `a refused lease rejects with the host's code and holds nothing`
and the backoff test in `tests/terminal.test.ts`; `a_failed_service_is_not_hidden_behind_a_terminal`.

By hand: two real terminals and a `systemctl --user restart`.

### S9 Rollback

Automated: `the_lease_is_refused_when_the_service_does_not_offer_it` and
`the_lease_is_refused_unless_the_service_offers_it`; `a refused attach leaves
an ordinary Pi with no channel` in `tests/terminal.test.ts`; the
`service-interface` Nix check asserts the option is off by default, that the
flag and the launcher are absent then, and that both appear with the option
on.

By hand: the switch itself, with the HUD watched.
