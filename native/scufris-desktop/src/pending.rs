//! Durable storage for an accepted transcript that has not been acknowledged.
//!
//! The companion and the backend must fail independently without losing text
//! the user already accepted. Everything else in the pill is recoverable by
//! speaking again; an accepted transcript is not, so it lives on disk from the
//! moment it exists until a discard or an acknowledgment retires it.
//!
//! Every operation reports failure. A store that silently swallowed a full disk
//! or a read-only directory would let the pill claim durability it does not
//! have, so the runtime submits nothing until a save is known to have landed.

use std::{
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Largest pending record the companion will read back.
pub const MAX_PENDING_BYTES: u64 = 64 * 1024;

/// Record format version. An unknown version is a corrupt record, not a guess.
const VERSION: u32 = 1;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// One accepted transcript waiting for its acknowledgment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    /// Identifier every retry of this transcript reuses.
    pub id: String,
    /// Accepted transcript text.
    pub text: String,
}

/// Failure to read or write the durable transcript.
#[derive(Debug, Error)]
pub enum PendingError {
    /// The file exists but does not hold a usable record.
    #[error("the saved transcript is unreadable")]
    Corrupt,
    /// The transcript is outside the bounds this store can read back.
    #[error("the transcript is too long to keep")]
    Unbounded,
    /// The filesystem refused the operation.
    #[error("the saved transcript could not be {operation}: {reason}")]
    Io {
        /// What was being attempted.
        operation: &'static str,
        /// The underlying failure.
        reason: String,
    },
}

impl PendingError {
    fn io(operation: &'static str, error: impl std::fmt::Display) -> Self {
        Self::Io {
            operation,
            reason: error.to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Record {
    version: u32,
    id: String,
    text: String,
    /// A transcript the user explicitly threw away. The marker exists so a
    /// removal that could not happen still prevents the text coming back.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    discarded: bool,
}

/// Durable home for the accepted transcript.
pub trait PendingStore: Send + Sync {
    /// Returns the stored transcript, `None` when there is none or when it was
    /// discarded, or the reason one exists but could not be read.
    fn load(&self) -> Result<Option<Pending>, PendingError>;
    /// Replaces the stored transcript atomically.
    fn save(&self, pending: &Pending) -> Result<(), PendingError>;
    /// Removes the stored transcript. Removing nothing is success.
    fn clear(&self) -> Result<(), PendingError>;
    /// Marks the stored transcript as discarded without removing the file.
    ///
    /// This is the fallback when a removal cannot happen: a tombstone still
    /// stops discarded words reappearing after a restart.
    fn tombstone(&self, id: &str) -> Result<(), PendingError>;
}

/// A [`PendingStore`] backed by one private file.
#[derive(Debug, Clone)]
pub struct FilePendingStore {
    path: PathBuf,
}

impl FilePendingStore {
    /// Creates a store over `path`. The file is created on first save.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the file this store owns.
    #[cfg(test)]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl PendingStore for FilePendingStore {
    fn load(&self) -> Result<Option<Pending>, PendingError> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(PendingError::io("read", error)),
        };
        let metadata = file
            .metadata()
            .map_err(|error| PendingError::io("read", error))?;
        if !metadata.is_file() {
            return Err(PendingError::Corrupt);
        }
        let mut encoded = Vec::new();
        Read::take(file, MAX_PENDING_BYTES + 1)
            .read_to_end(&mut encoded)
            .map_err(|error| PendingError::io("read", error))?;
        if encoded.len() as u64 > MAX_PENDING_BYTES {
            return Err(PendingError::Corrupt);
        }
        let record: Record = serde_json::from_slice(&encoded).map_err(|_| PendingError::Corrupt)?;
        if record.version != VERSION || !scufris_control::is_identifier(&record.id) {
            return Err(PendingError::Corrupt);
        }
        // A tombstone is a complete answer: there is nothing to restore.
        if record.discarded {
            return Ok(None);
        }
        if !scufris_control::is_submission_text(&record.text) {
            return Err(PendingError::Corrupt);
        }
        Ok(Some(Pending {
            id: record.id,
            text: record.text,
        }))
    }

    fn save(&self, pending: &Pending) -> Result<(), PendingError> {
        // Refuse what `load` would later call corrupt. Writing it would lose
        // the transcript at exactly the moment it is needed.
        if !scufris_control::is_identifier(&pending.id)
            || !scufris_control::is_submission_text(&pending.text)
        {
            return Err(PendingError::Unbounded);
        }
        self.write(&Record {
            version: VERSION,
            id: pending.id.clone(),
            text: pending.text.clone(),
            discarded: false,
        })
    }

    fn clear(&self) -> Result<(), PendingError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(PendingError::io("removed", error)),
        }
    }

    fn tombstone(&self, id: &str) -> Result<(), PendingError> {
        self.write(&Record {
            version: VERSION,
            id: id.to_string(),
            text: String::new(),
            discarded: true,
        })
    }
}

