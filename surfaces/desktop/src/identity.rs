//! Persistent desktop surface identity.

use std::{
    fs::OpenOptions,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

use scufris_control::is_identifier;

pub fn load_or_create(state_file: &Path) -> Result<String, String> {
    let directory = state_file
        .parent()
        .ok_or("the state file has no directory")?;
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let path: PathBuf = directory.join("surface-id");
    if path.exists() {
        let id = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let id = id.trim();
        if !is_identifier(id) {
            return Err("the persisted surface ID is invalid".into());
        }
        return Ok(id.to_string());
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
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|error| error.to_string())?;
    writeln!(file, "{id}").map_err(|error| error.to_string())?;
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
}
