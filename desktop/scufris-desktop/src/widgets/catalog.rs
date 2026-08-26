//! What widgets exist, and what each one is.
//!
//! A widget is a directory: `widget.toml` says what it is called and how big
//! its window is, and `widget.ts` renders into it. In-repo widgets live under
//! `desktop/widgets/` and are compiled into the binary by `build.rs`, so
//! discovery is a startup check over what was built rather than a walk of the
//! filesystem. `SCUFRIS_WIDGET_PATH` names extra roots on the person's own
//! machine, walked at startup and read from their compiled `widget.js`.
//!
//! Two rules make a root safe: the directory name is the widget's identifier,
//! and no widget ever shadows another. Neither is a preference. An identifier
//! that does not match its directory makes a widget impossible to find by the
//! name the person types, and a shadowed widget makes that name resolve to
//! something other than what it always did.
//!
//! Where the two kinds of root part is what happens to a widget that breaks a
//! rule. A widget that shipped is a build-time promise, so it stops the
//! companion and names the directory at fault. One on the search path is a
//! project on the person's machine that may be half-installed or gone, so it is
//! reported and passed over: a login session with no companion in it is the
//! worse of the two outcomes.

use std::{collections::BTreeMap, time::Duration};

use scufris_control::CatalogEntry;
use serde::Deserialize;
use tracing::{debug, warn};

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
    /// False when two panels asking the same question must still each get a
    /// process of their own.
    shared: Option<bool>,
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
    /// True while two panels asking the same question may share one process.
    ///
    /// The default, because a sampler answering the same question twice is one
    /// process doing the same work twice. A widget whose backend carries state
    /// of its own says otherwise: two timers of the same length are two
    /// timers, not one counted twice.
    pub shared: bool,
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
        let mut widgets: BTreeMap<String, Widget> = BTreeMap::new();
        for source in sources {
            let widget = install(*source, backends)?;
            if widgets.insert(widget.id.clone(), widget).is_some() {
                return Err(CatalogError::Duplicate {
                    id: source.directory.to_string(),
                });
            }
        }
        Ok(Self { widgets })
    }

    /// Adds widgets found outside the binary, keeping what is already here.
    ///
    /// External roots are additive and never override: a widget that shipped
    /// wins over one on the search path, and an earlier root wins over a later
    /// one. Nothing is shadowed, which is the rule duplicates exist to keep,
    /// and the person's `cpu` is still the `cpu` they have always had.
    ///
    /// A widget that will not install is reported and skipped rather than
    /// stopping the companion. A shipped widget that is wrong is a build
    /// failure and the developer sees it; one on the search path is a project
    /// on the person's own machine that may be half-installed or gone, and a
    /// login session with no companion in it is the worse of the two outcomes.
    /// The name it would have answered to simply resolves to nothing.
    pub fn extend(&mut self, sources: &[Source<'_>], backends: &[&str]) {
        for source in sources {
            match install(*source, backends) {
                Ok(widget) => {
                    if let Some(held) = self.widgets.get(&widget.id) {
                        warn!(
                            id = held.id,
                            "an external widget is already installed and was skipped"
                        );
                    } else {
                        self.widgets.insert(widget.id.clone(), widget);
                    }
                }
                Err(error) => warn!("an external widget was skipped: {error}"),
            }
        }
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

    /// Returns the widgets the person can summon, as `(identifier, name)`.
    ///
    /// A summon carries no payload, so the widget has to be able to fill itself:
    /// the ones that can are the ones with a backend behind them, which stands
    /// up on its own defaults. A widget that only ever shows what Scufris handed
    /// it would summon as an empty panel.
    pub fn summonable(&self) -> Vec<(String, String)> {
        self.widgets
            .values()
            .filter(|widget| widget.backend.is_some())
            .map(|widget| (widget.id.clone(), widget.name.clone()))
            .collect()
    }
}

/// The environment variable that names extra widget roots.
pub const WIDGET_PATH: &str = "SCUFRIS_WIDGET_PATH";

/// One widget directory read off a search path, holding its own text.
///
/// The compiled module rather than the TypeScript: nothing compiles anything
/// at startup, and the companion's closure has no compiler in it. A project
/// that ships a widget ships the `widget.js` its own build produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct External {
    directory: String,
    manifest: String,
    script: String,
}

