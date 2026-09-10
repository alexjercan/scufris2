//! Durable quiet briefing rows and terminal proactive delivery.

use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
};

use scufris_control::service::{
    BriefingCollectionState, BriefingDeliveryState, BriefingRow, BriefingWake, MAX_BRIEFING_ROWS,
    validate_briefing_row, validate_briefing_wake,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;

const FORMAT_VERSION: u32 = 1;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Queued {
    run_id: String,
    wake: BriefingWake,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    version: u32,
    rows: Vec<BriefingRow>,
    pending: Vec<Queued>,
}

/// The service-owned account of briefing presentation and terminal ingress.
pub struct BriefingStore {
    path: PathBuf,
    rows: Vec<BriefingRow>,
    pending: Vec<Queued>,
}

impl BriefingStore {
    pub fn open(path: PathBuf) -> Self {
        let mut store = Self {
            path,
            rows: Vec::new(),
            pending: Vec::new(),
        };
        if let Err(error) = store.load() {
            warn!(%error, path = %store.path.display(), "briefing state was rejected; starting empty");
            store.reject();
        }
        // A service crash can leave a dispatched item in progress. No model is
        // still attached to this process, so it is pending again.
        let mut changed = false;
        for row in &mut store.rows {
            if row.delivery == BriefingDeliveryState::InProgress {
                row.delivery = BriefingDeliveryState::Pending;
                changed = true;
            }
        }
        if changed && let Err(error) = store.persist() {
            warn!(%error, "recovered briefing state could not be stored");
        }
        store
    }

    pub fn rows(&self) -> Vec<BriefingRow> {
        self.rows.clone()
    }

    pub fn next(&self) -> Option<(&str, &BriefingWake)> {
        self.pending
            .first()
            .map(|queued| (queued.run_id.as_str(), &queued.wake))
    }

    /// Merges a generation update without allowing a stale writer to move it
    /// backward. A terminal wake is kept in the same atomic snapshot.
    pub fn upsert(
        &mut self,
        mut incoming: BriefingRow,
        wake: Option<BriefingWake>,
        already_delivered: bool,
    ) -> Result<(), StoreError> {
        let previous_rows = self.rows.clone();
        let previous_pending = self.pending.clone();
        // Collection cannot acknowledge delivery. The service computed this
        // from canonical replay, or explicitly accepted a wake-free legacy
        // row, before calling the store.
        incoming.delivery = if already_delivered {
            BriefingDeliveryState::Delivered
        } else {
            BriefingDeliveryState::Pending
        };
        if let Some(current) = self.rows.iter_mut().find(|row| row.id == incoming.id) {
            let forward = current.collection == BriefingCollectionState::Collecting
                || current.collection == incoming.collection;
            if forward {
                current.collection = incoming.collection;
                current.completed = current
                    .completed
                    .max(incoming.completed)
                    .min(current.total.max(incoming.total));
                current.total = current.total.max(incoming.total);
                current.failed = current.failed.max(incoming.failed).min(current.completed);
                current.summary = incoming.summary;
            }
            if current.delivery != BriefingDeliveryState::Delivered
                && incoming.delivery == BriefingDeliveryState::Delivered
            {
                current.delivery = BriefingDeliveryState::Delivered;
            }
        } else {
            self.rows.push(incoming.clone());
        }

        if let Some(wake) = wake {
            if self
                .pending
                .iter()
                .any(|queued| queued.wake.event_id == wake.event_id && queued.run_id != incoming.id)
            {
                self.rows = previous_rows;
                self.pending = previous_pending;
                return Err(StoreError::Malformed(
                    "briefing wake event belongs to another run".into(),
                ));
            }
            let delivered = self
                .rows
                .iter()
                .find(|row| row.id == incoming.id)
                .is_some_and(|row| row.delivery == BriefingDeliveryState::Delivered);
            if !delivered
                && !self
                    .pending
                    .iter()
                    .any(|queued| queued.run_id == incoming.id)
            {
                self.pending.push(Queued {
                    run_id: incoming.id,
                    wake,
                });
            }
        }
        self.pending.retain(|queued| {
            !self.rows.iter().any(|row| {
                row.id == queued.run_id && row.delivery == BriefingDeliveryState::Delivered
            })
        });
        self.rows.sort_by_key(|row| (row.since, row.id.clone()));
        while self.rows.len() > MAX_BRIEFING_ROWS {
            let Some(index) = self
                .rows
                .iter()
                .position(|row| row.delivery == BriefingDeliveryState::Delivered)
            else {
                self.rows = previous_rows;
                self.pending = previous_pending;
                return Err(StoreError::Full);
            };
            self.rows.remove(index);
        }
        if let Err(error) = self.persist() {
            self.rows = previous_rows;
            self.pending = previous_pending;
            return Err(error);
        }
        Ok(())
    }

    /// Closes the crash window after canonical replay was stored but the
    /// inbox acknowledgement was not. This runs before an agent can connect,
    /// so an answer already in replay cannot trigger a second model turn.
    pub fn recover_delivered(&mut self, contains: impl Fn(&str) -> bool) -> Result<(), StoreError> {
        let previous_rows = self.rows.clone();
        let previous_pending = self.pending.clone();
        let delivered: Vec<(String, String)> = self
            .pending
            .iter()
            .filter(|queued| contains(&queued.wake.event_id))
            .map(|queued| (queued.run_id.clone(), queued.wake.event_id.clone()))
            .collect();
        if delivered.is_empty() {
            return Ok(());
        }
        for (run_id, _) in &delivered {
            if let Some(row) = self.rows.iter_mut().find(|row| row.id == *run_id) {
                row.delivery = BriefingDeliveryState::Delivered;
            }
        }
        self.pending.retain(|queued| {
            !delivered
                .iter()
                .any(|(_, event_id)| event_id == &queued.wake.event_id)
        });
        if let Err(error) = self.persist() {
            self.rows = previous_rows;
            self.pending = previous_pending;
            return Err(error);
        }
        Ok(())
    }

    pub fn in_progress(&mut self, event_id: &str) -> Result<(), StoreError> {
        let previous_rows = self.rows.clone();
        let Some(run_id) = self
            .pending
            .iter()
            .find(|queued| queued.wake.event_id == event_id)
            .map(|queued| queued.run_id.clone())
        else {
            return Ok(());
        };
        if let Some(row) = self.rows.iter_mut().find(|row| row.id == run_id) {
            row.delivery = BriefingDeliveryState::InProgress;
        }
        if let Err(error) = self.persist() {
            self.rows = previous_rows;
            return Err(error);
        }
        Ok(())
    }

    pub fn retry(&mut self, event_id: &str) -> Result<(), StoreError> {
        let previous_rows = self.rows.clone();
        let Some(run_id) = self
            .pending
            .iter()
            .find(|queued| queued.wake.event_id == event_id)
            .map(|queued| queued.run_id.clone())
        else {
            return Ok(());
        };
        if let Some(row) = self.rows.iter_mut().find(|row| row.id == run_id) {
            row.delivery = BriefingDeliveryState::Pending;
        }
        if let Err(error) = self.persist() {
            self.rows = previous_rows;
            return Err(error);
        }
        Ok(())
    }

    pub fn acknowledge(&mut self, event_id: &str) -> Result<bool, StoreError> {
        let previous_rows = self.rows.clone();
        let previous_pending = self.pending.clone();
        let Some(index) = self
            .pending
            .iter()
            .position(|queued| queued.wake.event_id == event_id)
        else {
            return Ok(false);
        };
        let run_id = self.pending[index].run_id.clone();
        if let Some(row) = self.rows.iter_mut().find(|row| row.id == run_id) {
            row.delivery = BriefingDeliveryState::Delivered;
        }
        self.pending.remove(index);
        if let Err(error) = self.persist() {
            self.rows = previous_rows;
            self.pending = previous_pending;
            return Err(error);
        }
        Ok(true)
    }

    fn load(&mut self) -> Result<(), StoreError> {
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
            .open(&self.path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let metadata = file.metadata()?;
        if !metadata.file_type().is_file() {
            return Err(StoreError::Malformed("state is not a regular file".into()));
        }
        if metadata.len() > MAX_FILE_BYTES {
            return Err(StoreError::TooLarge);
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(StoreError::TooLarge);
        }
        let stored: Stored = serde_json::from_slice(&bytes)
            .map_err(|error| StoreError::Malformed(error.to_string()))?;
        if stored.version != FORMAT_VERSION
            || stored.rows.len() > MAX_BRIEFING_ROWS
            || stored.pending.len() > MAX_BRIEFING_ROWS
        {
            return Err(StoreError::Malformed("unsupported briefing state".into()));
        }
        let mut row_ids = HashSet::new();
        for row in &stored.rows {
            validate_briefing_row(row).map_err(|error| StoreError::Malformed(error.to_string()))?;
            if !row_ids.insert(row.id.as_str()) {
                return Err(StoreError::Malformed("duplicate briefing row".into()));
            }
        }
        let mut run_ids = HashSet::new();
        let mut event_ids = HashSet::new();
        for queued in &stored.pending {
            validate_briefing_wake(&queued.wake)
                .map_err(|error| StoreError::Malformed(error.to_string()))?;
            let row = stored
                .rows
                .iter()
                .find(|row| row.id == queued.run_id)
                .ok_or_else(|| StoreError::Malformed("briefing wake has no row".into()))?;
            if row.delivery == BriefingDeliveryState::Delivered
                || !run_ids.insert(queued.run_id.as_str())
                || !event_ids.insert(queued.wake.event_id.as_str())
            {
                return Err(StoreError::Malformed(
                    "duplicate or delivered briefing wake".into(),
                ));
            }
        }
        self.rows = stored.rows;
        self.pending = stored.pending;
        Ok(())
    }

    fn persist(&self) -> Result<(), StoreError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        let mut bytes = serde_json::to_vec(&Stored {
            version: FORMAT_VERSION,
            rows: self.rows.clone(),
            pending: self.pending.clone(),
        })?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(StoreError::TooLarge);
        }
        let temporary = self.path.with_extension("json.tmp");
        let _ = fs::remove_file(&temporary);
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            File::open(parent)?.sync_all()
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result.map_err(StoreError::from)
    }

    fn reject(&self) {
        let rejected = self.path.with_extension("json.corrupt");
        let _ = fs::remove_file(&rejected);
        let _ = fs::rename(&self.path, rejected);
    }
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("briefing state I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("briefing state encoding failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("briefing state is malformed: {0}")]
    Malformed(String),
    #[error("briefing state exceeds its bound")]
    TooLarge,
    #[error("all briefing row slots still require delivery")]
    Full,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, collection: BriefingCollectionState) -> BriefingRow {
        BriefingRow {
            id: id.into(),
            date: "2026-09-10".into(),
            profile: "nightly".into(),
            collection,
            delivery: BriefingDeliveryState::Pending,
            since: 1_789_000_000,
            completed: u32::from(collection != BriefingCollectionState::Collecting),
            total: 1,
            failed: u32::from(collection == BriefingCollectionState::Failed),
            summary: "measured state".into(),
        }
    }

    fn wake(id: &str) -> BriefingWake {
        BriefingWake {
            event_id: format!("briefing-{id}-terminal"),
            custom_type: "scufris-briefing".into(),
            text: "write the measured briefing".into(),
            details: None,
        }
    }

    #[test]
    fn terminal_ingress_and_ack_survive_restart() {
        let root = std::env::temp_dir().join(format!("scufris-briefings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let path = root.join("briefings.json");
        let mut store = BriefingStore::open(path.clone());
        store
            .upsert(
                row("generation-a", BriefingCollectionState::Failed),
                Some(wake("generation-a")),
                false,
            )
            .unwrap();
        store.in_progress("briefing-generation-a-terminal").unwrap();
        drop(store);

        let mut restored = BriefingStore::open(path.clone());
        assert_eq!(restored.rows()[0].delivery, BriefingDeliveryState::Pending);
        assert!(
            restored
                .acknowledge("briefing-generation-a-terminal")
                .unwrap()
        );
        drop(restored);
        let done = BriefingStore::open(path);
        assert_eq!(done.rows()[0].delivery, BriefingDeliveryState::Delivered);
        assert!(done.next().is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn canonical_delivery_recovery_prevents_a_second_dispatch() {
        let root =
            std::env::temp_dir().join(format!("scufris-briefing-canonical-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let path = root.join("briefings.json");
        let mut store = BriefingStore::open(path.clone());
        store
            .upsert(
                row("generation-a", BriefingCollectionState::Collected),
                Some(wake("generation-a")),
                false,
            )
            .unwrap();
        store.in_progress("briefing-generation-a-terminal").unwrap();
        drop(store);

        let mut restored = BriefingStore::open(path);
        restored
            .recover_delivered(|event_id| event_id == "briefing-generation-a-terminal")
            .unwrap();
        assert!(restored.next().is_none());
        assert_eq!(
            restored.rows()[0].delivery,
            BriefingDeliveryState::Delivered
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_generation_updates_do_not_reopen_terminal_state() {
        let root =
            std::env::temp_dir().join(format!("scufris-briefing-fence-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let path = root.join("briefings.json");
        let mut store = BriefingStore::open(path);
        store
            .upsert(
                row("generation-a", BriefingCollectionState::Collected),
                Some(wake("generation-a")),
                false,
            )
            .unwrap();
        store
            .upsert(
                row("generation-a", BriefingCollectionState::Collected),
                Some(BriefingWake {
                    event_id: "briefing-generation-a-other".into(),
                    ..wake("generation-a")
                }),
                false,
            )
            .unwrap();
        assert_eq!(
            store.next().map(|(_, wake)| wake.event_id.as_str()),
            Some("briefing-generation-a-terminal")
        );
        store.acknowledge("briefing-generation-a-terminal").unwrap();
        store
            .upsert(
                row("generation-a", BriefingCollectionState::Collecting),
                None,
                false,
            )
            .unwrap();
        store
            .upsert(
                row("generation-a", BriefingCollectionState::Failed),
                Some(BriefingWake {
                    event_id: "briefing-generation-a-other".into(),
                    ..wake("generation-a")
                }),
                false,
            )
            .unwrap();
        assert_eq!(
            store.rows()[0].collection,
            BriefingCollectionState::Collected
        );
        assert_eq!(store.rows()[0].delivery, BriefingDeliveryState::Delivered);
        assert!(store.next().is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
