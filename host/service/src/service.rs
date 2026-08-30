//! Canonical protocol v5 service state.

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
    AgentRequestBody, AgentResponse, AgentResponseBody, AgentState, CONVERSATION_ENTRIES,
    ConversationMessage, ConversationRole, MAX_DETAIL_BYTES, ScufrisState, SurfaceRegistration,
    SurfaceResponse, SurfaceResponseBody, WidgetCall, WidgetDefinition,
};
use serde_json::Value;
use tracing::{debug, error, info, warn};

use crate::{
    agent::{Agent, described},
    attachment::AttachmentStore,
    config::Config,
    rpc::{self, Command, DialogAnswer, Event, SessionState},
};

const BOOT: &str = "boot";
const MAX_EVENT_BYTES: usize = 4 * 1024 * 1024;
const HEALTHY: Duration = Duration::from_secs(10);
const RESTART_DELAY: Duration = Duration::from_secs(1);
const MAX_FAILURES: u32 = 3;
const HELLO_GRACE: Duration = Duration::from_secs(10);

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
    attention: AgentState,
    attention_detail: String,
    surfaces: HashMap<String, RegisteredSurface>,
    surface_by_connection: HashMap<u64, (String, u64)>,
    next_surface_generation: u64,
    agent: Option<AgentConnection>,
    conversation: VecDeque<ConversationMessage>,
    associated_surface: Option<String>,
    session_file: Option<PathBuf>,
    stopping: bool,
}

