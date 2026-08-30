//! Scufris protocol v5 typed channels.
//!
//! Surface, agent, and control traffic use separate Unix sockets and separate
//! enums. Each decoder accepts only its channel and direction.

use std::{env, io::BufRead, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ControlPathError, MessageError, chosen_runtime_dir, in_runtime_dir, is_identifier, read_line,
};

pub const SERVICE_VERSION: u32 = 5;
pub const SURFACE_FILE_NAME: &str = "surface.sock";
pub const AGENT_FILE_NAME: &str = "agent.sock";
pub const CONTROL_FILE_NAME: &str = "control.sock";
pub const CONTENT_FILE_NAME: &str = "content.sock";
pub const CONVERSATION_ENTRIES: usize = 200;
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
    },
    #[serde(rename = "surface.message_ack")]
    MessageAck { id: String },
    #[serde(rename = "surface.aborted")]
    Aborted { id: String },
    #[serde(rename = "surface.state")]
    State { state: ScufrisState, detail: String },
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        widgets: Option<Vec<WidgetCall>>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<String>,
    },
    #[serde(rename = "agent.state")]
    State { state: AgentState, detail: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Failed,
    Blocked,
    Clear,
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
    #[serde(rename = "agent.abort")]
    Abort { id: String },
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
fn validate_registration(surface: &SurfaceRegistration) -> Result<(), MessageError> {
    id(&surface.id, "surface id")?;
    text(&surface.name, MAX_SURFACE_NAME_BYTES, "surface name", false)?;
    widgets(&surface.widgets)
}
fn validate_conversation(message: &ConversationMessage) -> Result<(), MessageError> {
    id(&message.surface, "surface id")?;
    text(&message.text, MAX_TEXT_BYTES, "message text", false)?;
    if let Some(details) = &message.details {
        text(details, MAX_DETAILS_BYTES, "message details", false)?;
    }
    calls(&message.widgets)?;
    attachment_descriptors(&message.attachments)
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
        } => validate_conversation(&ConversationMessage {
            role: *role,
            surface: surface.clone(),
            text: text.clone(),
            details: details.clone(),
            widgets: widgets.clone(),
            attachments: attachments.clone(),
        }),
        SurfaceResponseBody::MessageAck { id: one } | SurfaceResponseBody::Aborted { id: one } => {
            id(one, "response id")
        }
        SurfaceResponseBody::State { detail, .. } => {
            text(detail, MAX_DETAIL_BYTES, "state detail", true)
        }
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
            details,
            widgets,
            attachments,
        } => {
            text(body, MAX_TEXT_BYTES, "response text", false)?;
            if let Some(details) = details {
                text(details, MAX_DETAILS_BYTES, "response details", false)?;
            }
            calls(widgets)?;
            attachment_ids(attachments)
        }
        AgentRequestBody::State { detail, .. } => {
            text(detail, MAX_DETAIL_BYTES, "state detail", true)
        }
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
        AgentResponseBody::Abort { id: one } => id(one, "abort id"),
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
        let line = b"{\"v\":5,\"type\":\"agent.hello\"}\n";
        assert!(read_agent_request(&mut Cursor::new(line)).is_ok());
        assert!(matches!(
            read_surface_request(&mut Cursor::new(line)),
            Err(MessageError::InvalidJson(_))
        ));
        let outbound = b"{\"v\":5,\"type\":\"surface.ready\",\"surface\":\"desk\"}\n";
        assert!(read_surface_response(&mut Cursor::new(outbound)).is_ok());
        assert!(read_surface_request(&mut Cursor::new(outbound)).is_err());
    }

    #[test]
    fn every_wrong_version_is_identified_before_body_decode() {
        for version in [0, 3, 4, u32::MAX] {
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
    fn bounded_atomic_response_round_trips() {
        let response = AgentRequest::new(AgentRequestBody::Response {
            text: "Done.".into(),
            details: Some("## Check\n\nPassed.".into()),
            widgets: Some(vec![WidgetCall {
                id: "call-1".into(),
                name: "summary".into(),
                arguments: serde_json::json!({"passed": 4}),
            }]),
            attachments: vec![],
        });
        let mut bytes = Vec::new();
        crate::write_message(&mut bytes, &response).unwrap();
        assert_eq!(
            read_agent_request(&mut Cursor::new(bytes)).unwrap(),
            response
        );
    }
}
