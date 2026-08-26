//! What widgets exist, and what each one is.
//!
//! A widget is a directory: `widget.toml` says what it is called and how big
//! its window is, and `widget.ts` renders into it. In-repo widgets live under
//! `desktop/widgets/` and are compiled into the binary by `build.rs`, so
//! discovery is a startup check over what was built rather than a walk of the
//! filesystem. External roots arrive with `SCUFRIS_WIDGET_PATH` later; the two
//! rules that make them safe are enforced here already.
//!
//! The two rules: the directory name is the widget's identifier, and a
//! duplicate identifier is a startup failure. Neither is a preference. An
//! identifier that does not match its directory makes a widget impossible to
//! find by the name the person types, and a duplicate silently shadows one
//! root's widget with another's.

use std::{collections::BTreeMap, time::Duration};

use scufris_control::CatalogEntry;
use serde::Deserialize;

/// How often a widget with a backend expects a reading, when it does not say.
///
/// One second. The number is only used to decide when silence has gone on long
/// enough to mark, so a widget that samples more slowly than this and never
/// says so is marked stale for a moment rather than being wrong.
const DEFAULT_CADENCE: Duration = Duration::from_secs(1);

/// One widget directory, as `build.rs` compiled it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source<'a> {
    /// The directory name, which has to be the widget's identifier.
    pub directory: &'a str,
    /// The text of `widget.toml`.
    pub manifest: &'a str,
    /// The compiled `widget.js` module.
    pub script: &'a str,
}

/// What `widget.toml` says.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    id: String,
    name: String,
    description: String,
    width: u32,
    height: u32,
    /// Which backend feeds it, if any.
    backend: Option<String>,
    /// How often a reading is expected, in milliseconds.
    cadence: Option<u64>,
}

/// One installed widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Widget {
    /// The identifier the daemon opens it by.
    pub id: String,
    /// The name the chrome prints, in the window's micro-title.
    pub name: String,
    /// What the widget is for, as the model reads it.
    pub description: String,
    /// Window width in logical pixels.
    pub width: u32,
    /// Window height in logical pixels.
    pub height: u32,
    /// Which backend feeds it, if any.
    pub backend: Option<String>,
    /// How often it expects a reading from that backend.
    pub cadence: Duration,
    /// The compiled module the shell window imports.
    pub script: String,
}

/// Why a widget could not be installed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    /// The manifest could not be read.
    #[error("widgets/{directory}/widget.toml could not be read: {detail}")]
    Unreadable {
        /// The directory whose manifest is wrong.
        directory: String,
        /// What the parser said.
        detail: String,
    },
    /// The manifest names an identifier other than its own directory.
    #[error("widgets/{directory} declares the id {id}; the directory name is the id")]
    Renamed {
        /// The directory the widget was found in.
        directory: String,
        /// The identifier the manifest claimed.
        id: String,
    },
    /// Two roots install the same identifier.
    #[error("two widget directories are both named {id}")]
    Duplicate {
        /// The identifier both directories claim.
        id: String,
    },
    /// The manifest names a backend that is not installed.
    #[error("widgets/{directory} names the backend {backend}, which is not installed")]
    NoBackend {
        /// The directory whose manifest is wrong.
        directory: String,
        /// The backend it asked for.
        backend: String,
    },
}

/// Every installed widget, by identifier.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Catalog {
    widgets: BTreeMap<String, Widget>,
}

