//! Persistent desktop surface identity.

use std::{
    fs::OpenOptions,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

use scufris_control::is_identifier;
use tracing::warn;

/// The stable name this companion answers to, minted once and kept.
///
/// An unreadable file is treated as an absent one. The write was not
/// crash-safe and the read was fail-closed, so a crash between creating the
/// file and its bytes reaching disk left a zero-length `surface-id` that made
/// `start` return before the tray was ever built: the companion exited on
/// every launch, the unit restarted it, and the only clue was one journal line
/// that did not name the file. Losing the identity costs a replay; refusing to
/// start costs the whole companion.
pub fn load_or_create(state_file: &Path) -> Result<String, String> {
    let directory = state_file
        .parent()
        .ok_or("the state file has no directory")?;
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let path: PathBuf = directory.join("surface-id");
    match std::fs::read_to_string(&path) {
        Ok(kept) if is_identifier(kept.trim()) => return Ok(kept.trim().to_string()),
        Ok(_) => {
            warn!(file = %path.display(), "the persisted surface ID is unusable; minting a new one")
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            warn!(file = %path.display(), %error, "the persisted surface ID could not be read; minting a new one");
        }
    }
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|error| error.to_string())?;
    let id = format!(
        "surface-{}",
        random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    // Written whole or not at all: a temporary, synced, then renamed over the
    // final name. `writeln!` straight into the destination is what left a
    // zero-length file behind when the machine stopped at the wrong moment.
    let temporary = directory.join(".surface-id.tmp");
    let _ = std::fs::remove_file(&temporary);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|error| format!("{}: {error}", temporary.display()))?;
    let written = writeln!(file, "{id}").and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("{}: {error}", temporary.display()));
    }
    std::fs::rename(&temporary, &path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(id)
}

pub fn diagnostic_name() -> String {
    std::env::var("SCUFRIS_DESKTOP_SURFACE_NAME")
        .ok()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            std::env::var("HOSTNAME")
                .ok()
                .filter(|name| !name.trim().is_empty())
        })
        .unwrap_or_else(|| "Scufris desktop".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_is_stable() {
        let root = std::env::temp_dir().join(format!("scufris-surface-id-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let state = root.join("pending.json");
        let first = load_or_create(&state).unwrap();
        assert_eq!(load_or_create(&state).unwrap(), first);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unusable_identity_is_replaced_rather_than_fatal() {
        // `start` runs this before the Tauri builder, so returning an error
        // here exits the process before the tray exists. A zero-length file -
        // what an interrupted write leaves - made the companion exit on every
        // launch, for good, and the file is under `.local/state`, so it
        // survives the reboot that caused it.
        let root =
            std::env::temp_dir().join(format!("scufris-surface-id-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let state = root.join("pending.json");
        let first = load_or_create(&state).unwrap();
        let path = root.join("surface-id");
        for unusable in ["", "  \n", "not an identifier", "\u{0}"] {
            std::fs::write(&path, unusable).unwrap();
            let minted = load_or_create(&state).unwrap();
            assert!(is_identifier(&minted), "{unusable:?} produced {minted:?}");
            assert_ne!(minted, first);
            // And it is kept, so the replacement happens once.
            assert_eq!(load_or_create(&state).unwrap(), minted);
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