impl Inner {
    fn state(&self) -> (ScufrisState, String) {
        if self.attention == AgentState::Failed || self.lifecycle == Lifecycle::Failed {
            let detail = if self.attention == AgentState::Failed {
                &self.attention_detail
            } else {
                &self.lifecycle_detail
            };
            return (ScufrisState::Failed, detail.clone());
        }
        if self.attention == AgentState::Blocked {
            return (ScufrisState::Blocked, self.attention_detail.clone());
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

    fn record(&mut self, message: ConversationMessage) {
        if self.conversation.len() == CONVERSATION_ENTRIES {
            self.conversation.pop_front();
        }
        self.conversation.push_back(message.clone());
        self.broadcast(message.into());
    }

    fn publish_state(&mut self) {
        let (state, detail) = self.state();
        self.broadcast(SurfaceResponseBody::State { state, detail });
    }
}

pub struct Service {
    config: Config,
    attachments: Arc<AttachmentStore>,
    inner: Mutex<Inner>,
}

impl Service {
    pub fn new(config: Config, attachments: Arc<AttachmentStore>) -> Arc<Self> {
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
                attention: AgentState::Clear,
                attention_detail: String::new(),
                surfaces: HashMap::new(),
                surface_by_connection: HashMap::new(),
                next_surface_generation: 0,
                agent: None,
                conversation: VecDeque::new(),
                associated_surface: None,
                session_file: None,
                stopping: false,
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|held| held.into_inner())
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
        for message in inner.conversation.clone() {
            let _ = outbox.try_send(SurfaceResponse::new(message.into()));
        }
        let (state, detail) = inner.state();
        let _ = outbox.try_send(SurfaceResponse::new(SurfaceResponseBody::State {
            state,
            detail,
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
        let definitions = held.registration.widgets.clone();
        let descriptors = match self.attachments.resolve(&attachments, true) {
            Ok(descriptors) => descriptors,
            Err(_) => {
                inner.send_surface(
                    &surface,
                    SurfaceResponseBody::Rejected {
                        id: Some(id),
                        operation: "message".into(),
                        code: "attachments_unavailable".into(),
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
                    code: "agent_unavailable".into(),
                    detail: "The Scufris agent is unavailable.".into(),
                },
            );
            return;
        }
        inner.associated_surface = Some(surface.clone());
        inner.record(ConversationMessage {
            role: ConversationRole::User,
            surface: surface.clone(),
            text,
            details: None,
            widgets: None,
            attachments: descriptors,
        });
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
            inner.send_surface(&surface, SurfaceResponseBody::Aborted { id });
        } else {
            inner.send_surface(
                &surface,
                SurfaceResponseBody::Rejected {
                    id: Some(id),
                    operation: "abort".into(),
                    code: "agent_unavailable".into(),
                    detail: "The Scufris agent is unavailable.".into(),
                },
            );
        }
    }

    pub fn register_agent(&self, connection: u64, outbox: SyncSender<AgentResponse>) -> bool {
        let mut inner = self.lock();
        if inner.agent.is_some() {
            info!(connection, "second agent connection rejected");
            let _ = outbox.try_send(AgentResponse::new(AgentResponseBody::Rejected {
                code: "agent_exists".into(),
                detail: "One agent is already connected.".into(),
            }));
            return false;
        }
        let _ = outbox.try_send(AgentResponse::new(AgentResponseBody::Ready));
        inner.agent = Some(AgentConnection { connection, outbox });
        inner.agent_joined = true;
        info!("agent connected");
        debug!(connection, "agent registration accepted");
        true
    }

    pub fn unregister_agent(&self, connection: u64) {
        let mut inner = self.lock();
        if inner
            .agent
            .as_ref()
            .is_some_and(|agent| agent.connection == connection)
        {
            inner.agent = None;
            info!("agent disconnected");
            debug!(connection, "agent registration removed");
        }
    }

    pub fn agent_request(&self, connection: u64, body: AgentRequestBody) {
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
            AgentRequestBody::Hello => {}
            AgentRequestBody::State { state, detail } => {
                inner.attention = state;
                inner.attention_detail = scufris_control::truncate(&detail, MAX_DETAIL_BYTES);
                inner.publish_state();
            }
            AgentRequestBody::Response {
                text,
                details,
                widgets,
                attachments,
            } => {
                let Some(surface) = inner.associated_surface.clone() else {
                    inner.send_agent(AgentResponseBody::Rejected {
                        code: "no_surface".into(),
                        detail: "No surface message is associated with this response.".into(),
                    });
                    return;
                };
                let Some(registration) = inner
                    .surfaces
                    .get(&surface)
                    .map(|held| held.registration.clone())
                else {
                    inner.send_agent(AgentResponseBody::Rejected {
                        code: "surface_unavailable".into(),
                        detail: "The associated surface is not connected.".into(),
                    });
                    return;
                };
                if let Some(calls) = &widgets
                    && let Err(detail) = validate_calls(calls, &registration.widgets)
                {
                    inner.send_agent(AgentResponseBody::Rejected {
                        code: "invalid_widgets".into(),
                        detail,
                    });
                    return;
                }
                let descriptors = match self.attachments.resolve(&attachments, true) {
                    Ok(descriptors) => descriptors,
                    Err(_) => {
                        inner.send_agent(AgentResponseBody::Rejected {
                            code: "attachments_unavailable".into(),
                            detail: "One or more attachments are unavailable.".into(),
                        });
                        return;
                    }
                };
                inner.record(ConversationMessage {
                    role: ConversationRole::Assistant,
                    surface,
                    text,
                    details,
                    widgets,
                    attachments: descriptors,
                });
            }
        }
    }

    pub fn control_state(&self) -> (ScufrisState, String) {
        self.lock().state()
    }

    pub fn start_agent(self: &Arc<Self>) {
        let mut inner = self.lock();
        if inner.stopping || inner.process.is_some() {
            return;
        }
        inner.process_generation += 1;
        inner.started = Instant::now();
        inner.agent_joined = false;
        let generation = inner.process_generation;
        let (agent, streams) = match Agent::start(&self.config) {
            Ok(started) => started,
            Err(error) => {
                inner.failures += 1;
                inner.lifecycle = Lifecycle::Failed;
                inner.lifecycle_detail = format!("The agent would not start: {error}");
                inner.publish_state();
                error!(%error, "the agent would not start");
                return;
            }
        };
        info!(command = described(&self.config), "the agent is starting");
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
                    inner.session_file = session.file.map(PathBuf::from);
                    inner.failures = 0;
                    inner.lifecycle = if session.streaming {
                        Lifecycle::Working
                    } else {
                        Lifecycle::Idle
                    };
                    inner.lifecycle_detail.clear();
                }
                inner.publish_state();
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
            inner.lifecycle_detail =
                format!("The agent stopped {} times in a row.", inner.failures);
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
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, sync_channel},
    };

    static NEXT_TEST: AtomicU64 = AtomicU64::new(1);

    fn service() -> Arc<Service> {
        let runtime = std::env::temp_dir().join(format!(
            "scufris-v5-service-{}-{}",
            std::process::id(),
            NEXT_TEST.fetch_add(1, Ordering::Relaxed)
        ));
        let config = Config::test(runtime);
        let attachments = AttachmentStore::open(config.attachment_dir.clone()).unwrap();
        Service::new(config, attachments)
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
        assert!(matches!(replay[2], SurfaceResponseBody::Ready { .. }));
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
        let held = service.lock().conversation.clone();
        assert_eq!(held.len(), CONVERSATION_ENTRIES);
        assert_eq!(held.front().unwrap().text, "line 5");
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
            SurfaceResponseBody::Rejected { code, .. } if code == "attachments_unavailable"
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
                text: "Passed.".into(),
                details: Some("## Results".into()),
                widgets: Some(vec![WidgetCall {
                    id: "w-1".into(),
                    name: "summary".into(),
                    arguments: serde_json::json!({"passed": 4}),
                }]),
                attachments: vec![],
            },
        );
        assert!(drain(&inbox).iter().any(|body| matches!(body, SurfaceResponseBody::Message { role: ConversationRole::Assistant, surface, details: Some(_), widgets: Some(_), .. } if surface == "one")));
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
                text: "Done.".into(),
                details: None,
                widgets: None,
                attachments: vec![],
            },
        );
        for inbox in [&one, &two] {
            assert!(drain(inbox).iter().any(|body| matches!(body, SurfaceResponseBody::Message { role: ConversationRole::Assistant, surface, .. } if surface == "two")));
        }
    }
}
