//! Canonical protocol v11 service state.

use std::{
    collections::{HashMap, HashSet},
    io::BufRead,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

use scufris_control::refusal;
use scufris_control::service::{
    AgentHolder, AgentRequestBody, AgentResponse, AgentResponseBody, AgentSession,
    AttachmentDescriptor, BriefingDeliveryState, BriefingRow, BriefingWake, ControlResponseBody,
    ConversationMessage, ConversationRole, FOREGROUND_OWNER, JobAction, JobRow, JobRowState,
    LeaseHolder, MAX_CONVERSATION_PAGE, ScufrisState, SurfaceRegistration, SurfaceResponse,
    SurfaceResponseBody, TERMINAL_SURFACE, UNPROMPTED_SURFACE, WidgetCall, WidgetDefinition,
};
use serde_json::Value;
use tracing::{debug, error, info, warn};

use crate::{
    agent::{Agent, described},
    attachment::AttachmentStore,
    briefings::{BriefingStore, DismissError},
    config::Config,
    conversation::ConversationHistory,
    rpc::{self, Command, DialogAnswer, Event, SessionState},
};

const BOOT: &str = "boot";
const MAX_EVENT_BYTES: usize = 4 * 1024 * 1024;
const HEALTHY: Duration = Duration::from_secs(10);
const RESTART_DELAY: Duration = Duration::from_secs(1);
const MAX_FAILURES: u32 = 3;
/// What a person can do about a service that has stopped trying.
///
/// Both ways in here are terminal: nothing retries after them, and the state
/// they publish is the only thing anyone sees. The tray's "Restart backend"
/// runs the same restart, so this is the sentence and not a second mechanism.
const RECOVERY: &str =
    " Restart it from the tray, or with `systemctl --user restart scufris-service`.";
const PROACTIVE_RECOVERY: &str = " Send a message to retry stopped briefings, restart it from the tray, or use `systemctl --user restart scufris-service`.";
const HELLO_GRACE: Duration = Duration::from_secs(10);
/// How often the lease holder is expected to say it is still there.
const LEASE_PING_INTERVAL: Duration = Duration::from_secs(5);
/// Three missed heartbeats end the lease, whether or not the socket closed.
///
/// A terminal that is stopped rather than killed keeps its control socket
/// open with nobody reading it, so the socket alone cannot say the agent is
/// gone. The deadline can, and it is the same path as a disconnection.
const LEASE_DEADLINE: Duration = Duration::from_secs(3 * 5);
/// How long a lease that asked to abort waits for the turn to settle.
#[cfg(not(test))]
const ABORT_SETTLE: Duration = Duration::from_secs(10);
#[cfg(test)]
const ABORT_SETTLE: Duration = Duration::from_millis(200);
/// How often that wait looks again.
#[cfg(not(test))]
const ABORT_STEP: Duration = Duration::from_millis(50);
#[cfg(test)]
const ABORT_STEP: Duration = Duration::from_millis(5);
const MAX_CONSECUTIVE_PROACTIVE_TURNS: u32 = 3;
#[cfg(not(test))]
const PROACTIVE_BACKOFF_MIN: Duration = Duration::from_secs(2);
#[cfg(test)]
const PROACTIVE_BACKOFF_MIN: Duration = Duration::from_millis(20);
#[cfg(not(test))]
const PROACTIVE_BACKOFF_MAX: Duration = Duration::from_secs(30);
#[cfg(test)]
const PROACTIVE_BACKOFF_MAX: Duration = Duration::from_millis(80);

fn proactive_backoff(completed: u32) -> Duration {
    let exponent = completed.saturating_sub(1).min(4);
    PROACTIVE_BACKOFF_MIN
        .saturating_mul(1_u32 << exponent)
        .min(PROACTIVE_BACKOFF_MAX)
}

#[derive(Clone)]
pub struct SurfaceSender {
    pub connection: u64,
    pub generation: u64,
    pub outbox: SyncSender<SurfaceResponse>,
}

struct RegisteredSurface {
    registration: SurfaceRegistration,
    sender: SurfaceSender,
}

struct AgentConnection {
    connection: u64,
    outbox: SyncSender<AgentResponse>,
    /// The lease generation this agent said hello with, when it is a leased
    /// terminal rather than the managed child.
    lease: Option<u64>,
}

/// One terminal lease: which control connection holds the agent, the
/// generation that fences the agent channel while it does, and when the
/// holder last said it was still there.
///
/// The lease is the connection. There is no way to be left detached with
/// nothing to put the agent back, because the control socket closing is the
/// release whether or not the holder said so.
#[derive(Debug, Clone)]
struct Lease {
    connection: u64,
    generation: u64,
    holder: LeaseHolder,
    last_ping: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    Starting,
    Working,
    Idle,
    Failed,
}

struct Inner {
    process: Option<Agent>,
    process_generation: u64,
    started: Instant,
    agent_joined: bool,
    failures: u32,
    lifecycle: Lifecycle,
    lifecycle_detail: String,
    /// Every delegated job the agent owns, as the surfaces draw them.
    ///
    /// This is the only account of delegated work the service keeps. The tray
    /// word is folded from it rather than sent alongside it, so the two cannot
    /// disagree about whether anything needs Alex.
    jobs: Vec<JobRow>,
    /// Durable quiet briefing rows and terminal proactive inbox.
    briefings: BriefingStore,
    /// Terminal event currently holding the one proactive model slot.
    active_proactive: Option<String>,
    /// Pi has delivered the exact queued custom message for that event.
    active_proactive_started: bool,
    /// Fast consecutive proactive turns are bounded independently of Pi's
    /// follow-up queue. A user turn or an inbox that stays empty through the
    /// backoff ends the sequence.
    consecutive_proactive: u32,
    /// Date/profile pairs whose proactive turn started in this sequence.
    /// Repeating one logical run is the runaway signature. A backlog of
    /// distinct daily or profile runs is allowed to drain.
    proactive_runs: HashSet<(String, String)>,
    proactive_not_before: Option<Instant>,
    proactive_timer_pending: bool,
    proactive_circuit_open: bool,
    surfaces: HashMap<String, RegisteredSurface>,
    surface_by_connection: HashMap<u64, (String, u64)>,
    next_surface_generation: u64,
    agent: Option<AgentConnection>,
    conversation: ConversationHistory,
    associated_surface: Option<String>,
    /// The identifier of the submission the open turn started from.
    ///
    /// A surface message and a typed terminal turn both carry one, and an
    /// answer that names it closes exactly that turn. Without it one answer
    /// closes whatever happens to be open, which loses answers the moment a
    /// person can type at any time.
    associated_turn: Option<String>,
    /// The newest session file any agent reported, and who reported it.
    ///
    /// This is what the next holder forks from. The managed child is started
    /// from it only when a terminal wrote it, because the child's own file is
    /// what `--continue` already finds.
    lineage_file: Option<PathBuf>,
    lineage_holder: AgentHolder,
    stopping: bool,
    /// The terminal lease, while a terminal holds the agent.
    lease: Option<Lease>,
    /// Every lease ever granted counts up, so a hello fenced by an old grant
    /// is refused after the terminal that held it has gone.
    lease_generation: u64,
}

impl Inner {
    /// The one word the tray wants, folded from the rows.
    ///
    /// A row outlives its job, so a failure keeps saying so until Alex files
    /// it rather than until the process happens to exit. Filing is the
    /// acknowledgement, and it is what puts the tray back to quiet.
    fn attention(&self) -> Option<(ScufrisState, &str)> {
        let worst = |wanted: JobRowState| {
            self.jobs
                .iter()
                .find(|row| row.state == wanted)
                .map(|row| row.summary.as_str())
        };
        if let Some(summary) = worst(JobRowState::Failed) {
            return Some((ScufrisState::Failed, summary));
        }
        worst(JobRowState::Blocked).map(|summary| (ScufrisState::Blocked, summary))
    }

    fn state(&self) -> (ScufrisState, String) {
        let attention = self.attention();
        if let Some((ScufrisState::Failed, detail)) = attention {
            return (ScufrisState::Failed, detail.to_string());
        }
        if self.proactive_circuit_open {
            return (
                ScufrisState::Failed,
                format!("Briefing delivery stopped for safety.{PROACTIVE_RECOVERY}"),
            );
        }
        if self.lifecycle == Lifecycle::Failed {
            return (ScufrisState::Failed, self.lifecycle_detail.clone());
        }
        if let Some((state, detail)) = attention {
            return (state, detail.to_string());
        }
        match self.lifecycle {
            Lifecycle::Working => (ScufrisState::Working, self.lifecycle_detail.clone()),
            Lifecycle::Starting => (ScufrisState::Starting, self.lifecycle_detail.clone()),
            Lifecycle::Idle => (ScufrisState::Idle, self.lifecycle_detail.clone()),
            Lifecycle::Failed => unreachable!(),
        }
    }

    fn send_surface(&mut self, surface: &str, body: SurfaceResponseBody) {
        debug!(surface, payload = ?body, "sending message to surface");
        let message = SurfaceResponse::new(body);
        let failure = self.surfaces.get(surface).and_then(|held| {
            match held.sender.outbox.try_send(message) {
                Ok(()) => None,
                Err(TrySendError::Full(_)) => Some("it stopped reading"),
                Err(TrySendError::Disconnected(_)) => Some("it is gone"),
            }
        });
        if let Some(reason) = failure {
            self.drop_surface(surface, reason);
        }
    }

    fn broadcast(&mut self, body: SurfaceResponseBody) {
        debug!(recipients = self.surfaces.len(), payload = ?body, "broadcasting message to surfaces");
        let message = SurfaceResponse::new(body);
        let mut failed = Vec::new();
        for (id, held) in &self.surfaces {
            match held.sender.outbox.try_send(message.clone()) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => failed.push((id.clone(), "it stopped reading")),
                Err(TrySendError::Disconnected(_)) => failed.push((id.clone(), "it is gone")),
            }
        }
        for (id, reason) in failed {
            self.drop_surface(&id, reason);
        }
    }

    fn drop_surface(&mut self, id: &str, reason: &str) {
        debug!(surface = id, reason, "a surface was dropped");
        if let Some(old) = self.surfaces.remove(id) {
            info!(
                surface = id,
                name = old.registration.name,
                "surface {} disconnected",
                old.registration.name
            );
            self.surface_by_connection.remove(&old.sender.connection);
        }
    }

    fn send_agent(&mut self, body: AgentResponseBody) -> bool {
        let Some(agent) = &self.agent else {
            debug!(payload = ?body, "message had no connected agent");
            return false;
        };
        debug!(connection = agent.connection, payload = ?body, "sending message to agent");
        if agent.outbox.try_send(AgentResponse::new(body)).is_err() {
            self.agent = None;
            return false;
        }
        true
    }

    /// Adds one message to the canonical replay and shows it everywhere.
    ///
    /// The sequence comes back because a terminal turn is acknowledged with
    /// the place it was recorded at. A replay that could not be stored still
    /// has one: the in-memory ring is the live conversation, and the next
    /// message retries the whole snapshot.
    fn record(&mut self, message: ConversationMessage) -> u64 {
        if let Err(error) = self.conversation.record(message.clone()) {
            warn!(%error, "the canonical conversation could not be stored");
        }
        let sequence = self.conversation.latest_sequence();
        self.broadcast(message.into());
        sequence
    }

    /// Which process is the agent right now.
    fn holder(&self) -> AgentHolder {
        match self.lease {
            Some(_) => AgentHolder::Terminal,
            None => AgentHolder::Managed,
        }
    }

    /// Forgets the agent connection and returns any proactive turn it was
    /// carrying to the inbox.
    fn agent_left(&mut self) {
        self.agent = None;
        self.active_proactive_started = false;
        if let Some(event_id) = self.active_proactive.take() {
            match self.briefings.retry(&event_id) {
                Ok(()) => {
                    self.proactive_not_before =
                        Some(Instant::now() + proactive_backoff(self.consecutive_proactive));
                }
                Err(error) => {
                    self.proactive_circuit_open = true;
                    self.lifecycle = Lifecycle::Failed;
                    self.lifecycle_detail = format!(
                        "Briefing delivery stopped because its retry could not be stored.{RECOVERY}"
                    );
                    if let Err(mark_error) = self.briefings.queued_delivery_failed() {
                        warn!(%mark_error, event = event_id, outcome = "persist_failed", "failed proactive deliveries could not be marked");
                    }
                    warn!(%error, event = event_id, outcome = "circuit_open", "interrupted briefing delivery could not be returned to pending");
                    self.publish_state();
                }
            }
        }
        self.publish_briefings();
    }

    /// Whether this agent connection is the terminal holding the live lease.
    fn agent_holds_lease(&self, connection: u64) -> bool {
        match (&self.agent, &self.lease) {
            (Some(agent), Some(lease)) => {
                agent.connection == connection && agent.lease == Some(lease.generation)
            }
            _ => false,
        }
    }

    /// Opens one owner turn: the words are recorded, the submission that
    /// started it owns the answer that closes it, and any proactive sequence
    /// ends.
    ///
    /// An explicit owner turn is also the in-process recovery boundary for a
    /// circuit stop, so a person can continue without restarting the whole
    /// service.
    fn owner_turn(
        &mut self,
        surface: String,
        turn: String,
        text: String,
        attachments: Vec<AttachmentDescriptor>,
    ) -> u64 {
        if self.proactive_circuit_open {
            match self.briefings.resume_stopped() {
                Ok(resumed) => {
                    self.proactive_circuit_open = false;
                    if self.lifecycle == Lifecycle::Failed
                        && self
                            .lifecycle_detail
                            .starts_with("Briefing delivery stopped")
                    {
                        self.lifecycle = Lifecycle::Idle;
                        self.lifecycle_detail.clear();
                    }
                    info!(
                        resumed,
                        outcome = "recovered",
                        "owner turn reset the proactive circuit"
                    );
                    self.publish_briefings();
                }
                Err(error) => {
                    warn!(%error, outcome = "persist_failed", "owner turn could not reset the proactive circuit")
                }
            }
        }
        if !self.proactive_circuit_open {
            self.consecutive_proactive = 0;
            self.proactive_runs.clear();
            self.proactive_not_before = None;
        }
        self.associated_surface = Some(surface.clone());
        self.associated_turn = Some(turn);
        self.record(ConversationMessage {
            role: ConversationRole::User,
            surface,
            text,
            details: None,
            widgets: None,
            attachments,
            receipts: Vec::new(),
        })
    }

    /// Closes the open turn, whatever it was waiting for.
    fn close_turn(&mut self) {
        self.associated_surface = None;
        self.associated_turn = None;
    }

    /// Whether this session carries the conversation the service recorded.
    ///
    /// Either it is the lineage file, or it is a fork of it, which Pi writes
    /// in the new session's header. Anything else is a session that has never
    /// seen these words.
    fn continues_lineage(&self, session: &AgentSession) -> bool {
        let Some(lineage) = &self.lineage_file else {
            // Nothing recorded a file yet, so there is no chain to break and
            // nothing the replay can prove this agent is missing.
            return true;
        };
        let lineage = lineage.as_path();
        Path::new(&session.file) == lineage
            || session
                .parent
                .as_deref()
                .is_some_and(|parent| Path::new(parent) == lineage)
    }

    /// Records the session one agent is writing as the lineage to fork from.
    fn adopt_session(&mut self, session: &AgentSession, holder: AgentHolder) {
        let file = PathBuf::from(&session.file);
        if self.lineage_file.as_deref() != Some(file.as_path()) || self.lineage_holder != holder {
            info!(
                session = session.id,
                file = session.file,
                cwd = session.cwd,
                parent = session.parent.as_deref().unwrap_or("none"),
                holder = holder.name(),
                "the session lineage moved"
            );
        }
        self.lineage_file = Some(file);
        self.lineage_holder = holder;
    }

    /// Hands a joining agent the tail of the canonical conversation.
    ///
    /// One bounded page and nothing else. The agent injects it as a single
    /// undisplayed message, so it costs the model one block of text rather
    /// than a replayed turn for every entry.
    fn send_catch_up(&mut self) {
        let latest = self.conversation.latest_sequence();
        let since = latest.saturating_sub(MAX_CONVERSATION_PAGE as u64);
        let (entries, _) = self.conversation.entries_since(since);
        if entries.is_empty() {
            return;
        }
        info!(
            since,
            entries = entries.len(),
            "the joining agent was sent the conversation it missed"
        );
        self.send_agent(AgentResponseBody::CatchUp { since, entries });
    }

    /// The surface this connection speaks for, if it still holds the name.
    fn speaking_surface(&self, connection: u64) -> Option<String> {
        let (surface, generation) = self.surface_by_connection.get(&connection)?;
        self.surfaces
            .get(surface)
            .filter(|held| held.sender.generation == *generation)
            .map(|_| surface.clone())
    }

    fn publish_state(&mut self) {
        let (state, detail) = self.state();
        let holder = self.holder();
        self.broadcast(SurfaceResponseBody::State {
            state,
            detail,
            holder,
        });
    }

    fn publish_briefings(&mut self) {
        self.broadcast(SurfaceResponseBody::Briefings {
            briefings: self.briefings.rows(),
        });
    }

    /// Gives one durable terminal item the model slot only while no user turn
    /// owns it. The row is stored as in progress before the wake crosses the
    /// volatile agent connection.
    fn dispatch_briefing_now(&mut self) {
        if self.lifecycle != Lifecycle::Idle
            || self.associated_surface.is_some()
            || self.active_proactive.is_some()
            || self.agent.is_none()
            || self.proactive_circuit_open
        {
            return;
        }
        let Some((run_id, wake)) = self.briefings.next() else {
            self.consecutive_proactive = 0;
            self.proactive_runs.clear();
            self.proactive_not_before = None;
            return;
        };
        let run_id = run_id.to_string();
        let wake = wake.clone();
        let logical_run = self.briefings.logical_run_for_event(&wake.event_id);
        let repeated_run = logical_run
            .as_ref()
            .is_some_and(|run| self.proactive_runs.contains(run));
        if self.consecutive_proactive >= MAX_CONSECUTIVE_PROACTIVE_TURNS && repeated_run {
            self.proactive_circuit_open = true;
            let failed = match self.briefings.queued_delivery_failed() {
                Ok(failed) => failed,
                Err(error) => {
                    self.lifecycle = Lifecycle::Failed;
                    self.lifecycle_detail = format!(
                        "Briefing delivery stopped, but its safety state could not be stored.{RECOVERY}"
                    );
                    warn!(
                        %error,
                        run = run_id,
                        event = wake.event_id,
                        outcome = "persist_failed",
                        pending = self.briefings.pending_len(),
                        "proactive circuit state could not be stored"
                    );
                    0
                }
            };
            error!(
                run = run_id,
                event = wake.event_id,
                outcome = "circuit_open",
                consecutive = self.consecutive_proactive,
                limit = MAX_CONSECUTIVE_PROACTIVE_TURNS,
                date = logical_run
                    .as_ref()
                    .map(|run| run.0.as_str())
                    .unwrap_or("unknown"),
                profile = logical_run
                    .as_ref()
                    .map(|run| run.1.as_str())
                    .unwrap_or("unknown"),
                failed,
                pending = self.briefings.pending_len(),
                "proactive briefing circuit opened; send a message or restart the service to retry"
            );
            self.publish_state();
            self.publish_briefings();
            return;
        }
        if let Err(error) = self.briefings.in_progress(&wake.event_id) {
            self.proactive_circuit_open = true;
            self.lifecycle = Lifecycle::Failed;
            self.lifecycle_detail = format!(
                "Briefing delivery stopped because its reservation could not be stored.{RECOVERY}"
            );
            warn!(
                %error,
                run = run_id,
                event = wake.event_id,
                outcome = "circuit_open",
                pending = self.briefings.pending_len(),
                "briefing delivery could not be reserved"
            );
            if let Err(mark_error) = self.briefings.queued_delivery_failed() {
                warn!(%mark_error, outcome = "persist_failed", "failed proactive deliveries could not be marked");
            }
            self.publish_state();
            self.publish_briefings();
            return;
        }
        let event_id = wake.event_id.clone();
        if self.send_agent(AgentResponseBody::Wake {
            proactive_id: Some(event_id.clone()),
            custom_type: wake.custom_type,
            text: wake.text,
            details: wake.details,
        }) {
            self.proactive_not_before = None;
            self.active_proactive = Some(event_id.clone());
            self.active_proactive_started = false;
            info!(
                run = run_id,
                event = event_id,
                outcome = "queued_in_pi",
                consecutive = self.consecutive_proactive,
                pending = self.briefings.pending_len(),
                "proactive briefing dispatched"
            );
        } else if let Err(error) = self.briefings.retry(&event_id) {
            self.proactive_circuit_open = true;
            self.lifecycle = Lifecycle::Failed;
            self.lifecycle_detail = format!(
                "Briefing delivery stopped because its retry could not be stored.{RECOVERY}"
            );
            warn!(
                %error,
                run = run_id,
                event = event_id,
                outcome = "circuit_open",
                pending = self.briefings.pending_len(),
                "briefing delivery could not be returned to pending"
            );
            self.publish_state();
        }
        self.publish_briefings();
    }
}

