//! The terminal lease against the built service binary.
//!
//! Everything the unit tests measure in-process is measured here across the
//! real sockets and a real child: the managed agent is a checked-in script
//! that records its pid and its arguments, so stopping it, starting it again,
//! and starting it from a fork are facts on disk rather than fields in a
//! struct.

use std::{
    io::BufReader,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use scufris_control::refusal;
use scufris_control::service::{
    AgentHolder, AgentRequest, AgentRequestBody, AgentResponseBody, AgentSession, ControlRequest,
    ControlRequestBody, ControlResponseBody, ConversationRole, FOREGROUND_OWNER, LeaseHolder,
    MAX_CONVERSATION_PAGE, ScufrisState, SurfaceRegistration, SurfaceRequest, SurfaceRequestBody,
    SurfaceResponseBody, TERMINAL_SURFACE, read_agent_response, read_control_response,
    read_surface_response,
};
use scufris_control::write_message;

const PATIENCE: Duration = Duration::from_secs(15);
static NEXT: AtomicU64 = AtomicU64::new(1);

/// How a run is told to offer the lease.
///
/// The flag is what a service started by hand takes. The variable is what the
/// unit the Home Manager module writes sets, and it sets it to `1`, so a
/// binary that reads only `true` and `false` there never starts at all on the
/// deployment that asked for the handoff.
#[derive(Clone, Copy)]
enum Lease {
    Off,
    Flag,
    Variable,
}

struct Harness {
    root: PathBuf,
    runtime: PathBuf,
    pids: PathBuf,
    args: PathBuf,
    sessions: PathBuf,
    service: Child,
}

impl Harness {
    fn start(lease: Lease) -> Self {
        let root = std::env::temp_dir().join(format!(
            "scufris-lease-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let runtime = root.join("run");
        let home = root.join("home");
        let data = root.join("data");
        for directory in [&runtime, &home, &data] {
            std::fs::create_dir_all(directory).unwrap();
        }
        let pids = root.join("pids");
        let args = root.join("args");
        let sessions = data.join("scufris/sessions");
        let mut command = Command::new(env!("CARGO_BIN_EXE_scufris-service"));
        command
            .env("HOME", &home)
            .env("XDG_DATA_HOME", &data)
            .env("SCUFRIS_RUNTIME_DIR", &runtime)
            .env(
                "SCUFRIS_SERVICE_AGENT",
                concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/leased-agent"),
            )
            .env("SCUFRIS_TEST_PIDS", &pids)
            .env("SCUFRIS_TEST_ARGS", &args)
            .env("SCUFRIS_TEST_SESSION_FILE", sessions.join("child.jsonl"))
            .env_remove("SCUFRIS_SERVICE_TERMINAL_LEASE")
            .env_remove("RUST_LOG")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(if std::env::var_os("SCUFRIS_TEST_VERBOSE").is_some() {
                Stdio::inherit()
            } else {
                Stdio::null()
            });
        match lease {
            Lease::Off => {}
            Lease::Flag => {
                command.arg("--terminal-lease");
            }
            Lease::Variable => {
                command.env("SCUFRIS_SERVICE_TERMINAL_LEASE", "1");
            }
        }
        let service = command.spawn().expect("the service binary starts");
        let harness = Self {
            root,
            runtime,
            pids,
            args,
            sessions,
            service,
        };
        harness.wait_for(|| harness.runtime.join("control.sock").exists());
        harness
    }

    fn wait_for(&self, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + PATIENCE;
        while !condition() {
            assert!(
                Instant::now() < deadline,
                "the condition was not met in time"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn agent_pids(&self) -> Vec<i32> {
        std::fs::read_to_string(&self.pids)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.trim().parse().ok())
            .collect()
    }

    /// Every command line the managed agent has been started with.
    fn agent_args(&self) -> Vec<String> {
        std::fs::read_to_string(&self.args)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn wait_for_agents(&self, count: usize) -> Vec<i32> {
        self.wait_for(|| self.agent_pids().len() >= count);
        self.agent_pids()
    }

    fn control(&self) -> Control {
        let stream = UnixStream::connect(self.runtime.join("control.sock")).unwrap();
        stream.set_read_timeout(Some(PATIENCE)).unwrap();
        let mut control = Control {
            reader: BufReader::new(stream.try_clone().unwrap()),
            stream,
        };
        write_message(
            &mut control.stream,
            &ControlRequest::new(ControlRequestBody::Hello),
        )
        .unwrap();
        assert!(matches!(control.read(), ControlResponseBody::Ready));
        control
    }

    fn surface(&self, id: &str) -> Surface {
        let stream = UnixStream::connect(self.runtime.join("surface.sock")).unwrap();
        stream.set_read_timeout(Some(PATIENCE)).unwrap();
        let mut surface = Surface {
            reader: BufReader::new(stream.try_clone().unwrap()),
            stream,
        };
        write_message(
            &mut surface.stream,
            &SurfaceRequest::new(SurfaceRequestBody::Hello {
                surface: SurfaceRegistration {
                    id: id.into(),
                    name: id.into(),
                    widgets: vec![],
                },
            }),
        )
        .unwrap();
        surface
    }

    fn agent(
        &self,
        lease: Option<u64>,
        session: Option<AgentSession>,
    ) -> (Agent, AgentResponseBody) {
        let stream = UnixStream::connect(self.runtime.join("agent.sock")).unwrap();
        stream.set_read_timeout(Some(PATIENCE)).unwrap();
        let mut agent = Agent {
            reader: BufReader::new(stream.try_clone().unwrap()),
            stream,
        };
        write_message(
            &mut agent.stream,
            &AgentRequest::new(AgentRequestBody::Hello { lease, session }),
        )
        .unwrap();
        let answer = agent.read();
        (agent, answer)
    }

    fn terminal_session(&self) -> AgentSession {
        AgentSession {
            id: "01J00000000000000000000T".into(),
            file: self
                .sessions
                .join("terminal.jsonl")
                .to_string_lossy()
                .into_owned(),
            cwd: self.root.to_string_lossy().into_owned(),
            parent: Some(
                self.sessions
                    .join("child.jsonl")
                    .to_string_lossy()
                    .into_owned(),
            ),
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let pid = Pid::from_raw(i32::try_from(self.service.id()).unwrap());
        let _ = kill(pid, Signal::SIGTERM);
        let deadline = Instant::now() + PATIENCE;
        while self.service.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        if self.service.try_wait().ok().flatten().is_none() {
            let _ = self.service.kill();
            let _ = self.service.wait();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Control {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Control {
    fn ask(&mut self, body: ControlRequestBody) -> ControlResponseBody {
        write_message(&mut self.stream, &ControlRequest::new(body)).unwrap();
        self.read()
    }

    fn read(&mut self) -> ControlResponseBody {
        read_control_response(&mut self.reader).unwrap().body
    }

    fn acquire(&mut self, id: &str) -> ControlResponseBody {
        self.ask(ControlRequestBody::LeaseAcquire {
            id: id.into(),
            holder: LeaseHolder {
                pid: std::process::id(),
                session_file: None,
                cwd: "/tmp".into(),
            },
            abort_working: false,
        })
    }

    fn state(&mut self) -> (ScufrisState, String, AgentHolder) {
        match self.ask(ControlRequestBody::State { id: "s".into() }) {
            ControlResponseBody::State {
                state,
                detail,
                holder,
                ..
            } => (state, detail, holder),
            other => panic!("expected a state, got {other:?}"),
        }
    }

    fn wait_for_state(&mut self, wanted: ScufrisState) -> String {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let (state, detail, _) = self.state();
            if state == wanted {
                return detail;
            }
            assert!(
                Instant::now() < deadline,
                "the service stayed {state:?} ({detail}) rather than {wanted:?}"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
}

struct Surface {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Surface {
    fn read(&mut self) -> SurfaceResponseBody {
        read_surface_response(&mut self.reader).unwrap().body
    }

    /// Everything up to and including the ready line of the handshake.
    fn replay(&mut self) -> Vec<SurfaceResponseBody> {
        let mut seen = Vec::new();
        loop {
            let body = self.read();
            let ready = matches!(body, SurfaceResponseBody::Ready { .. });
            seen.push(body);
            if ready {
                return seen;
            }
        }
    }

    fn next_message(&mut self) -> (ConversationRole, String, String) {
        loop {
            if let SurfaceResponseBody::Message {
                role,
                surface,
                text,
                ..
            } = self.read()
            {
                return (role, surface, text);
            }
        }
    }

    fn next_state(&mut self) -> (ScufrisState, String, AgentHolder) {
        loop {
            if let SurfaceResponseBody::State {
                state,
                detail,
                holder,
            } = self.read()
            {
                return (state, detail, holder);
            }
        }
    }
}

struct Agent {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Agent {
    fn tell(&mut self, body: AgentRequestBody) {
        write_message(&mut self.stream, &AgentRequest::new(body)).unwrap();
    }

    fn read(&mut self) -> AgentResponseBody {
        read_agent_response(&mut self.reader).unwrap().body
    }
}

fn alive(pid: i32) -> bool {
    let stat = Path::new("/proc").join(pid.to_string()).join("stat");
    match std::fs::read_to_string(stat) {
        // The state letter follows the parenthesised command name.
        Ok(line) => line
            .rsplit(')')
            .next()
            .and_then(|rest| rest.split_whitespace().next())
            .is_some_and(|state| state != "Z"),
        Err(_) => false,
    }
}

fn wait_until_gone(pid: i32) {
    let deadline = Instant::now() + PATIENCE;
    while alive(pid) {
        assert!(Instant::now() < deadline, "agent {pid} is still alive");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_lease_is_refused_when_the_service_does_not_offer_it() {
    let harness = Harness::start(Lease::Off);
    let first = harness.wait_for_agents(1)[0];
    let mut control = harness.control();
    control.wait_for_state(ScufrisState::Idle);
    assert!(matches!(
        control.acquire("l"),
        ControlResponseBody::Rejected { code, .. } if code == refusal::LEASE_DISABLED
    ));
    assert!(alive(first));
    assert_eq!(harness.agent_pids(), vec![first]);
    // Nothing offered, so the state still says the managed child is the agent.
    assert_eq!(control.state().2, AgentHolder::Managed);
}

#[test]
fn the_variable_the_deployment_sets_offers_the_lease() {
    // The unit sets `SCUFRIS_SERVICE_TERMINAL_LEASE=1`, which is also what the
    // reference documents. A service that refuses that value exits before it
    // binds anything, so the harness would not find a control socket here, and
    // the failure a person sees is a missing socket rather than a rejected
    // value.
    let harness = Harness::start(Lease::Variable);
    harness.wait_for_agents(1);
    let mut control = harness.control();
    control.wait_for_state(ScufrisState::Idle);
    assert!(matches!(
        control.acquire("l"),
        ControlResponseBody::Lease { .. }
    ));
}

#[test]
fn a_terminal_takes_the_agent_and_gives_it_back() {
    let harness = Harness::start(Lease::Flag);
    let first = harness.wait_for_agents(1)[0];
    let mut control = harness.control();
    control.wait_for_state(ScufrisState::Idle);
    assert_eq!(
        harness.agent_args(),
        vec![format!(
            "--session-dir {} --continue --mode rpc",
            harness.sessions.display()
        )]
    );
    let mut desk = harness.surface("desk");
    let handshake = desk.replay();
    assert!(
        !handshake
            .iter()
            .any(|body| matches!(body, SurfaceResponseBody::Message { .. }))
    );

    // Taking the agent stops the child before the grant comes back, and names
    // the session the next holder forks from.
    let grant = control.acquire("l1");
    let ControlResponseBody::Lease {
        generation,
        lineage_file,
        sequence,
        owner,
        ..
    } = grant
    else {
        panic!("expected a lease, got {grant:?}");
    };
    assert_eq!(generation, 1);
    assert_eq!(sequence, 0);
    assert_eq!(owner, FOREGROUND_OWNER);
    assert!(lineage_file.is_some_and(|file| file.ends_with("sessions/child.jsonl")));
    assert!(!alive(first));
    assert_eq!(
        desk.next_state(),
        (
            ScufrisState::Starting,
            "A terminal holds the agent.".to_string(),
            AgentHolder::Terminal
        )
    );
    let mut other = harness.control();
    assert!(matches!(
        other.acquire("l2"),
        ControlResponseBody::Rejected { code, .. } if code == refusal::LEASE_HELD
    ));
    assert!(matches!(
        other.ask(ControlRequestBody::LeaseRelease { id: "r0".into() }),
        ControlResponseBody::Rejected { code, .. } if code == refusal::NOT_LEASE_HOLDER
    ));
    assert!(matches!(
        other.ask(ControlRequestBody::LeasePing { id: "p0".into() }),
        ControlResponseBody::Rejected { code, .. } if code == refusal::LEASE_PING_STALE
    ));
    assert!(matches!(
        control.ask(ControlRequestBody::LeasePing { id: "p1".into() }),
        ControlResponseBody::LeasePong { id, generation: 1 } if id == "p1"
    ));

    // The fence on the agent channel.
    assert!(matches!(
        harness.agent(None, None).1,
        AgentResponseBody::Rejected { code, .. } if code == refusal::LEASE_REQUIRED
    ));
    assert!(matches!(
        harness.agent(Some(9), None).1,
        AgentResponseBody::Rejected { code, .. } if code == refusal::LEASE_REQUIRED
    ));
    let session = harness.terminal_session();
    let (mut terminal, admitted) = harness.agent(Some(generation), Some(session.clone()));
    assert!(matches!(admitted, AgentResponseBody::Ready));
    assert_eq!(desk.next_state().0, ScufrisState::Idle);
    assert_eq!(control.state().2, AgentHolder::Terminal);

    // A typed turn is acknowledged with where it was recorded, and its answer
    // closes exactly that turn.
    terminal.tell(AgentRequestBody::Turn {
        id: "t-0123456789abcdef".into(),
        text: "Typed in the terminal.".into(),
        images: 0,
    });
    assert!(matches!(
        terminal.read(),
        AgentResponseBody::TurnAck { id, sequence: 1 } if id == "t-0123456789abcdef"
    ));
    assert_eq!(
        desk.next_message(),
        (
            ConversationRole::User,
            TERMINAL_SURFACE.into(),
            "Typed in the terminal.".into()
        )
    );
    terminal.tell(AgentRequestBody::Activity { working: true });
    assert_eq!(desk.next_state().0, ScufrisState::Working);
    terminal.tell(AgentRequestBody::Response {
        text: "Answered from the terminal.".into(),
        turn_id: Some("t-0123456789abcdef".into()),
        proactive_id: None,
        details: None,
        widgets: None,
        attachments: vec![],
        receipts: vec![],
    });
    assert_eq!(
        desk.next_message(),
        (
            ConversationRole::Assistant,
            TERMINAL_SURFACE.into(),
            "Answered from the terminal.".into()
        )
    );
    terminal.tell(AgentRequestBody::Activity { working: false });
    assert_eq!(desk.next_state().0, ScufrisState::Idle);

    // The other direction: a surface message and a wake reach the terminal.
    write_message(
        &mut desk.stream,
        &SurfaceRequest::new(SurfaceRequestBody::Message {
            id: "m1".into(),
            text: "From the desk.".into(),
            attachments: vec![],
        }),
    )
    .unwrap();
    assert!(matches!(
        terminal.read(),
        AgentResponseBody::Message { id, text, .. } if id == "m1" && text == "From the desk."
    ));
    assert_eq!(
        desk.next_message(),
        (
            ConversationRole::User,
            "desk".into(),
            "From the desk.".into()
        )
    );
    assert!(matches!(
        control.ask(ControlRequestBody::Wake {
            id: "w1".into(),
            custom_type: "scufris-wake".into(),
            text: "Now.".into(),
            details: None,
        }),
        ControlResponseBody::WakeAck { .. }
    ));
    assert!(matches!(
        terminal.read(),
        AgentResponseBody::Wake { custom_type, text, .. }
            if custom_type == "scufris-wake" && text == "Now."
    ));

    // The same three entries come back through the control socket, which is
    // how a holder that did not fork catches up on its own terms.
    let answered = control.ask(ControlRequestBody::Conversation {
        id: "c1".into(),
        since: 0,
    });
    let ControlResponseBody::ConversationEntries { entries, more, .. } = answered else {
        panic!("expected a conversation page, got {answered:?}");
    };
    assert!(!more);
    assert_eq!(
        entries
            .iter()
            .map(|entry| (entry.role, entry.surface.clone()))
            .collect::<Vec<_>>(),
        vec![
            (ConversationRole::User, TERMINAL_SURFACE.to_string()),
            (ConversationRole::Assistant, TERMINAL_SURFACE.to_string()),
            (ConversationRole::User, "desk".to_string()),
        ]
    );
    assert!(entries.len() <= MAX_CONVERSATION_PAGE);

    // A surface joining now replays the terminal's turn and answer.
    let mut late = harness.surface("late");
    let replayed: Vec<_> = late
        .replay()
        .into_iter()
        .filter_map(|body| match body {
            SurfaceResponseBody::Message { role, surface, .. } => Some((role, surface)),
            _ => None,
        })
        .collect();
    assert_eq!(
        replayed,
        vec![
            (ConversationRole::User, TERMINAL_SURFACE.to_string()),
            (ConversationRole::Assistant, TERMINAL_SURFACE.to_string()),
            (ConversationRole::User, "desk".to_string()),
        ]
    );

    // Giving it back tells the terminal where the conversation went, restarts
    // the managed agent from the terminal's own session, and closes the
    // terminal's agent connection.
    assert!(matches!(
        control.ask(ControlRequestBody::LeaseRelease { id: "r1".into() }),
        ControlResponseBody::LeaseReleased { id } if id == "r1"
    ));
    assert!(matches!(
        terminal.read(),
        AgentResponseBody::Handoff {
            generation: 1,
            next: AgentHolder::Managed
        }
    ));
    let pids = harness.wait_for_agents(2);
    assert_eq!(pids.len(), 2);
    assert!(alive(pids[1]));
    assert!(read_agent_response(&mut terminal.reader).is_err());
    control.wait_for_state(ScufrisState::Idle);
    harness.wait_for(|| harness.agent_args().len() >= 2);
    assert_eq!(
        harness.agent_args()[1],
        format!(
            "--session-dir {} --fork {} --mode rpc",
            harness.sessions.display(),
            session.file
        )
    );

    // Closing the holding connection is the release as well, and a hello
    // fenced by the old generation is refused after a new grant.
    let mut second = harness.control();
    assert!(matches!(
        second.acquire("l3"),
        ControlResponseBody::Lease { generation: 2, .. }
    ));
    wait_until_gone(pids[1]);
    assert!(matches!(
        harness.agent(Some(1), None).1,
        AgentResponseBody::Rejected { code, .. } if code == refusal::LEASE_REQUIRED
    ));
    drop(second);
    let pids = harness.wait_for_agents(3);
    assert!(alive(pids[2]));
    control.wait_for_state(ScufrisState::Idle);
    assert!(matches!(
        harness.agent(Some(2), None).1,
        AgentResponseBody::Rejected { code, .. } if code == refusal::NOT_LEASE_HOLDER
    ));
}

#[test]
fn a_terminal_that_did_not_fork_is_told_what_it_missed() {
    let harness = Harness::start(Lease::Flag);
    harness.wait_for_agents(1);
    let mut control = harness.control();
    control.wait_for_state(ScufrisState::Idle);
    // The checked-in child records its arguments and answers the boot state,
    // but nothing in a shell script joins a socket. This stands in for the
    // extension the real child loads, so the conversation has a writer.
    let (mut child, admitted) = harness.agent(
        None,
        Some(AgentSession {
            id: "01J00000000000000000000C".into(),
            file: harness
                .sessions
                .join("child.jsonl")
                .to_string_lossy()
                .into_owned(),
            cwd: harness.root.to_string_lossy().into_owned(),
            parent: None,
        }),
    );
    assert!(matches!(admitted, AgentResponseBody::Ready));
    let mut desk = harness.surface("desk");
    desk.replay();
    write_message(
        &mut desk.stream,
        &SurfaceRequest::new(SurfaceRequestBody::Message {
            id: "m1".into(),
            text: "Remember the number nine.".into(),
            attachments: vec![],
        }),
    )
    .unwrap();
    assert!(matches!(
        child.read(),
        AgentResponseBody::Message { id, .. } if id == "m1"
    ));
    assert_eq!(desk.next_message().2, "Remember the number nine.");

    assert!(matches!(
        control.acquire("l1"),
        ControlResponseBody::Lease { generation: 1, .. }
    ));
    assert!(matches!(
        child.read(),
        AgentResponseBody::Handoff {
            generation: 1,
            next: AgentHolder::Terminal
        }
    ));
    // A plain terminal on a session of its own, with no parent in the chain.
    let (mut terminal, admitted) = harness.agent(
        Some(1),
        Some(AgentSession {
            id: "01J00000000000000000000P".into(),
            file: "/home/test/.pi/sessions/plain.jsonl".into(),
            cwd: "/home/test/scufris2".into(),
            parent: None,
        }),
    );
    assert!(matches!(admitted, AgentResponseBody::Ready));
    let caught_up = terminal.read();
    let AgentResponseBody::CatchUp { since, entries } = caught_up else {
        panic!("expected a catch-up, got {caught_up:?}");
    };
    assert_eq!(since, 0);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].text, "Remember the number nine.");
    assert_eq!(entries[0].surface, "desk");

    // Its own file is now the lineage, so the fork-back uses it even though
    // the terminal never forked from the child.
    assert!(matches!(
        control.ask(ControlRequestBody::LeaseRelease { id: "r1".into() }),
        ControlResponseBody::LeaseReleased { .. }
    ));
    harness.wait_for(|| harness.agent_args().len() >= 2);
    assert_eq!(
        harness.agent_args()[1],
        format!(
            "--session-dir {} --fork /home/test/.pi/sessions/plain.jsonl --mode rpc",
            harness.sessions.display()
        )
    );
}
