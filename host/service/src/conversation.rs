//! Durable canonical conversation replay.

use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use scufris_control::service::{
    CONVERSATION_ENTRIES, ConversationMessage, validate_conversation_message,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tracing::{info, warn};

const FORMAT_VERSION: u32 = 1;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredEntry {
    sequence: u64,
    message: ConversationMessage,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredConversation {
    version: u32,
    entries: Vec<StoredEntry>,
}

/// The bounded canonical replay and its private snapshot.
pub struct ConversationHistory {
    path: PathBuf,
    entries: VecDeque<StoredEntry>,
    next_sequence: u64,
}

impl ConversationHistory {
    /// Opens a replay snapshot. Invalid formats are isolated and become an
    /// empty replay rather than preventing the service from starting.
    pub fn open(path: PathBuf) -> Self {
        let mut history = Self {
            path,
            entries: VecDeque::new(),
            next_sequence: 1,
        };
        if let Err(error) = prepare_parent(&history.path) {
            warn!(%error, path = %history.path.display(), "the conversation directory could not be prepared");
            return history;
        }
        history.remove_temporary();

        match read_snapshot(&history.path) {
            Ok(None) => {}
            Ok(Some((entries, normalized))) => {
                history.entries = entries;
                history.set_next_sequence();
                if normalized && let Err(error) = history.persist() {
                    warn!(%error, path = %history.path.display(), "the normalized conversation could not be stored");
                }
            }
            Err(error) => history.reject(error),
        }
        info!(
            path = %history.path.display(),
            messages = history.entries.len(),
            "conversation history opened"
        );
        history
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn messages(&self) -> impl Iterator<Item = &ConversationMessage> {
        self.entries.iter().map(|entry| &entry.message)
    }

    /// Adds one message to the replay and atomically snapshots the complete
    /// new bound. The in-memory replay remains current if storage fails, so a
    /// later message retries the complete snapshot.
    pub fn record(&mut self, message: ConversationMessage) -> Result<(), PersistError> {
        if self.next_sequence == u64::MAX {
            self.resequence();
        }
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        if self.entries.len() == CONVERSATION_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(StoredEntry { sequence, message });
        self.persist()
    }

    fn set_next_sequence(&mut self) {
        let maximum = self
            .entries
            .iter()
            .map(|entry| entry.sequence)
            .max()
            .unwrap_or(0);
        if maximum == u64::MAX {
            self.resequence();
        } else {
            self.next_sequence = maximum + 1;
        }
    }

    fn resequence(&mut self) {
        for (index, entry) in self.entries.iter_mut().enumerate() {
            entry.sequence = index as u64 + 1;
        }
        self.next_sequence = self.entries.len() as u64 + 1;
    }

    fn persist(&self) -> Result<(), PersistError> {
        prepare_parent(&self.path)?;
        let state = StoredConversation {
            version: FORMAT_VERSION,
            entries: self.entries.iter().cloned().collect(),
        };
        let mut encoded = serde_json::to_vec(&state)?;
        encoded.push(b'\n');
        if encoded.len() as u64 > MAX_FILE_BYTES {
            return Err(PersistError::TooLarge);
        }

        let temporary = temporary_path(&self.path);
        remove_file_if_present(&temporary)?;
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(&encoded)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            sync_directory(parent(&self.path)?)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn remove_temporary(&self) {
        let temporary = temporary_path(&self.path);
        if let Err(error) = remove_file_if_present(&temporary) {
            warn!(%error, path = %temporary.display(), "an incomplete conversation snapshot could not be removed");
        }
    }

    fn reject(&self, error: LoadError) {
        let rejected = rejected_path(&self.path, error.is_incompatible());
        warn!(%error, path = %self.path.display(), rejected = %rejected.display(), "conversation history was rejected; starting with an empty replay");
        if let Err(rename_error) = (|| {
            remove_file_if_present(&rejected)?;
            fs::rename(&self.path, &rejected)?;
            fs::set_permissions(&rejected, fs::Permissions::from_mode(0o600))?;
            sync_directory(parent(&self.path)?)
        })() {
            warn!(%rename_error, path = %self.path.display(), "the rejected conversation history could not be isolated");
        }
    }
}

fn read_snapshot(path: &Path) -> Result<Option<(VecDeque<StoredEntry>, bool)>, LoadError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(LoadError::Malformed(
            "the snapshot is not a regular file".into(),
        ));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(LoadError::TooLarge);
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(LoadError::TooLarge);
    }

    let value: Value =
        serde_json::from_slice(&bytes).map_err(|error| LoadError::Malformed(error.to_string()))?;
    let version = value
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| LoadError::Malformed("the snapshot has no numeric version".into()))?;
    if version != u64::from(FORMAT_VERSION) {
        return Err(LoadError::Incompatible(version));
    }
    let state: StoredConversation =
        serde_json::from_value(value).map_err(|error| LoadError::Malformed(error.to_string()))?;

    let mut normalized = false;
    let mut positions = HashMap::new();
    let mut entries = VecDeque::with_capacity(state.entries.len().min(CONVERSATION_ENTRIES));
    for entry in state.entries {
        validate_conversation_message(&entry.message)
            .map_err(|error| LoadError::Malformed(error.to_string()))?;
        if let Some(index) = positions.get(&entry.sequence).copied() {
            if entries[index] != entry {
                return Err(LoadError::Malformed(format!(
                    "sequence {} names different messages",
                    entry.sequence
                )));
            }
            normalized = true;
            continue;
        }
        positions.insert(entry.sequence, entries.len());
        entries.push_back(entry);
    }
    while entries.len() > CONVERSATION_ENTRIES {
        entries.pop_front();
        normalized = true;
    }
    Ok(Some((entries, normalized)))
}

fn prepare_parent(path: &Path) -> io::Result<()> {
    let parent = parent(path)?;
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
}

fn parent(path: &Path) -> io::Result<&Path> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))
}

