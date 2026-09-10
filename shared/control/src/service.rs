//! Scufris protocol v9 typed channels.
//!
//! Surface, agent, and control traffic use separate Unix sockets and separate
//! enums. Each decoder accepts only its channel and direction.

use std::{env, io::BufRead, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ControlPathError, MAX_IDENTIFIER_LENGTH, MessageError, chosen_runtime_dir, in_runtime_dir,
    is_identifier, read_line,
};

pub const SERVICE_VERSION: u32 = 9;
pub const SURFACE_FILE_NAME: &str = "surface.sock";
pub const AGENT_FILE_NAME: &str = "agent.sock";
pub const CONTROL_FILE_NAME: &str = "control.sock";
pub const CONTENT_FILE_NAME: &str = "content.sock";
pub const CONVERSATION_ENTRIES: usize = 200;
/// The surface an unprompted answer carries when no surface has spoken yet.
///
/// A briefing or a finished job can answer a service nobody has used since it
/// started, and that answer still belongs on every screen. It is recorded
/// against this name instead of a surface, so every surface displays it and
/// none of them matches it: nothing is spoken, and no live widget call runs.
/// No surface may register it, which is what keeps that true.
pub const UNPROMPTED_SURFACE: &str = "unprompted";
pub const MAX_SURFACE_NAME_BYTES: usize = 256;
pub const MAX_TEXT_BYTES: usize = 8 * 1024;
pub const MAX_DETAILS_BYTES: usize = 32 * 1024;
pub const MAX_DETAIL_BYTES: usize = 4 * 1024;
pub const MAX_WIDGETS: usize = 32;
pub const MAX_WIDGET_DESCRIPTION_BYTES: usize = 2 * 1024;
pub const MAX_WIDGET_SCHEMA_BYTES: usize = 16 * 1024;
pub const MAX_WIDGET_ARGUMENTS_BYTES: usize = 16 * 1024;
pub const MAX_ATTACHMENTS: usize = 8;
pub const MAX_ATTACHMENT_NAME_BYTES: usize = 255;
pub const MAX_MEDIA_TYPE_BYTES: usize = 127;
pub const MAX_ATTACHMENT_BYTES: u64 = 16 * 1024 * 1024;
/// One citation group per job, and what a group may carry.
///
/// A message that reports more than four jobs is a message nobody reads as
/// evidence, and a group with more than six badges is a paragraph in a strip.
pub const MAX_CITATIONS: usize = 4;
pub const MAX_RECEIPTS: usize = 6;
pub const MAX_OFFERS: usize = 2;
pub const MAX_BADGE_BYTES: usize = 64;
/// How many job rows a surface is asked to draw at once.
///
/// A row outlives its job and is cleared by hand, so this is a ceiling on a
/// list that grows rather than on what is running. The extension drops the
/// oldest finished rows to stay inside it and never a live one.
pub const MAX_JOB_ROWS: usize = 8;
pub const MAX_JOB_SUMMARY_BYTES: usize = 512;
/// How many briefing generations a surface can be asked to draw.
pub const MAX_BRIEFING_ROWS: usize = 128;
pub const MAX_BRIEFING_SUMMARY_BYTES: usize = 256;

pub fn surface_socket_path() -> Result<PathBuf, ControlPathError> {
    socket_path(SURFACE_FILE_NAME)
}

pub fn agent_socket_path() -> Result<PathBuf, ControlPathError> {
    socket_path(AGENT_FILE_NAME)
}

pub fn control_socket_path() -> Result<PathBuf, ControlPathError> {
    socket_path(CONTROL_FILE_NAME)
}

pub fn content_socket_path() -> Result<PathBuf, ControlPathError> {
    socket_path(CONTENT_FILE_NAME)
}

fn socket_path(name: &str) -> Result<PathBuf, ControlPathError> {
    in_runtime_dir(chosen_runtime_dir(), env::var_os("XDG_RUNTIME_DIR"), name)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentDescriptor {
    pub id: String,
    pub name: String,
    pub media_type: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceRegistration {
    pub id: String,
    pub name: String,
    pub widgets: Vec<WidgetDefinition>,
}

/// What one measured fact says, in the only four words a badge has.
///
/// `Unknown` is not a no. A fetch that failed leaves `pushed` unmeasured, and
/// drawing that as "not pushed" would invent the one fact the receipt was
/// careful not to claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReceiptState {
    /// The fact is true.
    Measured,
    /// The fact was measured and is false.
    Refuted,
    /// The worker said it and no fact backs it.
    Claimed,
    /// It could not be measured, and the receipt says why.
    Unknown,
}

/// One measured fact about one job, as a badge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub label: String,
    pub value: String,
    pub state: ReceiptState,
}

/// One thing the agent offers to do next about one job.
///
/// The words are the agent's and the prompt behind them never crosses: a
/// surface sends the identifier back and the agent knows what it stored
/// against it. `taken` is what stops a reconnection from handing back a live
/// button for work already done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub taken: bool,
}

/// Every badge one message carries about one job.
///
/// Grouped by job and labelled with it, so a message that reports two jobs
/// binds each rank of badges to its own without the prose being parsed or
/// broken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Citation {
    pub job_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<Receipt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub offers: Vec<Offer>,
}

/// What a delegated job is doing, for the list at the foot of the chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobRowState {
    Working,
    Blocked,
    Done,
    Failed,
}

impl JobRowState {
    /// Whether the job has stopped. A stopped row is filed rather than ended.
    pub fn terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed)
    }
}

/// One delegated job, as a row.
///
/// The row outlives the job: finishing does not remove it, so an overnight run
/// is still there in the morning and filing it is the acknowledgement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobRow {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub state: JobRowState,
    /// Unix seconds the job started, for the age the row shows.
    pub since: u64,
    pub summary: String,
}

/// What a surface asks of one job row.
///
/// A live job can only be stopped and a finished one can only be filed, so the
/// two never apply to the same row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobAction {
    Cancel,
    Archive,
}

/// The collection half of one briefing generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BriefingCollectionState {
    Collecting,
    Collected,
    Failed,
}