impl External {
    /// Borrows this directory as a source the catalog can read.
    pub fn source(&self) -> Source<'_> {
        Source {
            directory: &self.directory,
            manifest: &self.manifest,
            script: &self.script,
        }
    }
}

/// Reads every widget directory on one search path, in the order given.
///
/// The path is separated the way `PATH` is. A root that is not there is passed
/// over without a word: the variable describes the person's machine, and a
/// project they have not installed yet is not a fault. A directory inside a
/// root that is missing one of its two files, or that cannot be read, is
/// reported and passed over - it says it is a widget and is not one.
///
/// Nothing here reads a manifest. That is [`install`]'s job, and it is the same
/// job for a widget that shipped and one that did not.
pub fn search(path: &str) -> Vec<External> {
    let mut found = Vec::new();
    for root in path.split(':').filter(|root| !root.is_empty()) {
        let listing = match std::fs::read_dir(root) {
            Ok(listing) => listing,
            Err(error) => {
                debug!(root, "no widgets on this root: {error}");
                continue;
            }
        };
        // Sorted, so what a root offers does not depend on the order the
        // filesystem happened to hand its entries over. Two roots stay in the
        // order the person wrote them, because that order is a preference.
        let mut directories: Vec<_> = listing.filter_map(Result::ok).map(|at| at.path()).collect();
        directories.sort();
        for directory in directories {
            let Some(name) = directory.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let manifest = directory.join("widget.toml");
            if !manifest.is_file() {
                continue;
            }
            match (
                std::fs::read_to_string(&manifest),
                std::fs::read_to_string(directory.join("widget.js")),
            ) {
                (Ok(manifest), Ok(script)) => found.push(External {
                    directory: name.to_string(),
                    manifest,
                    script,
                }),
                (manifest, script) => {
                    let error = manifest.err().or(script.err());
                    warn!(
                        root,
                        name, "a widget directory could not be read: {error:?}"
                    );
                }
            }
        }
    }
    found
}