impl Catalog {
    /// Reads every manifest and answers with the catalog, or with the first
    /// widget that would have been wrong to install.
    ///
    /// A failure here stops the companion at startup. That is the point: a
    /// widget the person asks for by name and that resolves to something else
    /// is worse than a companion that says which directory is at fault.
    pub fn build(sources: &[Source<'_>], backends: &[&str]) -> Result<Self, CatalogError> {
        let mut widgets = BTreeMap::new();
        for source in sources {
            let manifest: Manifest =
                toml::from_str(source.manifest).map_err(|error| CatalogError::Unreadable {
                    directory: source.directory.to_string(),
                    detail: error.to_string(),
                })?;
            if manifest.id != source.directory {
                return Err(CatalogError::Renamed {
                    directory: source.directory.to_string(),
                    id: manifest.id,
                });
            }
            // A widget naming a backend nothing installs is a panel that opens
            // and then never shows a number. Caught here rather than there, for
            // the reason a renamed widget is.
            if let Some(backend) = &manifest.backend
                && !backends.contains(&backend.as_str())
            {
                return Err(CatalogError::NoBackend {
                    directory: source.directory.to_string(),
                    backend: backend.clone(),
                });
            }
            let widget = Widget {
                id: manifest.id,
                name: manifest.name,
                description: manifest.description,
                width: manifest.width,
                height: manifest.height,
                backend: manifest.backend,
                cadence: manifest
                    .cadence
                    .map_or(DEFAULT_CADENCE, Duration::from_millis),
                script: source.script.to_string(),
            };
            if widgets.insert(widget.id.clone(), widget).is_some() {
                return Err(CatalogError::Duplicate {
                    id: source.directory.to_string(),
                });
            }
        }
        Ok(Self { widgets })
    }

    /// Returns one installed widget.
    pub fn get(&self, id: &str) -> Option<&Widget> {
        self.widgets.get(id)
    }

    /// Returns the catalog as the daemon reads it.
    ///
    /// The script is left behind: the daemon types a tool from these entries
    /// and never runs the module.
    pub fn entries(&self) -> Vec<CatalogEntry> {
        self.widgets
            .values()
            .map(|widget| CatalogEntry {
                id: widget.id.clone(),
                name: widget.name.clone(),
                description: widget.description.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = r#"
id = "note"
name = "Note"
description = "Show a short note"
width = 250
height = 110
"#;

    /// What the tests are told is installed.
    const BACKENDS: &[&str] = &["system"];

    fn source<'a>(directory: &'a str, manifest: &'a str) -> Source<'a> {
        Source {
            directory,
            manifest,
            script: "export function mount() {}",
        }
    }

    #[test]
    fn a_manifest_becomes_the_widget_the_daemon_opens_by_name() {
        let catalog =
            Catalog::build(&[source("note", NOTE)], BACKENDS).expect("the manifest is well formed");
        let widget = catalog.get("note").expect("note is installed");
        assert_eq!(widget.name, "Note");
        assert_eq!((widget.width, widget.height), (250, 110));
        assert_eq!(catalog.get("weather"), None);
    }

    #[test]
    fn the_catalog_the_daemon_reads_carries_the_names_and_not_the_code() {
        let catalog =
            Catalog::build(&[source("note", NOTE)], BACKENDS).expect("the manifest is well formed");
        let entries = catalog.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "note");
        assert_eq!(entries[0].description, "Show a short note");
    }

    #[test]
    fn a_widget_that_renames_itself_is_a_startup_failure() {
        // The person and the model both name a widget by its directory. One
        // that answers to something else cannot be opened by the name it is
        // filed under, and would be found only by reading its manifest.
        let renamed = NOTE.replace("\"note\"", "\"scratch\"");
        assert_eq!(
            Catalog::build(&[source("note", &renamed)], BACKENDS),
            Err(CatalogError::Renamed {
                directory: "note".into(),
                id: "scratch".into(),
            })
        );
    }

    #[test]
    fn two_directories_with_one_name_are_a_startup_failure_rather_than_a_shadow() {
        assert_eq!(
            Catalog::build(&[source("note", NOTE), source("note", NOTE)], BACKENDS),
            Err(CatalogError::Duplicate { id: "note".into() })
        );
    }

    #[test]
    fn a_manifest_that_is_missing_a_field_names_the_directory_at_fault() {
        let short = "id = \"note\"\nname = \"Note\"\n";
        let error =
            Catalog::build(&[source("note", short)], BACKENDS).expect_err("the manifest is short");
        assert!(matches!(
            &error,
            CatalogError::Unreadable { directory, .. } if directory == "note"
        ));
        assert!(
            error.to_string().contains("widgets/note/widget.toml"),
            "the message does not say which manifest: {error}"
        );
    }

    #[test]
    fn a_key_nothing_reads_is_a_startup_failure_rather_than_a_setting_that_does_nothing() {
        let typo = format!("{NOTE}widht = 250\n");
        assert!(matches!(
            Catalog::build(&[source("note", &typo)], BACKENDS),
            Err(CatalogError::Unreadable { .. })
        ));
    }

    #[test]
    fn a_widget_naming_a_backend_nobody_installs_is_a_startup_failure() {
        // Otherwise it is a panel that opens, draws nothing, and never says
        // why - which is the one outcome the whole supervisor exists to avoid.
        let fed = format!("{NOTE}backend = \"weather-station\"\n");
        assert_eq!(
            Catalog::build(&[source("note", &fed)], BACKENDS),
            Err(CatalogError::NoBackend {
                directory: "note".into(),
                backend: "weather-station".into(),
            })
        );
    }

    #[test]
    fn a_widget_that_does_not_say_how_often_it_expects_a_reading_is_given_a_second() {
        let fed = format!("{NOTE}backend = \"system\"\n");
        let catalog =
            Catalog::build(&[source("note", &fed)], BACKENDS).expect("the backend is installed");
        let widget = catalog.get("note").expect("note is installed");
        assert_eq!(widget.backend.as_deref(), Some("system"));
        assert_eq!(widget.cadence, DEFAULT_CADENCE);

        let quick = format!("{NOTE}backend = \"system\"\ncadence = 250\n");
        let catalog =
            Catalog::build(&[source("note", &quick)], BACKENDS).expect("the backend is installed");
        assert_eq!(
            catalog.get("note").expect("note is installed").cadence,
            Duration::from_millis(250)
        );
    }

    #[test]
    fn every_widget_shipped_with_the_companion_installs() {
        // The generated table is what `build.rs` walked. A widget that was
        // added to the tree but cannot be installed fails here rather than on
        // the first person who asks for it.
        let catalog = Catalog::build(super::super::INSTALLED, &super::super::backends::names())
            .expect("the shipped widgets install");
        assert!(!catalog.entries().is_empty(), "no widget is shipped");
        assert!(catalog.get("note").is_some(), "the note widget is missing");
        // And every backend a shipped widget names is a backend that shipped.
        let cpu = catalog.get("cpu").expect("the cpu widget is missing");
        assert_eq!(cpu.backend.as_deref(), Some("system"));
    }
}
