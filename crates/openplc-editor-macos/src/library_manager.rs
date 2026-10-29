use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use open_rust_plc_compiler::library::StlibArchive;
use ring::digest::{digest, SHA256};

use crate::atomic_file;
use crate::public_library_catalog::{
    download_public_library, list_public_libraries, PublicLibrary, PublicLibraryPage,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::project::OpenProject;

const REGISTRY_FILE: &str = "registry.json";

const BUNDLED_LIBRARY_RAW: &[(&str, &str)] = &[
    (
        "additional-function-blocks.stlib",
        include_str!("../resources/libraries/additional-function-blocks.stlib"),
    ),
    (
        "iec-standard-fb.stlib",
        include_str!("../resources/libraries/iec-standard-fb.stlib"),
    ),
    (
        "iec-std-functions.stlib",
        include_str!("../resources/libraries/iec-std-functions.stlib"),
    ),
    (
        "oscat-basic.stlib",
        include_str!("../resources/libraries/oscat-basic.stlib"),
    ),
    (
        "plcopen-softmotion.stlib",
        include_str!("../resources/libraries/plcopen-softmotion.stlib"),
    ),
];

static BUNDLED_LIBRARIES: OnceLock<Result<Vec<StlibArchive>, String>> = OnceLock::new();

fn bundled_archives() -> Result<Vec<StlibArchive>, String> {
    BUNDLED_LIBRARIES
        .get_or_init(|| {
            BUNDLED_LIBRARY_RAW
                .iter()
                .map(|(name, raw)| StlibArchive::parse(raw, name))
                .collect()
        })
        .clone()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLibraryRef {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledLibrary {
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub version: String,
    pub bundled: bool,
    pub installed_at: String,
    pub origin: String,
    pub archive_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct LoadedLibraries {
    pub archives: Vec<StlibArchive>,
    pub missing: Vec<ProjectLibraryRef>,
}

#[derive(Debug, Clone)]
pub struct LibraryManager {
    libraries_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryRegistry {
    format_version: String,
    #[serde(default)]
    libraries: BTreeMap<String, RegistryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryEntry {
    version: String,
    installed_at: String,
    stlib_path: PathBuf,
    origin: String,
}

impl Default for LibraryRegistry {
    fn default() -> Self {
        Self {
            format_version: "1.0".to_owned(),
            libraries: BTreeMap::new(),
        }
    }
}

impl LibraryManager {
    pub fn new() -> Result<Self, String> {
        Ok(Self::with_libraries_dir(
            crate::app_paths::data_root()?.join("libraries"),
        ))
    }

    pub fn with_libraries_dir(libraries_dir: PathBuf) -> Self {
        Self { libraries_dir }
    }

    pub fn install_library(&self, source_path: &Path) -> Result<InstalledLibrary, String> {
        let extension = source_path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default();
        if extension.eq_ignore_ascii_case("stlib") {
            return self.install_stlib(source_path);
        }
        if extension.eq_ignore_ascii_case("lib") || extension.eq_ignore_ascii_case("library") {
            let bytes = fs::read(source_path)
                .map_err(|error| format!("read {}: {error}", source_path.display()))?;
            let filename = source_path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| "CODESYS library filename is not valid UTF-8".to_owned())?;
            let archive = crate::codesys_import::prepare_codesys_upload(&bytes, filename)?;
            let raw = serde_json::to_string(&archive)
                .map_err(|error| format!("serialize converted CODESYS library: {error}"))?;
            return self.install_archive(archive, raw, "codesys");
        }
        Err("Unsupported library format; expected .stlib, .lib, or .library.".to_owned())
    }

    pub fn install_stlib(&self, source_path: &Path) -> Result<InstalledLibrary, String> {
        if !source_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("stlib"))
        {
            return Err("Unsupported library format; expected a .stlib archive.".to_owned());
        }
        let raw = fs::read_to_string(source_path)
            .map_err(|error| format!("read {}: {error}", source_path.display()))?;
        let archive = StlibArchive::parse(&raw, &source_path.display().to_string())?;
        self.install_archive(archive, raw, "stlib")
    }

    fn install_archive(
        &self,
        archive: StlibArchive,
        raw: String,
        origin: &str,
    ) -> Result<InstalledLibrary, String> {
        validate_library_name(&archive.manifest.name)?;
        if self.is_bundled(&archive.manifest.name)? {
            return Err(format!(
                "Cannot install '{}' — a bundled library with this name already exists.",
                archive.manifest.name
            ));
        }
        let library_dir = self.libraries_dir.join(&archive.manifest.name);
        fs::create_dir_all(&library_dir)
            .map_err(|error| format!("create {}: {error}", library_dir.display()))?;
        let archive_path = library_dir.join(format!("{}.stlib", archive.manifest.name));
        atomic_file::write(&archive_path, raw)
            .map_err(|error| format!("write {}: {error}", archive_path.display()))?;

        let installed_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| format!("format install timestamp: {error}"))?;
        let mut registry = self.read_registry()?;
        registry.libraries.insert(
            archive.manifest.name.clone(),
            RegistryEntry {
                version: archive.manifest.version.clone(),
                installed_at: installed_at.clone(),
                stlib_path: archive_path.clone(),
                origin: origin.to_owned(),
            },
        );
        self.write_registry(&registry)?;

        Ok(InstalledLibrary {
            name: archive.manifest.name,
            display_name: archive.manifest.display_name,
            description: archive.manifest.description,
            version: archive.manifest.version,
            bundled: false,
            installed_at,
            origin: origin.to_owned(),
            archive_path: Some(archive_path),
        })
    }

    pub fn list_installed(&self) -> Result<Vec<InstalledLibrary>, String> {
        let mut installed = bundled_archives()?
            .into_iter()
            .map(|archive| InstalledLibrary {
                name: archive.manifest.name,
                display_name: archive.manifest.display_name,
                description: archive.manifest.description,
                version: archive.manifest.version,
                bundled: true,
                installed_at: String::new(),
                origin: "bundled".to_owned(),
                archive_path: None,
            })
            .collect::<Vec<_>>();
        let registry = self.read_registry()?;
        let mut user = Vec::new();
        for (name, entry) in registry.libraries {
            validate_library_name(&name)?;
            let archive_path = self.archive_path(&name);
            if archive_path != entry.stlib_path || !archive_path.is_file() {
                continue;
            }
            let archive = self.read_archive(&name)?;
            user.push(InstalledLibrary {
                name,
                display_name: archive.manifest.display_name,
                description: archive.manifest.description,
                version: archive.manifest.version,
                bundled: false,
                installed_at: entry.installed_at,
                origin: entry.origin,
                archive_path: Some(archive_path),
            });
        }
        user.sort_by(|left, right| left.name.cmp(&right.name));
        installed.extend(user);
        Ok(installed)
    }

    pub fn load_all(&self) -> Result<Vec<StlibArchive>, String> {
        let mut archives = bundled_archives()?;
        let registry = self.read_registry()?;
        for (name, entry) in registry.libraries {
            validate_library_name(&name)?;
            let path = self.archive_path(&name);
            if path == entry.stlib_path && path.is_file() {
                archives.push(self.read_archive(&name)?);
            }
        }
        Ok(archives)
    }

    pub fn query_public_catalog(&self, query: &str) -> Result<PublicLibraryPage, String> {
        list_public_libraries(query)
    }

    pub fn install_from_catalog(
        &self,
        library: &PublicLibrary,
    ) -> Result<InstalledLibrary, String> {
        let raw = download_public_library(&library.id)?;
        let actual = hex_lower(digest(&SHA256, raw.as_bytes()).as_ref());
        if !actual.eq_ignore_ascii_case(&library.sha256) {
            return Err(format!(
                "Public library {}@{} failed SHA-256 verification (expected {}, got {}).",
                library.name, library.version, library.sha256, actual
            ));
        }
        let archive = StlibArchive::parse(&raw, &format!("public catalog {}", library.id))?;
        if !archive.manifest.name.eq_ignore_ascii_case(&library.name)
            || archive.manifest.version != library.version
        {
            return Err(format!(
                "Public library metadata does not match downloaded archive: catalog {}@{}, archive {}@{}.",
                library.name, library.version, archive.manifest.name, archive.manifest.version
            ));
        }
        self.install_archive(archive, raw, "catalog")
    }

    pub fn uninstall(&self, name: &str) -> Result<(), String> {
        validate_library_name(name)?;
        if self.is_bundled(name)? {
            return Err(format!("Cannot uninstall bundled library '{name}'."));
        }
        let mut registry = self.read_registry()?;
        if registry.libraries.remove(name).is_none() {
            return Err(format!("Library '{name}' is not installed."));
        }
        let library_dir = self.libraries_dir.join(name);
        if library_dir.exists() {
            fs::remove_dir_all(&library_dir)
                .map_err(|error| format!("remove {}: {error}", library_dir.display()))?;
        }
        self.write_registry(&registry)
    }

    pub fn is_bundled(&self, name: &str) -> Result<bool, String> {
        Ok(bundled_archives()?
            .iter()
            .any(|archive| archive.manifest.name.eq_ignore_ascii_case(name)))
    }

    pub fn load_enabled(&self, refs: &[ProjectLibraryRef]) -> Result<LoadedLibraries, String> {
        let mut archives = bundled_archives()?;
        let mut missing = Vec::new();
        for library_ref in refs {
            validate_library_name(&library_ref.name)?;
            let path = self.archive_path(&library_ref.name);
            if !path.is_file() {
                missing.push(library_ref.clone());
                continue;
            }
            let archive = self.read_archive(&library_ref.name)?;
            if !archive
                .manifest
                .name
                .eq_ignore_ascii_case(&library_ref.name)
            {
                return Err(format!(
                    "Installed library {} has mismatched manifest name {:?}.",
                    path.display(),
                    archive.manifest.name
                ));
            }
            archives.push(archive);
        }
        Ok(LoadedLibraries { archives, missing })
    }

    pub fn load_project_libraries(&self, project: &OpenProject) -> Result<LoadedLibraries, String> {
        self.load_enabled(&project_library_refs(project)?)
    }

    fn archive_path(&self, name: &str) -> PathBuf {
        self.libraries_dir.join(name).join(format!("{name}.stlib"))
    }

    fn read_archive(&self, name: &str) -> Result<StlibArchive, String> {
        let path = self.archive_path(name);
        let raw = fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        StlibArchive::parse(&raw, &path.display().to_string())
    }

    fn read_registry(&self) -> Result<LibraryRegistry, String> {
        let path = self.libraries_dir.join(REGISTRY_FILE);
        if !path.exists() {
            return Ok(LibraryRegistry::default());
        }
        let raw = fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        serde_json::from_str(&raw).map_err(|error| format!("parse {}: {error}", path.display()))
    }

    fn write_registry(&self, registry: &LibraryRegistry) -> Result<(), String> {
        fs::create_dir_all(&self.libraries_dir)
            .map_err(|error| format!("create {}: {error}", self.libraries_dir.display()))?;
        let path = self.libraries_dir.join(REGISTRY_FILE);
        let mut raw = serde_json::to_string_pretty(registry)
            .map_err(|error| format!("serialize {}: {error}", path.display()))?;
        raw.push('\n');
        atomic_file::write(&path, raw).map_err(|error| format!("write {}: {error}", path.display()))
    }
}

pub fn project_library_refs(project: &OpenProject) -> Result<Vec<ProjectLibraryRef>, String> {
    let Some(value) = project.project.data.get("libraries") else {
        return Ok(Vec::new());
    };
    let entries = value
        .as_array()
        .ok_or_else(|| "project data.libraries must be an array".to_owned())?;
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| project_library_ref(entry, index))
        .collect()
}

fn project_library_ref(value: &Value, index: usize) -> Result<ProjectLibraryRef, String> {
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            format!("project data.libraries[{index}].name must be a non-empty string")
        })?;
    validate_library_name(name)?;
    let version = value
        .get("version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .map(str::to_owned);
    Ok(ProjectLibraryRef {
        name: name.to_owned(),
        version,
    })
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[usize::from(byte >> 4)] as char);
        output.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    output
}