/// Reads one widget directory, or says why it cannot be installed.
fn install(source: Source<'_>, backends: &[&str]) -> Result<Widget, CatalogError> {
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
    // A widget naming a backend nothing installs is a panel that opens and then
    // never shows a number. Caught here rather than there, for the reason a
    // renamed widget is.
    if let Some(backend) = &manifest.backend
        && !backends.contains(&backend.as_str())
    {
        return Err(CatalogError::NoBackend {
            directory: source.directory.to_string(),
            backend: backend.clone(),
        });
    }
    Ok(Widget {
        id: manifest.id,
        name: manifest.name,
        description: manifest.description,
        width: manifest.width,
        height: manifest.height,
        backend: manifest.backend,
        cadence: manifest
            .cadence
            .map_or(DEFAULT_CADENCE, Duration::from_millis),
        shared: manifest.shared.unwrap_or(true),
        script: source.script.to_string(),
    })
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

    #[test]
    fn only_a_widget_that_can_fill_itself_is_offered_to_be_summoned() {
        let plain = NOTE.to_string();
        let fed = format!("{NOTE}backend = \"system\"\n")
            .replace("id = \"note\"", "id = \"gauge\"")
            .replace("name = \"Note\"", "name = \"Gauge\"");
        let catalog = Catalog::build(&[source("note", &plain), source("gauge", &fed)], BACKENDS)
            .expect("both widgets install");
        // The note only ever shows what it was handed, so a summoned one would
        // be an empty panel. The gauge fills itself.
        assert_eq!(
            catalog.summonable(),
            vec![("gauge".to_string(), "Gauge".to_string())]
        );
    }
    /// A widget root on disk, taken down with the test that made it.
    struct Root(std::path::PathBuf);

    impl Root {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("scufris-widget-root-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("the root is writable");
            Self(path)
        }

        /// Writes one widget directory, or only the files that were given.
        fn widget(&self, directory: &str, manifest: Option<&str>, script: Option<&str>) -> &Self {
            let at = self.0.join(directory);
            std::fs::create_dir_all(&at).expect("the directory is writable");
            if let Some(manifest) = manifest {
                std::fs::write(at.join("widget.toml"), manifest).expect("the manifest is written");
            }
            if let Some(script) = script {
                std::fs::write(at.join("widget.js"), script).expect("the module is written");
            }
            self
        }

        fn path(&self) -> String {
            self.0.to_str().expect("the root is nameable").to_string()
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_widget_on_the_search_path_installs_beside_the_ones_that_shipped() {
        let root = Root::new("adds");
        let clock = NOTE.replace("note", "clock").replace("Note", "Clock");
        root.widget("clock", Some(&clock), Some("export function mount() {}"));

        let found = search(&root.path());
        let sources: Vec<_> = found.iter().map(External::source).collect();
        let mut catalog =
            Catalog::build(&[source("note", NOTE)], BACKENDS).expect("the note installs");
        catalog.extend(&sources, BACKENDS);

        assert!(catalog.get("note").is_some(), "the shipped widget is gone");
        let clock = catalog
            .get("clock")
            .expect("the external widget is missing");
        assert_eq!(clock.name, "Clock");
        assert_eq!(clock.script, "export function mount() {}");
    }

    #[test]
    fn a_widget_on_the_search_path_never_replaces_one_that_shipped() {
        let root = Root::new("shadow");
        let theirs = NOTE.replace("Show a short note", "Something else entirely");
        root.widget("note", Some(&theirs), Some("export function mount() {}"));

        let found = search(&root.path());
        let sources: Vec<_> = found.iter().map(External::source).collect();
        let mut catalog =
            Catalog::build(&[source("note", NOTE)], BACKENDS).expect("the note installs");
        catalog.extend(&sources, BACKENDS);

        // The name still resolves to what it always did.
        assert_eq!(
            catalog.get("note").expect("note is installed").description,
            "Show a short note"
        );
    }

    #[test]
    fn a_broken_widget_on_the_search_path_is_passed_over_rather_than_fatal() {
        let root = Root::new("broken");
        // One with no module, one whose manifest disagrees with its directory,
        // one that names a backend nothing installs, and one that is fine.
        root.widget("half", Some(NOTE), None);
        root.widget("renamed", Some(NOTE), Some("export function mount() {}"));
        root.widget(
            "hungry",
            Some(&format!(
                "{}backend = \"weather-station\"\n",
                NOTE.replace("note", "hungry").replace("Note", "Hungry")
            )),
            Some("export function mount() {}"),
        );
        let clock = NOTE.replace("note", "clock").replace("Note", "Clock");
        root.widget("clock", Some(&clock), Some("export function mount() {}"));

        let found = search(&root.path());
        // The half-written one never even reaches the catalog: it is missing a
        // file rather than saying something wrong.
        assert_eq!(found.len(), 3, "{found:?}");
        let sources: Vec<_> = found.iter().map(External::source).collect();
        let mut catalog = Catalog::default();
        catalog.extend(&sources, BACKENDS);

        assert!(catalog.get("clock").is_some(), "the good one is missing");
        assert!(catalog.get("renamed").is_none());
        assert!(catalog.get("hungry").is_none());
    }

    #[test]
    fn a_root_that_is_not_there_is_passed_over_without_a_word() {
        let root = Root::new("present");
        let clock = NOTE.replace("note", "clock").replace("Note", "Clock");
        root.widget("clock", Some(&clock), Some("export function mount() {}"));
        // An empty entry, one that does not exist, and one that does.
        let path = format!("::/nowhere/scufris-widgets:{}", root.path());
        let found = search(&path);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].source().directory, "clock");
    }
}