pub struct Service {
    config: Config,
    attachments: Arc<AttachmentStore>,
    inner: Mutex<Inner>,
}

impl Service {
    pub fn new(config: Config, attachments: Arc<AttachmentStore>) -> Arc<Self> {
        let conversation = ConversationHistory::open(config.conversation_file.clone());
        let mut briefings = BriefingStore::open(config.briefing_file.clone());
        match briefings.recover_delivered(|event_id| conversation.contains_delivery(event_id)) {
            Ok(recovered) if recovered > 0 => info!(
                outcome = "recovered_delivery",
                recovered,
                pending = briefings.pending_len(),
                "canonical briefing deliveries recovered at startup"
            ),
            Ok(_) => {}
            Err(error) => warn!(
                %error,
                outcome = "persist_failed",
                pending = briefings.pending_len(),
                "canonical briefing delivery recovery could not be stored"
            ),
        }
        Arc::new(Self {
            config,
            attachments,
            inner: Mutex::new(Inner {
                process: None,
                process_generation: 0,
                started: Instant::now(),
                agent_joined: false,
                failures: 0,
                lifecycle: Lifecycle::Starting,
                lifecycle_detail: String::new(),
                jobs: Vec::new(),
                briefings,
                active_proactive: None,
                active_proactive_started: false,
                consecutive_proactive: 0,
                proactive_runs: HashSet::new(),
                proactive_not_before: None,
                proactive_timer_pending: false,
                proactive_circuit_open: false,
                surfaces: HashMap::new(),
                surface_by_connection: HashMap::new(),
                next_surface_generation: 0,
                agent: None,
                conversation,
                associated_surface: None,
                associated_turn: None,
                lineage_file: None,
                lineage_holder: AgentHolder::Managed,
                stopping: false,
                lease: None,
                lease_generation: 0,
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|held| held.into_inner())
    }

    /// Dispatch now or arm one bounded backoff timer. Repeated reconciliation
    /// while the timer is armed does not create more timers or bypass it.
    fn dispatch_briefing(self: &Arc<Self>) {
        let wait = {
            let mut inner = self.lock();
            if inner.lifecycle != Lifecycle::Idle
                || inner.associated_surface.is_some()
                || inner.active_proactive.is_some()
                || inner.agent.is_none()
                || inner.proactive_circuit_open
            {
                return;
            }
            match inner.proactive_not_before {
                Some(deadline) if deadline > Instant::now() => {
                    if inner.proactive_timer_pending {
                        return;
                    }
                    inner.proactive_timer_pending = true;
                    Some(deadline.saturating_duration_since(Instant::now()))
                }
                _ => {
                    inner.dispatch_briefing_now();
                    None
                }
            }
        };
        let Some(wait) = wait else {
            return;
        };
        let service = Arc::downgrade(self);
        thread::spawn(move || {
            thread::sleep(wait);
            let Some(service) = service.upgrade() else {
                return;
            };
            let mut inner = service.lock();
            inner.proactive_timer_pending = false;
            if inner
                .proactive_not_before
                .is_some_and(|deadline| deadline > Instant::now())
            {
                drop(inner);
                service.dispatch_briefing();
                return;
            }
            inner.dispatch_briefing_now();
        });
    }

    /// Reconcile the exact Pi turn after its extension reports settlement.
    /// Agent-socket ordering puts any atomic response before this marker.
    fn proactive_settled(self: &Arc<Self>, event_id: String) {
        let mut inner = self.lock();
        if inner.active_proactive.as_deref() != Some(&event_id) {
            warn!(
                event = event_id,
                expected = inner.active_proactive.as_deref().unwrap_or("none"),
                outcome = "ignored",
                "stale proactive settlement was ignored"
            );
            return;
        }
        let started = inner.active_proactive_started;
        let run_id = inner
            .briefings
            .run_for_event(&event_id)
            .unwrap_or("unknown")
            .to_string();
        if let Some(message) = inner.conversation.delivery_message(&event_id).cloned() {
            match inner
                .briefings
                .recover_delivered(|candidate| candidate == event_id)
            {
                Ok(recovered) if recovered > 0 => {
                    inner.active_proactive = None;
                    inner.active_proactive_started = false;
                    inner.broadcast(message.into());
                    let wait = proactive_backoff(inner.consecutive_proactive);
                    inner.proactive_not_before = Some(Instant::now() + wait);
                    info!(
                        run = run_id,
                        event = event_id,
                        outcome = "recovered_delivery",
                        backoff_ms = wait.as_millis(),
                        pending = inner.briefings.pending_len(),
                        "proactive briefing recovered after its acknowledgement failed"
                    );
                    inner.publish_briefings();
                    drop(inner);
                    self.dispatch_briefing();
                    return;
                }
                Ok(_) => {}
                Err(error) => warn!(
                    %error,
                    run = run_id,
                    event = event_id,
                    outcome = "persist_failed",
                    pending = inner.briefings.pending_len(),
                    "canonical proactive delivery could not close its inbox"
                ),
            }
        }
        inner.active_proactive = None;
        inner.active_proactive_started = false;
        match inner.briefings.retry(&event_id) {
            Ok(()) => {
                let wait = proactive_backoff(inner.consecutive_proactive);
                inner.proactive_not_before = Some(Instant::now() + wait);
                warn!(
                    run = run_id,
                    event = event_id,
                    outcome = "retry",
                    started,
                    consecutive = inner.consecutive_proactive,
                    backoff_ms = wait.as_millis(),
                    pending = inner.briefings.pending_len(),
                    "proactive briefing settled without a correlated response"
                );
            }
            Err(error) => {
                inner.proactive_circuit_open = true;
                inner.lifecycle = Lifecycle::Failed;
                inner.lifecycle_detail = format!(
                    "Briefing delivery stopped because its retry could not be stored.{RECOVERY}"
                );
                if let Err(mark_error) = inner.briefings.queued_delivery_failed() {
                    warn!(
                        %mark_error,
                        run = run_id,
                        event = event_id,
                        outcome = "persist_failed",
                        "failed proactive deliveries could not be marked"
                    );
                }
                error!(
                    %error,
                    run = run_id,
                    event = event_id,
                    outcome = "circuit_open",
                    pending = inner.briefings.pending_len(),
                    "proactive briefing retry could not be stored; circuit opened"
                );
                inner.publish_state();
            }
        }
        inner.publish_briefings();
        drop(inner);
        self.dispatch_briefing();
    }

    pub fn register_surface(
        &self,
        connection: u64,
        registration: SurfaceRegistration,
        outbox: SyncSender<SurfaceResponse>,
    ) -> u64 {
        let mut inner = self.lock();
        info!(
            surface = registration.id,
            name = registration.name,
            "surface {} connected",
            registration.name
        );
        inner.next_surface_generation += 1;
        let generation = inner.next_surface_generation;
        if let Some(old) = inner.surfaces.remove(&registration.id) {
            inner.surface_by_connection.remove(&old.sender.connection);
            info!(
                surface = registration.id,
                old = old.sender.connection,
                connection,
                "a surface registration was replaced"
            );
        }
        debug!(
            connection,
            surface = registration.id,
            name = registration.name,
            generation,
            widgets = registration.widgets.len(),
            replay = inner.conversation.len(),
            registration = ?registration,
            "surface registration accepted"
        );
        // Replay, state, and ready are queued while the same lock excludes broadcasts.
        for message in inner.conversation.messages().cloned() {
            let _ = outbox.try_send(SurfaceResponse::new(message.into()));
        }
        let (state, detail) = inner.state();
        let _ = outbox.try_send(SurfaceResponse::new(SurfaceResponseBody::State {
            state,
            detail,
            holder: inner.holder(),
        }));
        // A row outlives its job, so the list is backlog and not just news: a
        // surface that joined this morning has to be told about the night's
        // finished work as well as about what is running.
        let _ = outbox.try_send(SurfaceResponse::new(SurfaceResponseBody::Jobs {
            jobs: inner.jobs.clone(),
        }));
        let _ = outbox.try_send(SurfaceResponse::new(SurfaceResponseBody::Briefings {
            briefings: inner.briefings.rows(),
        }));
        let _ = outbox.try_send(SurfaceResponse::new(SurfaceResponseBody::Ready {
            surface: registration.id.clone(),
        }));
        inner
            .surface_by_connection
            .insert(connection, (registration.id.clone(), generation));
        inner.surfaces.insert(
            registration.id.clone(),
            RegisteredSurface {
                registration,
                sender: SurfaceSender {
                    connection,
                    generation,
                    outbox,
                },
            },
        );
        generation
    }

    pub fn unregister_surface(&self, connection: u64, generation: u64) {
        let mut inner = self.lock();
        let Some((id, current)) = inner.surface_by_connection.get(&connection).cloned() else {
            return;
        };
        if current != generation {
            return;
        }
        let matches = inner.surfaces.get(&id).is_some_and(|held| {
            held.sender.connection == connection && held.sender.generation == generation
        });
        if matches {
            if let Some(removed) = inner.surfaces.remove(&id) {
                info!(
                    surface = id,
                    name = removed.registration.name,
                    "surface {} disconnected",
                    removed.registration.name
                );
            }
            debug!(
                connection,
                surface = id,
                generation,
                "surface registration removed"
            );
        }
        inner.surface_by_connection.remove(&connection);
    }

    pub fn surface_message(
        &self,
        connection: u64,
        id: String,
        text: String,
        attachments: Vec<String>,
    ) {
        let mut inner = self.lock();
        let Some((surface, generation)) = inner.surface_by_connection.get(&connection).cloned()
        else {
            return;
        };
        let Some(held) = inner.surfaces.get(&surface) else {
            return;
        };
        if held.sender.generation != generation {
            return;
        }
        // A durable proactive turn reserves its own slot. Refuse rather than
        // steer user words into it; every surface keeps a refused submission
        // in its field, so no user turn is lost or captured by the briefing.
        if inner.active_proactive.is_some() {
            inner.send_surface(
                &surface,
                SurfaceResponseBody::Rejected {
                    id: Some(id),
                    operation: "message".into(),
                    code: refusal::NO_FREE_SLOT.into(),
                    detail: "A briefing is finishing. Send this again when it is done.".into(),
                },
            );
            return;
        }
        let definitions = held.registration.widgets.clone();
        let descriptors = match self.attachments.resolve(&attachments, true) {
            Ok(descriptors) => descriptors,
            Err(_) => {
                inner.send_surface(
                    &surface,
                    SurfaceResponseBody::Rejected {
                        id: Some(id),
                        operation: "message".into(),
                        code: refusal::ATTACHMENTS_UNAVAILABLE.into(),
                        detail: "One or more attachments are unavailable.".into(),
                    },
                );
                return;
            }
        };
        debug!(
            connection,
            surface,
            message_id = id,
            text,
            text_bytes = text.len(),
            widgets = definitions.len(),
            "surface message received"
        );
        if !inner.send_agent(AgentResponseBody::Message {
            id: id.clone(),
            text: text.clone(),
            widgets: definitions,
            attachments: descriptors.clone(),
        }) {
            inner.send_surface(
                &surface,
                SurfaceResponseBody::Rejected {
                    id: Some(id),
                    operation: "message".into(),
                    code: refusal::AGENT_UNAVAILABLE.into(),
                    detail: "The Scufris agent is unavailable.".into(),
                },
            );
            return;
        }
        // This surface opens a turn and owns the answer that closes it.
        inner.owner_turn(surface.clone(), id.clone(), text, descriptors);
        inner.send_surface(&surface, SurfaceResponseBody::MessageAck { id });
    }

    pub fn surface_abort(&self, connection: u64, id: String) {
        let mut inner = self.lock();
        let Some((surface, generation)) = inner.surface_by_connection.get(&connection).cloned()
        else {
            return;
        };
        if !inner
            .surfaces
            .get(&surface)
            .is_some_and(|held| held.sender.generation == generation)
        {
            return;
        }
        debug!(
            connection,
            surface,
            message_id = id,
            "surface abort received"
        );
        if inner.send_agent(AgentResponseBody::Abort { id: id.clone() }) {
            // The turn ends here without an answer, so nobody is owed one.
            inner.close_turn();
            inner.send_surface(&surface, SurfaceResponseBody::Aborted { id });
        } else {
            inner.send_surface(
                &surface,
                SurfaceResponseBody::Rejected {
                    id: Some(id),
                    operation: "abort".into(),
                    code: refusal::AGENT_UNAVAILABLE.into(),
                    detail: "The Scufris agent is unavailable.".into(),
                },
            );
        }
    }

    #[cfg(test)]
    pub fn register_agent(
        self: &Arc<Self>,
        connection: u64,
        outbox: SyncSender<AgentResponse>,
    ) -> bool {
        self.admit_agent(connection, None, None, outbox)
    }

    /// Admits one agent connection, or refuses it with the reason.
    ///
    /// `lease` is the generation the hello carried. While a lease is held,
    /// only a hello naming the live generation is the agent: the managed
    /// child's own connection, still closing, and a terminal whose lease has
    /// since ended are both refused. With no lease held, a hello that names
    /// one is refused too, because nothing granted it.
    ///
    /// `session` is the file this agent is writing. It becomes the lineage
    /// the next holder forks from, and its parent is what says whether this
    /// agent already has the conversation or has to be told it.
    pub fn admit_agent(
        self: &Arc<Self>,
        connection: u64,
        lease: Option<u64>,
        session: Option<AgentSession>,
        outbox: SyncSender<AgentResponse>,
    ) -> bool {
        let mut inner = self.lock();
        let refused = match (&inner.lease, lease) {
            (Some(held), Some(named)) if held.generation == named => None,
            (Some(held), _) => Some((
                refusal::LEASE_REQUIRED,
                format!(
                    "A terminal holds lease {}. Say hello with its generation.",
                    held.generation
                ),
            )),
            (None, Some(_)) => Some((
                refusal::NOT_LEASE_HOLDER,
                "No terminal lease is held.".to_string(),
            )),
            (None, None) => None,
        };
        let refused = refused.or_else(|| {
            inner.agent.as_ref().map(|_| {
                (
                    refusal::AGENT_EXISTS,
                    "One agent is already connected.".to_string(),
                )
            })
        });
        if let Some((code, detail)) = refused {
            info!(connection, code, "agent connection rejected");
            let _ = outbox.try_send(AgentResponse::new(AgentResponseBody::Rejected {
                id: None,
                code: code.into(),
                detail,
            }));
            return false;
        }
        let _ = outbox.try_send(AgentResponse::new(AgentResponseBody::Ready));
        inner.agent = Some(AgentConnection {
            connection,
            outbox,
            lease,
        });
        inner.agent_joined = true;
        let holder = inner.holder();
        // An agent that continues the lineage already has the conversation in
        // its context. One that does not has a new file and no memory of it,
        // and catch-up is the only thing that makes the next answer honest.
        let catch_up = match &session {
            Some(session) => !inner.continues_lineage(session),
            None => inner.lineage_file.is_some(),
        };
        if let Some(session) = &session {
            inner.adopt_session(session, holder);
        }
        if let Some(generation) = lease {
            // A terminal at its prompt is idle. Nothing else will say so: the
            // lifecycle events the service reads come from the RPC child's
            // stdout, and a terminal Pi has no such stream.
            inner.lifecycle = Lifecycle::Idle;
            inner.lifecycle_detail.clear();
            inner.publish_state();
            info!(generation, "the leased terminal connected as the agent");
        } else {
            info!("agent connected");
        }
        if catch_up {
            inner.send_catch_up();
        }
        debug!(connection, ?lease, catch_up, "agent registration accepted");
        drop(inner);
        self.dispatch_briefing();
        true
    }

    pub fn unregister_agent(self: &Arc<Self>, connection: u64) {
        let mut inner = self.lock();
        if inner
            .agent
            .as_ref()
            .is_some_and(|agent| agent.connection == connection)
        {
            inner.agent_left();
            info!("agent disconnected");
            debug!(connection, "agent registration removed");
        }
    }

    pub fn agent_request(self: &Arc<Self>, connection: u64, body: AgentRequestBody) {
        let mut inner = self.lock();
        if !inner
            .agent
            .as_ref()
            .is_some_and(|agent| agent.connection == connection)
        {
            return;
        }
        debug!(connection, payload = ?body, "agent message received");
        match body {
            // The handshake is answered where the connection is admitted, and
            // the reader breaks on a second one.
            AgentRequestBody::Hello { .. } => {}
            AgentRequestBody::Session {
                id,
                file,
                cwd,
                parent,
            } => {
                let holder = inner.holder();
                inner.adopt_session(
                    &AgentSession {
                        id,
                        file,
                        cwd,
                        parent,
                    },
                    holder,
                );
            }
            AgentRequestBody::Turn { id, text, images } => {
                if !inner.agent_holds_lease(connection) {
                    inner.send_agent(AgentResponseBody::Rejected {
                        id: Some(id),
                        code: refusal::NOT_LEASE_HOLDER.into(),
                        detail: "Only the terminal holding the lease records turns.".into(),
                    });
                    return;
                }
                // The words are already in Pi, so unlike a surface message
                // there is nothing to refuse. A reserved proactive slot is
                // logged and left alone: the briefing's answer is correlated
                // by its own identifier and this turn's by its own, so the
                // two can interleave without either being lost.
                if inner.active_proactive.is_some() {
                    warn!(
                        connection,
                        "a terminal turn arrived while a proactive slot was reserved"
                    );
                }
                // Images stay in the terminal. Saying how many there were is
                // the honest half of what the HUD can show of them.
                let text = match images {
                    0 => text,
                    1 => format!("{text}\n\n[1 image]"),
                    many => format!("{text}\n\n[{many} images]"),
                };
                info!(
                    turn = id,
                    text_bytes = text.len(),
                    images,
                    "terminal turn recorded"
                );
                let sequence =
                    inner.owner_turn(TERMINAL_SURFACE.to_string(), id.clone(), text, Vec::new());
                inner.send_agent(AgentResponseBody::TurnAck { id, sequence });
            }
            AgentRequestBody::Activity { working } => {
                // Any agent may report this. For the managed child its own
                // RPC stdout says the same thing and says it first, so this
                // is the terminal's only way to be counted and the child's
                // harmless repetition.
                if inner.lifecycle == Lifecycle::Failed {
                    return;
                }
                inner.lifecycle = if working {
                    Lifecycle::Working
                } else {
                    Lifecycle::Idle
                };
                inner.lifecycle_detail.clear();
                inner.publish_state();
                if !working {
                    drop(inner);
                    self.dispatch_briefing();
                }
            }
            AgentRequestBody::ProactiveStarted { proactive_id } => {
                let run_id = inner
                    .briefings
                    .run_for_event(&proactive_id)
                    .unwrap_or("unknown")
                    .to_string();
                if inner.active_proactive.as_deref() != Some(&proactive_id) {
                    warn!(
                        run = run_id,
                        event = proactive_id,
                        expected = inner.active_proactive.as_deref().unwrap_or("none"),
                        outcome = "ignored",
                        pending = inner.briefings.pending_len(),
                        "stale proactive turn start was ignored"
                    );
                } else if inner.active_proactive_started {
                    info!(
                        run = run_id,
                        event = proactive_id,
                        outcome = "deduplicated",
                        consecutive = inner.consecutive_proactive,
                        pending = inner.briefings.pending_len(),
                        "proactive turn start was already recorded"
                    );
                } else {
                    inner.active_proactive_started = true;
                    inner.consecutive_proactive += 1;
                    if let Some(run) = inner.briefings.logical_run_for_event(&proactive_id) {
                        inner.proactive_runs.insert(run);
                    }
                    info!(
                        run = run_id,
                        event = proactive_id,
                        outcome = "started",
                        consecutive = inner.consecutive_proactive,
                        pending = inner.briefings.pending_len(),
                        "proactive turn started"
                    );
                }
            }
            AgentRequestBody::ProactiveSettled { proactive_id } => {
                drop(inner);
                self.proactive_settled(proactive_id);
            }
            AgentRequestBody::Jobs { jobs } => {
                inner.jobs = jobs.clone();
                inner.broadcast(SurfaceResponseBody::Jobs { jobs });
                inner.publish_state();
            }
            AgentRequestBody::Response {
                text,
                turn_id,
                proactive_id,
                details,
                widgets,
                attachments,
                receipts,
            } => {
                if let Some(event_id) = proactive_id {
                    let run_id = inner
                        .briefings
                        .run_for_event(&event_id)
                        .unwrap_or("unknown")
                        .to_string();
                    if inner.active_proactive.as_deref() != Some(&event_id) {
                        warn!(
                            run = run_id,
                            event = event_id,
                            expected = inner.active_proactive.as_deref().unwrap_or("none"),
                            outcome = "ignored",
                            pending = inner.briefings.pending_len(),
                            "stale proactive response was ignored"
                        );
                        return;
                    }
                    if !inner.active_proactive_started {
                        inner.active_proactive_started = true;
                        inner.consecutive_proactive += 1;
                        if let Some(run) = inner.briefings.logical_run_for_event(&event_id) {
                            inner.proactive_runs.insert(run);
                        }
                        warn!(
                            run = run_id,
                            event = event_id,
                            outcome = "inferred_start",
                            consecutive = inner.consecutive_proactive,
                            pending = inner.briefings.pending_len(),
                            "proactive response arrived before its turn-start marker"
                        );
                    }
                    let descriptors = match self.attachments.resolve(&attachments, true) {
                        Ok(descriptors) => descriptors,
                        Err(_) => {
                            inner.send_agent(AgentResponseBody::Rejected {
                                id: None,
                                code: refusal::ATTACHMENTS_UNAVAILABLE.into(),
                                detail: "One or more attachments are unavailable.".into(),
                            });
                            Vec::new()
                        }
                    };
                    let message = ConversationMessage {
                        role: ConversationRole::Assistant,
                        surface: UNPROMPTED_SURFACE.to_string(),
                        text,
                        details,
                        // A proactive item has no surface widget registry.
                        widgets: None,
                        attachments: descriptors,
                        receipts,
                    };
                    let recorded = match inner
                        .conversation
                        .record_delivery(&event_id, message.clone())
                    {
                        Ok(recorded) => recorded,
                        Err(error) => {
                            warn!(
                                %error,
                                run = run_id,
                                event = event_id,
                                outcome = "persist_failed",
                                pending = inner.briefings.pending_len(),
                                "briefing response was not durably recorded"
                            );
                            return;
                        }
                    };
                    let durable_message = if recorded {
                        message
                    } else {
                        inner
                            .conversation
                            .delivery_message(&event_id)
                            .cloned()
                            .expect("a deduplicated delivery has a canonical message")
                    };
                    let acknowledged = match inner.briefings.acknowledge(&event_id) {
                        Ok(found) => found,
                        Err(error) => {
                            warn!(
                                %error,
                                run = run_id,
                                event = event_id,
                                outcome = "persist_failed",
                                pending = inner.briefings.pending_len(),
                                "briefing delivery acknowledgement was not stored; the correlated response remains retryable"
                            );
                            return;
                        }
                    };
                    if acknowledged {
                        inner.active_proactive = None;
                        inner.active_proactive_started = false;
                        inner.broadcast(durable_message.into());
                        let wait = proactive_backoff(inner.consecutive_proactive);
                        inner.proactive_not_before = Some(Instant::now() + wait);
                        info!(
                            run = run_id,
                            event = event_id,
                            replay = if recorded { "inserted" } else { "deduplicated" },
                            outcome = "delivered",
                            backoff_ms = wait.as_millis(),
                            pending = inner.briefings.pending_len(),
                            "proactive briefing acknowledged"
                        );
                    } else {
                        inner.active_proactive = None;
                        inner.active_proactive_started = false;
                        inner.proactive_circuit_open = true;
                        if let Err(error) = inner.briefings.queued_delivery_failed() {
                            warn!(%error, outcome = "persist_failed", "failed proactive deliveries could not be marked");
                        }
                        error!(
                            run = run_id,
                            event = event_id,
                            outcome = "missing",
                            pending = inner.briefings.pending_len(),
                            "proactive event was absent from its briefing inbox; circuit opened"
                        );
                        inner.publish_state();
                    }
                    inner.publish_briefings();
                    drop(inner);
                    self.dispatch_briefing();
                    return;
                }
                // An answer belongs to the turn that asked for it, and the
                // turn says which one it is. A surface owns its turn until
                // that answer arrives; an answer nobody asked for - a morning
                // briefing, a finished job - is recorded against a surface
                // name no surface may hold, so every screen shows it and none
                // speaks it. An owner that left before its answer came is
                // treated the same way, because losing the answer is the
                // worst of the outcomes available here. Presentation belongs
                // to the surface that asked; being told what happened belongs
                // to all of them.
                let destination = match &turn_id {
                    Some(named) if inner.associated_turn.as_deref() == Some(named.as_str()) => {
                        let surface = inner.associated_surface.clone();
                        inner.close_turn();
                        surface
                    }
                    Some(named) => {
                        // The turn it names is not the open one. It was
                        // aborted, or superseded by a later one. The words
                        // still happened in the terminal, so they are
                        // recorded there and the open turn stays open.
                        warn!(
                            turn = named,
                            open = inner.associated_turn.as_deref().unwrap_or("none"),
                            "an answer named a turn that is no longer open"
                        );
                        Some(TERMINAL_SURFACE.to_string())
                    }
                    // A reserved proactive slot means the briefing's answer is
                    // the one that is owed. An answer with no identifier at
                    // all cannot be shown to be it, so it is recorded where
                    // nobody is spoken to rather than dropped, and the turn
                    // that is owed one stays open.
                    None if inner.active_proactive.is_some() => {
                        warn!(
                            "an uncorrelated response arrived while a proactive slot was reserved"
                        );
                        None
                    }
                    None => {
                        let surface = inner.associated_surface.clone();
                        inner.close_turn();
                        surface
                    }
                };
                let owner = destination.as_ref().and_then(|surface| {
                    inner
                        .surfaces
                        .get(surface)
                        .map(|held| (surface.clone(), held.registration.widgets.clone()))
                });
                // A terminal answer has no widget registry to be checked
                // against, so its calls are dropped and it is told so: the
                // agent is connected and a mistake it can fix should be
                // visible. A surface that left is told nothing, because
                // nothing is listening for it.
                let terminal_owner = destination
                    .as_ref()
                    .filter(|surface| *surface == TERMINAL_SURFACE)
                    .cloned();
                if owner.is_none() && terminal_owner.is_some() && widgets.is_some() {
                    inner.send_agent(AgentResponseBody::Rejected {
                        id: turn_id.clone(),
                        code: refusal::INVALID_WIDGETS.into(),
                        detail: "A terminal answer has no surface to draw widgets on.".into(),
                    });
                }
                let mut widgets = if owner.is_some() { widgets } else { None };
                // A widget call is best-effort presentation and an attachment
                // is not the answer either, so neither is worth the words. The
                // agent is told what was wrong, because a mistake it can fix
                // should be visible somewhere; but the prose is recorded, and
                // it reaches the person who asked. Refusing the whole response
                // threw the answer away over a widget name, and no screen said
                // so - the turn had already ended on the agent's side, nothing
                // reads this refusal and retries, and Alex was left looking at
                // his own question with nothing under it.
                let invalid = match (&owner, &widgets) {
                    (Some((_, definitions)), Some(calls)) => {
                        validate_calls(calls, definitions).err()
                    }
                    _ => None,
                };
                if let Some(detail) = invalid {
                    inner.send_agent(AgentResponseBody::Rejected {
                        id: turn_id.clone(),
                        code: refusal::INVALID_WIDGETS.into(),
                        detail,
                    });
                    widgets = None;
                }
                let descriptors = match self.attachments.resolve(&attachments, true) {
                    Ok(descriptors) => descriptors,
                    Err(_) => {
                        inner.send_agent(AgentResponseBody::Rejected {
                            id: turn_id.clone(),
                            code: refusal::ATTACHMENTS_UNAVAILABLE.into(),
                            detail: "One or more attachments are unavailable.".into(),
                        });
                        Vec::new()
                    }
                };
                inner.record(ConversationMessage {
                    role: ConversationRole::Assistant,
                    surface: owner
                        .map(|(surface, _)| surface)
                        .or(terminal_owner)
                        .unwrap_or_else(|| UNPROMPTED_SURFACE.to_string()),
                    text,
                    details,
                    widgets,
                    attachments: descriptors,
                    receipts,
                });
            }
        }
    }

    /// Relays one surface's request about a job row to the agent that owns it.
    ///
    /// Nothing is decided here. What stopping a job costs and what filing one
    /// means both belong to the agent; the service only carries the verb, and
    /// the row list it publishes next is the answer.
    pub fn surface_job_command(&self, connection: u64, id: String, action: JobAction) {
        let mut inner = self.lock();
        let Some(surface) = inner.speaking_surface(connection) else {
            return;
        };
        debug!(
            connection,
            surface,
            job = id,
            ?action,
            "job command received"
        );
        if !inner.send_agent(AgentResponseBody::JobCommand {
            id: id.clone(),
            action,
        }) {
            inner.send_surface(
                &surface,
                SurfaceResponseBody::Rejected {
                    id: Some(id),
                    operation: "job".into(),
                    code: refusal::AGENT_UNAVAILABLE.into(),
                    detail: "The Scufris agent is unavailable.".into(),
                },
            );
        }
    }

    /// Dismisses one terminal delivered briefing from every surface.
    ///
    /// Registration is the authorization boundary, as it is for job controls.
    /// The store changes only presentation state; collection artifacts,
    /// canonical replay, and the bounded audit remain intact.
    pub fn surface_briefing_dismiss(&self, connection: u64, id: String) {
        let mut inner = self.lock();
        let Some(surface) = inner.speaking_surface(connection) else {
            return;
        };
        debug!(
            connection,
            surface,
            briefing = id,
            "briefing dismissal received"
        );
        match inner.briefings.dismiss(&id) {
            Ok(_) => inner.publish_briefings(),
            Err(DismissError::Unavailable) => inner.send_surface(
                &surface,
                SurfaceResponseBody::Rejected {
                    id: Some(id),
                    operation: "briefing".into(),
                    code: refusal::BRIEFING_UNAVAILABLE.into(),
                    detail: "That briefing generation is not retained.".into(),
                },
            ),
            Err(DismissError::NotDismissible) => inner.send_surface(
                &surface,
                SurfaceResponseBody::Rejected {
                    id: Some(id),
                    operation: "briefing".into(),
                    code: refusal::BRIEFING_NOT_DISMISSIBLE.into(),
                    detail:
                        "A briefing can be dismissed only after its terminal response is delivered."
                            .into(),
                },
            ),
            Err(DismissError::Store(error)) => {
                warn!(%error, briefing = id, "briefing dismissal could not be stored");
                inner.send_surface(
                    &surface,
                    SurfaceResponseBody::Rejected {
                        id: Some(id),
                        operation: "briefing".into(),
                        code: refusal::BRIEFING_DISMISSAL_FAILED.into(),
                        detail: "The briefing dismissal could not be stored.".into(),
                    },
                );
            }
        }
    }

    /// Takes one offer, once.
    ///
    /// The words behind an offer are the agent's and never crossed, so the
    /// identifier is the whole request. Spent is recorded against the message
    /// that carries the badge, because that is what a reconnecting surface
    /// replays: without it, every restart would hand back a live button for
    /// work already done.
    pub fn surface_offer_take(&self, connection: u64, id: String) {
        let mut inner = self.lock();
        let Some(surface) = inner.speaking_surface(connection) else {
            return;
        };
        debug!(connection, surface, offer = id, "offer taken");
        match inner.conversation.take_offer(&id) {
            Ok(true) => {}
            Ok(false) => {
                // Already taken, or in a message the ring has dropped. Either
                // way the work is not offered twice.
                inner.send_surface(
                    &surface,
                    SurfaceResponseBody::Rejected {
                        id: Some(id),
                        operation: "offer".into(),
                        code: refusal::OFFER_UNAVAILABLE.into(),
                        detail: "That offer is no longer open.".into(),
                    },
                );
                return;
            }
            Err(error) => {
                // The offer is spent in memory whatever storage did, so the
                // press still counts. Losing the snapshot costs the mark on a
                // later restart, not the work.
                warn!(%error, "the taken offer could not be stored");
            }
        }
        inner.broadcast(SurfaceResponseBody::OfferTaken { id: id.clone() });
        if !inner.send_agent(AgentResponseBody::OfferTake { id: id.clone() }) {
            inner.send_surface(
                &surface,
                SurfaceResponseBody::Rejected {
                    id: Some(id),
                    operation: "offer".into(),
                    code: refusal::AGENT_UNAVAILABLE.into(),
                    detail: "The Scufris agent is unavailable.".into(),
                },
            );
        }
    }

    /// The state word alone, for a test that does not care who holds it.
    #[cfg(test)]
    pub fn control_state(&self) -> (ScufrisState, String) {
        self.lock().state()
    }

    /// Everything `scufris-ctl state` prints, including who holds the agent
    /// and the file the next holder would fork from.
    pub fn control_state_full(&self, id: String) -> ControlResponseBody {
        let inner = self.lock();
        let (state, detail) = inner.state();
        ControlResponseBody::State {
            id,
            state,
            detail,
            holder: inner.holder(),
            generation: inner.lease.as_ref().map(|lease| lease.generation),
            session_dir: self.config.session_dir.to_string_lossy().into_owned(),
            lineage_file: inner
                .lineage_file
                .as_ref()
                .map(|file| file.to_string_lossy().into_owned()),
        }
    }

    /// One bounded page of the canonical conversation after `since`.
    ///
    /// Local only, like every control verb. A terminal uses it to catch up on
    /// its own terms; `scufris-ctl` uses it to show what was said.
    pub fn control_conversation(&self, id: String, since: u64) -> ControlResponseBody {
        let inner = self.lock();
        let (entries, more) = inner.conversation.entries_since(since);
        ControlResponseBody::ConversationEntries { id, entries, more }
    }

    /// Takes the agent for the control connection asking, stopping the
    /// managed child first.
    ///
    /// Blocking for as long as the child takes to stop, which is bounded by
    /// the same goodbye every shutdown gets. The terminal must not connect to
    /// the agent channel before the child has let go of the session, so the
    /// reply is the signal that it may.
    pub fn control_lease_acquire(
        self: &Arc<Self>,
        connection: u64,
        id: String,
        holder: LeaseHolder,
        abort_working: bool,
    ) -> ControlResponseBody {
        if !self.config.terminal_lease {
            return ControlResponseBody::Rejected {
                id,
                code: refusal::LEASE_DISABLED.into(),
                detail: "The terminal lease is not enabled on this service.".into(),
            };
        }
        let mut inner = self.lock();
        if inner.stopping {
            return ControlResponseBody::Rejected {
                id,
                code: refusal::LEASE_DISABLED.into(),
                detail: "The service is stopping.".into(),
            };
        }
        match &inner.lease {
            Some(held) if held.connection == connection => {
                let generation = held.generation;
                return self.lease_reply(&inner, id, generation);
            }
            Some(held) => {
                info!(connection, holder = held.connection, "lease refused: held");
                return ControlResponseBody::Rejected {
                    id,
                    code: refusal::LEASE_HELD.into(),
                    detail: format!("Another connection holds lease {}.", held.generation),
                };
            }
            None => {}
        }
        // A service that has stopped restarting its agent is not a service a
        // terminal should quietly stand in for. The state says what is wrong
        // and how to fix it, and hiding it behind a working terminal would
        // leave the managed path broken and unseen.
        if inner.lifecycle == Lifecycle::Failed {
            return ControlResponseBody::Rejected {
                id,
                code: refusal::AGENT_BUSY.into(),
                detail: inner.lifecycle_detail.clone(),
            };
        }
        if inner.lifecycle == Lifecycle::Working {
            if !abort_working {
                info!(connection, "lease refused: the agent is mid-turn");
                return ControlResponseBody::Rejected {
                    id,
                    code: refusal::AGENT_BUSY.into(),
                    detail: "The agent is answering. Ask again with abort_working to stop it."
                        .into(),
                };
            }
            inner.send_agent(AgentResponseBody::Abort { id: id.clone() });
            inner.close_turn();
            drop(inner);
            let waited = self.wait_for_settle();
            inner = self.lock();
            if inner.stopping || inner.lease.is_some() {
                return ControlResponseBody::Rejected {
                    id,
                    code: refusal::LEASE_HELD.into(),
                    detail: "The agent was taken while this request waited.".into(),
                };
            }
            info!(
                connection,
                settled = waited,
                "the agent was aborted for a lease"
            );
        }
        inner.lease_generation += 1;
        let generation = inner.lease_generation;
        info!(
            connection,
            generation,
            pid = holder.pid,
            cwd = holder.cwd,
            "the terminal lease was granted"
        );
        inner.lease = Some(Lease {
            connection,
            generation,
            holder,
            last_ping: Instant::now(),
        });
        // A new process generation makes the child's reader thread stale, so
        // its exit is not the failure that restarts it.
        inner.process_generation += 1;
        let child = inner.process.take();
        if inner.agent.is_some() {
            inner.send_agent(AgentResponseBody::Handoff {
                generation,
                next: AgentHolder::Terminal,
            });
            inner.agent_left();
        }
        inner.lifecycle = Lifecycle::Starting;
        inner.lifecycle_detail = "A terminal holds the agent.".into();
        inner.publish_state();
        let reply = self.lease_reply(&inner, id, generation);
        drop(inner);
        if let Some(child) = child {
            let status = child.stop();
            info!(generation, ?status, "the agent was stopped for a lease");
        } else {
            info!(generation, "the lease was granted with no agent to stop");
        }
        self.watch_lease(generation);
        reply
    }

    /// Waits, without the lock, for the open turn to end.
    ///
    /// Bounded: an agent that will not settle is stopped anyway, because the
    /// person asked for the terminal and a turn nobody is watching is not a
    /// reason to refuse them.
    fn wait_for_settle(&self) -> bool {
        let mut waited = Duration::ZERO;
        while waited < ABORT_SETTLE {
            if self.lock().lifecycle != Lifecycle::Working {
                return true;
            }
            thread::sleep(ABORT_STEP);
            waited = waited.saturating_add(ABORT_STEP);
        }
        warn!("the agent did not settle before the lease took it");
        false
    }

    fn lease_reply(&self, inner: &Inner, id: String, generation: u64) -> ControlResponseBody {
        ControlResponseBody::Lease {
            id,
            generation,
            session_dir: self.config.session_dir.to_string_lossy().into_owned(),
            lineage_file: inner
                .lineage_file
                .as_ref()
                .map(|file| file.to_string_lossy().into_owned()),
            sequence: inner.conversation.latest_sequence(),
            owner: FOREGROUND_OWNER.to_string(),
        }
    }

    /// The holder saying it is still there.
    pub fn control_lease_ping(&self, connection: u64, id: String) -> ControlResponseBody {
        let mut inner = self.lock();
        match &mut inner.lease {
            Some(held) if held.connection == connection => {
                held.last_ping = Instant::now();
                let generation = held.generation;
                ControlResponseBody::LeasePong { id, generation }
            }
            _ => ControlResponseBody::Rejected {
                id,
                code: refusal::LEASE_PING_STALE.into(),
                detail: "This connection does not hold the lease.".into(),
            },
        }
    }

    /// Gives the agent back before the connection closes.
    pub fn control_lease_release(
        self: &Arc<Self>,
        connection: u64,
        id: String,
    ) -> ControlResponseBody {
        let inner = self.lock();
        if !inner
            .lease
            .as_ref()
            .is_some_and(|held| held.connection == connection)
        {
            return ControlResponseBody::Rejected {
                id,
                code: refusal::NOT_LEASE_HOLDER.into(),
                detail: "This connection does not hold the lease.".into(),
            };
        }
        self.end_lease(inner, "released");
        ControlResponseBody::LeaseReleased { id }
    }

    /// A control connection closed. If it held the lease, that is the release.
    pub fn control_disconnected(self: &Arc<Self>, connection: u64) {
        let inner = self.lock();
        if inner
            .lease
            .as_ref()
            .is_some_and(|held| held.connection == connection)
        {
            self.end_lease(inner, "disconnected");
        }
    }

    /// Ends a lease whose holder stopped saying it was there.
    ///
    /// A terminal that is stopped rather than killed keeps its socket open
    /// with nobody reading it, so the socket alone never says the agent is
    /// gone. One thread per generation, which ends with the lease it watches.
    fn watch_lease(self: &Arc<Self>, generation: u64) {
        let watcher = Arc::clone(self);
        thread::spawn(move || {
            loop {
                thread::sleep(LEASE_PING_INTERVAL);
                if watcher.sweep_lease(generation) {
                    return;
                }
            }
        });
    }

    /// One heartbeat check. Returns true when there is nothing left to watch.
    fn sweep_lease(self: &Arc<Self>, generation: u64) -> bool {
        let inner = self.lock();
        let Some(held) = inner.lease.as_ref() else {
            return true;
        };
        if held.generation != generation {
            return true;
        }
        if held.last_ping.elapsed() < LEASE_DEADLINE {
            return false;
        }
        warn!(
            generation,
            pid = held.holder.pid,
            "the lease holder stopped answering"
        );
        self.end_lease(inner, "heartbeat");
        true
    }

    fn end_lease(self: &Arc<Self>, mut inner: MutexGuard<'_, Inner>, how: &str) {
        let Some(held) = inner.lease.take() else {
            return;
        };
        // The terminal's agent connection is dropped here rather than waited
        // for: its outbox closing ends the writer, and the reader thread's
        // own unregister finds nothing left to remove.
        if inner
            .agent
            .as_ref()
            .is_some_and(|agent| agent.lease == Some(held.generation))
        {
            inner.send_agent(AgentResponseBody::Handoff {
                generation: held.generation,
                next: AgentHolder::Managed,
            });
            inner.agent_left();
        }
        info!(
            generation = held.generation,
            how, "the terminal lease ended"
        );
        if inner.stopping {
            return;
        }
        inner.lifecycle = Lifecycle::Starting;
        inner.lifecycle_detail = "The agent is restarting.".into();
        inner.publish_state();
        drop(inner);
        self.start_agent();
    }

    /// Durably imports one generation-fenced briefing lifecycle update.
    ///
    /// Collection writes its own run before calling this. The service then
    /// stores the quiet row and any terminal wake before acknowledging the
    /// control request. No connected agent is required.
    pub fn control_briefing(
        self: &Arc<Self>,
        id: String,
        briefing: BriefingRow,
        wake: Option<BriefingWake>,
    ) -> ControlResponseBody {
        let mut inner = self.lock();
        let run_id = briefing.id.clone();
        let event_id = wake.as_ref().map(|wake| wake.event_id.clone());
        let delivered = wake
            .as_ref()
            .is_some_and(|wake| inner.conversation.contains_delivery(&wake.event_id))
            || (wake.is_none() && briefing.delivery == BriefingDeliveryState::Delivered);
        let outcome = match inner.briefings.upsert(briefing, wake, delivered) {
            Ok(outcome) => outcome,
            Err(error) => {
                warn!(
                    %error,
                    request = id,
                    run = run_id,
                    event = event_id.as_deref().unwrap_or("none"),
                    outcome = "rejected",
                    pending = inner.briefings.pending_len(),
                    "briefing ingress was rejected"
                );
                return ControlResponseBody::Rejected {
                    id,
                    code: refusal::NO_FREE_SLOT.into(),
                    detail: error.to_string(),
                };
            }
        };
        let circuit_failed = if inner.proactive_circuit_open {
            match inner.briefings.queued_delivery_failed() {
                Ok(failed) => failed,
                Err(error) => {
                    warn!(
                        %error,
                        request = id,
                        run = run_id,
                        event = event_id.as_deref().unwrap_or("none"),
                        outcome = "rejected",
                        pending = inner.briefings.pending_len(),
                        "briefing ingress arrived after the circuit opened but could not be marked failed"
                    );
                    return ControlResponseBody::Rejected {
                        id,
                        code: refusal::NO_FREE_SLOT.into(),
                        detail: format!(
                            "Briefing delivery is stopped, but its safety state could not be stored: {error}"
                        ),
                    };
                }
            }
        } else {
            0
        };
        info!(
            request = id,
            run = run_id,
            event = event_id.as_deref().unwrap_or("none"),
            outcome = outcome.name(),
            delivered,
            circuit_open = inner.proactive_circuit_open,
            circuit_failed,
            pending = inner.briefings.pending_len(),
            "briefing ingress stored"
        );
        inner.publish_briefings();
        drop(inner);
        self.dispatch_briefing();
        ControlResponseBody::BriefingAck { id }
    }

    /// Hands one proactive wake to the agent, or says why it did not land.
    ///
    /// A wake is words from outside the agent process, not a turn the owner
    /// took. So nothing here records a conversation entry, nothing is
    /// broadcast, and nothing touches the response association: it marks a
    /// surface turn that is still unanswered, and a wake neither opens one nor
    /// closes one. An answer that closes an owner's outstanding turn is still
    /// that surface's, because the owner asked first and is waiting; an answer
    /// with no turn outstanding - the ordinary case for a briefing, however
    /// recently a surface spoke - is `unprompted`. A caller with no agent to
    /// reach is told so, because its own durable state is the fallback and it
    /// has to know it needs one.
    pub fn control_wake(
        &self,
        id: String,
        custom_type: String,
        text: String,
        details: Option<Value>,
    ) -> ControlResponseBody {
        let mut inner = self.lock();
        debug!(
            wake_id = id,
            custom_type,
            text_bytes = text.len(),
            "control wake received"
        );
        if inner.active_proactive.is_some() {
            return ControlResponseBody::Rejected {
                id,
                code: refusal::NO_FREE_SLOT.into(),
                detail: "A durable proactive response is already in progress.".into(),
            };
        }
        if inner.send_agent(AgentResponseBody::Wake {
            proactive_id: None,
            custom_type,
            text,
            details,
        }) {
            ControlResponseBody::WakeAck { id }
        } else {
            ControlResponseBody::Rejected {
                id,
                code: refusal::AGENT_UNAVAILABLE.into(),
                detail: "The Scufris agent is unavailable.".into(),
            }
        }
    }

    /// The session the managed child should be started from.
    ///
    /// Only a terminal's file. The child's own is what `--continue` already
    /// finds, and forking it on every restart would make one new file per
    /// restart for nothing.
    fn fork_from(inner: &Inner) -> Option<PathBuf> {
        match inner.lineage_holder {
            AgentHolder::Terminal => inner.lineage_file.clone(),
            AgentHolder::Managed => None,
        }
    }

    pub fn start_agent(self: &Arc<Self>) {
        let mut inner = self.lock();
        if inner.stopping || inner.process.is_some() || inner.lease.is_some() {
            return;
        }
        inner.process_generation += 1;
        inner.started = Instant::now();
        inner.agent_joined = false;
        let generation = inner.process_generation;
        let fork = Self::fork_from(&inner);
        let (agent, streams) = match Agent::start(&self.config, fork.as_deref()) {
            Ok(started) => started,
            Err(error) => {
                inner.failures += 1;
                inner.lifecycle = Lifecycle::Failed;
                inner.lifecycle_detail = format!("The agent would not start: {error}.{RECOVERY}");
                inner.publish_state();
                error!(%error, "the agent would not start");
                return;
            }
        };
        info!(
            command = described(&self.config, fork.as_deref()),
            "the agent is starting"
        );
        let writer = agent.writer();
        inner.process = Some(agent);
        inner.lifecycle = Lifecycle::Starting;
        inner.lifecycle_detail.clear();
        inner.publish_state();
        drop(inner);

        let reader = Arc::clone(self);
        thread::spawn(move || {
            read_events(&reader, streams.stdout);
            reader.agent_ended(generation);
        });
        thread::spawn(move || drain_stderr(streams.stderr));
        let waiting = Arc::clone(self);
        thread::spawn(move || {
            thread::sleep(HELLO_GRACE);
            if waiting.agent_is_silent(generation) {
                warn!("the agent has not connected to agent.sock");
            }
        });
        if let Err(error) = writer.send(&Command::GetState { id: BOOT.into() }) {
            warn!(%error, "the agent would not take its first command");
        }
    }

    fn agent_is_silent(&self, generation: u64) -> bool {
        let inner = self.lock();
        !inner.stopping
            && inner.process_generation == generation
            && inner.process.is_some()
            && !inner.agent_joined
    }

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
                    inner.lifecycle = Lifecycle::Failed;
                    inner.lifecycle_detail =
                        error.unwrap_or_else(|| "The agent did not report its state.".into());
                } else {
                    let session = SessionState::from_data(&data);
                    // The child's own hello says this too, and says more. This
                    // stays for one release, for a child whose extension is
                    // older than the service that started it.
                    if let Some(file) = session.file
                        && inner.lineage_file.is_none()
                    {
                        inner.lineage_file = Some(PathBuf::from(file));
                        inner.lineage_holder = AgentHolder::Managed;
                    }
                    inner.failures = 0;
                    inner.lifecycle = if session.streaming {
                        Lifecycle::Working
                    } else {
                        Lifecycle::Idle
                    };
                    inner.lifecycle_detail.clear();
                }
                inner.publish_state();
                drop(inner);
                self.dispatch_briefing();
            }
            Event::AgentStart => {
                let mut inner = self.lock();
                inner.lifecycle = Lifecycle::Working;
                inner.lifecycle_detail.clear();
                inner.publish_state();
            }
            Event::AgentSettled => {
                let mut inner = self.lock();
                inner.lifecycle = Lifecycle::Idle;
                inner.lifecycle_detail.clear();
                inner.publish_state();
                drop(inner);
                self.dispatch_briefing();
            }
            Event::ExtensionUiRequest {
                id,
                method,
                message,
            } => self.answer_dialog(&id, &method, message.as_deref()),
            Event::ExtensionError { error } => warn!(
                error = error.unwrap_or_else(|| "an extension threw".into()),
                "extension"
            ),
            _ => {}
        }
    }

    fn answer_dialog(&self, id: &str, method: &str, message: Option<&str>) {
        if !rpc::is_dialog(method) {
            if method == "notify" {
                info!(notify = message.unwrap_or_default(), "the agent said");
            }
            return;
        }
        let writer = self.lock().process.as_ref().map(Agent::writer);
        if let Some(writer) = writer {
            let _ = writer.send(&DialogAnswer::cancel(id.to_string()));
        }
    }

    fn agent_ended(self: &Arc<Self>, generation: u64) {
        let mut inner = self.lock();
        if inner.process_generation != generation {
            return;
        }
        let agent = inner.process.take();
        let ran = inner.started.elapsed();
        let stopping = inner.stopping;
        drop(inner);
        if let Some(agent) = agent {
            let _ = agent.stop();
        }
        if stopping {
            return;
        }
        let mut inner = self.lock();
        if ran >= HEALTHY {
            inner.failures = 0;
        }
        inner.failures += 1;
        if inner.failures >= MAX_FAILURES {
            inner.lifecycle = Lifecycle::Failed;
            inner.lifecycle_detail = format!(
                "The agent stopped {} times in a row.{RECOVERY}",
                inner.failures
            );
            inner.publish_state();
            return;
        }
        inner.lifecycle = Lifecycle::Starting;
        inner.lifecycle_detail = "The agent is restarting.".into();
        inner.publish_state();
        drop(inner);
        thread::sleep(RESTART_DELAY);
        self.start_agent();
    }

    pub fn shutdown(&self) {
        let mut inner = self.lock();
        inner.stopping = true;
        inner.lease = None;
        let agent = inner.process.take();
        drop(inner);
        if let Some(agent) = agent {
            let _ = agent.stop();
        }
    }
}