fn validate_library_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.starts_with('.')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!("Invalid library name {name:?}."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{LibraryManager, ProjectLibraryRef};

    fn demo_stlib(name: &str, version: &str) -> String {
        format!(
            r#"{{
  "formatVersion": 1,
  "manifest": {{
    "name": "{name}", "version": "{version}", "namespace": "demo",
    "functions": [], "functionBlocks": [], "types": [], "globals": [],
    "headers": [], "isBuiltin": false
  }},
  "chunks": [], "dependencies": []
}}"#
        )
    }

    #[test]
    fn installs_lists_and_loads_enabled_stlib() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("demo.stlib");
        fs::write(&source, demo_stlib("demo-lib", "1.2.3")).unwrap();
        let manager = LibraryManager::with_libraries_dir(temp.path().join("pool"));

        let installed = manager.install_stlib(&source).unwrap();
        assert_eq!(installed.name, "demo-lib");
        assert_eq!(installed.version, "1.2.3");
        assert!(installed.archive_path.as_ref().unwrap().is_file());

        let listed = manager.list_installed().unwrap();
        assert_eq!(listed.len(), 6);
        assert_eq!(listed.last().unwrap().name, "demo-lib");

        let loaded = manager
            .load_enabled(&[
                ProjectLibraryRef {
                    name: "demo-lib".to_owned(),
                    version: Some("1.2.3".to_owned()),
                },
                ProjectLibraryRef {
                    name: "missing-lib".to_owned(),
                    version: Some("9.0.0".to_owned()),
                },
            ])
            .unwrap();
        assert_eq!(loaded.archives.len(), 6);
        assert_eq!(loaded.archives.last().unwrap().manifest.name, "demo-lib");
        assert_eq!(loaded.missing.len(), 1);
        assert_eq!(loaded.missing[0].name, "missing-lib");
    }

    #[test]
    fn rejects_path_traversal_manifest_names() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("bad.stlib");
        fs::write(&source, demo_stlib("../escape", "1.0.0")).unwrap();
        let manager = LibraryManager::with_libraries_dir(temp.path().join("pool"));
        let error = manager.install_stlib(&source).unwrap_err();
        assert!(error.contains("Invalid library name"));
        assert!(!temp.path().join("escape").exists());
    }

    #[test]
    fn bundled_catalog_matches_pinned_strucpp_and_is_always_loaded() {
        let temp = tempdir().unwrap();
        let manager = LibraryManager::with_libraries_dir(temp.path().join("pool"));
        let installed = manager.list_installed().unwrap();
        let bundled = installed
            .iter()
            .filter(|library| library.bundled)
            .collect::<Vec<_>>();
        assert_eq!(bundled.len(), 5);
        assert_eq!(bundled[0].name, "additional-function-blocks");
        assert_eq!(bundled[1].name, "iec-standard-fb");
        assert_eq!(bundled[2].name, "iec-std-functions");
        assert_eq!(bundled[3].name, "oscat-basic");
        assert_eq!(bundled[4].name, "plcopen-softmotion");
        assert!(bundled.iter().all(|library| library.archive_path.is_none()));
        let loaded = manager.load_enabled(&[]).unwrap();
        assert_eq!(loaded.archives.len(), 5);
        assert!(loaded.missing.is_empty());
    }

    #[test]
    fn bundled_libraries_cannot_be_shadowed_or_uninstalled() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("iec-standard-fb.stlib");
        fs::write(&source, demo_stlib("iec-standard-fb", "99.0.0")).unwrap();
        let manager = LibraryManager::with_libraries_dir(temp.path().join("pool"));
        let install_error = manager.install_stlib(&source).unwrap_err();
        assert!(install_error.contains("bundled library"));
        let uninstall_error = manager.uninstall("iec-standard-fb").unwrap_err();
        assert!(uninstall_error.contains("Cannot uninstall bundled library"));
    }

    #[test]
    fn user_library_can_be_uninstalled_without_touching_bundled_catalog() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("demo.stlib");
        fs::write(&source, demo_stlib("demo-lib", "1.0.0")).unwrap();
        let manager = LibraryManager::with_libraries_dir(temp.path().join("pool"));
        manager.install_stlib(&source).unwrap();
        manager.uninstall("demo-lib").unwrap();
        let installed = manager.list_installed().unwrap();
        assert_eq!(
            installed.iter().filter(|library| library.bundled).count(),
            5
        );
        assert!(!installed.iter().any(|library| library.name == "demo-lib"));
    }
}
