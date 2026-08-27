//! The service itself: one agent, one state, one transcript, many clients.
//!
//! Everything that has to be agreed on lives behind one lock. The lock is
//! never held across anything that can block: the agent's stdin is written
//! through a handle cloned out from under it, a client is written to through a
//! bounded channel its own thread drains, and stopping the agent happens after
//! the guard is dropped. That is the whole concurrency argument.
//!
//! The state is the smallest thing that answers "what is Scufris doing".
//! `agent_start` and `agent_settled` are its edges, and the mapping is here so
//! that no client ever learns Pi's vocabulary. Speaking and listening are not
//! in it: they are what a frontend is doing, and a service that knew about
//! them would be a service in the audio path.
//!
//! The debug lease is the one piece of policy worth stating twice. A control
//! client asks for the agent, the service stops it, and the lease is held by
//! that connection and nothing else. When the connection closes - a clean
//! exit, a Ctrl-C, a closed terminal, a killed client - the kernel closes the
//! socket, the reader ends, and the agent starts again. Nothing has to be
//! remembered and nothing has to be trapped, which is what makes it impossible
//! to be left detached with no way back.

use std::{
    collections::{HashMap, VecDeque},
    io::BufRead,
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

use scufris_control::service::{
    CatalogEntry, Role, ScufrisState, ServiceBody, ServiceMessage, Speaker, TranscriptEntry,
    WidgetCommand, WidgetReport, refusal,
};
use tracing::{debug, error, info, warn};

use crate::{
    agent::{Agent, described},
    config::Config,
    rpc::{self, Command, DialogAnswer, Event, SessionState, Streaming},
};

/// Correlation identifier of the one command the service sends unasked.
const BOOT: &str = "boot";

/// How many lines of conversation a connecting frontend is handed.
///
/// A screenful, not a history. Anything deeper is in the session file and is
/// fetched from the agent with `get_entries`.
const TRANSCRIPT_ENTRIES: usize = 200;

/// Longest line accepted from the agent. A complete message can be large.
const MAX_EVENT_BYTES: usize = 4 * 1024 * 1024;

/// How long an agent has to run before its start counts as having worked.
const HEALTHY: Duration = Duration::from_secs(10);

/// How long to wait before starting an agent that just died.
const RESTART_DELAY: Duration = Duration::from_secs(1);

/// How many quick deaths in a row stop the service from trying again.
const MAX_FAILURES: u32 = 3;

/// How long the agent has to connect back before the service says it has not.
///
/// Generous, because this is a Node process loading extensions and the cost of
/// saying it too early is a warning about nothing.
const HELLO_GRACE: Duration = Duration::from_secs(10);

/// What a widget command is answered with when there is no screen to open it
/// on. The agent is waiting on an answer, so silence would hang it.
const NO_FRONTEND: &str = "no_frontend";

/// One connected client, as the service holds it.
struct Client {
    role: Role,
    outbox: SyncSender<ServiceMessage>,
}

/// One command sent to the agent that a client is waiting on the answer to.
struct Pending {
    client: u64,
    id: String,
}

/// Everything the service has to agree with itself about.
struct Inner {
    agent: Option<Agent>,
    /// Which agent run the current reader thread belongs to.
    generation: u64,
    /// When the current agent started, for telling a crash loop from a crash.
    started: Instant,
    /// Whether the current agent has connected back as a client.
    ///
    /// The service reads Pi's own event stream, so an agent carrying none of
    /// the Scufris extensions still holds a working conversation. What is
    /// missing is only what the agent alone can say, so nothing fails and
    /// nothing is reported. This is what makes that visible.
    agent_joined: bool,
    failures: u32,
    state: ScufrisState,
    detail: String,
    session_file: Option<PathBuf>,
    transcript: VecDeque<TranscriptEntry>,
    /// The widgets the frontend last announced, kept for an agent that
    /// connects after it. The agent types its tools from this, and an agent
    /// that restarted has to be told again.
    catalog: Vec<CatalogEntry>,
    clients: HashMap<u64, Client>,
    /// The connection holding the debug lease, when one does.
    lease: Option<u64>,
    pending: HashMap<String, Pending>,
    commands: u64,
    stopping: bool,
}

impl Inner {
    /// Sends one message to one client, dropping a client that cannot keep up.
    fn send(&mut self, client: u64, body: ServiceBody) {
        let message = ServiceMessage::new(body);
        let deliverable = match self.clients.get(&client) {
            Some(held) => held.outbox.try_send(message),
            None => return,
        };
        if let Err(error) = deliverable {
            self.drop_client(client, &error);
        }
    }

    /// Sends one message to every frontend.
    fn push_frontends(&mut self, body: ServiceBody) {
        let message = ServiceMessage::new(body);
        let mut failed = Vec::new();
        for (id, client) in &self.clients {
            if client.role != Role::Frontend {
                continue;
            }
            if let Err(error) = client.outbox.try_send(message.clone()) {
                failed.push((*id, error));
            }
        }
        for (id, error) in failed {
            self.drop_client(id, &error);
        }
    }

    /// Forgets a client whose outbox is full or gone.
    ///
    /// A full outbox is a client that stopped reading. Keeping it would mean
    /// either blocking the service on it or growing without a bound, and a
    /// surface that is not reading is a surface that is not showing anything.
    fn drop_client(&mut self, client: u64, error: &TrySendError<ServiceMessage>) {
        let reason = match error {
            TrySendError::Full(_) => "it stopped reading",
            TrySendError::Disconnected(_) => "it is gone",
        };
        debug!(client, reason, "a client was dropped");
        self.clients.remove(&client);
    }

    /// Records a new state and tells the frontends, if anything changed.
    fn set_state(&mut self, state: ScufrisState, detail: String) {
        // A held lease owns the state. Events from an agent that is on its way
        // out must not report the conversation as running again.
        if self.lease.is_some() && state != ScufrisState::Detached {
            return;
        }
        if self.state == state && self.detail == detail {
            return;
        }
        info!(state = state.name(), detail, "state");
        self.state = state;
        self.detail = detail.clone();
        self.push_frontends(ServiceBody::State {
            id: None,
            state,
            detail,
        });
    }

    /// Sends one message to the connected agent, if one is connected.
    ///
    /// Returns false when there is nothing in the agent role, so the caller can
    /// decide whether that is worth answering.
    fn push_agent(&mut self, body: ServiceBody) -> bool {
        let Some(agent) = self.of_role(Role::Agent).first().copied() else {
            return false;
        };
        self.send(agent, body);
        true
    }

    /// Returns every connected client in one role.
    fn of_role(&self, role: Role) -> Vec<u64> {
        self.clients
            .iter()
            .filter(|(_, held)| held.role == role)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Puts one line on the ring and pushes it to the frontends.
    fn record(&mut self, entry: TranscriptEntry) {
        if self.transcript.len() == TRANSCRIPT_ENTRIES {
            self.transcript.pop_front();
        }
        self.transcript.push_back(entry.clone());
        self.push_frontends(ServiceBody::Transcript { entry });
    }

    /// Returns true when this client is the connected agent.
    ///
    /// These messages carry no request identifier, so a wrong sender is logged
    /// and dropped rather than refused: there is nothing to echo a refusal on.
    fn is_agent(&self, client: u64) -> bool {
        let agent = self.clients.get(&client).map(|held| held.role) == Some(Role::Agent);
        if !agent {
            warn!(client, "an agent-only message came from another role");
        }
        agent
    }

    /// Answers every command the departing agent will never answer.
    fn fail_pending(&mut self, detail: &str) {
        for (_, waiting) in std::mem::take(&mut self.pending) {
            self.send(
                waiting.client,
                ServiceBody::Refused {
                    id: waiting.id,
                    code: refusal::AGENT_UNAVAILABLE.into(),
                    detail: detail.to_string(),
                },
            );
        }
    }
}

/// The running service.
pub struct Service {
    config: Config,
    inner: Mutex<Inner>,
}

impl Service {
    /// Creates a service with no agent running.
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new(Self {
            config,
            inner: Mutex::new(Inner {
                agent: None,
                generation: 0,
                started: Instant::now(),
                agent_joined: false,
                failures: 0,
                state: ScufrisState::Starting,
                detail: String::new(),
                session_file: None,
                transcript: VecDeque::new(),
                catalog: Vec::new(),
                clients: HashMap::new(),
                lease: None,
                pending: HashMap::new(),
                commands: 0,
                stopping: false,
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|held| held.into_inner())
    }

    /// Says so when the agent this service started never connected to it.
    ///
    /// It is not a fault and nothing is stopped: the state, the transcript and
    /// `send` all come off Pi's own event stream, so the conversation works.
    /// What silently does not is everything only the agent can report - what
    /// it answered, the paragraph to speak, and every widget command - and the
    /// result looks exactly like a broken speaker. The binary is named because
    /// the usual cause is a `scufris` from somewhere else on `PATH`, built
    /// without the service extension.
    fn report_a_silent_agent(&self, generation: u64) {
        if !self.agent_is_silent(generation) {
            return;
        }
        warn!(
            agent = %self.config.agent.display(),
            "the agent has not connected to the service: it cannot report what it said, \
             what to speak, or any widget. It is probably built without the Scufris \
             service extension. Point --agent or SCUFRIS_SERVICE_AGENT at one that has it."
        );
    }

    /// Whether the agent of `generation` is the one running and has never
    /// connected.
    ///
    /// The generation is what makes a late watcher harmless: an agent that has
    /// already been replaced is not the one anybody is waiting on.
    fn agent_is_silent(&self, generation: u64) -> bool {
        let inner = self.lock();
        !inner.stopping
            && inner.generation == generation
            && inner.agent.is_some()
            && !inner.agent_joined
    }

    /// Starts the agent, unless one is running, a lease holds it, or the
    /// service is shutting down.
    pub fn start_agent(self: &Arc<Self>) {
        let mut inner = self.lock();
        if inner.stopping || inner.lease.is_some() || inner.agent.is_some() {
            return;
        }
        inner.generation += 1;
        inner.started = Instant::now();
        inner.agent_joined = false;
        let generation = inner.generation;
        let (agent, streams) = match Agent::start(&self.config) {
            Ok(started) => started,
            Err(error) => {
                inner.failures += 1;
                error!(%error, agent = %self.config.agent.display(), "the agent would not start");
                inner.set_state(
                    ScufrisState::Error,
                    format!("the agent would not start: {error}"),
                );
                return;
            }
        };
        info!(command = described(&self.config), "the agent is starting");
        let writer = agent.writer();
        inner.agent = Some(agent);
        inner.set_state(ScufrisState::Starting, String::new());
        drop(inner);

        let reader = Arc::clone(self);
        thread::Builder::new()
            .name("scufris-agent-events".into())
            .spawn(move || {
                read_events(&reader, streams.stdout);
                reader.agent_ended(generation);
            })
            .expect("the event reader thread starts");
        thread::Builder::new()
            .name("scufris-agent-stderr".into())
            .spawn(move || drain_stderr(streams.stderr))
            .expect("the stderr thread starts");

        let waiting = Arc::clone(self);
        thread::Builder::new()
            .name("scufris-agent-hello".into())
            .spawn(move || {
                thread::sleep(HELLO_GRACE);
                waiting.report_a_silent_agent(generation);
            })
            .expect("the hello thread starts");

        // The one command nobody asked for. Its answer is how the service
        // learns which session file the conversation is in, which is the one
        // thing a debug lease cannot be handed without.
        if let Err(error) = writer.send(&Command::GetState { id: BOOT.into() }) {
            warn!(%error, "the agent would not take its first command");
        }
    }

    /// Applies one event from the agent.
    pub fn apply(self: &Arc<Self>, event: Event) {
        match event {
            Event::Response {
                id: Some(id),
                success,
                error,
                data,
            } if id == BOOT => {
                let mut inner = self.lock();
                if !success {
                    let detail = error.unwrap_or_else(|| "the agent refused to say".into());
                    warn!(detail, "the agent would not report its session");
                    inner.set_state(ScufrisState::Error, detail);
                    return;
                }
                let session = SessionState::from_data(&data);
                if let Some(file) = &session.file {
                    info!(session = file, "the agent is on a session");
                }
                inner.session_file = session.file.map(PathBuf::from);
                inner.failures = 0;
                let state = if session.streaming {
                    ScufrisState::Working
                } else {
                    ScufrisState::Idle
                };
                inner.set_state(state, String::new());
            }
            Event::Response {
                id: Some(id),
                success,
                error,
                ..
            } => {
                let mut inner = self.lock();
                let Some(waiting) = inner.pending.remove(&id) else {
                    return;
                };
                let body = if success {
                    ServiceBody::Ok { id: waiting.id }
                } else {
                    ServiceBody::Refused {
                        id: waiting.id,
                        code: refusal::AGENT_REFUSED.into(),
                        detail: error.unwrap_or_else(|| "the agent refused it".into()),
                    }
                };
                inner.send(waiting.client, body);
            }
            Event::Response { id: None, .. } => {}
            Event::AgentStart => self.lock().set_state(ScufrisState::Working, String::new()),
            Event::AgentSettled => self.lock().set_state(ScufrisState::Idle, String::new()),
            Event::MessageEnd { message } => {
                let Some(entry) = rpc::transcript_entry(&message) else {
                    return;
                };
                self.lock().record(entry);
            }
            Event::ExtensionUiRequest {
                id,
                method,
                message,
            } => self.answer_dialog(&id, &method, message.as_deref()),
            Event::ExtensionError { error } => {
                warn!(
                    error = error.unwrap_or_else(|| "an extension threw".into()),
                    "extension"
                );
            }
            Event::Other => {}
        }
    }

    /// Answers one extension user-interface request.
    ///
    /// A dialog blocks the agent until something answers it. Nothing here can
    /// ask a person yet, so every dialog is cancelled, which is what the
    /// extension sees when a person presses Escape. The fire-and-forget half
    /// is logged and left alone: the agent is not waiting on those, and an
    /// answer to one is a line Pi has to throw away.
    fn answer_dialog(&self, id: &str, method: &str, message: Option<&str>) {
        if !rpc::is_dialog(method) {
            if method == "notify" {
                info!(notify = message.unwrap_or_default(), "the agent said");
            } else {
                debug!(method, "the agent asked for a surface there is none of");
            }
            return;
        }
        let writer = {
            let inner = self.lock();
            inner.agent.as_ref().map(Agent::writer)
        };
        let Some(writer) = writer else { return };
        debug!(method, "a dialog was cancelled: there is nobody to ask");
        if let Err(error) = writer.send(&DialogAnswer::cancel(id.to_string())) {
            warn!(%error, method, "a dialog could not be answered, the agent may be stuck");
        }
    }

    /// Handles the agent's stream ending, and decides whether to start again.
    fn agent_ended(self: &Arc<Self>, generation: u64) {
        let mut inner = self.lock();
        if inner.generation != generation {
            return;
        }
        let agent = inner.agent.take();
        inner.fail_pending("the agent stopped");
        let ran = inner.started.elapsed();
        let stopping = inner.stopping;
        let leased = inner.lease.is_some();
        drop(inner);

        if let Some(agent) = agent {
            let status = agent.stop();
            info!(?status, ?ran, "the agent ended");
        }
        if stopping || leased {
            // Expected: the service is going down, or a terminal took the
            // session. Neither is a fault and neither wants a restart.
            return;
        }

        let mut inner = self.lock();
        if ran >= HEALTHY {
            inner.failures = 0;
        }
        inner.failures += 1;
        let failures = inner.failures;
        if failures >= MAX_FAILURES {
            error!(failures, "the agent keeps stopping, leaving it stopped");
            inner.set_state(
                ScufrisState::Error,
                format!("the agent stopped {failures} times in a row"),
            );
            return;
        }
        inner.set_state(ScufrisState::Starting, "the agent is restarting".into());
        drop(inner);
        thread::sleep(RESTART_DELAY);
        self.start_agent();
    }

    /// Registers one connected client and gives it what its role is owed.
    ///
    /// A second frontend replaces the first rather than being fanned out to,
    /// and so does a second agent. By L1 there is one surface and one agent, so
    /// a second connection is that one having restarted, and holding the old
    /// one open would leave the new one talking to a socket nobody reads.
    pub fn register(&self, client: u64, role: Role, outbox: SyncSender<ServiceMessage>) {
        let mut inner = self.lock();
        if role != Role::Control {
            for id in inner.of_role(role) {
                info!(
                    client = id,
                    role = role.name(),
                    "a second client took the first one's place"
                );
                inner.clients.remove(&id);
            }
        }
        inner.clients.insert(client, Client { role, outbox });
        debug!(client, role = role.name(), "a client connected");
        inner.send(client, ServiceBody::Welcome { role });
        match role {
            Role::Frontend => {
                let (state, detail) = (inner.state, inner.detail.clone());
                inner.send(
                    client,
                    ServiceBody::State {
                        id: None,
                        state,
                        detail,
                    },
                );
                for entry in inner.transcript.clone() {
                    inner.send(client, ServiceBody::Transcript { entry });
                }
            }
            Role::Agent => {
                if !inner.agent_joined {
                    inner.agent_joined = true;
                    info!("the agent is a client of the service");
                }
                // The agent is replaced under the frontend every time a debug
                // lease ends, and the catalog was announced once, long before
                // that. Remembering it is what makes the new agent's widget
                // tools exist without the frontend having to reconnect.
                if inner.catalog.is_empty() {
                    return;
                }
                let widgets = inner.catalog.clone();
                inner.send(
                    client,
                    ServiceBody::Report {
                        report: WidgetReport::Catalog { widgets },
                    },
                );
            }
            Role::Control => {}
        }
    }

    /// Puts one line the assistant said on the transcript.
    ///
    /// The agent reports this rather than the service reading it off the event
    /// stream: Scufris answers through a tool call, not an assistant text
    /// block, and what it meant to say is the agent's to know.
    pub fn said(&self, client: u64, text: String) {
        let mut inner = self.lock();
        if !inner.is_agent(client) {
            return;
        }
        inner.record(TranscriptEntry {
            speaker: Speaker::Assistant,
            text,
        });
    }

    /// Hands one line to whatever owns the speaker.
    ///
    /// Not kept and not queued. Speech that arrived while nothing was listening
    /// is speech nobody asked for, and the transcript already has the words.
    pub fn speak(&self, client: u64, text: String) {
        let mut inner = self.lock();
        if !inner.is_agent(client) {
            return;
        }
        inner.push_frontends(ServiceBody::Speak { text });
    }

    /// Relays one widget command from the agent to the frontend.
    ///
    /// The service does not read the command. It relays it because neither end
    /// knows where the other one is: the agent is a child it starts and the
    /// frontend is a window that comes and goes.
    pub fn relay_widget(&self, client: u64, command: WidgetCommand) {
        let mut inner = self.lock();
        if !inner.is_agent(client) {
            return;
        }
        if inner.of_role(Role::Frontend).is_empty() {
            // The agent is waiting on an answer to this, so it gets one.
            let id = command.id().to_string();
            debug!(command = command.name(), "a widget command has no screen");
            inner.send(
                client,
                ServiceBody::Report {
                    report: WidgetReport::Failed {
                        id,
                        code: NO_FRONTEND.into(),
                        detail: "there is no frontend connected".into(),
                    },
                },
            );
            return;
        }
        inner.push_frontends(ServiceBody::Widget { command });
    }

    /// Relays the agent's request for the conversation window.
    ///
    /// The service answers this one itself rather than waiting for the
    /// frontend, because the only failure the agent can do anything about is
    /// that there is no screen, and the service is what knows that. A window
    /// the frontend owns raising itself does not half happen.
    pub fn relay_conversation(&self, client: u64, id: String, up: bool) {
        let mut inner = self.lock();
        if !inner.is_agent(client) {
            return;
        }
        if inner.of_role(Role::Frontend).is_empty() {
            debug!(up, "the conversation window has no screen");
            inner.send(
                client,
                ServiceBody::Refused {
                    id,
                    code: NO_FRONTEND.into(),
                    detail: "there is no frontend connected".into(),
                },
            );
            return;
        }
        inner.push_frontends(ServiceBody::Conversation { up });
        inner.send(client, ServiceBody::Ok { id });
    }

    /// Relays one widget report from the frontend to the agent.
    pub fn relay_report(&self, client: u64, report: WidgetReport) {
        let mut inner = self.lock();
        if inner.clients.get(&client).map(|held| held.role) != Some(Role::Frontend) {
            warn!(
                client,
                "a widget report came from something that is not a frontend"
            );
            return;
        }
        if let WidgetReport::Catalog { widgets } = &report {
            info!(
                widgets = widgets.len(),
                "the frontend announced its widgets"
            );
            inner.catalog = widgets.clone();
        }
        if !inner.push_agent(ServiceBody::Report { report }) {
            debug!(client, "a widget report arrived with no agent to take it");
        }
    }

    /// Forgets one client, releasing its debug lease if it held one.
    pub fn unregister(self: &Arc<Self>, client: u64) {
        let mut inner = self.lock();
        inner.clients.remove(&client);
        if inner.lease != Some(client) {
            return;
        }
        // This is the whole lease. The connection is gone, so the agent comes
        // back, whether the terminal exited cleanly or was killed outright.
        inner.lease = None;
        info!(client, "the debug lease was released");
        drop(inner);
        self.start_agent();
    }

    /// Answers one client's request for the current state.
    pub fn report_state(&self, client: u64, id: String) {
        let mut inner = self.lock();
        let (state, detail) = (inner.state, inner.detail.clone());
        inner.send(
            client,
            ServiceBody::State {
                id: Some(id),
                state,
                detail,
            },
        );
    }

    /// Sends one line to the agent as a user message.
    ///
    /// A submission during a run is a steer rather than a refusal, which is
    /// what makes one activation key enough: what you say is a prompt when the
    /// assistant is idle and a steer when it is working.
    pub fn submit(&self, client: u64, id: String, text: String) {
        let streaming = if self.lock().state == ScufrisState::Working {
            Some(Streaming::Steer)
        } else {
            None
        };
        self.command(client, id, |correlation| Command::Prompt {
            id: correlation,
            message: text,
            streaming_behavior: streaming,
        });
    }

    /// Ends the current agent run.
    pub fn abort(&self, client: u64, id: String) {
        self.command(client, id, |correlation| Command::Abort { id: correlation });
    }

    /// Sends one correlated command and leaves the client waiting for the
    /// agent's own answer.
    fn command(&self, client: u64, id: String, build: impl FnOnce(String) -> Command) {
        let mut inner = self.lock();
        if inner.lease.is_some() {
            inner.send(
                client,
                ServiceBody::Refused {
                    id,
                    code: refusal::DETACHED.into(),
                    detail: "a terminal has the session".into(),
                },
            );
            return;
        }
        let Some(writer) = inner.agent.as_ref().map(Agent::writer) else {
            inner.send(
                client,
                ServiceBody::Refused {
                    id,
                    code: refusal::AGENT_UNAVAILABLE.into(),
                    detail: "the agent is not running".into(),
                },
            );
            return;
        };
        inner.commands += 1;
        let correlation = format!("c-{}", inner.commands);
        let command = build(correlation.clone());
        inner.pending.insert(
            correlation.clone(),
            Pending {
                client,
                id: id.clone(),
            },
        );
        drop(inner);

        if let Err(error) = writer.send(&command) {
            let mut inner = self.lock();
            inner.pending.remove(&correlation);
            warn!(%error, "the agent would not take a command");
            inner.send(
                client,
                ServiceBody::Refused {
                    id,
                    code: refusal::AGENT_UNAVAILABLE.into(),
                    detail: format!("the agent would not take it: {error}"),
                },
            );
        }
    }

    /// Takes the agent away and hands the caller the command line that resumes
    /// its session in a terminal.
    ///
    /// The lease belongs to this connection from here until it closes.
    pub fn begin_debug(&self, client: u64, id: String) {
        let mut inner = self.lock();
        if inner.lease.is_some() {
            inner.send(
                client,
                ServiceBody::Refused {
                    id,
                    code: refusal::DEBUG_HELD.into(),
                    detail: "a terminal already has the session".into(),
                },
            );
            return;
        }
        if inner.clients.get(&client).map(|held| held.role) != Some(Role::Control) {
            inner.send(
                client,
                ServiceBody::Refused {
                    id,
                    code: refusal::WRONG_ROLE.into(),
                    detail: "only a control client may take the session".into(),
                },
            );
            return;
        }
        inner.lease = Some(client);
        let agent = inner.agent.take();
        let session = inner.session_file.clone();
        inner.fail_pending("a terminal took the session");
        inner.set_state(ScufrisState::Detached, "a terminal has the session".into());
        drop(inner);

        if let Some(agent) = agent {
            let status = agent.stop();
            info!(?status, "the agent was handed to a terminal");
        }
        let mut inner = self.lock();
        inner.send(
            client,
            ServiceBody::Debug {
                id,
                program: self.config.agent.display().to_string(),
                args: self.config.debug_args(session.as_deref()),
            },
        );
    }

    /// Stops the agent for good. The service is going down.
    pub fn shutdown(&self) {
        let mut inner = self.lock();
        inner.stopping = true;
        let agent = inner.agent.take();
        inner.fail_pending("the service is stopping");
        drop(inner);
        if let Some(agent) = agent {
            let status = agent.stop();
            info!(?status, "the agent stopped with the service");
        }
    }
}

/// Reads the agent's event stream until it ends.
fn read_events(service: &Arc<Service>, stdout: impl std::io::Read) {
    let mut reader = std::io::BufReader::new(stdout);
    loop {
        let mut line = Vec::new();
        let mut bounded = std::io::Read::take(&mut reader, (MAX_EVENT_BYTES + 1) as u64);
        match bounded.read_until(b'\n', &mut line) {
            Ok(0) => return,
            Ok(_) => {}
            Err(error) => {
                warn!(%error, "the agent's events could not be read");
                return;
            }
        }
        if line.len() > MAX_EVENT_BYTES {
            warn!("an agent event was oversized and the stream was abandoned");
            return;
        }
        // LF is the only record separator in RPC mode. A trailing CR is
        // tolerated on input, and anything else stays inside the JSON.
        while matches!(line.last(), Some(b'\n' | b'\r')) {
            line.pop();
        }
        if line.is_empty() {
            continue;
        }
        match serde_json::from_slice::<Event>(&line) {
            Ok(event) => service.apply(event),
            Err(error) => {
                warn!(%error, "an agent event did not parse");
            }
        }
    }
}

/// Keeps the agent's diagnostics in the journal rather than in a full pipe.
///
/// Draining matters more than reading: a child whose stderr nobody reads
/// blocks on its first long backtrace.
fn drain_stderr(stderr: impl std::io::Read) {
    let reader = std::io::BufReader::new(stderr);
    for line in reader.split(b'\n') {
        match line {
            Ok(line) if line.is_empty() => {}
            Ok(line) => warn!(agent = %String::from_utf8_lossy(&line), "agent"),
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::Path,
        sync::mpsc::{Receiver, sync_channel},
    };

    use scufris_control::service::Posture;
    use serde_json::json;

    use super::*;

    fn service() -> Arc<Service> {
        Service::new(Config {
            agent: PathBuf::from("/nonexistent/scufris"),
            session_dir: PathBuf::from("/srv/sessions"),
            socket: PathBuf::from("/run/user/1000/scufris/service.sock"),
            working_dir: std::env::temp_dir(),
        })
    }

    fn connect(service: &Arc<Service>, client: u64, role: Role) -> Receiver<ServiceMessage> {
        let (outbox, inbox) = sync_channel(crate::server::OUTBOX);
        service.register(client, role, outbox);
        assert_eq!(
            inbox.try_recv().expect("it is welcomed").body,
            ServiceBody::Welcome { role }
        );
        inbox
    }

    fn drain(inbox: &Receiver<ServiceMessage>) -> Vec<ServiceBody> {
        std::iter::from_fn(|| inbox.try_recv().ok())
            .map(|message| message.body)
            .collect()
    }

    #[test]
    fn a_frontend_is_given_the_state_and_the_conversation_when_it_connects() {
        let service = service();
        service.apply(Event::MessageEnd {
            message: json!({ "role": "user", "content": "hello" }),
        });
        service.apply(Event::MessageEnd {
            message: json!({ "role": "assistant", "content": [{ "type": "text", "text": "hi" }] }),
        });
        let inbox = connect(&service, 1, Role::Frontend);
        assert_eq!(
            drain(&inbox),
            [
                ServiceBody::State {
                    id: None,
                    state: ScufrisState::Starting,
                    detail: String::new(),
                },
                ServiceBody::Transcript {
                    entry: TranscriptEntry {
                        speaker: Speaker::User,
                        text: "hello".into(),
                    },
                },
                ServiceBody::Transcript {
                    entry: TranscriptEntry {
                        speaker: Speaker::Assistant,
                        text: "hi".into(),
                    },
                },
            ]
        );
        // A control client is not a surface. It asks and it is answered, and
        // it is never handed a backlog it did not ask for.
        let control = connect(&service, 2, Role::Control);
        assert!(drain(&control).is_empty());
    }

    #[test]
    fn state_changes_reach_frontends_and_repeats_do_not() {
        let service = service();
        let inbox = connect(&service, 1, Role::Frontend);
        drain(&inbox);
        service.apply(Event::AgentStart);
        service.apply(Event::AgentStart);
        service.apply(Event::AgentSettled);
        assert_eq!(
            drain(&inbox),
            [
                ServiceBody::State {
                    id: None,
                    state: ScufrisState::Working,
                    detail: String::new(),
                },
                ServiceBody::State {
                    id: None,
                    state: ScufrisState::Idle,
                    detail: String::new(),
                },
            ]
        );
    }

    #[test]
    fn the_first_answer_names_the_session_and_settles_the_state() {
        let service = service();
        service.apply(Event::Response {
            id: Some(BOOT.into()),
            success: true,
            error: None,
            data: json!({ "sessionFile": "/srv/sessions/one.jsonl", "isStreaming": true }),
        });
        assert_eq!(service.lock().state, ScufrisState::Working);
        assert_eq!(
            service.lock().session_file.as_deref(),
            Some(Path::new("/srv/sessions/one.jsonl"))
        );
    }

    #[test]
    fn a_transcript_ring_holds_a_screenful_and_forgets_the_rest() {
        let service = service();
        for index in 0..TRANSCRIPT_ENTRIES + 10 {
            service.apply(Event::MessageEnd {
                message: json!({ "role": "user", "content": format!("line {index}") }),
            });
        }
        let held = service.lock().transcript.clone();
        assert_eq!(held.len(), TRANSCRIPT_ENTRIES);
        assert_eq!(held.front().expect("it has a front").text, "line 10");
        assert_eq!(
            held.back().expect("it has a back").text,
            format!("line {}", TRANSCRIPT_ENTRIES + 9)
        );
    }

    #[test]
    fn a_second_frontend_takes_the_first_one_s_place() {
        let service = service();
        let first = connect(&service, 1, Role::Frontend);
        let second = connect(&service, 2, Role::Frontend);
        drain(&first);
        drain(&second);
        service.apply(Event::AgentStart);
        assert!(
            drain(&first).is_empty(),
            "the displaced frontend is no longer written to"
        );
        assert_eq!(drain(&second).len(), 1);
    }

    #[test]
    fn with_no_agent_a_submission_is_refused_rather_than_swallowed() {
        let service = service();
        let inbox = connect(&service, 1, Role::Control);
        service.submit(1, "c-1".into(), "hello".into());
        assert_eq!(
            drain(&inbox),
            [ServiceBody::Refused {
                id: "c-1".into(),
                code: refusal::AGENT_UNAVAILABLE.into(),
                detail: "the agent is not running".into(),
            }]
        );
    }

    #[test]
    fn the_client_that_asked_is_the_client_that_is_answered() {
        let service = service();
        let one = connect(&service, 1, Role::Control);
        let two = connect(&service, 2, Role::Control);
        // Two clients waiting at once. The correlation is the service's own,
        // because a client identifier is only unique to that client.
        service.lock().pending.insert(
            "c-1".into(),
            Pending {
                client: 1,
                id: "mine".into(),
            },
        );
        service.lock().pending.insert(
            "c-2".into(),
            Pending {
                client: 2,
                id: "mine".into(),
            },
        );
        service.apply(Event::Response {
            id: Some("c-2".into()),
            success: false,
            error: Some("no".into()),
            data: serde_json::Value::Null,
        });
        assert!(drain(&one).is_empty());
        assert_eq!(
            drain(&two),
            [ServiceBody::Refused {
                id: "mine".into(),
                code: refusal::AGENT_REFUSED.into(),
                detail: "no".into(),
            }]
        );
        service.apply(Event::Response {
            id: Some("c-1".into()),
            success: true,
            error: None,
            data: serde_json::Value::Null,
        });
        assert_eq!(drain(&one), [ServiceBody::Ok { id: "mine".into() }]);
    }

    #[test]
    fn a_debug_lease_detaches_and_is_held_by_the_connection_that_asked() {
        let service = service();
        let frontend = connect(&service, 1, Role::Frontend);
        let control = connect(&service, 2, Role::Control);
        drain(&frontend);
        service.begin_debug(2, "c-1".into());

        assert_eq!(service.lock().state, ScufrisState::Detached);
        assert_eq!(
            drain(&control),
            [ServiceBody::Debug {
                id: "c-1".into(),
                program: "/nonexistent/scufris".into(),
                // No session file has been reported, so the directory and
                // `--continue` name the same conversation.
                args: vec![
                    "--session-dir".into(),
                    "/srv/sessions".into(),
                    "--continue".into(),
                ],
            }]
        );
        assert_eq!(
            drain(&frontend),
            [ServiceBody::State {
                id: None,
                state: ScufrisState::Detached,
                detail: "a terminal has the session".into(),
            }]
        );

        // While it is held, the conversation belongs to the terminal.
        service.submit(1, "c-2".into(), "hello".into());
        assert_eq!(
            drain(&frontend),
            [ServiceBody::Refused {
                id: "c-2".into(),
                code: refusal::DETACHED.into(),
                detail: "a terminal has the session".into(),
            }]
        );
        // And an agent event cannot report the conversation as running again.
        service.apply(Event::AgentSettled);
        assert_eq!(service.lock().state, ScufrisState::Detached);
    }

    #[test]
    fn a_second_debug_is_refused_while_a_lease_is_held() {
        let service = service();
        let first = connect(&service, 1, Role::Control);
        let second = connect(&service, 2, Role::Control);
        service.begin_debug(1, "c-1".into());
        drain(&first);
        service.begin_debug(2, "c-2".into());
        assert_eq!(
            drain(&second),
            [ServiceBody::Refused {
                id: "c-2".into(),
                code: refusal::DEBUG_HELD.into(),
                detail: "a terminal already has the session".into(),
            }]
        );
    }

    #[test]
    fn a_frontend_may_not_take_the_session_away_from_itself() {
        let service = service();
        let inbox = connect(&service, 1, Role::Frontend);
        drain(&inbox);
        service.begin_debug(1, "c-1".into());
        assert_eq!(
            drain(&inbox),
            [ServiceBody::Refused {
                id: "c-1".into(),
                code: refusal::WRONG_ROLE.into(),
                detail: "only a control client may take the session".into(),
            }]
        );
        assert_eq!(service.lock().state, ScufrisState::Starting);
    }

    #[test]
    fn losing_the_connection_releases_the_lease() {
        let service = service();
        connect(&service, 1, Role::Control);
        service.begin_debug(1, "c-1".into());
        assert_eq!(service.lock().state, ScufrisState::Detached);
        // The connection closing is the only signal, and it arrives the same
        // way whether the terminal exited or the client was killed.
        service.unregister(1);
        assert!(service.lock().lease.is_none());
        // Starting again failed here, because the configured agent does not
        // exist. What matters is that the lease is gone and the service tried.
        assert_eq!(service.lock().state, ScufrisState::Error);
    }

    #[test]
    fn a_client_that_stops_reading_is_dropped_rather_than_blocking_the_service() {
        let service = service();
        let (outbox, inbox) = sync_channel(1);
        service.register(1, Role::Frontend, outbox);
        // One welcome fills the channel of depth one. Everything after it has
        // nowhere to go, and the service must not wait for it.
        service.apply(Event::AgentStart);
        assert!(!service.lock().clients.contains_key(&1));
        drop(inbox);
    }

    #[test]
    fn the_agent_puts_its_own_answer_on_the_transcript() {
        // The service cannot read this off the event stream: Scufris answers
        // through a tool call rather than an assistant text block.
        let service = service();
        let frontend = connect(&service, 1, Role::Frontend);
        let agent = connect(&service, 2, Role::Agent);
        drain(&frontend);
        service.said(2, "the harness is green".into());
        assert_eq!(
            drain(&frontend),
            [ServiceBody::Transcript {
                entry: TranscriptEntry {
                    speaker: Speaker::Assistant,
                    text: "the harness is green".into(),
                },
            }]
        );
        assert_eq!(service.lock().transcript.len(), 1);
        assert!(drain(&agent).is_empty());
    }

    #[test]
    fn speech_goes_to_the_speaker_and_is_never_kept() {
        let service = service();
        let frontend = connect(&service, 1, Role::Frontend);
        drain(&frontend);
        connect(&service, 2, Role::Agent);
        service.speak(2, "the harness is green".into());
        assert_eq!(
            drain(&frontend),
            [ServiceBody::Speak {
                text: "the harness is green".into(),
            }]
        );
        // Speech that arrived while nothing was listening is speech nobody
        // asked for, so a frontend that connects later is not handed it.
        assert!(service.lock().transcript.is_empty());
        let later = connect(&service, 3, Role::Frontend);
        assert_eq!(
            drain(&later),
            [ServiceBody::State {
                id: None,
                state: ScufrisState::Starting,
                detail: String::new(),
            }]
        );
    }

    #[test]
    fn a_control_client_cannot_speak_for_the_agent() {
        let service = service();
        let frontend = connect(&service, 1, Role::Frontend);
        let control = connect(&service, 2, Role::Control);
        drain(&frontend);
        service.said(2, "I am the assistant".into());
        service.speak(2, "I am the assistant".into());
        service.relay_widget(2, WidgetCommand::Clear { id: "w-1".into() });
        assert!(drain(&frontend).is_empty());
        assert!(drain(&control).is_empty());
        assert!(service.lock().transcript.is_empty());
    }

    #[test]
    fn widgets_are_relayed_both_ways_between_the_agent_and_the_frontend() {
        let service = service();
        let frontend = connect(&service, 1, Role::Frontend);
        let agent = connect(&service, 2, Role::Agent);
        drain(&frontend);
        let command = WidgetCommand::Open {
            id: "w-1".into(),
            widget: "clock".into(),
            posture: Posture::Exhibit,
            data: json!({}),
        };
        service.relay_widget(2, command.clone());
        assert_eq!(drain(&frontend), [ServiceBody::Widget { command }]);
        let report = WidgetReport::Opened {
            id: "w-1".into(),
            surface: "clock-1".into(),
        };
        service.relay_report(1, report.clone());
        assert_eq!(drain(&agent), [ServiceBody::Report { report }]);
    }

    #[test]
    fn a_widget_command_with_no_screen_is_answered_rather_than_dropped() {
        // The agent waits on an answer to every command it sends. Silence here
        // is a tool call that never returns.
        let service = service();
        let agent = connect(&service, 1, Role::Agent);
        service.relay_widget(
            1,
            WidgetCommand::Update {
                id: "w-1".into(),
                surface: "clock-1".into(),
                data: json!({}),
            },
        );
        assert_eq!(
            drain(&agent),
            [ServiceBody::Report {
                report: WidgetReport::Failed {
                    id: "w-1".into(),
                    code: NO_FRONTEND.into(),
                    detail: "there is no frontend connected".into(),
                },
            }]
        );
    }

    /// The service answers this one itself. The frontend is told and says
    /// nothing back, because a window it owns raising itself does not half
    /// happen and the agent could not act on it if it did.
    #[test]
    fn the_conversation_window_is_relayed_and_answered_by_the_service() {
        let service = service();
        let frontend = connect(&service, 1, Role::Frontend);
        let agent = connect(&service, 2, Role::Agent);
        drain(&frontend);
        service.relay_conversation(2, "c-1".into(), true);
        assert_eq!(
            drain(&frontend),
            [ServiceBody::Conversation { up: true }],
            "the window is told what to be"
        );
        assert_eq!(drain(&agent), [ServiceBody::Ok { id: "c-1".into() }]);
        service.relay_conversation(2, "c-2".into(), false);
        assert_eq!(drain(&frontend), [ServiceBody::Conversation { up: false }]);
        assert_eq!(drain(&agent), [ServiceBody::Ok { id: "c-2".into() }]);
    }

    /// The one failure the agent can do anything about, and the only reason
    /// this carries an identifier at all: a machine with no screen is a
    /// machine where saying "it is on screen" would be a lie.
    #[test]
    fn asking_for_the_conversation_window_with_no_screen_is_refused() {
        let service = service();
        let agent = connect(&service, 1, Role::Agent);
        service.relay_conversation(1, "c-1".into(), true);
        assert_eq!(
            drain(&agent),
            [ServiceBody::Refused {
                id: "c-1".into(),
                code: NO_FRONTEND.into(),
                detail: "there is no frontend connected".into(),
            }]
        );
    }

    /// The same rule every agent-only verb has. A terminal is not a screen.
    #[test]
    fn a_control_client_cannot_ask_for_the_conversation_window() {
        let service = service();
        let frontend = connect(&service, 1, Role::Frontend);
        let control = connect(&service, 2, Role::Control);
        drain(&frontend);
        service.relay_conversation(2, "c-1".into(), true);
        assert!(drain(&frontend).is_empty());
        assert!(drain(&control).is_empty());
    }

    #[test]
    fn the_catalog_is_remembered_for_the_agent_that_comes_after_it() {
        // The agent is replaced under the frontend every time a debug lease
        // ends, and the catalog was announced once, before any of that.
        let service = service();
        connect(&service, 1, Role::Frontend);
        let widgets = vec![CatalogEntry {
            id: "clock".into(),
            name: "Clock".into(),
            description: "Shows the time in one zone.".into(),
        }];
        service.relay_report(
            1,
            WidgetReport::Catalog {
                widgets: widgets.clone(),
            },
        );
        let agent = connect(&service, 2, Role::Agent);
        assert_eq!(
            drain(&agent),
            [ServiceBody::Report {
                report: WidgetReport::Catalog {
                    widgets: widgets.clone()
                },
            }]
        );
        // And a restarted agent is told again, because it kept nothing.
        let restarted = connect(&service, 3, Role::Agent);
        assert_eq!(
            drain(&restarted),
            [ServiceBody::Report {
                report: WidgetReport::Catalog { widgets },
            }]
        );
    }

    #[test]
    fn a_second_agent_takes_the_first_one_s_place() {
        let service = service();
        let first = connect(&service, 1, Role::Agent);
        let second = connect(&service, 2, Role::Agent);
        connect(&service, 3, Role::Frontend);
        service.relay_report(
            3,
            WidgetReport::Closed {
                surface: "clock-1".into(),
            },
        );
        assert!(
            drain(&first).is_empty(),
            "the displaced agent is no longer written to"
        );
        assert_eq!(drain(&second).len(), 1);
    }

    #[test]
    fn the_agent_is_asked_for_its_session_and_the_state_says_it_is_starting() {
        // The configured agent does not exist, so the failure path is what is
        // reachable here: it is reported rather than retried forever.
        let service = service();
        service.start_agent();
        assert_eq!(service.lock().state, ScufrisState::Error);
        assert_eq!(service.lock().failures, 1);
    }

    #[test]
    fn an_agent_that_never_connects_back_is_noticed() {
        // A running agent is what makes this reachable, and the stand-in agent
        // is one: it stays up until the service says goodbye and it connects
        // to nothing, which is the whole case.
        let home = std::env::temp_dir().join(format!("scufris-silent-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();

        let service = Service::new(Config {
            agent: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agent"),
            session_dir: home.join("sessions"),
            socket: home.join("service.sock"),
            working_dir: home.clone(),
        });
        service.start_agent();
        let generation = service.lock().generation;
        assert!(
            service.agent_is_silent(generation),
            "an agent that has said nothing was taken for one that had"
        );

        // Connecting in the agent role is the hello. Nothing else counts: a
        // frontend is not the half that reports what was said.
        connect(&service, 1, Role::Frontend);
        assert!(service.agent_is_silent(generation));
        connect(&service, 2, Role::Agent);
        assert!(
            !service.agent_is_silent(generation),
            "the agent connected and was still called silent"
        );

        // The watcher for an agent that has been replaced says nothing, so a
        // restart is never reported against whoever is running now.
        assert!(!service.agent_is_silent(generation - 1));

        service.shutdown();
        std::fs::remove_dir_all(&home).unwrap();
    }
}