/// The independent terminal-delivery half of one briefing generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BriefingDeliveryState {
    Pending,
    InProgress,
    Delivered,
}

/// Quiet lifecycle state for one generation-fenced scheduled briefing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefingRow {
    /// Opaque generation identity. Date and profile are labels, not identity.
    pub id: String,
    pub date: String,
    pub profile: String,
    pub collection: BriefingCollectionState,
    pub delivery: BriefingDeliveryState,
    /// Unix seconds when collection began.
    pub since: u64,
    pub completed: u32,
    pub total: u32,
    pub failed: u32,
    pub summary: String,
}

impl BriefingRow {
    /// Collection or terminal delivery still has work to do.
    pub fn active(&self) -> bool {
        self.collection == BriefingCollectionState::Collecting
            || self.delivery != BriefingDeliveryState::Delivered
    }

    /// A terminal collection used at least one source failure.
    ///
    /// This is the complete definition of partial. It uses only the measured
    /// collection state and failed-source count, never prose or a summary.
    pub fn partial(&self) -> bool {
        self.collection == BriefingCollectionState::Collected && self.failed > 0
    }

    /// A delivered result that stays visible until presentation dismissal.
    pub fn requires_attention(&self) -> bool {
        self.delivery == BriefingDeliveryState::Delivered
            && (self.collection == BriefingCollectionState::Failed || self.partial())
    }

    /// Whether a surface may dismiss this generation from presentation.
    pub fn dismissible(&self) -> bool {
        self.collection != BriefingCollectionState::Collecting
            && self.delivery == BriefingDeliveryState::Delivered
    }
}