impl FilePendingStore {
    fn write(&self, record: &Record) -> Result<(), PendingError> {
        let encoded =
            serde_json::to_vec(record).map_err(|error| PendingError::io("encoded", error))?;
        let parent = self
            .path
            .parent()
            .ok_or_else(|| PendingError::io("written", "the file has no directory"))?;
        fs::create_dir_all(parent).map_err(|error| PendingError::io("written", error))?;
        // A partial file must never be readable as the pending transcript, so
        // the record is written beside its target and renamed into place.
        let temporary = parent.join(format!(
            ".pending-{}-{}-{}.tmp",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| -> std::io::Result<()> {
            let mut file = File::options()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(&encoded)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(PendingError::io("written", error));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "scufris-pending-{}-{}-{name}",
                process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn store(&self) -> FilePendingStore {
            FilePendingStore::new(self.0.join("state").join("pending.json"))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn pending() -> Pending {
        Pending {
            id: "pill-3".into(),
            text: "remember the milk".into(),
        }
    }

    #[test]
    fn an_accepted_transcript_survives_a_new_process() {
        let scratch = Scratch::new("survives");
        let store = scratch.store();
        assert_eq!(store.load().unwrap(), None);
        store.save(&pending()).unwrap();

        // A separate store over the same path stands in for the next process.
        let restarted = FilePendingStore::new(store.path());
        assert_eq!(restarted.load().unwrap(), Some(pending()));
    }

    #[test]
    fn saving_creates_the_directory_and_a_private_file() {
        let scratch = Scratch::new("private");
        let store = scratch.store();
        store.save(&pending()).unwrap();
        let mode = fs::metadata(store.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn saving_replaces_the_previous_transcript_and_leaves_no_temporary_files() {
        let scratch = Scratch::new("replace");
        let store = scratch.store();
        store.save(&pending()).unwrap();
        store
            .save(&Pending {
                id: "pill-4".into(),
                text: "and the bread".into(),
            })
            .unwrap();
        assert_eq!(store.load().unwrap().unwrap().text, "and the bread");
        let directory = store.path().parent().unwrap();
        let leftovers: Vec<_> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name != "pending.json")
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn clearing_removes_the_transcript_and_is_safe_to_repeat() {
        let scratch = Scratch::new("clear");
        let store = scratch.store();
        store.save(&pending()).unwrap();
        store.clear().unwrap();
        assert_eq!(store.load().unwrap(), None);
        store.clear().unwrap();
        assert!(!store.path().exists());
    }

    #[test]
    fn an_unreadable_record_is_reported_rather_than_read_as_absent() {
        let scratch = Scratch::new("unusable");
        let store = scratch.store();
        fs::create_dir_all(store.path().parent().unwrap()).unwrap();

        for body in [
            "not json".to_string(),
            serde_json::json!({ "version": 2, "id": "pill-1", "text": "x" }).to_string(),
            serde_json::json!({ "version": 1, "id": "pill 1", "text": "x" }).to_string(),
            serde_json::json!({ "version": 1, "id": "pill-1", "text": "  " }).to_string(),
            serde_json::json!({
                "version": 1,
                "id": "pill-1",
                "text": "x".repeat(MAX_PENDING_BYTES as usize)
            })
            .to_string(),
        ] {
            fs::write(store.path(), &body).unwrap();
            // A corrupt record must never be mistaken for "nothing was saved",
            // because that is what would let the next save destroy it silently.
            assert!(
                matches!(store.load(), Err(PendingError::Corrupt)),
                "{}",
                &body[..body.len().min(40)]
            );
        }
    }

    #[test]
    fn a_directory_in_place_of_the_record_is_corrupt_not_absent() {
        let scratch = Scratch::new("directory");
        let store = scratch.store();
        fs::create_dir_all(store.path()).unwrap();
        assert!(matches!(store.load(), Err(PendingError::Corrupt)));
    }

    #[test]
    fn non_ascii_text_survives_the_round_trip_it_was_accepted_for() {
        let scratch = Scratch::new("unicode");
        let store = scratch.store();
        // Characters that are one UTF-16 unit but three UTF-8 bytes, and one
        // that is two UTF-16 units and four bytes. A store that measured a
        // different way from the service would accept these and then refuse to
        // read them back, losing the transcript at the worst moment.
        let text = format!("{}{}", "\u{4f60}\u{597d}".repeat(1_000), "\u{1f600}");
        assert!(text.len() > text.chars().count());
        let pending = Pending {
            id: "pill-1".into(),
            text: text.clone(),
        };
        store.save(&pending).unwrap();
        assert_eq!(store.load().unwrap(), Some(pending));

        // Beyond the shared bound the store refuses rather than writing a
        // record it could never load.
        let oversized = Pending {
            id: "pill-2".into(),
            text: "\u{4f60}".repeat(scufris_control::MAX_SUBMISSION_TEXT_BYTES),
        };
        assert!(matches!(
            store.save(&oversized),
            Err(PendingError::Unbounded)
        ));
        // The transcript already kept is untouched.
        assert_eq!(store.load().unwrap().unwrap().text, text);
    }

    #[test]
    fn a_write_that_cannot_land_is_reported_instead_of_logged() {
        let scratch = Scratch::new("readonly");
        let store = scratch.store();
        store.save(&pending()).unwrap();
        let directory = store.path().parent().unwrap().to_path_buf();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap();

        let error = store.save(&pending()).unwrap_err();
        assert!(
            matches!(
                error,
                PendingError::Io {
                    operation: "written",
                    ..
                }
            ),
            "{error}"
        );
        assert!(store.clear().is_err());
        assert!(
            error
                .to_string()
                .starts_with("the saved transcript could not be written")
        );

        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
}