fn validate_calls(calls: &[WidgetCall], definitions: &[WidgetDefinition]) -> Result<(), String> {
    for call in calls {
        let Some(definition) = definitions
            .iter()
            .find(|definition| definition.name == call.name)
        else {
            return Err(format!("No registered widget is named {}.", call.name));
        };
        validate_schema(&call.arguments, &definition.input_schema)
            .map_err(|detail| format!("{}: {detail}", call.name))?;
    }
    Ok(())
}

fn validate_schema(value: &Value, schema: &Value) -> Result<(), String> {
    if let Some(expected) = schema.get("type").and_then(Value::as_str) {
        let valid = match expected {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => false,
        };
        if !valid {
            return Err(format!("expected {expected}"));
        }
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        return Err("value is not in enum".into());
    }
    if let (Some(object), Some(properties)) = (
        value.as_object(),
        schema.get("properties").and_then(Value::as_object),
    ) {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for name in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(name) {
                    return Err(format!("missing required property {name}"));
                }
            }
        }
        if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
            for name in object.keys() {
                if !properties.contains_key(name) {
                    return Err(format!("unknown property {name}"));
                }
            }
        }
        for (name, child) in object {
            if let Some(child_schema) = properties.get(name) {
                validate_schema(child, child_schema)?;
            }
        }
    }
    if let (Some(array), Some(items)) = (value.as_array(), schema.get("items")) {
        for child in array {
            validate_schema(child, items)?;
        }
    }
    Ok(())
}