/// The terminal model turn attached to a briefing update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefingWake {
    /// Stable across retries and service restarts.
    pub event_id: String,
    pub custom_type: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationMessage {
    pub role: ConversationRole,
    pub surface: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widgets: Option<Vec<WidgetCall>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentDescriptor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub receipts: Vec<Citation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScufrisState {
    Failed,
    Blocked,
    Working,
    Starting,
    Idle,
}

impl ScufrisState {
    pub fn name(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Working => "working",
            Self::Starting => "starting",
            Self::Idle => "idle",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceRequest {
    pub v: u32,
    #[serde(flatten)]
    pub body: SurfaceRequestBody,
}

impl SurfaceRequest {
    pub fn new(body: SurfaceRequestBody) -> Self {
        Self {
            v: SERVICE_VERSION,
            body,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum SurfaceRequestBody {
    #[serde(rename = "surface.hello")]
    Hello { surface: SurfaceRegistration },
    #[serde(rename = "surface.message")]
    Message {
        id: String,
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<String>,
    },
    #[serde(rename = "surface.abort")]
    Abort { id: String },
    /// Stop or file one job row.
    ///
    /// The surface names the row and the verb, and nothing else: what stopping
    /// costs and what filing means both belong to the agent that owns the job.
    #[serde(rename = "job.command")]
    JobCommand { id: String, action: JobAction },
    /// Dismiss one terminal, delivered briefing from surface presentation.
    ///
    /// The opaque generation ID is the whole request. The service keeps the
    /// run and audit row and changes only its durable presentation state.
    #[serde(rename = "briefing.dismiss")]
    BriefingDismiss { id: String },
    /// Take one offer the agent made.
    ///
    /// The identifier is the whole request. The prompt behind an offer was
    /// written by the agent and stayed there, so a surface cannot put words
    /// into the conversation by pressing a button.
    #[serde(rename = "offer.take")]
    OfferTake { id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceResponse {
    pub v: u32,
    #[serde(flatten)]
    pub body: SurfaceResponseBody,
}

impl SurfaceResponse {
    pub fn new(body: SurfaceResponseBody) -> Self {
        Self {
            v: SERVICE_VERSION,
            body,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum SurfaceResponseBody {
    #[serde(rename = "surface.message")]
    Message {
        role: ConversationRole,
        surface: String,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        widgets: Option<Vec<WidgetCall>>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<AttachmentDescriptor>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        receipts: Vec<Citation>,
    },
    #[serde(rename = "surface.message_ack")]
    MessageAck { id: String },
    #[serde(rename = "surface.aborted")]
    Aborted { id: String },
    #[serde(rename = "surface.state")]
    State { state: ScufrisState, detail: String },
    /// Every job row at once, replacing whatever the surface holds.
    ///
    /// Sent whenever the list changes and replayed on connect, because a row
    /// outlives its job: a surface that joined this morning has to be told
    /// about the night's finished work as well as what is running.
    #[serde(rename = "surface.jobs")]
    Jobs { jobs: Vec<JobRow> },
    /// Every durable briefing generation, replacing the surface's list.
    #[serde(rename = "surface.briefings")]
    Briefings { briefings: Vec<BriefingRow> },
    /// One offer has been taken, and is not on offer again.
    ///
    /// Broadcast rather than sent to the surface that pressed it: the badge is
    /// in a message every screen is showing.
    #[serde(rename = "surface.offer_taken")]
    OfferTaken { id: String },
    #[serde(rename = "surface.ready")]
    Ready { surface: String },
    #[serde(rename = "surface.rejected")]
    Rejected {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        operation: String,
        code: String,
        detail: String,
    },
}

impl From<ConversationMessage> for SurfaceResponseBody {
    fn from(message: ConversationMessage) -> Self {
        Self::Message {
            role: message.role,
            surface: message.surface,
            text: message.text,
            details: message.details,
            widgets: message.widgets,
            attachments: message.attachments,
            receipts: message.receipts,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRequest {
    pub v: u32,
    #[serde(flatten)]
    pub body: AgentRequestBody,
}

impl AgentRequest {
    pub fn new(body: AgentRequestBody) -> Self {
        Self {
            v: SERVICE_VERSION,
            body,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum AgentRequestBody {
    #[serde(rename = "agent.hello")]
    Hello,
    #[serde(rename = "agent.response")]
    Response {
        text: String,
        /// Correlates only a service-owned proactive terminal turn.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proactive_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        widgets: Option<Vec<WidgetCall>>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        receipts: Vec<Citation>,
    },
    /// Every delegated job the agent owns, whole.
    ///
    /// This replaced one aggregate word and one detail string. The word was
    /// all a surface got about any number of jobs, so a blocked job and three
    /// blocked jobs read the same and neither said which. The tray still wants
    /// one word, and the service folds these rows down to it: one source, two
    /// readings.
    #[serde(rename = "agent.jobs")]
    Jobs { jobs: Vec<JobRow> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentResponse {
    pub v: u32,
    #[serde(flatten)]
    pub body: AgentResponseBody,
}

impl AgentResponse {
    pub fn new(body: AgentResponseBody) -> Self {
        Self {
            v: SERVICE_VERSION,
            body,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum AgentResponseBody {
    #[serde(rename = "agent.ready")]
    Ready,
    #[serde(rename = "agent.message")]
    Message {
        id: String,
        text: String,
        widgets: Vec<WidgetDefinition>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<AttachmentDescriptor>,
    },
    /// A proactive message from outside the agent process.
    ///
    /// This is not a user turn. The service neither records it in the
    /// canonical conversation nor echoes it to a surface, and it leaves the
    /// response association alone. The extension delivers it as the follow-up
    /// wake a briefing already uses, under the caller's own custom type.
    #[serde(rename = "agent.wake")]
    Wake {
        /// Present only for a durable service-owned proactive item.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proactive_id: Option<String>,
        custom_type: String,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
    },
    #[serde(rename = "agent.abort")]
    Abort { id: String },
    /// A surface asked something of one job row.
    #[serde(rename = "agent.job_command")]
    JobCommand { id: String, action: JobAction },
    /// A surface took one offer. The agent holds the words it stands for.
    #[serde(rename = "agent.offer_take")]
    OfferTake { id: String },
    #[serde(rename = "agent.rejected")]
    Rejected { code: String, detail: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlRequest {
    pub v: u32,
    #[serde(flatten)]
    pub body: ControlRequestBody,
}
impl ControlRequest {
    pub fn new(body: ControlRequestBody) -> Self {
        Self {
            v: SERVICE_VERSION,
            body,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum ControlRequestBody {
    #[serde(rename = "control.hello")]
    Hello,
    #[serde(rename = "control.state")]
    State { id: String },
    /// Deliver one proactive message to the foreground conversation.
    ///
    /// The control socket carries this and the surface socket does not: a wake
    /// is not a second way to drive the conversation, and the remote surface
    /// gateway speaks only the surface channel.
    #[serde(rename = "control.wake")]
    Wake {
        id: String,
        custom_type: String,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
    },
    /// Upsert one generation-fenced briefing lifecycle event. A terminal wake
    /// is queued durably before this request is acknowledged.
    #[serde(rename = "control.briefing")]
    Briefing {
        id: String,
        briefing: BriefingRow,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wake: Option<BriefingWake>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlResponse {
    pub v: u32,
    #[serde(flatten)]
    pub body: ControlResponseBody,
}
impl ControlResponse {
    pub fn new(body: ControlResponseBody) -> Self {
        Self {
            v: SERVICE_VERSION,
            body,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum ControlResponseBody {
    #[serde(rename = "control.ready")]
    Ready,
    #[serde(rename = "control.state")]
    State {
        id: String,
        state: ScufrisState,
        detail: String,
    },
    #[serde(rename = "control.wake_ack")]
    WakeAck { id: String },
    #[serde(rename = "control.briefing_ack")]
    BriefingAck { id: String },
    #[serde(rename = "control.rejected")]
    Rejected {
        id: String,
        code: String,
        detail: String,
    },
}

pub fn read_surface_request(reader: &mut impl BufRead) -> Result<SurfaceRequest, MessageError> {
    read_exact(reader, validate_surface_request)
}
pub fn read_agent_request(reader: &mut impl BufRead) -> Result<AgentRequest, MessageError> {
    read_exact(reader, validate_agent_request)
}
pub fn read_control_request(reader: &mut impl BufRead) -> Result<ControlRequest, MessageError> {
    read_exact(reader, validate_control_request)
}
pub fn read_surface_response(reader: &mut impl BufRead) -> Result<SurfaceResponse, MessageError> {
    read_exact(reader, validate_surface_response)
}
pub fn read_agent_response(reader: &mut impl BufRead) -> Result<AgentResponse, MessageError> {
    read_exact(reader, validate_agent_response)
}
pub fn read_control_response(reader: &mut impl BufRead) -> Result<ControlResponse, MessageError> {
    read_exact(reader, validate_control_response)
}

fn read_exact<T: for<'de> Deserialize<'de>>(
    reader: &mut impl BufRead,
    validate: fn(&T) -> Result<(), MessageError>,
) -> Result<T, MessageError> {
    let line = read_line(reader)?;
    let value: Value = serde_json::from_slice(&line)?;
    let version = value.get("v").and_then(Value::as_u64).unwrap_or(0) as u32;
    if version != SERVICE_VERSION {
        return Err(MessageError::UnsupportedVersion(version));
    }
    let message: T = serde_json::from_value(value)?;
    validate(&message)?;
    Ok(message)
}

fn bytes(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |v| v.len())
}
fn text(
    value: &str,
    max: usize,
    field: &'static str,
    allow_empty: bool,
) -> Result<(), MessageError> {
    if (!allow_empty && value.trim().is_empty())
        || value.len() > max
        || value.contains(['\0', '\r'])
    {
        return Err(MessageError::InvalidSubmission(field));
    }
    Ok(())
}
fn id(value: &str, field: &'static str) -> Result<(), MessageError> {
    if is_identifier(value) {
        Ok(())
    } else {
        Err(MessageError::InvalidSubmission(field))
    }
}
fn widgets(definitions: &[WidgetDefinition]) -> Result<(), MessageError> {
    if definitions.len() > MAX_WIDGETS {
        return Err(MessageError::InvalidSubmission("widgets"));
    }
    for widget in definitions {
        id(&widget.name, "widget name")?;
        text(
            &widget.description,
            MAX_WIDGET_DESCRIPTION_BYTES,
            "widget description",
            true,
        )?;
        if !widget.input_schema.is_object() || bytes(&widget.input_schema) > MAX_WIDGET_SCHEMA_BYTES
        {
            return Err(MessageError::InvalidSubmission("widget schema"));
        }
    }
    Ok(())
}
pub fn validate_attachment_descriptor(value: &AttachmentDescriptor) -> Result<(), MessageError> {
    id(&value.id, "attachment id")?;
    text(
        &value.name,
        MAX_ATTACHMENT_NAME_BYTES,
        "attachment name",
        false,
    )?;
    let valid_media_type = value.media_type.len() <= MAX_MEDIA_TYPE_BYTES
        && value
            .media_type
            .split_once('/')
            .is_some_and(|(major, minor)| {
                !major.is_empty()
                    && !minor.is_empty()
                    && !minor.contains('/')
                    && major.bytes().chain(minor.bytes()).all(|byte| {
                        byte.is_ascii_alphanumeric()
                            || matches!(
                                byte,
                                b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-'
                            )
                    })
            });
    if value.name.contains(['/', '\\'])
        || value.name.chars().any(char::is_control)
        || !valid_media_type
        || value.size == 0
        || value.size > MAX_ATTACHMENT_BYTES
    {
        return Err(MessageError::InvalidSubmission("attachment"));
    }
    Ok(())
}
fn attachment_descriptors(value: &[AttachmentDescriptor]) -> Result<(), MessageError> {
    if value.len() > MAX_ATTACHMENTS {
        return Err(MessageError::InvalidSubmission("attachments"));
    }
    for (index, descriptor) in value.iter().enumerate() {
        validate_attachment_descriptor(descriptor)?;
        if value[..index]
            .iter()
            .any(|previous| previous.id == descriptor.id)
        {
            return Err(MessageError::InvalidSubmission("attachments"));
        }
    }
    Ok(())
}
fn attachment_ids(value: &[String]) -> Result<(), MessageError> {
    if value.len() > MAX_ATTACHMENTS {
        return Err(MessageError::InvalidSubmission("attachments"));
    }
    for (index, attachment_id) in value.iter().enumerate() {
        id(attachment_id, "attachment id")?;
        if value[..index].contains(attachment_id) {
            return Err(MessageError::InvalidSubmission("attachments"));
        }
    }
    Ok(())
}
fn calls(value: &Option<Vec<WidgetCall>>) -> Result<(), MessageError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.len() > MAX_WIDGETS {
        return Err(MessageError::InvalidSubmission("widget calls"));
    }
    for call in value {
        id(&call.id, "widget call id")?;
        id(&call.name, "widget call name")?;
        if bytes(&call.arguments) > MAX_WIDGET_ARGUMENTS_BYTES {
            return Err(MessageError::InvalidSubmission("widget arguments"));
        }
    }
    Ok(())
}
/// Validates one proactive wake independently of its channel.
///
/// The control request and the agent response carry the same three fields, so
/// what the control socket accepts is exactly what the agent is handed.
fn wake(custom_type: &str, body: &str, details: &Option<Value>) -> Result<(), MessageError> {
    id(custom_type, "wake custom type")?;
    text(body, MAX_TEXT_BYTES, "wake text", false)?;
    if let Some(details) = details
        && (!details.is_object() || bytes(details) > MAX_DETAILS_BYTES)
    {
        return Err(MessageError::InvalidSubmission("wake details"));
    }
    Ok(())
}
/// Validates the badges one message carries, grouped by job.
///
/// Every bound here is on presentation: a strip is read at a glance and a
/// message that cites five jobs is not one. Nothing about the receipt itself
/// is checked, because none of it was written by a model.
fn citations(value: &[Citation]) -> Result<(), MessageError> {
    if value.len() > MAX_CITATIONS {
        return Err(MessageError::InvalidSubmission("receipts"));
    }
    for (index, citation) in value.iter().enumerate() {
        id(&citation.job_id, "citation job id")?;
        // One group per job. Two groups naming the same job would draw two
        // strips under one message with the same label on both, and there
        // would be no reading that says which badge belongs where.
        if value[..index]
            .iter()
            .any(|previous| previous.job_id == citation.job_id)
        {
            return Err(MessageError::InvalidSubmission("receipts"));
        }
        if citation.badges.len() > MAX_RECEIPTS || citation.offers.len() > MAX_OFFERS {
            return Err(MessageError::InvalidSubmission("receipts"));
        }
        for badge in &citation.badges {
            text(&badge.label, MAX_BADGE_BYTES, "receipt label", false)?;
            text(&badge.value, MAX_BADGE_BYTES, "receipt value", false)?;
        }
        for offer in &citation.offers {
            id(&offer.id, "offer id")?;
            text(&offer.label, MAX_BADGE_BYTES, "offer label", false)?;
        }
    }
    // An offer identifier is what a surface sends back, so it names one offer
    // in the whole message and not one inside its own group.
    let mut offers = Vec::new();
    for offer in value.iter().flat_map(|citation| &citation.offers) {
        if offers.contains(&&offer.id) {
            return Err(MessageError::InvalidSubmission("offers"));
        }
        offers.push(&offer.id);
    }
    Ok(())
}
fn job_rows(value: &[JobRow]) -> Result<(), MessageError> {
    if value.len() > MAX_JOB_ROWS {
        return Err(MessageError::InvalidSubmission("jobs"));
    }
    for (index, row) in value.iter().enumerate() {
        id(&row.id, "job id")?;
        if value[..index].iter().any(|previous| previous.id == row.id) {
            return Err(MessageError::InvalidSubmission("jobs"));
        }
        // A project identifier is a relative path, so it carries slashes an
        // identifier never may. It is drawn, not resolved, so bounded text
        // with no control characters is the whole requirement.
        if let Some(project) = &row.project {
            text(project, MAX_IDENTIFIER_LENGTH, "job project", false)?;
        }
        // A row whose job has not said anything yet is a row with no summary,
        // and that is worth drawing: it says the job started.
        text(&row.summary, MAX_JOB_SUMMARY_BYTES, "job summary", true)?;
    }
    Ok(())
}

pub fn validate_briefing_row(row: &BriefingRow) -> Result<(), MessageError> {
    id(&row.id, "briefing id")?;
    text(&row.date, 10, "briefing date", false)?;
    if row.date.len() != 10
        || !row.date.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 4 | 7) {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
    {
        return Err(MessageError::InvalidSubmission("briefing date"));
    }
    id(&row.profile, "briefing profile")?;
    if row.completed > row.total || row.failed > row.completed {
        return Err(MessageError::InvalidSubmission("briefing counts"));
    }
    text(
        &row.summary,
        MAX_BRIEFING_SUMMARY_BYTES,
        "briefing summary",
        true,
    )
}

fn briefing_rows(value: &[BriefingRow]) -> Result<(), MessageError> {
    if value.len() > MAX_BRIEFING_ROWS {
        return Err(MessageError::InvalidSubmission("briefings"));
    }
    for (index, row) in value.iter().enumerate() {
        validate_briefing_row(row)?;
        if value[..index].iter().any(|previous| previous.id == row.id) {
            return Err(MessageError::InvalidSubmission("briefings"));
        }
    }
    Ok(())
}

pub fn validate_briefing_wake(value: &BriefingWake) -> Result<(), MessageError> {
    id(&value.event_id, "briefing event id")?;
    wake(&value.custom_type, &value.text, &value.details)
}
fn validate_registration(surface: &SurfaceRegistration) -> Result<(), MessageError> {
    id(&surface.id, "surface id")?;
    if surface.id == UNPROMPTED_SURFACE {
        return Err(MessageError::InvalidSubmission("surface id"));
    }
    text(&surface.name, MAX_SURFACE_NAME_BYTES, "surface name", false)?;
    widgets(&surface.widgets)
}
/// Validates one canonical conversation message independently of its channel.
///
/// Durable replay uses the same bounds as a live surface response, so a state
/// file cannot restore a message no surface would accept.
pub fn validate_conversation_message(message: &ConversationMessage) -> Result<(), MessageError> {
    id(&message.surface, "surface id")?;
    text(&message.text, MAX_TEXT_BYTES, "message text", false)?;
    if let Some(details) = &message.details {
        text(details, MAX_DETAILS_BYTES, "message details", false)?;
    }
    calls(&message.widgets)?;
    attachment_descriptors(&message.attachments)?;
    citations(&message.receipts)
}
fn validate_surface_request(message: &SurfaceRequest) -> Result<(), MessageError> {
    match &message.body {
        SurfaceRequestBody::Hello { surface } => validate_registration(surface),
        SurfaceRequestBody::Message {
            id: one,
            text: body,
            attachments,
        } => {
            id(one, "message id")?;
            text(body, MAX_TEXT_BYTES, "message text", false)?;
            attachment_ids(attachments)
        }
        SurfaceRequestBody::Abort { id: one } => id(one, "abort id"),
        SurfaceRequestBody::JobCommand { id: one, .. } => id(one, "job id"),
        SurfaceRequestBody::BriefingDismiss { id: one } => id(one, "briefing id"),
        SurfaceRequestBody::OfferTake { id: one } => id(one, "offer id"),
    }
}
fn validate_surface_response(message: &SurfaceResponse) -> Result<(), MessageError> {
    match &message.body {
        SurfaceResponseBody::Message {
            role,
            surface,
            text,
            details,
            widgets,
            attachments,
            receipts,
        } => validate_conversation_message(&ConversationMessage {
            role: *role,
            surface: surface.clone(),
            text: text.clone(),
            details: details.clone(),
            widgets: widgets.clone(),
            attachments: attachments.clone(),
            receipts: receipts.clone(),
        }),
        SurfaceResponseBody::MessageAck { id: one } | SurfaceResponseBody::Aborted { id: one } => {
            id(one, "response id")
        }
        SurfaceResponseBody::State { detail, .. } => {
            text(detail, MAX_DETAIL_BYTES, "state detail", true)
        }
        SurfaceResponseBody::Jobs { jobs } => job_rows(jobs),
        SurfaceResponseBody::Briefings { briefings } => briefing_rows(briefings),
        SurfaceResponseBody::OfferTaken { id: one } => id(one, "offer id"),
        SurfaceResponseBody::Ready { surface } => id(surface, "surface id"),
        SurfaceResponseBody::Rejected {
            id: one,
            operation,
            code,
            detail,
        } => {
            if let Some(one) = one {
                id(one, "rejection id")?;
            }
            id(operation, "operation")?;
            id(code, "rejection code")?;
            text(detail, MAX_DETAIL_BYTES, "rejection detail", true)
        }
    }
}
fn validate_agent_request(message: &AgentRequest) -> Result<(), MessageError> {
    match &message.body {
        AgentRequestBody::Hello => Ok(()),
        AgentRequestBody::Response {
            text: body,
            proactive_id,
            details,
            widgets,
            attachments,
            receipts,
        } => {
            if let Some(one) = proactive_id {
                id(one, "proactive id")?;
            }
            text(body, MAX_TEXT_BYTES, "response text", false)?;
            if let Some(details) = details {
                text(details, MAX_DETAILS_BYTES, "response details", false)?;
            }
            calls(widgets)?;
            attachment_ids(attachments)?;
            citations(receipts)
        }
        AgentRequestBody::Jobs { jobs } => job_rows(jobs),
    }
}
fn validate_agent_response(message: &AgentResponse) -> Result<(), MessageError> {
    match &message.body {
        AgentResponseBody::Ready => Ok(()),
        AgentResponseBody::Message {
            id: one,
            text: body,
            widgets: definitions,
            attachments,
        } => {
            id(one, "message id")?;
            text(body, MAX_TEXT_BYTES, "message text", false)?;
            widgets(definitions)?;
            attachment_descriptors(attachments)
        }
        AgentResponseBody::Wake {
            proactive_id,
            custom_type,
            text: body,
            details,
        } => {
            if let Some(one) = proactive_id {
                id(one, "proactive id")?;
            }
            wake(custom_type, body, details)
        }
        AgentResponseBody::Abort { id: one } => id(one, "abort id"),
        AgentResponseBody::JobCommand { id: one, .. } => id(one, "job id"),
        AgentResponseBody::OfferTake { id: one } => id(one, "offer id"),
        AgentResponseBody::Rejected { code, detail } => {
            id(code, "rejection code")?;
            text(detail, MAX_DETAIL_BYTES, "rejection detail", true)
        }
    }
}
fn validate_control_request(message: &ControlRequest) -> Result<(), MessageError> {
    match &message.body {
        ControlRequestBody::Hello => Ok(()),
        ControlRequestBody::State { id: one } => id(one, "state id"),
        ControlRequestBody::Wake {
            id: one,
            custom_type,
            text: body,
            details,
        } => {
            id(one, "wake id")?;
            wake(custom_type, body, details)
        }
        ControlRequestBody::Briefing {
            id: one,
            briefing,
            wake: terminal,
        } => {
            id(one, "briefing request id")?;
            validate_briefing_row(briefing)?;
            if let Some(terminal) = terminal {
                validate_briefing_wake(terminal)?;
            }
            Ok(())
        }
    }
}
fn validate_control_response(message: &ControlResponse) -> Result<(), MessageError> {
    match &message.body {
        ControlResponseBody::Ready => Ok(()),
        ControlResponseBody::State {
            id: one, detail, ..
        } => {
            id(one, "state id")?;
            text(detail, MAX_DETAIL_BYTES, "state detail", true)
        }
        ControlResponseBody::WakeAck { id: one } => id(one, "wake id"),
        ControlResponseBody::BriefingAck { id: one } => id(one, "briefing request id"),
        ControlResponseBody::Rejected {
            id: one,
            code,
            detail,
        } => {
            id(one, "rejection id")?;
            id(code, "rejection code")?;
            text(detail, MAX_DETAIL_BYTES, "rejection detail", true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn channels_and_directions_are_distinct() {
        let line = b"{\"v\":9,\"type\":\"agent.hello\"}\n";
        assert!(read_agent_request(&mut Cursor::new(line)).is_ok());
        assert!(matches!(
            read_surface_request(&mut Cursor::new(line)),
            Err(MessageError::InvalidJson(_))
        ));
        let outbound = b"{\"v\":9,\"type\":\"surface.ready\",\"surface\":\"desk\"}\n";
        assert!(read_surface_response(&mut Cursor::new(outbound)).is_ok());
        assert!(read_surface_request(&mut Cursor::new(outbound)).is_err());
    }

    #[test]
    fn no_surface_may_register_the_name_an_unprompted_answer_carries() {
        // An unprompted answer is displayed against a name no surface holds,
        // which is what keeps it unspoken. A surface allowed to take that name
        // would speak every briefing the owner never asked for.
        let hello = |id: &str| {
            format!(
                "{{\"v\":9,\"type\":\"surface.hello\",\"surface\":{{\"id\":\"{id}\",\"name\":\"Desk\",\"widgets\":[]}}}}\n"
            )
        };
        assert!(read_surface_request(&mut Cursor::new(hello("desk"))).is_ok());
        assert!(matches!(
            read_surface_request(&mut Cursor::new(hello(UNPROMPTED_SURFACE))),
            Err(MessageError::InvalidSubmission("surface id"))
        ));
    }

    #[test]
    fn every_wrong_version_is_identified_before_body_decode() {
        for version in [0, 5, 6, u32::MAX] {
            let line = format!("{{\"v\":{version},\"type\":\"anything\"}}\n");
            assert!(
                matches!(read_surface_request(&mut Cursor::new(line)), Err(MessageError::UnsupportedVersion(v)) if v == version)
            );
        }
    }

    #[test]
    fn protocol_paths_are_three_distinct_files() {
        assert_ne!(SURFACE_FILE_NAME, AGENT_FILE_NAME);
        assert_ne!(SURFACE_FILE_NAME, CONTROL_FILE_NAME);
        assert_ne!(AGENT_FILE_NAME, CONTROL_FILE_NAME);
        assert_ne!(CONTENT_FILE_NAME, SURFACE_FILE_NAME);
        assert_ne!(CONTENT_FILE_NAME, AGENT_FILE_NAME);
        assert_ne!(CONTENT_FILE_NAME, CONTROL_FILE_NAME);
    }

    #[test]
    fn attachment_references_and_descriptors_are_strict_and_bounded() {
        let descriptor = AttachmentDescriptor {
            id: "att_0123456789".into(),
            name: "diagram.png".into(),
            media_type: "image/png".into(),
            size: 184_223,
        };
        let response = AgentResponse::new(AgentResponseBody::Message {
            id: "message-1".into(),
            text: "See the diagram.".into(),
            widgets: vec![],
            attachments: vec![descriptor.clone()],
        });
        let mut encoded = Vec::new();
        crate::write_message(&mut encoded, &response).unwrap();
        assert_eq!(
            read_agent_response(&mut Cursor::new(encoded)).unwrap(),
            response
        );

        for invalid in [
            AttachmentDescriptor {
                name: "../secret".into(),
                ..descriptor.clone()
            },
            AttachmentDescriptor {
                media_type: "image png".into(),
                ..descriptor.clone()
            },
            AttachmentDescriptor {
                size: MAX_ATTACHMENT_BYTES + 1,
                ..descriptor.clone()
            },
        ] {
            let message = AgentResponse::new(AgentResponseBody::Message {
                id: "message-1".into(),
                text: "See it.".into(),
                widgets: vec![],
                attachments: vec![invalid],
            });
            let mut encoded = Vec::new();
            crate::write_message(&mut encoded, &message).unwrap();
            assert!(read_agent_response(&mut Cursor::new(encoded)).is_err());
        }

        let duplicate = SurfaceRequest::new(SurfaceRequestBody::Message {
            id: "message-1".into(),
            text: "See it.".into(),
            attachments: vec![descriptor.id.clone(), descriptor.id],
        });
        let mut encoded = Vec::new();
        crate::write_message(&mut encoded, &duplicate).unwrap();
        assert!(read_surface_request(&mut Cursor::new(encoded)).is_err());
    }

    #[test]
    fn canonical_details_are_optional_bounded_markdown_data() {
        let message = ConversationMessage {
            role: ConversationRole::Assistant,
            surface: "desk".into(),
            text: "Literal **plain** text.".into(),
            details: Some(
                "# Result\n\n- **Passed**\n- [Report](https://example.com/report)\n\n```rs\nlet answer = 42;\n```"
                    .into(),
            ),
            widgets: None,
            attachments: vec![],
            receipts: vec![],
        };
        assert!(validate_conversation_message(&message).is_ok());
        assert!(
            validate_conversation_message(&ConversationMessage {
                details: None,
                ..message.clone()
            })
            .is_ok()
        );
        for malformed in [
            String::new(),
            "  \n".into(),
            "bad\rline".into(),
            "bad\0line".into(),
            "x".repeat(MAX_DETAILS_BYTES + 1),
        ] {
            assert!(
                validate_conversation_message(&ConversationMessage {
                    details: Some(malformed),
                    ..message.clone()
                })
                .is_err()
            );
        }
    }

    #[test]
    fn a_wake_carries_the_same_bounds_on_control_and_agent_channels() {
        let control = |text: String, details: Option<Value>| {
            let mut bytes = Vec::new();
            crate::write_message(
                &mut bytes,
                &ControlRequest::new(ControlRequestBody::Wake {
                    id: "wake-1".into(),
                    custom_type: "scufris-briefing".into(),
                    text,
                    details,
                }),
            )
            .unwrap();
            read_control_request(&mut Cursor::new(bytes))
        };
        let agent = |text: String, details: Option<Value>| {
            let mut bytes = Vec::new();
            crate::write_message(
                &mut bytes,
                &AgentResponse::new(AgentResponseBody::Wake {
                    proactive_id: None,
                    custom_type: "scufris-briefing".into(),
                    text,
                    details,
                }),
            )
            .unwrap();
            read_agent_response(&mut Cursor::new(bytes))
        };
        let details = |bytes: usize| Some(serde_json::json!({"note": "x".repeat(bytes)}));

        assert!(control("Wake up.".into(), details(16)).is_ok());
        assert!(agent("Wake up.".into(), details(16)).is_ok());
        assert!(control("Wake up.".into(), None).is_ok());
        for (text, details) in [
            (String::new(), None),
            ("   ".into(), None),
            ("x".repeat(MAX_TEXT_BYTES + 1), None),
            ("Wake up.".into(), details(MAX_DETAILS_BYTES)),
            ("Wake up.".into(), Some(serde_json::json!("not an object"))),
        ] {
            assert!(control(text.clone(), details.clone()).is_err());
            assert!(agent(text, details).is_err());
        }
    }

    #[test]
    fn only_the_control_channel_carries_a_wake() {
        // A wake is not a second way to drive the conversation, so the channel
        // the remote gateway speaks cannot express one.
        let line = b"{\"v\":9,\"type\":\"control.wake\",\"id\":\"wake-1\",\"custom_type\":\"scufris-wake\",\"text\":\"Wake up.\"}\n";
        assert!(read_control_request(&mut Cursor::new(line)).is_ok());
        assert!(read_surface_request(&mut Cursor::new(line)).is_err());
        assert!(read_agent_request(&mut Cursor::new(line)).is_err());
    }

    #[test]
    fn bounded_atomic_response_round_trips() {
        let response = AgentRequest::new(AgentRequestBody::Response {
            proactive_id: None,
            text: "Done.".into(),
            details: Some("## Check\n\nPassed.".into()),
            widgets: Some(vec![WidgetCall {
                id: "call-1".into(),
                name: "summary".into(),
                arguments: serde_json::json!({"passed": 4}),
            }]),
            attachments: vec![],
            receipts: vec![],
        });
        let mut bytes = Vec::new();
        crate::write_message(&mut bytes, &response).unwrap();
        assert_eq!(
            read_agent_request(&mut Cursor::new(bytes)).unwrap(),
            response
        );
    }

    fn cited(job_id: &str) -> Citation {
        Citation {
            job_id: job_id.into(),
            badges: vec![Receipt {
                label: "landed".into(),
                value: "yes".into(),
                state: ReceiptState::Measured,
            }],
            offers: vec![Offer {
                id: format!("offer-{job_id}"),
                label: "push master".into(),
                taken: false,
            }],
        }
    }

    #[test]
    fn badges_are_grouped_by_job_and_one_job_gets_one_group() {
        // Position binds nothing here: the job identifier is the whole of the
        // binding, so two groups naming the same job would put two labelled
        // strips under one message with no reading that says which is which.
        let response = AgentRequest::new(AgentRequestBody::Response {
            proactive_id: None,
            text: "Both jobs finished.".into(),
            details: None,
            widgets: None,
            attachments: vec![],
            receipts: vec![cited("750a4de8a80d"), cited("01ccbac98b97")],
        });
        let mut bytes = Vec::new();
        crate::write_message(&mut bytes, &response).unwrap();
        assert_eq!(
            read_agent_request(&mut Cursor::new(bytes)).unwrap(),
            response
        );

        let doubled = AgentRequest::new(AgentRequestBody::Response {
            proactive_id: None,
            text: "One job, twice.".into(),
            details: None,
            widgets: None,
            attachments: vec![],
            receipts: vec![cited("750a4de8a80d"), cited("750a4de8a80d")],
        });
        let mut bytes = Vec::new();
        crate::write_message(&mut bytes, &doubled).unwrap();
        assert!(matches!(
            read_agent_request(&mut Cursor::new(bytes)),
            Err(MessageError::InvalidSubmission("receipts"))
        ));
    }

    #[test]
    fn an_offer_identifier_names_one_offer_in_the_whole_message() {
        // The identifier is all a surface sends back. Two offers sharing one
        // would make the press ambiguous, and the ambiguity would be resolved
        // by whichever the agent looked up first.
        let mut second = cited("01ccbac98b97");
        second.offers[0].id = cited("750a4de8a80d").offers[0].id.clone();
        let clashing = AgentRequest::new(AgentRequestBody::Response {
            proactive_id: None,
            text: "Two offers, one name.".into(),
            details: None,
            widgets: None,
            attachments: vec![],
            receipts: vec![cited("750a4de8a80d"), second],
        });
        let mut bytes = Vec::new();
        crate::write_message(&mut bytes, &clashing).unwrap();
        assert!(matches!(
            read_agent_request(&mut Cursor::new(bytes)),
            Err(MessageError::InvalidSubmission("offers"))
        ));
    }

    #[test]
    fn a_citation_is_bounded_the_way_a_strip_is_read() {
        let badge = Receipt {
            label: "landed".into(),
            value: "yes".into(),
            state: ReceiptState::Measured,
        };
        let group = |badges: usize, offers: usize| {
            let mut bytes = Vec::new();
            crate::write_message(
                &mut bytes,
                &AgentRequest::new(AgentRequestBody::Response {
                    proactive_id: None,
                    text: "Done.".into(),
                    details: None,
                    widgets: None,
                    attachments: vec![],
                    receipts: vec![Citation {
                        job_id: "750a4de8a80d".into(),
                        badges: vec![badge.clone(); badges],
                        offers: (0..offers)
                            .map(|index| Offer {
                                id: format!("offer-{index}"),
                                label: "push master".into(),
                                taken: false,
                            })
                            .collect(),
                    }],
                }),
            )
            .unwrap();
            read_agent_request(&mut Cursor::new(bytes))
        };
        assert!(group(MAX_RECEIPTS, MAX_OFFERS).is_ok());
        assert!(group(MAX_RECEIPTS + 1, 0).is_err());
        assert!(group(1, MAX_OFFERS + 1).is_err());
    }

    #[test]
    fn a_job_row_survives_the_job_and_keeps_its_own_name() {
        let row = |id: &str, state: JobRowState| JobRow {
            id: id.into(),
            project: Some("personal/scufris2".into()),
            state,
            since: 1_757_000_000,
            summary: "reviewed 9 commits".into(),
        };
        let listed = AgentRequest::new(AgentRequestBody::Jobs {
            jobs: vec![
                row("01ccbac98b97", JobRowState::Done),
                row("3f81c204b1e9", JobRowState::Working),
            ],
        });
        let mut bytes = Vec::new();
        crate::write_message(&mut bytes, &listed).unwrap();
        assert_eq!(read_agent_request(&mut Cursor::new(bytes)).unwrap(), listed);
        assert!(JobRowState::Done.terminal());
        assert!(JobRowState::Failed.terminal());
        assert!(!JobRowState::Working.terminal());
        assert!(!JobRowState::Blocked.terminal());

        // A project identifier is a relative path and keeps its slashes.
        for (invalid, field) in [
            (vec![row("01ccbac98b97", JobRowState::Done); 2], "jobs"),
            (vec![row("x", JobRowState::Done); MAX_JOB_ROWS + 1], "jobs"),
            (
                vec![JobRow {
                    project: Some("p".repeat(MAX_IDENTIFIER_LENGTH + 1)),
                    ..row("01ccbac98b97", JobRowState::Done)
                }],
                "job project",
            ),
        ] {
            let mut bytes = Vec::new();
            crate::write_message(
                &mut bytes,
                &AgentRequest::new(AgentRequestBody::Jobs { jobs: invalid }),
            )
            .unwrap();
            assert!(matches!(
                read_agent_request(&mut Cursor::new(bytes)),
                Err(MessageError::InvalidSubmission(named)) if named == field
            ));
        }
    }

    #[test]
    fn briefing_attention_uses_only_measured_terminal_fields() {
        let row = BriefingRow {
            id: "generation-a".into(),
            date: "2026-09-10".into(),
            profile: "morning".into(),
            collection: BriefingCollectionState::Collected,
            delivery: BriefingDeliveryState::Delivered,
            since: 1_757_000_000,
            completed: 3,
            total: 3,
            failed: 1,
            summary: "Summary prose does not classify this row.".into(),
        };
        assert!(row.partial());
        assert!(row.requires_attention());
        assert!(row.dismissible());
        assert!(!row.active());

        let successful = BriefingRow {
            failed: 0,
            ..row.clone()
        };
        assert!(!successful.partial());
        assert!(!successful.requires_attention());
        assert!(successful.dismissible());

        let collecting = BriefingRow {
            collection: BriefingCollectionState::Collecting,
            delivery: BriefingDeliveryState::Pending,
            ..row
        };
        assert!(collecting.active());
        assert!(!collecting.partial());
        assert!(!collecting.requires_attention());
        assert!(!collecting.dismissible());
    }

    #[test]
    fn only_a_surface_asks_for_a_job_to_stop_or_be_filed() {
        // The two verbs are the surface's, and the agent hears them relayed.
        // Neither is a way to drive the conversation, so the request channel
        // that carries them is the one a registered surface already speaks.
        let line =
            b"{\"v\":9,\"type\":\"job.command\",\"id\":\"3f81c204b1e9\",\"action\":\"cancel\"}\n";
        assert!(read_surface_request(&mut Cursor::new(line)).is_ok());
        assert!(read_agent_request(&mut Cursor::new(line)).is_err());
        assert!(read_control_request(&mut Cursor::new(line)).is_err());

        let take = b"{\"v\":9,\"type\":\"offer.take\",\"id\":\"offer-1\"}\n";
        assert!(read_surface_request(&mut Cursor::new(take)).is_ok());
        assert!(read_agent_request(&mut Cursor::new(take)).is_err());

        let dismiss = b"{\"v\":9,\"type\":\"briefing.dismiss\",\"id\":\"generation-a\"}\n";
        assert!(read_surface_request(&mut Cursor::new(dismiss)).is_ok());
        assert!(read_agent_request(&mut Cursor::new(dismiss)).is_err());
        assert!(read_control_request(&mut Cursor::new(dismiss)).is_err());

        let invalid = b"{\"v\":9,\"type\":\"briefing.dismiss\",\"id\":\"../generation\"}\n";
        assert!(matches!(
            read_surface_request(&mut Cursor::new(invalid)),
            Err(MessageError::InvalidSubmission("briefing id"))
        ));
    }
}