fn temporary_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        ".{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ))
}

fn rejected_path(path: &Path, incompatible: bool) -> PathBuf {
    let suffix = if incompatible {
        "incompatible"
    } else {
        "corrupt"
    };
    path.with_file_name(format!(
        "{}.{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        suffix
    ))
}

fn remove_file_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[derive(Debug, Error)]
pub enum PersistError {
    #[error("conversation snapshot I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("conversation snapshot encoding failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("conversation snapshot exceeds {MAX_FILE_BYTES} bytes")]
    TooLarge,
}

#[derive(Debug, Error)]
enum LoadError {
    #[error("conversation snapshot I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("conversation snapshot is malformed: {0}")]
    Malformed(String),
    #[error("conversation snapshot version {0} is not supported")]
    Incompatible(u64),
    #[error("conversation snapshot exceeds {MAX_FILE_BYTES} bytes")]
    TooLarge,
}

impl LoadError {
    fn is_incompatible(&self) -> bool {
        matches!(self, Self::Incompatible(_))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use scufris_control::service::{AttachmentDescriptor, ConversationRole, WidgetCall};
    use serde_json::json;

    use super::*;

    static NEXT_TEST: AtomicU64 = AtomicU64::new(1);

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "scufris-conversation-{}-{}-{name}",
                std::process::id(),
                NEXT_TEST.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn state(&self) -> PathBuf {
            self.0.join("scufris/conversation.json")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn message(index: usize) -> ConversationMessage {
        ConversationMessage {
            role: if index.is_multiple_of(2) {
                ConversationRole::User
            } else {
                ConversationRole::Assistant
            },
            surface: format!("surface-{}", index % 3),
            text: format!("message {index}"),
            details: (index == 1).then(|| "## Durable details".into()),
            widgets: (index == 1).then(|| {
                vec![WidgetCall {
                    id: "widget-1".into(),
                    name: "summary".into(),
                    arguments: json!({"count": 4}),
                }]
            }),
            attachments: if index == 1 {
                vec![AttachmentDescriptor {
                    id: "attachment-1".into(),
                    name: "diagram.png".into(),
                    media_type: "image/png".into(),
                    size: 42,
                }]
            } else {
                Vec::new()
            },
        }
    }

    fn messages(history: &ConversationHistory) -> Vec<ConversationMessage> {
        history.messages().cloned().collect()
    }

    #[test]
    fn a_restart_restores_the_complete_canonical_schema_in_order() {
        let scratch = Scratch::new("restore");
        let path = scratch.state();
        let expected = vec![message(0), message(1), message(2)];
        {
            let mut history = ConversationHistory::open(path.clone());
            for message in &expected {
                history.record(message.clone()).unwrap();
            }
        }

        let restored = ConversationHistory::open(path.clone());
        assert_eq!(messages(&restored), expected);
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!temporary_path(&scratch.state()).exists());
    }

    #[test]
    fn only_the_latest_two_hundred_messages_survive() {
        let scratch = Scratch::new("bound");
        let path = scratch.state();
        let mut history = ConversationHistory::open(path.clone());
        for index in 0..CONVERSATION_ENTRIES + 7 {
            history.record(message(index)).unwrap();
        }
        drop(history);

        let restored = ConversationHistory::open(path);
        assert_eq!(restored.len(), CONVERSATION_ENTRIES);
        assert_eq!(restored.messages().next().unwrap().text, "message 7");
        assert_eq!(restored.messages().last().unwrap().text, "message 206");
    }

    #[test]
    fn repeated_sequence_records_are_deduplicated_without_deduplicating_content() {
        let scratch = Scratch::new("deduplicate");
        let path = scratch.state();
        prepare_parent(&path).unwrap();
        let repeated = message(1);
        let state = StoredConversation {
            version: FORMAT_VERSION,
            entries: vec![
                StoredEntry {
                    sequence: 8,
                    message: repeated.clone(),
                },
                StoredEntry {
                    sequence: 8,
                    message: repeated.clone(),
                },
                StoredEntry {
                    sequence: 9,
                    message: repeated.clone(),
                },
            ],
        };
        fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();

        let restored = ConversationHistory::open(path.clone());
        assert_eq!(messages(&restored), vec![repeated.clone(), repeated]);
        let normalized: StoredConversation =
            serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(normalized.entries.len(), 2);
        assert_eq!(normalized.entries[0].sequence, 8);
        assert_eq!(normalized.entries[1].sequence, 9);
    }

    #[test]
    fn malformed_and_incompatible_snapshots_are_isolated_and_replaced_safely() {
        for (name, bytes, suffix) in [
            ("malformed", b"not json".as_slice(), "corrupt"),
            (
                "incompatible",
                br#"{"version":99,"entries":[]}"#.as_slice(),
                "incompatible",
            ),
        ] {
            let scratch = Scratch::new(name);
            let path = scratch.state();
            prepare_parent(&path).unwrap();
            fs::write(&path, bytes).unwrap();

            let mut restored = ConversationHistory::open(path.clone());
            assert_eq!(restored.len(), 0);
            assert!(!path.exists());
            let rejected = path.with_file_name(format!("conversation.json.{suffix}"));
            assert_eq!(fs::read(rejected).unwrap(), bytes);

            restored.record(message(0)).unwrap();
            drop(restored);
            assert_eq!(messages(&ConversationHistory::open(path)), vec![message(0)]);
        }
    }

    #[test]
    fn an_incomplete_temporary_snapshot_never_replaces_the_last_complete_one() {
        let scratch = Scratch::new("temporary");
        let path = scratch.state();
        let mut history = ConversationHistory::open(path.clone());
        history.record(message(0)).unwrap();
        drop(history);
        fs::write(temporary_path(&path), b"partial").unwrap();

        let restored = ConversationHistory::open(path.clone());
        assert_eq!(messages(&restored), vec![message(0)]);
        assert!(!temporary_path(&path).exists());
    }
}