fn read_events(service: &Arc<Service>, stdout: impl std::io::Read) {
    let mut reader = std::io::BufReader::new(stdout);
    loop {
        let mut line = Vec::new();
        let mut bounded = std::io::Read::take(&mut reader, (MAX_EVENT_BYTES + 1) as u64);
        match bounded.read_until(b'\n', &mut line) {
            Ok(0) => return,
            Ok(_) => {}
            Err(error) => {
                warn!(%error, "the agent events could not be read");
                return;
            }
        }
        if line.len() > MAX_EVENT_BYTES {
            warn!("an agent event was oversized");
            return;
        }
        while matches!(line.last(), Some(b'\n' | b'\r')) {
            line.pop();
        }
        if line.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_slice::<Event>(&line) {
            service.apply(event);
        }
    }
}

fn drain_stderr(stderr: impl std::io::Read) {
    for line in std::io::BufReader::new(stderr).split(b'\n') {
        match line {
            Ok(line) if !line.is_empty() => warn!(agent = %String::from_utf8_lossy(&line), "agent"),
            Ok(_) => {}
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc::{Receiver, sync_channel},
        },
    };

    use scufris_control::service::{
        BriefingCollectionState, BriefingDeliveryState, BriefingWake, CONVERSATION_ENTRIES,
        Citation, Offer, Receipt, ReceiptState,
    };

    static NEXT_TEST: AtomicU64 = AtomicU64::new(1);

    fn test_runtime() -> PathBuf {
        let runtime = std::env::temp_dir().join(format!(
            "scufris-v5-service-{}-{}",
            std::process::id(),
            NEXT_TEST.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&runtime);
        runtime
    }
    fn service_at(config: Config) -> Arc<Service> {
        let attachments = AttachmentStore::open(config.attachment_dir.clone()).unwrap();
        Service::new(config, attachments)
    }
    fn service() -> Arc<Service> {
        service_at(Config::test(test_runtime()))
    }
    fn surface(
        service: &Arc<Service>,
        connection: u64,
        id: &str,
    ) -> (u64, Receiver<SurfaceResponse>) {
        let (outbox, inbox) = sync_channel(256);
        let generation = service.register_surface(
            connection,
            SurfaceRegistration {
                id: id.into(),
                name: id.into(),
                widgets: vec![],
            },
            outbox,
        );
        while inbox.try_recv().is_ok() {}
        (generation, inbox)
    }
    fn drain(inbox: &Receiver<SurfaceResponse>) -> Vec<SurfaceResponseBody> {
        std::iter::from_fn(|| inbox.try_recv().ok())
            .map(|m| m.body)
            .collect()
    }

    fn briefing_row(id: &str) -> BriefingRow {
        BriefingRow {
            id: id.into(),
            date: "2026-09-10".into(),
            profile: "nightly".into(),
            collection: BriefingCollectionState::Collected,
            delivery: BriefingDeliveryState::Pending,
            since: 1_789_000_000,
            completed: 2,
            total: 2,
            failed: 0,
            summary: "2 of 2 sources answered".into(),
        }
    }

    fn briefing_wake(id: &str) -> BriefingWake {
        BriefingWake {
            event_id: format!("briefing-{id}-terminal"),
            custom_type: "scufris-briefing".into(),
            text: "Write the measured nightly briefing.".into(),
            details: Some(serde_json::json!({"generation": id})),
        }
    }

    fn submit_briefing(service: &Arc<Service>, id: &str) {
        assert!(matches!(
            service.control_briefing(
                format!("update-{id}"),
                briefing_row(id),
                Some(briefing_wake(id)),
            ),
            ControlResponseBody::BriefingAck { .. }
        ));
    }

    fn proactive_started(id: &str) -> AgentRequestBody {
        AgentRequestBody::ProactiveStarted {
            proactive_id: format!("briefing-{id}-terminal"),
        }
    }

    fn proactive_settled(id: &str) -> AgentRequestBody {
        AgentRequestBody::ProactiveSettled {
            proactive_id: format!("briefing-{id}-terminal"),
        }
    }

    fn proactive_answer(id: &str, text: &str) -> AgentRequestBody {
        AgentRequestBody::Response {
            turn_id: None,
            proactive_id: Some(format!("briefing-{id}-terminal")),
            text: text.into(),
            details: None,
            widgets: None,
            attachments: Vec::new(),
            receipts: Vec::new(),
        }
    }

    fn receive_assistant_text(inbox: &Receiver<SurfaceResponse>) -> String {
        loop {
            let message = inbox
                .recv_timeout(Duration::from_secs(1))
                .expect("an assistant message");
            if let SurfaceResponseBody::Message {
                role: ConversationRole::Assistant,
                text,
                ..
            } = message.body
            {
                return text;
            }
        }
    }

    #[test]
    fn one_terminal_generation_waits_for_the_user_and_is_recorded_once() {
        let runtime = test_runtime();
        let config = Config::test(runtime.clone());
        let service = service_at(config.clone());
        let (_, surface_in) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Ready
        ));
        service.apply(Event::AgentSettled);

        service.surface_message(1, "owner-turn".into(), "First answer me.".into(), vec![]);
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Message { .. }
        ));
        assert!(matches!(
            service.control_briefing(
                "update-a".into(),
                briefing_row("generation-a"),
                Some(briefing_wake("generation-a")),
            ),
            ControlResponseBody::BriefingAck { .. }
        ));
        assert!(
            agent_in.try_recv().is_err(),
            "the owner turn kept the model slot"
        );
        assert!(
            drain(&surface_in)
                .iter()
                .any(|body| matches!(body, SurfaceResponseBody::Briefings { .. }))
        );

        service.agent_request(
            10,
            AgentRequestBody::Response {
                turn_id: None,
                proactive_id: None,
                text: "The owner's answer.".into(),
                details: None,
                widgets: None,
                attachments: vec![],
                receipts: vec![],
            },
        );
        service.apply(Event::AgentSettled);
        let event_id = "briefing-generation-a-terminal";
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Wake {
                proactive_id: Some(ref id),
                ..
            } if id == event_id
        ));
        service.agent_request(10, proactive_started("generation-a"));

        service.surface_message(1, "held".into(), "Do not capture this.".into(), vec![]);
        assert!(
            drain(&surface_in).iter().any(|body| matches!(
                body,
                SurfaceResponseBody::Rejected { code, .. } if code == refusal::NO_FREE_SLOT
            )),
            "a user submission was refused while the proactive slot was reserved"
        );
        assert!(agent_in.try_recv().is_err());

        service.agent_request(
            10,
            AgentRequestBody::Response {
                turn_id: None,
                proactive_id: Some(event_id.into()),
                text: "The nightly briefing.".into(),
                details: None,
                widgets: None,
                attachments: vec![],
                receipts: vec![],
            },
        );
        service.agent_request(10, proactive_settled("generation-a"));
        assert_eq!(
            drain(&surface_in)
                .iter()
                .filter(|body| matches!(body, SurfaceResponseBody::Message { text, .. } if text == "The nightly briefing."))
                .count(),
            1
        );
        drop(service);

        let restored = service_at(config);
        let (agent, agent_in) = sync_channel(8);
        restored.register_agent(20, agent);
        agent_in.recv().unwrap();
        restored.apply(Event::AgentSettled);
        assert!(
            agent_in.try_recv().is_err(),
            "canonical replay suppressed retry"
        );
        assert!(matches!(
            restored.control_briefing(
                "update-again".into(),
                briefing_row("generation-a"),
                Some(briefing_wake("generation-a")),
            ),
            ControlResponseBody::BriefingAck { .. }
        ));
        assert!(
            agent_in.try_recv().is_err(),
            "duplicate terminal ingress stayed done"
        );
        std::fs::remove_dir_all(runtime).unwrap();
    }

    #[test]
    fn only_the_matching_proactive_turn_can_acknowledge_a_delivery() {
        let service = service();
        let (_, surface_in) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.apply(Event::AgentSettled);
        submit_briefing(&service, "generation-a");
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Wake {
                proactive_id: Some(ref id),
                ..
            } if id == "briefing-generation-a-terminal"
        ));
        service.agent_request(10, proactive_started("generation-a"));

        service.agent_request(10, proactive_answer("generation-b", "wrong turn"));
        assert_eq!(
            service.lock().briefings.audit_rows()[0].delivery,
            BriefingDeliveryState::InProgress
        );
        assert!(drain(&surface_in).iter().all(|body| !matches!(
            body,
            SurfaceResponseBody::Message {
                role: ConversationRole::Assistant,
                ..
            }
        )));

        service.agent_request(10, proactive_answer("generation-a", "matching turn"));
        service.agent_request(10, proactive_settled("generation-a"));
        assert_eq!(receive_assistant_text(&surface_in), "matching turn");
        assert_eq!(
            service.lock().briefings.audit_rows()[0].delivery,
            BriefingDeliveryState::Delivered
        );
    }

    #[test]
    fn only_an_exact_settlement_retries_a_proactive_message_not_delivered_by_pi() {
        let service = service();
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.apply(Event::AgentSettled);
        submit_briefing(&service, "generation-a");
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Wake { .. }
        ));

        service.apply(Event::AgentSettled);
        thread::sleep(PROACTIVE_BACKOFF_MIN.saturating_mul(2));
        assert!(agent_in.try_recv().is_err());
        let inner = service.lock();
        assert_eq!(
            inner.briefings.audit_rows()[0].delivery,
            BriefingDeliveryState::InProgress
        );
        assert_eq!(
            inner.active_proactive.as_deref(),
            Some("briefing-generation-a-terminal")
        );
        assert!(!inner.active_proactive_started);
        assert_eq!(inner.consecutive_proactive, 0);
        drop(inner);

        service.agent_request(10, proactive_settled("generation-a"));
        assert!(matches!(
            agent_in
                .recv_timeout(Duration::from_secs(5))
                .expect("the exact abandoned wake is retried")
                .body,
            AgentResponseBody::Wake {
                proactive_id: Some(ref id),
                ..
            } if id == "briefing-generation-a-terminal"
        ));
        service.agent_request(10, proactive_started("generation-a"));
        service.agent_request(10, proactive_answer("generation-a", "right turn"));
        service.agent_request(10, proactive_settled("generation-a"));
        assert_eq!(
            service.lock().briefings.audit_rows()[0].delivery,
            BriefingDeliveryState::Delivered
        );
    }

    #[test]
    fn an_exact_proactive_settlement_without_a_response_retries_with_backoff() {
        let service = service();
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.apply(Event::AgentSettled);
        submit_briefing(&service, "generation-a");
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Wake { .. }
        ));
        service.agent_request(10, proactive_started("generation-a"));
        service.apply(Event::AgentStart);
        service.apply(Event::AgentSettled);

        let settled_at = Instant::now();
        service.agent_request(10, proactive_settled("generation-a"));
        assert!(matches!(
            agent_in
                .recv_timeout(Duration::from_secs(5))
                .expect("the backed-off retry")
                .body,
            AgentResponseBody::Wake { .. }
        ));
        assert!(settled_at.elapsed() >= PROACTIVE_BACKOFF_MIN);
        let inner = service.lock();
        assert_eq!(inner.consecutive_proactive, 1);
        assert!(!inner.active_proactive_started);
        assert_eq!(
            inner.briefings.audit_rows()[0].delivery,
            BriefingDeliveryState::InProgress
        );
    }

    #[test]
    fn proactive_response_is_safe_for_both_settled_event_orderings() {
        for settle_first in [true, false] {
            let service = service();
            let (_, surface_in) = surface(&service, 1, "one");
            let (agent, agent_in) = sync_channel(8);
            service.register_agent(10, agent);
            agent_in.recv().unwrap();
            service.apply(Event::AgentSettled);
            submit_briefing(&service, "generation-a");
            assert!(matches!(
                agent_in.recv().unwrap().body,
                AgentResponseBody::Wake { .. }
            ));
            service.agent_request(10, proactive_started("generation-a"));
            if settle_first {
                service.apply(Event::AgentSettled);
            }
            service.agent_request(10, proactive_answer("generation-a", "canonical"));
            if !settle_first {
                service.apply(Event::AgentSettled);
            }
            service.agent_request(10, proactive_settled("generation-a"));
            assert_eq!(receive_assistant_text(&surface_in), "canonical");
            thread::sleep(Duration::from_millis(20));
            assert!(agent_in.try_recv().is_err());
            assert_eq!(
                service.lock().briefings.audit_rows()[0].delivery,
                BriefingDeliveryState::Delivered
            );
        }
    }

    #[test]
    fn a_distinct_queued_backlog_drains_without_opening_the_circuit() {
        let service = service();
        for (index, suffix) in ['a', 'b', 'c', 'd', 'e'].into_iter().enumerate() {
            let id = format!("generation-{suffix}");
            let mut row = briefing_row(&id);
            row.date = format!("2026-09-{:02}", index + 1);
            assert!(matches!(
                service.control_briefing(format!("update-{id}"), row, Some(briefing_wake(&id)),),
                ControlResponseBody::BriefingAck { .. }
            ));
        }
        let (agent, agent_in) = sync_channel(16);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.apply(Event::AgentSettled);
        for suffix in ['a', 'b', 'c', 'd', 'e'] {
            assert!(matches!(
                agent_in
                    .recv_timeout(Duration::from_secs(2))
                    .expect("the distinct backlog item")
                    .body,
                AgentResponseBody::Wake { .. }
            ));
            service.agent_request(10, proactive_started(&format!("generation-{suffix}")));
            service.agent_request(
                10,
                proactive_answer(&format!("generation-{suffix}"), &format!("answer {suffix}")),
            );
            service.agent_request(10, proactive_settled(&format!("generation-{suffix}")));
        }
        let inner = service.lock();
        assert!(!inner.proactive_circuit_open);
        assert_eq!(inner.briefings.pending_len(), 0);
        assert!(
            inner
                .briefings
                .audit_rows()
                .iter()
                .all(|row| row.delivery == BriefingDeliveryState::Delivered)
        );
    }

    #[test]
    fn repeated_logical_runs_back_off_and_open_the_circuit() {
        let service = service();
        let (_, surface_in) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(16);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.apply(Event::AgentSettled);
        for suffix in ['a', 'b', 'c', 'd', 'e'] {
            submit_briefing(&service, &format!("generation-{suffix}"));
        }

        let mut received_at = Vec::new();
        for suffix in ['a', 'b', 'c'] {
            let message = agent_in
                .recv_timeout(Duration::from_secs(2))
                .expect("the next backoff dispatch");
            assert!(matches!(message.body, AgentResponseBody::Wake { .. }));
            received_at.push(Instant::now());
            service.agent_request(10, proactive_started(&format!("generation-{suffix}")));
            service.apply(Event::AgentSettled);
            service.agent_request(
                10,
                proactive_answer(&format!("generation-{suffix}"), &format!("answer {suffix}")),
            );
            service.agent_request(10, proactive_settled(&format!("generation-{suffix}")));
        }
        assert!(received_at[1].duration_since(received_at[0]) >= PROACTIVE_BACKOFF_MIN);
        assert!(
            received_at[2].duration_since(received_at[1])
                >= PROACTIVE_BACKOFF_MIN.saturating_mul(2)
        );
        assert!(agent_in.recv_timeout(Duration::from_millis(150)).is_err());
        let inner = service.lock();
        let rows = inner.briefings.audit_rows();
        for id in ["generation-d", "generation-e"] {
            let failed = rows.iter().find(|row| row.id == id).unwrap();
            assert_eq!(failed.delivery, BriefingDeliveryState::Failed);
            assert_eq!(failed.summary, "2 of 2 sources answered");
        }
        assert_eq!(inner.briefings.pending_len(), 0);
        assert!(inner.proactive_circuit_open);
        let (state, detail) = inner.state();
        assert_eq!(state, ScufrisState::Failed);
        assert!(detail.contains("restart"));
        drop(inner);

        // A continuous producer may add more unique generations after the
        // circuit opens. They receive the same durable stop state and cannot
        // wait as ordinary pending work for a restart to run another burst.
        submit_briefing(&service, "generation-f");
        assert!(agent_in.recv_timeout(Duration::from_millis(50)).is_err());
        let inner = service.lock();
        let failed = inner
            .briefings
            .audit_rows()
            .into_iter()
            .find(|row| row.id == "generation-f")
            .unwrap();
        assert_eq!(failed.delivery, BriefingDeliveryState::Failed);
        assert_eq!(failed.summary, "2 of 2 sources answered");
        assert_eq!(inner.briefings.pending_len(), 0);
        drop(inner);

        service.surface_message(
            1,
            "owner-recovery".into(),
            "Continue and retry stopped briefings.".into(),
            Vec::new(),
        );
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Message { .. }
        ));
        let inner = service.lock();
        assert!(!inner.proactive_circuit_open);
        assert_eq!(inner.consecutive_proactive, 0);
        assert_eq!(inner.briefings.pending_len(), 3);
        assert_eq!(inner.state().0, ScufrisState::Idle);
        assert!(drain(&surface_in).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Briefings { briefings }
                if briefings.iter().all(|row| row.delivery == BriefingDeliveryState::Pending)
        )));
    }

    #[test]
    fn external_input_resets_the_proactive_turn_counter() {
        let service = service();
        let (_, surface_in) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.apply(Event::AgentSettled);
        service.lock().consecutive_proactive = 2;
        service.surface_message(
            1,
            "external-turn".into(),
            "new external input".into(),
            Vec::new(),
        );
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Message { .. }
        ));
        assert_eq!(service.lock().consecutive_proactive, 0);
        drop(surface_in);
    }

    #[test]
    fn a_failed_delivery_reservation_opens_the_circuit_before_a_wake() {
        let service = service();
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        submit_briefing(&service, "generation-a");
        service.lock().briefings.fail_persistence(true);
        service.apply(Event::AgentSettled);
        assert!(agent_in.try_recv().is_err());
        let inner = service.lock();
        assert!(inner.proactive_circuit_open);
        assert!(inner.active_proactive.is_none());
        assert_eq!(inner.consecutive_proactive, 0);
        assert_eq!(
            inner.briefings.audit_rows()[0].delivery,
            BriefingDeliveryState::Pending
        );
        assert_eq!(inner.state().0, ScufrisState::Failed);
    }

    #[test]
    fn proactive_delivery_waits_for_both_durable_writes() {
        for fail_conversation in [true, false] {
            let runtime = test_runtime();
            let mut config = Config::test(runtime.clone());
            let blocked = runtime.join("blocked");
            if fail_conversation {
                fs::create_dir_all(&runtime).unwrap();
                fs::write(&blocked, "not a directory").unwrap();
                config.conversation_file = blocked.join("conversation.json");
            }
            let service = service_at(config);
            let (_, surface_in) = surface(&service, 1, "one");
            let (agent, agent_in) = sync_channel(8);
            service.register_agent(10, agent);
            agent_in.recv().unwrap();
            service.apply(Event::AgentSettled);
            submit_briefing(&service, "generation-a");
            assert!(matches!(
                agent_in.recv().unwrap().body,
                AgentResponseBody::Wake { .. }
            ));
            service.agent_request(10, proactive_started("generation-a"));
            if !fail_conversation {
                service.lock().briefings.fail_persistence(true);
                service.apply(Event::AgentSettled);
            }
            service.agent_request(10, proactive_answer("generation-a", "one durable answer"));
            assert!(drain(&surface_in).iter().all(|body| !matches!(
                body,
                SurfaceResponseBody::Message {
                    role: ConversationRole::Assistant,
                    ..
                }
            )));
            assert_eq!(
                service.lock().briefings.audit_rows()[0].delivery,
                BriefingDeliveryState::InProgress
            );

            if fail_conversation {
                fs::remove_file(&blocked).unwrap();
                fs::create_dir(&blocked).unwrap();
            } else {
                service.lock().briefings.fail_persistence(false);
            }
            service.agent_request(10, proactive_settled("generation-a"));
            if fail_conversation {
                assert!(matches!(
                    agent_in
                        .recv_timeout(Duration::from_secs(5))
                        .expect("the durable retry")
                        .body,
                    AgentResponseBody::Wake { .. }
                ));
                service.agent_request(10, proactive_started("generation-a"));
                service.agent_request(10, proactive_answer("generation-a", "one durable answer"));
                service.agent_request(10, proactive_settled("generation-a"));
            }
            assert_eq!(receive_assistant_text(&surface_in), "one durable answer");
            assert_eq!(
                service.lock().briefings.audit_rows()[0].delivery,
                BriefingDeliveryState::Delivered
            );
            service.agent_request(10, proactive_answer("generation-a", "duplicate"));
            assert!(drain(&surface_in).iter().all(|body| !matches!(
                body,
                SurfaceResponseBody::Message {
                    role: ConversationRole::Assistant,
                    ..
                }
            )));
            assert_eq!(service.lock().conversation.len(), 1);
            drop(service);
            fs::remove_dir_all(runtime).unwrap();
        }
    }

    #[test]
    fn a_dismissed_partial_briefing_disappears_everywhere_but_stays_in_audit() {
        let runtime = test_runtime();
        let config = Config::test(runtime.clone());
        let service = service_at(config.clone());
        let (_, one) = surface(&service, 1, "one");
        let (_, two) = surface(&service, 2, "two");
        let mut partial = briefing_row("generation-partial");
        partial.failed = 1;
        partial.delivery = BriefingDeliveryState::Delivered;
        service.control_briefing("update-partial".into(), partial, None);
        for inbox in [&one, &two] {
            assert!(drain(inbox).iter().any(|body| matches!(
                body,
                SurfaceResponseBody::Briefings { briefings }
                    if briefings.len() == 1 && briefings[0].id == "generation-partial"
            )));
        }

        service.surface_briefing_dismiss(1, "generation-partial".into());
        for inbox in [&one, &two] {
            assert!(drain(inbox).iter().any(|body| matches!(
                body,
                SurfaceResponseBody::Briefings { briefings } if briefings.is_empty()
            )));
        }
        service.surface_briefing_dismiss(1, "generation-partial".into());
        assert!(drain(&one).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Briefings { briefings } if briefings.is_empty()
        )));
        drop(service);

        let stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config.briefing_file).unwrap()).unwrap();
        assert_eq!(stored["rows"].as_array().unwrap().len(), 1);
        assert_eq!(
            stored["dismissed"],
            serde_json::json!(["generation-partial"])
        );

        let restored = service_at(config);
        let (outbox, inbox) = sync_channel(32);
        restored.register_surface(
            3,
            SurfaceRegistration {
                id: "three".into(),
                name: "three".into(),
                widgets: vec![],
            },
            outbox,
        );
        assert!(
            std::iter::from_fn(|| inbox.try_recv().ok()).any(|message| matches!(
                message.body,
                SurfaceResponseBody::Briefings { briefings } if briefings.is_empty()
            ))
        );
        std::fs::remove_dir_all(runtime).unwrap();
    }

    #[test]
    fn briefing_dismissal_refuses_unknown_or_undelivered_ids() {
        let service = service();
        let (_, inbox) = surface(&service, 1, "one");
        service.surface_briefing_dismiss(1, "generation-missing".into());
        assert!(drain(&inbox).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Rejected { operation, code, .. }
                if operation == "briefing" && code == refusal::BRIEFING_UNAVAILABLE
        )));

        let active = BriefingRow {
            collection: BriefingCollectionState::Collecting,
            completed: 0,
            ..briefing_row("generation-active")
        };
        service.control_briefing("update-active".into(), active, None);
        drain(&inbox);
        service.surface_briefing_dismiss(1, "generation-active".into());
        assert!(drain(&inbox).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Rejected { operation, code, .. }
                if operation == "briefing" && code == refusal::BRIEFING_NOT_DISMISSIBLE
        )));
    }

    #[test]
    fn two_surfaces_receive_identical_live_messages_and_replay() {
        let service = service();
        let (_, one) = surface(&service, 1, "one");
        let (_, two) = surface(&service, 2, "two");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.surface_message(1, "m-1".into(), "hello".into(), vec![]);
        let first: Vec<_> = drain(&one)
            .into_iter()
            .filter(|b| matches!(b, SurfaceResponseBody::Message { .. }))
            .collect();
        let second: Vec<_> = drain(&two)
            .into_iter()
            .filter(|b| matches!(b, SurfaceResponseBody::Message { .. }))
            .collect();
        assert_eq!(first, second);
        let (outbox, replay) = sync_channel(256);
        service.register_surface(
            3,
            SurfaceRegistration {
                id: "three".into(),
                name: "three".into(),
                widgets: vec![],
            },
            outbox,
        );
        let replay = drain(&replay);
        assert!(matches!(replay[0], SurfaceResponseBody::Message { .. }));
        assert!(matches!(replay[1], SurfaceResponseBody::State { .. }));
        assert!(matches!(replay[2], SurfaceResponseBody::Jobs { .. }));
        assert!(matches!(replay[3], SurfaceResponseBody::Briefings { .. }));
        assert!(matches!(replay[4], SurfaceResponseBody::Ready { .. }));
    }

    #[test]
    fn restart_and_reconnect_replay_each_persisted_message_once() {
        let runtime = test_runtime();
        let config = Config::test(runtime.clone());
        let service = service_at(config.clone());
        let (_, original) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.surface_message(1, "m-1".into(), "survives".into(), vec![]);
        agent_in.recv().unwrap();
        assert_eq!(
            drain(&original)
                .iter()
                .filter(|body| matches!(body, SurfaceResponseBody::Message { .. }))
                .count(),
            1
        );
        drop(service);

        let restored = service_at(config);
        for (connection, expected_generation) in [(2, 1), (3, 2)] {
            let (outbox, replay) = sync_channel(256);
            let generation = restored.register_surface(
                connection,
                SurfaceRegistration {
                    id: "one".into(),
                    name: "one".into(),
                    widgets: vec![],
                },
                outbox,
            );
            assert_eq!(generation, expected_generation);
            let replay = drain(&replay);
            assert_eq!(
                replay
                    .iter()
                    .filter(|body| matches!(body, SurfaceResponseBody::Message { .. }))
                    .count(),
                1
            );
            assert!(matches!(
                &replay[0],
                SurfaceResponseBody::Message { text, .. } if text == "survives"
            ));
            assert!(matches!(replay[1], SurfaceResponseBody::State { .. }));
            assert!(matches!(replay[2], SurfaceResponseBody::Jobs { .. }));
            assert!(matches!(replay[3], SurfaceResponseBody::Briefings { .. }));
            assert!(matches!(replay[4], SurfaceResponseBody::Ready { .. }));
        }
        drop(restored);
        std::fs::remove_dir_all(runtime).unwrap();
    }

    #[test]
    fn replacement_is_generation_safe() {
        let service = service();
        let (old, old_inbox) = surface(&service, 1, "same");
        let (new, current) = surface(&service, 2, "same");
        assert_ne!(old, new);
        service.unregister_surface(1, old);
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.surface_message(2, "m-1".into(), "still here".into(), vec![]);
        assert!(
            drain(&current)
                .iter()
                .any(|body| matches!(body, SurfaceResponseBody::Message { .. }))
        );
        assert!(drain(&old_inbox).is_empty());
    }

    #[test]
    fn only_one_agent_is_accepted() {
        let service = service();
        let (one, first) = sync_channel(4);
        let (two, second) = sync_channel(4);
        assert!(service.register_agent(1, one));
        assert!(!service.register_agent(2, two));
        assert!(matches!(
            first.recv().unwrap().body,
            AgentResponseBody::Ready
        ));
        assert!(matches!(
            second.recv().unwrap().body,
            AgentResponseBody::Rejected { .. }
        ));
    }

    #[test]
    fn a_slow_surface_is_removed_without_affecting_another() {
        let service = service();
        let (slow_out, _slow_in) = sync_channel(3);
        service.register_surface(
            1,
            SurfaceRegistration {
                id: "slow".into(),
                name: "slow".into(),
                widgets: vec![],
            },
            slow_out,
        );
        let (_, fast) = surface(&service, 2, "fast");
        let (agent, agent_in) = sync_channel(16);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.surface_message(2, "m-1".into(), "one".into(), vec![]);
        agent_in.recv().unwrap();
        service.surface_message(2, "m-2".into(), "two".into(), vec![]);
        assert!(!service.lock().surfaces.contains_key("slow"));
        assert!(service.lock().surfaces.contains_key("fast"));
        assert!(
            drain(&fast)
                .iter()
                .any(|body| matches!(body, SurfaceResponseBody::Message { .. }))
        );
    }

    #[test]
    fn the_conversation_ring_retains_exactly_the_latest_bound() {
        let service = service();
        let (_, inbox) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(CONVERSATION_ENTRIES + 8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        for index in 0..CONVERSATION_ENTRIES + 5 {
            service.surface_message(1, format!("m-{index}"), format!("line {index}"), vec![]);
            agent_in.recv().unwrap();
            while inbox.try_recv().is_ok() {}
        }
        let held: Vec<_> = service.lock().conversation.messages().cloned().collect();
        assert_eq!(held.len(), CONVERSATION_ENTRIES);
        assert_eq!(held.first().unwrap().text, "line 5");
    }

    #[test]
    fn only_service_owned_attachments_enter_canonical_messages() {
        let service = service();
        let descriptor = service
            .attachments
            .put("diagram.png".into(), "image/png".into(), b"image")
            .unwrap();
        let (_, surface_in) = surface(&service, 1, "one");
        while surface_in.try_recv().is_ok() {}
        let (agent_out, agent_in) = sync_channel(8);
        service.register_agent(10, agent_out);
        agent_in.recv().unwrap();

        service.surface_message(
            1,
            "m-1".into(),
            "What is this?".into(),
            vec![descriptor.id.clone()],
        );
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Message { attachments, .. } if attachments == [descriptor.clone()]
        ));
        assert!(drain(&surface_in).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Message { attachments, .. } if attachments == std::slice::from_ref(&descriptor)
        )));

        service.surface_message(
            1,
            "m-2".into(),
            "Invented.".into(),
            vec!["att_missing".into()],
        );
        assert!(drain(&surface_in).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Rejected { code, .. } if code == refusal::ATTACHMENTS_UNAVAILABLE
        )));
    }

    #[test]
    fn latest_sender_and_atomic_widgets_are_associated() {
        let service = service();
        let (outbox, inbox) = sync_channel(256);
        service.register_surface(1, SurfaceRegistration { id: "one".into(), name: "One".into(), widgets: vec![WidgetDefinition { name: "summary".into(), description: "Summary".into(), input_schema: serde_json::json!({"type":"object","properties":{"passed":{"type":"integer"}},"required":["passed"],"additionalProperties":false}) }] }, outbox);
        while inbox.try_recv().is_ok() {}
        let (agent_out, agent_in) = sync_channel(8);
        service.register_agent(10, agent_out);
        agent_in.recv().unwrap();
        service.surface_message(1, "m-1".into(), "test".into(), vec![]);
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Message { .. }
        ));
        service.agent_request(
            10,
            AgentRequestBody::Response {
                turn_id: None,
                proactive_id: None,
                text: "Passed.".into(),
                details: Some("## Results".into()),
                widgets: Some(vec![WidgetCall {
                    id: "w-1".into(),
                    name: "summary".into(),
                    arguments: serde_json::json!({"passed": 4}),
                }]),
                attachments: vec![],
                receipts: vec![],
            },
        );
        assert!(drain(&inbox).iter().any(|body| matches!(body, SurfaceResponseBody::Message { role: ConversationRole::Assistant, surface, details: Some(_), widgets: Some(_), .. } if surface == "one")));
    }

    #[test]
    fn an_unknown_widget_costs_the_call_and_not_the_answer() {
        // The model names a widget the surface never registered. The prose is
        // the answer to a question Alex asked and is sitting in front of; the
        // widget is presentation. Only the widget is lost, and the agent is
        // told which one.
        let service = service();
        let (outbox, inbox) = sync_channel(256);
        service.register_surface(
            1,
            SurfaceRegistration {
                id: "one".into(),
                name: "One".into(),
                widgets: vec![WidgetDefinition {
                    name: "summary".into(),
                    description: "Summary".into(),
                    input_schema: serde_json::json!({"type":"object"}),
                }],
            },
            outbox,
        );
        while inbox.try_recv().is_ok() {}
        let (agent_out, agent_in) = sync_channel(8);
        service.register_agent(10, agent_out);
        agent_in.recv().unwrap();
        service.surface_message(1, "m-1".into(), "test".into(), vec![]);
        agent_in.recv().unwrap();
        service.agent_request(
            10,
            AgentRequestBody::Response {
                turn_id: None,
                proactive_id: None,
                text: "Passed.".into(),
                details: None,
                widgets: Some(vec![WidgetCall {
                    id: "w-1".into(),
                    name: "summary-panel".into(),
                    arguments: serde_json::json!({}),
                }]),
                attachments: vec![],
                receipts: vec![],
            },
        );
        assert!(drain(&inbox).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Message {
                role: ConversationRole::Assistant,
                surface,
                text,
                widgets: None,
                ..
            } if surface == "one" && text == "Passed."
        )));
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Rejected { ref code, .. } if code == refusal::INVALID_WIDGETS
        ));
    }

    #[test]
    fn an_expired_attachment_costs_the_file_and_not_the_answer() {
        let service = service();
        let (_, one) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.surface_message(1, "m-1".into(), "start".into(), vec![]);
        agent_in.recv().unwrap();
        service.agent_request(
            10,
            AgentRequestBody::Response {
                turn_id: None,
                proactive_id: None,
                text: "Here it is.".into(),
                details: None,
                widgets: None,
                attachments: vec!["gone".into()],
                receipts: vec![],
            },
        );
        assert!(drain(&one).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Message {
                role: ConversationRole::Assistant,
                text,
                attachments,
                ..
            } if text == "Here it is." && attachments.is_empty()
        )));
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Rejected { ref code, .. } if code == refusal::ATTACHMENTS_UNAVAILABLE
        ));
    }

    #[test]
    fn a_cross_surface_steer_moves_the_response_association() {
        let service = service();
        let (_, one) = surface(&service, 1, "one");
        let (_, two) = surface(&service, 2, "two");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.surface_message(1, "m-1".into(), "start".into(), vec![]);
        agent_in.recv().unwrap();
        service.surface_message(2, "m-2".into(), "steer".into(), vec![]);
        agent_in.recv().unwrap();
        service.agent_request(
            10,
            AgentRequestBody::Response {
                turn_id: None,
                proactive_id: None,
                text: "Done.".into(),
                details: None,
                widgets: None,
                attachments: vec![],
                receipts: vec![],
            },
        );
        for inbox in [&one, &two] {
            assert!(drain(inbox).iter().any(|body| matches!(body, SurfaceResponseBody::Message { role: ConversationRole::Assistant, surface, .. } if surface == "two")));
        }
    }

    #[test]
    fn an_answer_to_a_surface_that_left_reaches_the_others_unprompted() {
        // Alex spoke from the phone last night and the phone is gone by
        // morning. The answer is worth more to the screens that are still here
        // than a refusal is to the agent.
        let service = service();
        let (generation, one) = surface(&service, 1, "one");
        let (_, two) = surface(&service, 2, "two");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.surface_message(1, "m-1".into(), "start".into(), vec![]);
        agent_in.recv().unwrap();
        service.unregister_surface(1, generation);
        while one.try_recv().is_ok() {}
        service.agent_request(
            10,
            AgentRequestBody::Response {
                turn_id: None,
                proactive_id: None,
                text: "Done.".into(),
                details: None,
                widgets: None,
                attachments: vec![],
                receipts: vec![],
            },
        );
        assert!(drain(&two).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Message { role: ConversationRole::Assistant, surface, .. }
                if surface == UNPROMPTED_SURFACE
        )));
        // Nothing was refused: the agent hears no more about it.
        assert!(agent_in.try_recv().is_err());
    }

    #[test]
    fn an_answer_to_a_surface_that_left_carries_no_widgets() {
        // A widget belongs to the surface that asked, and that surface is not
        // here to draw it, so the answer travels as prose alone.
        let service = service();
        let (outbox, one) = sync_channel(256);
        let generation = service.register_surface(
            1,
            SurfaceRegistration {
                id: "one".into(),
                name: "One".into(),
                widgets: vec![WidgetDefinition {
                    name: "summary".into(),
                    description: "Summary".into(),
                    input_schema: serde_json::json!({"type":"object","additionalProperties":false}),
                }],
            },
            outbox,
        );
        while one.try_recv().is_ok() {}
        let (_, two) = surface(&service, 2, "two");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.surface_message(1, "m-1".into(), "start".into(), vec![]);
        agent_in.recv().unwrap();
        service.unregister_surface(1, generation);
        service.agent_request(
            10,
            AgentRequestBody::Response {
                turn_id: None,
                proactive_id: None,
                text: "Done.".into(),
                details: None,
                widgets: Some(vec![WidgetCall {
                    id: "w-1".into(),
                    name: "summary".into(),
                    arguments: serde_json::json!({}),
                }]),
                attachments: vec![],
                receipts: vec![],
            },
        );
        assert!(drain(&two).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Message { surface, widgets: None, .. }
                if surface == UNPROMPTED_SURFACE
        )));
        assert!(agent_in.try_recv().is_err());
    }

    #[test]
    fn a_refused_answer_leaves_the_turn_open_for_a_corrected_one() {
        // The owner is still waiting: a rejection is not an answer, so the
        // association it was refused against has to survive it.
        let service = service();
        let (_, one) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.surface_message(1, "m-1".into(), "start".into(), vec![]);
        agent_in.recv().unwrap();
        service.agent_request(
            10,
            AgentRequestBody::Response {
                turn_id: None,
                proactive_id: None,
                text: "Done.".into(),
                details: None,
                widgets: None,
                attachments: vec!["att_missing".into()],
                receipts: vec![],
            },
        );
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Rejected { code, .. } if code == refusal::ATTACHMENTS_UNAVAILABLE
        ));
        service.agent_request(
            10,
            AgentRequestBody::Response {
                turn_id: None,
                proactive_id: None,
                text: "Done.".into(),
                details: None,
                widgets: None,
                attachments: vec![],
                receipts: vec![],
            },
        );
        assert!(drain(&one).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Message { role: ConversationRole::Assistant, surface, .. }
                if surface == "one"
        )));
    }

    fn wake(service: &Arc<Service>, id: &str, text: &str) -> ControlResponseBody {
        service.control_wake(id.into(), "scufris-wake".into(), text.into(), None)
    }

    #[test]
    fn a_wake_reaches_the_agent_and_is_neither_recorded_nor_echoed() {
        let service = service();
        let (_, one) = surface(&service, 1, "one");
        let (_, two) = surface(&service, 2, "two");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        // A systemd timer, not the owner, has words for the foreground.
        let answer = service.control_wake(
            "wake-1".into(),
            "scufris-briefing".into(),
            "The morning briefing is collected.".into(),
            Some(serde_json::json!({"profile": "morning"})),
        );
        assert!(matches!(answer, ControlResponseBody::WakeAck { id } if id == "wake-1"));
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Wake {
                custom_type,
                text,
                details,
                ..
            } if custom_type == "scufris-briefing"
                && text == "The morning briefing is collected."
                && details == Some(serde_json::json!({"profile": "morning"}))
        ));
        // It is not a turn the owner took: no conversation entry, no echo.
        assert_eq!(service.lock().conversation.len(), 0);
        assert!(drain(&one).is_empty());
        assert!(drain(&two).is_empty());
    }

    #[test]
    fn a_wake_owns_its_answer_only_when_no_owner_turn_is_open() {
        let service = service();
        let (_, one) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        let answer = |service: &Arc<Service>| {
            service.agent_request(
                10,
                AgentRequestBody::Response {
                    turn_id: None,
                    proactive_id: None,
                    text: "Looked.".into(),
                    details: None,
                    widgets: None,
                    attachments: vec![],
                    receipts: vec![],
                },
            );
        };

        // Nobody has spoken yet, so what the wake produces is still unprompted.
        assert!(matches!(
            wake(&service, "wake-1", "Look at this."),
            ControlResponseBody::WakeAck { .. }
        ));
        agent_in.recv().unwrap();
        answer(&service);
        assert!(drain(&one).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Message { surface, .. } if surface == UNPROMPTED_SURFACE
        )));

        // The owner speaks and a wake lands before the answer. The owner asked
        // first and is waiting, so the answer that arrives is still his.
        service.surface_message(1, "m-1".into(), "hello".into(), vec![]);
        agent_in.recv().unwrap();
        while one.try_recv().is_ok() {}
        wake(&service, "wake-2", "And this.");
        agent_in.recv().unwrap();
        answer(&service);
        assert!(drain(&one).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Message { surface, .. } if surface == "one"
        )));

        // That answer closed the turn. Nobody is waiting now, so the next
        // wake's answer is unprompted however recently the owner spoke.
        wake(&service, "wake-3", "And later still.");
        agent_in.recv().unwrap();
        answer(&service);
        assert!(drain(&one).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Message { surface, .. } if surface == UNPROMPTED_SURFACE
        )));
    }

    #[test]
    fn a_wake_with_no_agent_is_refused_with_a_code_and_a_detail() {
        // A caller that cannot reach the foreground has to be told, because
        // its own durable state is the only fallback it has.
        let service = service();
        assert!(matches!(
            wake(&service, "wake-1", "Nobody is home."),
            ControlResponseBody::Rejected { id, code, detail }
                if id == "wake-1" && code == refusal::AGENT_UNAVAILABLE && !detail.is_empty()
        ));
    }

    #[test]
    fn an_unprompted_response_is_shown_by_every_surface_and_spoken_by_none() {
        let service = service();
        let (_, one) = surface(&service, 1, "one");
        let (_, two) = surface(&service, 2, "two");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        // A morning briefing reaches a service nobody has spoken to yet.
        service.agent_request(
            10,
            AgentRequestBody::Response {
                turn_id: None,
                proactive_id: None,
                text: "Good morning.".into(),
                details: None,
                widgets: Some(vec![WidgetCall {
                    id: "w-1".into(),
                    name: "summary".into(),
                    arguments: serde_json::json!({}),
                }]),
                attachments: vec![],
                receipts: vec![],
            },
        );
        for inbox in [&one, &two] {
            assert!(drain(inbox).iter().any(|body| matches!(
                body,
                SurfaceResponseBody::Message {
                    role: ConversationRole::Assistant,
                    surface,
                    widgets: None,
                    ..
                } if surface == UNPROMPTED_SURFACE
            )));
        }
        // Nothing was refused, and no surface was named, so nothing is spoken.
        assert!(agent_in.try_recv().is_err());

        // The owner's first message associates a surface, and the answer after
        // it is that surface's to speak.
        service.surface_message(1, "m-1".into(), "hello".into(), vec![]);
        agent_in.recv().unwrap();
        service.agent_request(
            10,
            AgentRequestBody::Response {
                turn_id: None,
                proactive_id: None,
                text: "Good morning again.".into(),
                details: None,
                widgets: None,
                attachments: vec![],
                receipts: vec![],
            },
        );
        assert!(drain(&one).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Message {
                role: ConversationRole::Assistant,
                surface,
                ..
            } if surface == "one"
        )));
    }

    fn row(id: &str, state: JobRowState, summary: &str) -> JobRow {
        JobRow {
            id: id.into(),
            project: Some("scufris2".into()),
            state,
            since: 1_757_000_000,
            summary: summary.into(),
        }
    }

    #[test]
    fn the_tray_word_is_folded_from_the_rows_and_a_failed_row_holds_it() {
        // The aggregate this replaced was sent as its own field, so a surface
        // could be told `clear` while a row said `failed`. There is one source
        // now: filing the row is what puts the tray back to quiet, and nothing
        // else can.
        let service = service();
        let (_, inbox) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();

        service.agent_request(
            10,
            AgentRequestBody::Jobs {
                jobs: vec![
                    row("3f81c204b1e9", JobRowState::Working, "reviewing G1"),
                    row(
                        "9ad0117e5c22",
                        JobRowState::Blocked,
                        "needs the display slot",
                    ),
                    row("16f0eceb1bb9", JobRowState::Failed, "the harness exited 1"),
                ],
            },
        );
        assert_eq!(
            service.control_state(),
            (ScufrisState::Failed, "the harness exited 1".into())
        );
        let published = drain(&inbox);
        assert!(
            published
                .iter()
                .any(|body| matches!(body, SurfaceResponseBody::Jobs { jobs } if jobs.len() == 3))
        );

        // The blocked job is still blocked, so filing the failure uncovers it
        // rather than clearing the tray.
        service.agent_request(
            10,
            AgentRequestBody::Jobs {
                jobs: vec![
                    row("3f81c204b1e9", JobRowState::Working, "reviewing G1"),
                    row(
                        "9ad0117e5c22",
                        JobRowState::Blocked,
                        "needs the display slot",
                    ),
                ],
            },
        );
        assert_eq!(
            service.control_state(),
            (ScufrisState::Blocked, "needs the display slot".into())
        );

        // Filing the last row leaves nothing needing him, and the word falls
        // back to whatever the service itself is doing.
        service.agent_request(10, AgentRequestBody::Jobs { jobs: vec![] });
        assert_eq!(service.control_state().0, ScufrisState::Starting);
    }

    #[test]
    fn a_restarted_logical_job_returns_after_the_prior_snapshot_was_cleared() {
        let service = service();
        let (_, inbox) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        let id = "3f81c204b1e9";

        service.agent_request(
            10,
            AgentRequestBody::Jobs {
                jobs: vec![row(id, JobRowState::Done, "generation one complete")],
            },
        );
        assert!(drain(&inbox).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Jobs { jobs }
                if jobs.len() == 1 && jobs[0].id == id && jobs[0].state == JobRowState::Done
        )));

        // Filing generation one reaches the service as a whole empty list.
        service.agent_request(10, AgentRequestBody::Jobs { jobs: vec![] });
        assert!(
            drain(&inbox)
                .iter()
                .any(|body| matches!(body, SurfaceResponseBody::Jobs { jobs } if jobs.is_empty()))
        );

        // Steering reuses the logical ID. The new active generation is still a
        // new whole snapshot and must be broadcast to every surface.
        service.agent_request(
            10,
            AgentRequestBody::Jobs {
                jobs: vec![row(
                    id,
                    JobRowState::Working,
                    "foreground guidance submitted",
                )],
            },
        );
        assert!(drain(&inbox).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Jobs { jobs }
                if jobs.len() == 1
                    && jobs[0].id == id
                    && jobs[0].state == JobRowState::Working
        )));
    }

    #[test]
    fn an_offer_is_taken_once_and_stays_taken_in_the_replay() {
        // Spent lives on the message that carries the badge, because that
        // message is what a reconnecting surface replays. Without it every
        // restart would hand back a live button for work already done.
        let service = service();
        let (_, inbox) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.surface_message(1, "m-1".into(), "land it".into(), vec![]);
        agent_in.recv().unwrap();
        service.agent_request(
            10,
            AgentRequestBody::Response {
                turn_id: None,
                proactive_id: None,
                text: "Landed it, and it is not pushed.".into(),
                details: None,
                widgets: None,
                attachments: vec![],
                receipts: vec![Citation {
                    job_id: "750a4de8a80d".into(),
                    badges: vec![Receipt {
                        label: "pushed".into(),
                        value: "no".into(),
                        state: ReceiptState::Refuted,
                    }],
                    offers: vec![Offer {
                        id: "offer-1".into(),
                        label: "push master".into(),
                        taken: false,
                    }],
                }],
            },
        );
        while agent_in.try_recv().is_ok() {}
        while inbox.try_recv().is_ok() {}

        service.surface_offer_take(1, "offer-1".into());
        assert!(matches!(
            agent_in.try_recv().unwrap().body,
            AgentResponseBody::OfferTake { id } if id == "offer-1"
        ));
        assert!(
            drain(&inbox).iter().any(
                |body| matches!(body, SurfaceResponseBody::OfferTaken { id } if id == "offer-1")
            )
        );

        // Pressed twice, or pressed on a screen that had not heard yet.
        service.surface_offer_take(1, "offer-1".into());
        assert!(agent_in.try_recv().is_err());
        assert!(drain(&inbox).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Rejected { code, .. } if code == refusal::OFFER_UNAVAILABLE
        )));

        let (outbox, replay) = sync_channel(256);
        service.register_surface(
            2,
            SurfaceRegistration {
                id: "two".into(),
                name: "two".into(),
                widgets: vec![],
            },
            outbox,
        );
        assert!(drain(&replay).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::Message { receipts, .. }
                if receipts.iter().any(|citation| citation.offers.iter().all(|offer| offer.taken))
        )));
    }

    #[test]
    fn a_job_command_reaches_the_agent_that_owns_the_job() {
        let service = service();
        let (_, inbox) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(8);
        service.register_agent(10, agent);
        agent_in.recv().unwrap();
        service.surface_job_command(1, "3f81c204b1e9".into(), JobAction::Cancel);
        assert!(matches!(
            agent_in.try_recv().unwrap().body,
            AgentResponseBody::JobCommand { id, action }
                if id == "3f81c204b1e9" && action == JobAction::Cancel
        ));
        // Nothing is decided here: what stopping costs belongs to the agent,
        // and the row list it publishes next is the answer.
        assert!(drain(&inbox).is_empty());
    }
    /// The checked-in stand-in agent, so a lease test measures a real child
    /// being stopped and started rather than a path that does not exist.
    fn leasable_service() -> Arc<Service> {
        let mut config = Config::test(test_runtime());
        config.agent = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/agent"));
        config.session_dir = config.attachment_dir.with_file_name("sessions");
        service_at(config)
    }

    fn rejection(inbox: &Receiver<AgentResponse>) -> (Option<String>, String) {
        match inbox.recv().unwrap().body {
            AgentResponseBody::Rejected { id, code, .. } => (id, code),
            other => panic!("expected a rejection, got {other:?}"),
        }
    }

    fn holder(pid: u32) -> LeaseHolder {
        LeaseHolder {
            pid,
            session_file: None,
            cwd: "/home/test/scufris2".into(),
        }
    }

    fn acquire(service: &Arc<Service>, connection: u64, id: &str) -> ControlResponseBody {
        service.control_lease_acquire(connection, id.into(), holder(4242), false)
    }

    /// One leased terminal at its prompt, as every terminal test starts.
    fn leased(
        service: &Arc<Service>,
        connection: u64,
    ) -> (Receiver<AgentResponse>, ControlResponseBody) {
        let granted = acquire(service, 100, "l");
        let (terminal, terminal_in) = sync_channel(8);
        assert!(service.admit_agent(connection, Some(1), None, terminal));
        assert!(matches!(
            terminal_in.recv().unwrap().body,
            AgentResponseBody::Ready
        ));
        (terminal_in, granted)
    }

    fn session(file: &str, parent: Option<&str>) -> AgentSession {
        AgentSession {
            id: "01J000000000000000000000".into(),
            file: file.into(),
            cwd: "/home/test/scufris2".into(),
            parent: parent.map(str::to_string),
        }
    }

    #[test]
    fn the_lease_is_refused_unless_the_service_offers_it() {
        let mut config = Config::test(test_runtime());
        config.terminal_lease = false;
        let service = service_at(config);
        assert!(matches!(
            acquire(&service, 100, "l"),
            ControlResponseBody::Rejected { code, .. } if code == refusal::LEASE_DISABLED
        ));
        // Nothing granted, so a hello that names a generation is a stranger.
        let (agent, agent_in) = sync_channel(8);
        assert!(!service.admit_agent(10, Some(1), None, agent));
        assert_eq!(rejection(&agent_in).1, refusal::NOT_LEASE_HOLDER);
    }

    #[test]
    fn a_lease_stops_the_child_and_fences_the_agent_channel() {
        let service = leasable_service();
        service.start_agent();
        assert!(service.lock().process.is_some());
        let (_, inbox) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(8);
        assert!(service.register_agent(10, agent));
        agent_in.recv().unwrap();

        let granted = acquire(&service, 100, "l1");
        assert!(matches!(
            &granted,
            ControlResponseBody::Lease { id, generation: 1, lineage_file: None, sequence: 0, owner, .. }
                if id == "l1" && owner == FOREGROUND_OWNER
        ));
        // The managed child is told where the conversation went before it is
        // stopped, so its own shutdown knows this was a handoff.
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Handoff {
                generation: 1,
                next: AgentHolder::Terminal
            }
        ));
        // The child is stopped and reaped, the managed agent connection is
        // gone, and a restart is not attempted while the lease is held.
        assert!(service.lock().process.is_none());
        assert!(agent_in.recv().is_err());
        service.start_agent();
        assert!(service.lock().process.is_none());
        assert!(drain(&inbox).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::State { state: ScufrisState::Starting, detail, holder: AgentHolder::Terminal }
                if detail == "A terminal holds the agent."
        )));
        // The same holder asking again gets the same grant.
        assert!(matches!(
            acquire(&service, 100, "l2"),
            ControlResponseBody::Lease { generation: 1, .. }
        ));
        assert!(matches!(
            acquire(&service, 101, "l3"),
            ControlResponseBody::Rejected { code, .. } if code == refusal::LEASE_HELD
        ));

        // The fence: no generation, a wrong one, then the right one.
        let (plain, plain_in) = sync_channel(8);
        assert!(!service.admit_agent(11, None, None, plain));
        assert_eq!(rejection(&plain_in).1, refusal::LEASE_REQUIRED);
        let (stale, stale_in) = sync_channel(8);
        assert!(!service.admit_agent(12, Some(7), None, stale));
        assert_eq!(rejection(&stale_in).1, refusal::LEASE_REQUIRED);
        let (terminal, terminal_in) = sync_channel(8);
        assert!(service.admit_agent(13, Some(1), None, terminal));
        assert!(matches!(
            terminal_in.recv().unwrap().body,
            AgentResponseBody::Ready
        ));
        assert!(drain(&inbox).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::State {
                state: ScufrisState::Idle,
                holder: AgentHolder::Terminal,
                ..
            }
        )));
        let (second, second_in) = sync_channel(8);
        assert!(!service.admit_agent(14, Some(1), None, second));
        assert_eq!(rejection(&second_in).1, refusal::AGENT_EXISTS);

        // Release from a stranger is refused; from the holder it restarts.
        assert!(matches!(
            service.control_lease_release(101, "r1".into()),
            ControlResponseBody::Rejected { code, .. } if code == refusal::NOT_LEASE_HOLDER
        ));
        assert!(matches!(
            service.control_lease_release(100, "r2".into()),
            ControlResponseBody::LeaseReleased { id } if id == "r2"
        ));
        assert!(matches!(
            terminal_in.recv().unwrap().body,
            AgentResponseBody::Handoff {
                generation: 1,
                next: AgentHolder::Managed
            }
        ));
        assert!(service.lock().process.is_some());
        assert!(service.lock().lease.is_none());
        assert!(terminal_in.recv().is_err());
        assert!(drain(&inbox).iter().any(|body| matches!(
            body,
            SurfaceResponseBody::State { state: ScufrisState::Starting, detail, holder: AgentHolder::Managed }
                if detail == "The agent is restarting."
        )));

        // A second grant counts up, and the old generation is dead.
        assert!(matches!(
            acquire(&service, 200, "l4"),
            ControlResponseBody::Lease { generation: 2, .. }
        ));
        assert!(service.lock().process.is_none());
        let (old, old_in) = sync_channel(8);
        assert!(!service.admit_agent(15, Some(1), None, old));
        assert_eq!(rejection(&old_in).1, refusal::LEASE_REQUIRED);
        // The lease is the connection: closing it is the release.
        service.control_disconnected(201);
        assert!(service.lock().lease.is_some());
        service.control_disconnected(200);
        assert!(service.lock().lease.is_none());
        assert!(service.lock().process.is_some());
        service.shutdown();
    }

    #[test]
    fn a_terminal_turn_and_its_answer_are_recorded_under_the_terminal_name() {
        let service = service();
        let (_, inbox) = surface(&service, 1, "one");
        let (terminal_in, _) = leased(&service, 13);
        drain(&inbox);

        service.agent_request(
            13,
            AgentRequestBody::Turn {
                id: "t-0123456789abcdef".into(),
                text: "Typed at the keyboard.".into(),
                images: 0,
            },
        );
        // The turn is acknowledged with the place it was recorded at, which is
        // what lets the terminal attach its answer to exactly this turn.
        assert!(matches!(
            terminal_in.recv().unwrap().body,
            AgentResponseBody::TurnAck { id, sequence: 1 } if id == "t-0123456789abcdef"
        ));
        let recorded = drain(&inbox);
        assert!(matches!(
            &recorded[..],
            [SurfaceResponseBody::Message { role: ConversationRole::User, surface, text, .. }]
                if surface == TERMINAL_SURFACE && text == "Typed at the keyboard."
        ));
        // The answer keeps the terminal's name, and a widget call has no
        // registration to be checked against, so it is dropped and the agent
        // is told rather than the answer being thrown away.
        service.agent_request(
            13,
            AgentRequestBody::Response {
                text: "Answered on the screen.".into(),
                turn_id: Some("t-0123456789abcdef".into()),
                proactive_id: None,
                details: Some("more".into()),
                widgets: Some(vec![WidgetCall {
                    id: "w1".into(),
                    name: "summary".into(),
                    arguments: serde_json::json!({}),
                }]),
                attachments: vec![],
                receipts: vec![],
            },
        );
        assert_eq!(
            rejection(&terminal_in),
            (
                Some("t-0123456789abcdef".to_string()),
                refusal::INVALID_WIDGETS.to_string()
            )
        );
        let answered = drain(&inbox);
        assert!(matches!(
            &answered[..],
            [SurfaceResponseBody::Message { role: ConversationRole::Assistant, surface, text, widgets: None, .. }]
                if surface == TERMINAL_SURFACE && text == "Answered on the screen."
        ));
        assert!(service.lock().associated_surface.is_none());
        assert!(service.lock().associated_turn.is_none());

        // A surface joining later replays both, in order.
        let (outbox, replay) = sync_channel(256);
        service.register_surface(
            2,
            SurfaceRegistration {
                id: "two".into(),
                name: "two".into(),
                widgets: vec![],
            },
            outbox,
        );
        let replayed: Vec<_> = drain(&replay)
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
            ]
        );
    }

    #[test]
    fn a_turn_says_how_many_images_it_carried_and_only_the_holder_may_send_one() {
        let service = service();
        let (_, inbox) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(8);
        assert!(service.register_agent(10, agent));
        agent_in.recv().unwrap();
        service.agent_request(
            10,
            AgentRequestBody::Turn {
                id: "t-00000000000000ff".into(),
                text: "Not mine to say.".into(),
                images: 0,
            },
        );
        assert_eq!(
            rejection(&agent_in),
            (
                Some("t-00000000000000ff".to_string()),
                refusal::NOT_LEASE_HOLDER.to_string()
            )
        );
        assert!(drain(&inbox).is_empty());
        assert_eq!(service.lock().conversation.len(), 0);
        service.unregister_agent(10);

        // Images stay in the terminal. How many there were is the honest half
        // of what a screen that cannot show them can say.
        let (terminal_in, _) = leased(&service, 13);
        drain(&inbox);
        service.agent_request(
            13,
            AgentRequestBody::Turn {
                id: "t-00000000000000a1".into(),
                text: "Look at this.".into(),
                images: 2,
            },
        );
        assert!(matches!(
            terminal_in.recv().unwrap().body,
            AgentResponseBody::TurnAck { .. }
        ));
        assert!(matches!(
            &drain(&inbox)[..],
            [SurfaceResponseBody::Message { text, .. }]
                if text == "Look at this.\n\n[2 images]"
        ));
    }

    #[test]
    fn an_answer_closes_only_the_turn_it_names() {
        let service = service();
        let (_, inbox) = surface(&service, 1, "one");
        let (terminal_in, _) = leased(&service, 13);
        drain(&inbox);
        service.agent_request(
            13,
            AgentRequestBody::Turn {
                id: "t-1111111111111111".into(),
                text: "First.".into(),
                images: 0,
            },
        );
        terminal_in.recv().unwrap();
        drain(&inbox);

        // An answer to a turn that is no longer open is still words that
        // happened in the terminal. It is recorded there, and the turn that
        // is still owed an answer stays open.
        service.agent_request(
            13,
            AgentRequestBody::Response {
                text: "An answer to something older.".into(),
                turn_id: Some("t-2222222222222222".into()),
                proactive_id: None,
                details: None,
                widgets: None,
                attachments: vec![],
                receipts: vec![],
            },
        );
        assert!(matches!(
            &drain(&inbox)[..],
            [SurfaceResponseBody::Message { surface, .. }] if surface == TERMINAL_SURFACE
        ));
        assert_eq!(
            service.lock().associated_turn.as_deref(),
            Some("t-1111111111111111")
        );
        // The one it names closes it.
        service.agent_request(
            13,
            AgentRequestBody::Response {
                text: "The answer to the first.".into(),
                turn_id: Some("t-1111111111111111".into()),
                proactive_id: None,
                details: None,
                widgets: None,
                attachments: vec![],
                receipts: vec![],
            },
        );
        assert!(service.lock().associated_turn.is_none());
        assert!(terminal_in.try_recv().is_err());
    }

    #[test]
    fn an_answer_nobody_can_correlate_is_recorded_rather_than_dropped() {
        let service = service();
        let (_, inbox) = surface(&service, 1, "one");
        let (terminal_in, _) = leased(&service, 13);
        submit_briefing(&service, "b1");
        assert!(matches!(
            terminal_in.recv().unwrap().body,
            AgentResponseBody::Wake { .. }
        ));
        assert!(service.lock().active_proactive.is_some());
        drain(&inbox);

        // The slot is reserved for the briefing's own answer. An answer that
        // names no turn cannot be shown to be it, so it is recorded where
        // nobody is spoken to rather than thrown away.
        service.agent_request(
            13,
            AgentRequestBody::Response {
                text: "Something else entirely.".into(),
                turn_id: None,
                proactive_id: None,
                details: None,
                widgets: None,
                attachments: vec![],
                receipts: vec![],
            },
        );
        assert!(matches!(
            &drain(&inbox)[..],
            [SurfaceResponseBody::Message { surface, text, .. }]
                if surface == UNPROMPTED_SURFACE && text == "Something else entirely."
        ));
        assert!(service.lock().active_proactive.is_some());
        // A proactive identifier that is not the reserved one is still
        // dropped: it claims to close a delivery it cannot close.
        service.agent_request(13, proactive_answer("other", "Not this run."));
        assert!(drain(&inbox).is_empty());
    }

    #[test]
    fn a_working_agent_is_not_taken_unless_the_caller_asks_for_it() {
        let service = leasable_service();
        service.start_agent();
        let (agent, agent_in) = sync_channel(8);
        assert!(service.register_agent(10, agent));
        agent_in.recv().unwrap();
        service.apply(Event::AgentStart);
        assert_eq!(service.control_state().0, ScufrisState::Working);

        assert!(matches!(
            service.control_lease_acquire(100, "l1".into(), holder(7), false),
            ControlResponseBody::Rejected { code, .. } if code == refusal::AGENT_BUSY
        ));
        assert!(service.lock().lease.is_none());
        assert!(service.lock().process.is_some());

        // Asking to abort sends one, and the grant waits for the turn to end.
        let settling = Arc::clone(&service);
        let settler = thread::spawn(move || {
            thread::sleep(ABORT_STEP);
            settling.apply(Event::AgentSettled);
        });
        let granted = service.control_lease_acquire(100, "l2".into(), holder(7), true);
        settler.join().unwrap();
        assert!(matches!(
            granted,
            ControlResponseBody::Lease { generation: 1, .. }
        ));
        assert!(matches!(
            agent_in.recv().unwrap().body,
            AgentResponseBody::Abort { id } if id == "l2"
        ));
        assert!(service.lock().process.is_none());
        service.shutdown();
    }

    #[test]
    fn a_failed_service_is_not_hidden_behind_a_terminal() {
        let service = service();
        {
            let mut inner = service.lock();
            inner.lifecycle = Lifecycle::Failed;
            inner.lifecycle_detail = "The agent stopped 3 times in a row.".into();
        }
        assert!(matches!(
            acquire(&service, 100, "l"),
            ControlResponseBody::Rejected { code, detail, .. }
                if code == refusal::AGENT_BUSY && detail == "The agent stopped 3 times in a row."
        ));
    }

    #[test]
    fn a_holder_that_stops_answering_loses_the_lease_and_a_late_ping_says_so() {
        let service = leasable_service();
        let granted = acquire(&service, 100, "l");
        assert!(matches!(granted, ControlResponseBody::Lease { .. }));
        assert!(matches!(
            service.control_lease_ping(100, "p1".into()),
            ControlResponseBody::LeasePong { id, generation: 1 } if id == "p1"
        ));
        assert!(matches!(
            service.control_lease_ping(101, "p2".into()),
            ControlResponseBody::Rejected { code, .. } if code == refusal::LEASE_PING_STALE
        ));
        // A holder that is still answering keeps it.
        assert!(!service.sweep_lease(1));
        assert!(service.lock().lease.is_some());
        // Three missed heartbeats end it as if the socket had closed. The
        // clock is moved rather than waited out: this is the check the
        // watching thread runs, and running it here runs all of it.
        {
            let mut inner = service.lock();
            let held = inner.lease.as_mut().expect("the lease is held");
            held.last_ping = Instant::now() - LEASE_DEADLINE - LEASE_PING_INTERVAL;
        }
        assert!(service.sweep_lease(1));
        assert!(
            service.lock().lease.is_none(),
            "the lease outlived its holder"
        );
        assert!(matches!(
            service.control_lease_ping(100, "p3".into()),
            ControlResponseBody::Rejected { code, .. } if code == refusal::LEASE_PING_STALE
        ));
        service.shutdown();
    }

    #[test]
    fn the_lineage_is_what_the_next_holder_forks_from() {
        let service = leasable_service();
        // The managed child's own file is what --continue already finds, so
        // nothing is forked from it.
        let (agent, agent_in) = sync_channel(8);
        assert!(service.admit_agent(
            10,
            None,
            Some(session("/srv/sessions/child.jsonl", None)),
            agent
        ));
        agent_in.recv().unwrap();
        assert!(Service::fork_from(&service.lock()).is_none());

        let granted = acquire(&service, 100, "l");
        assert!(matches!(
            &granted,
            ControlResponseBody::Lease { lineage_file: Some(file), .. }
                if file == "/srv/sessions/child.jsonl"
        ));
        // A fork of that file carries the conversation, so it is told nothing
        // it already has, and it becomes what the child is started from.
        let (terminal, terminal_in) = sync_channel(8);
        assert!(service.admit_agent(
            13,
            Some(1),
            Some(session(
                "/srv/sessions/terminal.jsonl",
                Some("/srv/sessions/child.jsonl")
            )),
            terminal
        ));
        assert!(matches!(
            terminal_in.recv().unwrap().body,
            AgentResponseBody::Ready
        ));
        assert!(terminal_in.try_recv().is_err());
        assert_eq!(
            Service::fork_from(&service.lock()),
            Some(PathBuf::from("/srv/sessions/terminal.jsonl"))
        );
        // A later switch inside the terminal moves the lineage with it.
        service.agent_request(
            13,
            AgentRequestBody::Session {
                id: "01J000000000000000000001".into(),
                file: "/srv/sessions/branch.jsonl".into(),
                cwd: "/home/test/scufris2".into(),
                parent: Some("/srv/sessions/terminal.jsonl".into()),
            },
        );
        assert_eq!(
            Service::fork_from(&service.lock()),
            Some(PathBuf::from("/srv/sessions/branch.jsonl"))
        );
        service.shutdown();
    }

    #[test]
    fn an_agent_that_does_not_continue_the_lineage_is_told_what_it_missed() {
        let service = service();
        let (_, inbox) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(8);
        assert!(service.admit_agent(
            10,
            None,
            Some(session("/srv/sessions/child.jsonl", None)),
            agent
        ));
        agent_in.recv().unwrap();
        service.surface_message(1, "m1".into(), "Remember the number nine.".into(), vec![]);
        drain(&inbox);
        acquire(&service, 100, "l");
        // A plain terminal on its own session has never seen these words.
        let (terminal, terminal_in) = sync_channel(8);
        assert!(service.admit_agent(
            13,
            Some(1),
            Some(session("/home/test/.pi/other.jsonl", None)),
            terminal
        ));
        assert!(matches!(
            terminal_in.recv().unwrap().body,
            AgentResponseBody::Ready
        ));
        let caught_up = terminal_in.recv().unwrap().body;
        let AgentResponseBody::CatchUp { since, entries } = caught_up else {
            panic!("expected a catch-up, got {caught_up:?}");
        };
        assert_eq!(since, 0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].text, "Remember the number nine.");
        assert_eq!(entries[0].surface, "one");
        // The page is the tail of the replay, and the same page comes back
        // through the control socket for a holder that asks for itself.
        assert!(matches!(
            service.control_conversation("c1".into(), 0),
            ControlResponseBody::ConversationEntries { entries, more: false, .. }
                if entries.len() == 1
        ));
        assert!(matches!(
            service.control_conversation("c2".into(), 1),
            ControlResponseBody::ConversationEntries { entries, .. } if entries.is_empty()
        ));
    }

    #[test]
    fn a_catch_up_page_is_bounded_and_says_when_more_follows() {
        let service = service();
        let (_, inbox) = surface(&service, 1, "one");
        let (agent, agent_in) = sync_channel(256);
        assert!(service.register_agent(10, agent));
        agent_in.recv().unwrap();
        for index in 0..(MAX_CONVERSATION_PAGE + 5) {
            service.surface_message(1, format!("m{index}"), format!("Line {index}."), vec![]);
            service.lock().close_turn();
        }
        drain(&inbox);
        let ControlResponseBody::ConversationEntries { entries, more, .. } =
            service.control_conversation("c1".into(), 0)
        else {
            panic!("expected a page");
        };
        assert_eq!(entries.len(), MAX_CONVERSATION_PAGE);
        assert!(more, "a page that stopped short says so");
        let last = entries.last().expect("the page is not empty").sequence;
        assert!(matches!(
            service.control_conversation("c2".into(), last),
            ControlResponseBody::ConversationEntries { entries, more: false, .. }
                if entries.len() == 5
        ));
    }

    #[test]
    fn the_state_a_control_client_reads_says_who_holds_the_agent() {
        let service = leasable_service();
        assert!(matches!(
            service.control_state_full("s1".into()),
            ControlResponseBody::State {
                holder: AgentHolder::Managed,
                generation: None,
                lineage_file: None,
                ..
            }
        ));
        // The launcher starts a terminal in this directory, so the service
        // answering it is what keeps one default from being written twice.
        assert!(matches!(
            service.control_state_full("s1".into()),
            ControlResponseBody::State { session_dir, .. }
                if session_dir == service.config.session_dir.to_string_lossy()
        ));
        {
            let mut inner = service.lock();
            inner.lineage_file = Some(PathBuf::from("/srv/sessions/child.jsonl"));
        }
        acquire(&service, 100, "l");
        assert!(matches!(
            service.control_state_full("s2".into()),
            ControlResponseBody::State {
                holder: AgentHolder::Terminal,
                generation: Some(1),
                lineage_file: Some(file),
                ..
            } if file == "/srv/sessions/child.jsonl"
        ));
        service.shutdown();
    }

    #[test]
    fn a_leased_terminal_reports_its_own_activity_and_still_hears_everyone() {
        let service = service();
        let (_, inbox) = surface(&service, 1, "one");
        let (terminal_in, _) = leased(&service, 13);
        drain(&inbox);
        service.agent_request(13, AgentRequestBody::Activity { working: true });
        assert!(matches!(
            &drain(&inbox)[..],
            [SurfaceResponseBody::State {
                state: ScufrisState::Working,
                ..
            }]
        ));
        assert_eq!(service.control_state().0, ScufrisState::Working);
        service.agent_request(13, AgentRequestBody::Activity { working: false });
        assert!(matches!(
            &drain(&inbox)[..],
            [SurfaceResponseBody::State {
                state: ScufrisState::Idle,
                ..
            }]
        ));
        // A surface's words still reach the leased terminal, and a wake does.
        service.surface_message(1, "m1".into(), "From the phone.".into(), vec![]);
        assert!(matches!(
            terminal_in.recv().unwrap().body,
            AgentResponseBody::Message { id, text, .. } if id == "m1" && text == "From the phone."
        ));
        assert!(matches!(
            service.control_wake("w1".into(), "scufris-wake".into(), "Now.".into(), None),
            ControlResponseBody::WakeAck { .. }
        ));
        assert!(matches!(
            terminal_in.recv().unwrap().body,
            AgentResponseBody::Wake { custom_type, .. } if custom_type == "scufris-wake"
        ));
    }
}
