use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

use crate::atomic_file;
use crate::plc_model::{ParsedPou, PlcVariable, PouBody, PouInterface, PouLanguage, PouType};
use crate::pou_parser::{create_fallback_pou, parse_pou_from_string};

const PROJECT_FILE: &str = "project.json";
const POU_DIRS: &[(&str, PouKind)] = &[
    ("pous/programs", PouKind::Program),
    ("pous/functions", PouKind::Function),
    ("pous/function-blocks", PouKind::FunctionBlock),
];

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("project path does not contain project.json: {0}")]
    InvalidProject(PathBuf),
    #[error("project directory is not empty: {0}")]
    DirectoryNotEmpty(PathBuf),
    #[error("project path is not a directory: {0}")]
    NotDirectory(PathBuf),
    #[error("application data directory is unavailable")]
    NoApplicationData,
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectKind {
    PlcProject,
    PlcLibrary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    St,
    Il,
    Ld,
    Fbd,
}

impl Language {
    pub fn label(self) -> &'static str {
        match self {
            Self::St => "Structured Text",
            Self::Il => "Instruction List",
            Self::Ld => "Ladder Diagram",
            Self::Fbd => "Functional Block Diagram",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::St => "st",
            Self::Il => "il",
            Self::Ld => "ld",
            Self::Fbd => "fbd",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PouKind {
    Program,
    Function,
    FunctionBlock,
}

impl PouKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Program => "Programs",
            Self::Function => "Functions",
            Self::FunctionBlock => "Function Blocks",
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewProjectRequest {
    pub kind: ProjectKind,
    pub name: String,
    pub path: PathBuf,
    pub language: Language,
    pub interval: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenProject {
    pub root: PathBuf,
    pub project: ProjectDocument,
    pub pous: Vec<PouFile>,
    pub warnings: Vec<String>,
    pub device_configuration: Value,
    pub device_pin_mapping: Value,
    pub library_manifest: Option<String>,
}

impl OpenProject {
    pub fn name(&self) -> &str {
        &self.project.meta.name
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectDocument {
    pub meta: ProjectMeta,
    pub data: Value,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectMeta {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ProjectKind,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PouFile {
    pub name: String,
    pub kind: PouKind,
    pub language: Option<PouLanguage>,
    pub relative_path: PathBuf,
    pub absolute_path: PathBuf,
    pub parsed: Option<ParsedPou>,
    pub variables_text: Option<String>,
    pub raw_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub project_file_path: PathBuf,
    pub created_at: String,
    pub last_opened_at: String,
}

#[derive(Clone)]
pub struct ProjectService {
    history_path: PathBuf,
}

impl ProjectService {
    #[cfg(test)]
    pub(crate) fn with_history_path(history_path: PathBuf) -> Self {
        Self { history_path }
    }

    pub fn new() -> Result<Self, ProjectError> {
        let base = BaseDirs::new().ok_or(ProjectError::NoApplicationData)?;
        let history_path = crate::app_paths::data_root()
            .map_err(|_| ProjectError::NoApplicationData)?
            .join("User")
            .join("History")
            .join("projects.json");
        let legacy_history_path = base
            .data_dir()
            .join("open-plc-editor")
            .join("User")
            .join("History")
            .join("projects.json");
        migrate_legacy_history(&legacy_history_path, &history_path)?;
        Ok(Self { history_path })
    }

    pub fn recent_projects(&self) -> Result<Vec<RecentProject>, ProjectError> {
        if !self.history_path.exists() {
            return Ok(Vec::new());
        }
        let bytes = fs::read(&self.history_path)?;
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        let mut projects: Vec<RecentProject> = serde_json::from_slice(&bytes)?;
        projects.retain(|project| project.path.join(PROJECT_FILE).is_file());
        Ok(projects)
    }

    pub fn open(&self, path: impl AsRef<Path>) -> Result<OpenProject, ProjectError> {
        let root = normalize_root(path.as_ref());
        if !root.is_dir() {
            return Err(ProjectError::NotDirectory(root));
        }
        let project_path = root.join(PROJECT_FILE);
        if !project_path.is_file() {
            return Err(ProjectError::InvalidProject(root));
        }

        let mut warnings = Vec::new();
        let mut project = match serde_json::from_slice::<ProjectDocument>(&fs::read(&project_path)?)
        {
            Ok(project) => project,
            Err(_) => {
                warnings.push(
                    "project.json is malformed and could not be read. Using defaults.".to_owned(),
                );
                default_project_document()
            }
        };
        let device_configuration = read_json_or_default(
            &root.join("devices/configuration.json"),
            default_device_configuration(),
            "devices/configuration.json",
            &mut warnings,
        );
        let device_pin_mapping = read_json_or_default(
            &root.join("devices/pin-mapping.json"),
            json!({}),
            "devices/pin-mapping.json",
            &mut warnings,
        );
        let library_manifest = (project.meta.kind == ProjectKind::PlcLibrary)
            .then(|| fs::read_to_string(root.join("library.json")).unwrap_or_default());
        let servers = read_json_directory(&root.join("devices/servers"), "server", &mut warnings)?;
        let remote_devices =
            read_json_directory(&root.join("devices/remote"), "remote device", &mut warnings)?;
        if let Some(data) = project.data.as_object_mut() {
            data.insert("servers".to_owned(), Value::Array(servers));
            data.insert("remoteDevices".to_owned(), Value::Array(remote_devices));
        }
        let (mut pous, mut pou_warnings) = scan_pous(&root)?;
        warnings.append(&mut pou_warnings);
        restore_debug_flags(&mut project, &mut pous);

        self.record_recent(&root, &project.meta.name)?;
        Ok(OpenProject {
            root,
            project,
            pous,
            warnings,
            device_configuration,
            device_pin_mapping,
            library_manifest,
        })
    }

    pub fn create(&self, request: &NewProjectRequest) -> Result<OpenProject, ProjectError> {
        if request.name.trim().is_empty() {
            return Err(ProjectError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "project name is required",
            )));
        }
        if request.path.exists() {
            if !request.path.is_dir() {
                return Err(ProjectError::NotDirectory(request.path.clone()));
            }
            if fs::read_dir(&request.path)?.next().is_some() {
                return Err(ProjectError::DirectoryNotEmpty(request.path.clone()));
            }
        } else {
            fs::create_dir_all(&request.path)?;
        }

        for relative in [
            "devices",
            "devices/servers",
            "devices/remote",
            "pous/functions",
            "pous/function-blocks",
            "pous/programs",
        ] {
            fs::create_dir_all(request.path.join(relative))?;
        }

        let project_value = create_project_json(request);
        write_json(request.path.join(PROJECT_FILE), &project_value)?;
        write_json(
            request.path.join("devices/configuration.json"),
            &json!({
                "deviceBoard": "OpenPLC Simulator",
                "communicationPort": "",
                "runtimeIpAddress": "",
                "allowInsecureRuntimeTls": false,
                "vendorScreenData": {},
                "vendorScreenDataByBoard": {},
                "selectedPlatformOptions": {}
            }),
        )?;
        write_json(request.path.join("devices/pin-mapping.json"), &json!({}))?;

        match request.kind {
            ProjectKind::PlcProject => {
                let path = request
                    .path
                    .join("pous/programs")
                    .join(format!("main.{}", request.language.extension()));
                atomic_file::write(&path, default_main_pou(request.language))?;
            }
            ProjectKind::PlcLibrary => {
                let manifest = json!({
                    "name": request.name,
                    "displayName": request.name,
                    "version": "0.1.0",
                    "namespace": namespace(&request.name),
                    "description": ""
                });
                let mut content = serde_json::to_string_pretty(&manifest)?;
                content.push('\n');
                atomic_file::write(&request.path.join("library.json"), content)?;
            }
        }

        self.open(&request.path)
    }

    pub fn remove_recent_project(&self, project_path: &Path) -> Result<(), ProjectError> {
        let projects = self
            .recent_projects()?
            .into_iter()
            .filter(|project| project.path != project_path)
            .collect::<Vec<_>>();
        if let Some(parent) = self.history_path.parent() {
            fs::create_dir_all(parent)?;
        }
        atomic_file::write(&self.history_path, serde_json::to_vec_pretty(&projects)?)?;
        Ok(())
    }

    pub fn save_text(&self, file: &PouFile, content: &str) -> Result<(), ProjectError> {
        atomic_file::write(&file.absolute_path, content)?;
        Ok(())
    }

    pub fn save_project_file(
        &self,
        project_root: &Path,
        relative_path: &str,
        content: &str,
    ) -> Result<(), ProjectError> {
        let path = project_root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        atomic_file::write(&path, content)?;
        Ok(())
    }

    pub fn delete_project_file(
        &self,
        project_root: &Path,
        relative_path: &str,
    ) -> Result<(), ProjectError> {
        let path = project_root.join(relative_path);
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn record_recent(&self, root: &Path, name: &str) -> Result<(), ProjectError> {
        let mut projects = self.recent_projects().unwrap_or_default();
        let now = iso_timestamp();
        let root = root.to_path_buf();
        let project_file_path = root.join(PROJECT_FILE);
        if let Some(existing) = projects.iter_mut().find(|project| project.path == root) {
            existing.name = name.to_owned();
            existing.project_file_path = project_file_path;
            existing.last_opened_at = now;
        } else {
            projects.push(RecentProject {
                name: name.to_owned(),
                path: root,
                project_file_path,
                created_at: now.clone(),
                last_opened_at: now,
            });
        }
        projects.sort_by(|a, b| b.last_opened_at.cmp(&a.last_opened_at));
        if let Some(parent) = self.history_path.parent() {
            fs::create_dir_all(parent)?;
        }
        atomic_file::write(&self.history_path, serde_json::to_vec_pretty(&projects)?)?;
        Ok(())
    }
}

fn restore_debug_flags(project: &mut ProjectDocument, pous: &mut [PouFile]) {
    let Some(debug_variables) = project.project_debug_variables().cloned() else {
        return;
    };

    if let Some(global_names) = debug_variables.get("global").and_then(Value::as_array) {
        let watched = global_names
            .iter()
            .filter_map(Value::as_str)
            .collect::<std::collections::HashSet<_>>();
        if let Some(globals) = project
            .data
            .get_mut("configuration")
            .and_then(|value| value.get_mut("resource"))
            .and_then(|value| value.get_mut("globalVariables"))
            .and_then(Value::as_array_mut)
        {
            for global in globals {
                let Some(name) = global.get("name").and_then(Value::as_str) else {
                    continue;
                };
                if watched.contains(name) {
                    if let Some(object) = global.as_object_mut() {
                        object.insert("debug".to_owned(), Value::Bool(true));
                    }
                }
            }
        }
    }

    if let Some(pou_debug) = debug_variables.get("pous").and_then(Value::as_object) {
        for file in pous {
            let Some(parsed) = file.parsed.as_mut() else {
                continue;
            };
            let Some(names) = pou_debug.get(&parsed.name).and_then(Value::as_array) else {
                continue;
            };
            let watched = names
                .iter()
                .filter_map(Value::as_str)
                .collect::<std::collections::HashSet<_>>();
            for variable in &mut parsed.interface.variables {
                if watched.contains(variable.name.as_str()) {
                    variable.debug = Some(true);
                }
            }
        }
    }
}

impl ProjectDocument {
    fn project_debug_variables(&self) -> Option<&Value> {
        self.data.get("debugVariables")
    }
}

pub(crate) fn sync_debug_variable_metadata(project: &mut OpenProject) {
    let global_names = project
        .project
        .data
        .get("configuration")
        .and_then(|value| value.get("resource"))
        .and_then(|value| value.get("globalVariables"))
        .and_then(Value::as_array)
        .map(|globals| {
            globals
                .iter()
                .filter(|global| global.get("debug").and_then(Value::as_bool) == Some(true))
                .filter_map(|global| global.get("name").and_then(Value::as_str))
                .map(|name| Value::String(name.to_owned()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut pou_debug = serde_json::Map::new();
    for file in &project.pous {
        let Some(parsed) = file.parsed.as_ref() else {
            continue;
        };
        let names = parsed
            .interface
            .variables
            .iter()
            .filter(|variable| variable.debug == Some(true))
            .map(|variable| Value::String(variable.name.clone()))
            .collect::<Vec<_>>();
        if !names.is_empty() {
            pou_debug.insert(parsed.name.clone(), Value::Array(names));
        }
    }

    let mut debug_variables = serde_json::Map::new();
    if !global_names.is_empty() {
        debug_variables.insert("global".to_owned(), Value::Array(global_names));
    }
    if !pou_debug.is_empty() {
        debug_variables.insert("pous".to_owned(), Value::Object(pou_debug));
    }

    if let Some(data) = project.project.data.as_object_mut() {
        if debug_variables.is_empty() {
            data.remove("debugVariables");
        } else {
            data.insert("debugVariables".to_owned(), Value::Object(debug_variables));
        }
    }
}

fn normalize_root(path: &Path) -> PathBuf {
    if path.file_name().is_some_and(|name| name == PROJECT_FILE) {
        path.parent().unwrap_or(path).to_path_buf()
    } else {
        path.to_path_buf()
    }
}

#[derive(Debug, Clone)]
struct RawPouCandidate {
    name: String,
    kind: PouKind,
    extension: String,
    relative_path: PathBuf,
    absolute_path: PathBuf,
    raw_content: String,
}

fn scan_pous(root: &Path) -> Result<(Vec<PouFile>, Vec<String>), ProjectError> {
    let mut candidates = Vec::new();
    for (directory, kind) in POU_DIRS {
        scan_pou_directory(root, &root.join(directory), *kind, &mut candidates)?;
    }

    // Match OpenPLC's deduplicatePouFiles: a text POU replaces a legacy
    // JSON POU with the same basename, while an already-present text POU wins.
    let mut by_name: HashMap<String, (usize, bool)> = HashMap::new();
    let mut deduped: Vec<RawPouCandidate> = Vec::new();
    for candidate in candidates {
        let text_based = candidate.extension != "json";
        if let Some((index, existing_text_based)) = by_name.get(&candidate.name).copied() {
            if text_based && !existing_text_based {
                deduped[index] = candidate.clone();
                by_name.insert(candidate.name.clone(), (index, true));
            }
        } else {
            let index = deduped.len();
            by_name.insert(candidate.name.clone(), (index, text_based));
            deduped.push(candidate);
        }
    }

    let function_blocks = deduped
        .iter()
        .filter(|candidate| candidate.kind == PouKind::FunctionBlock)
        .map(|candidate| candidate.name.clone())
        .collect::<Vec<_>>();
    let mut warnings = Vec::new();
    let mut files = Vec::new();

    for candidate in deduped {
        let pou_type = pou_type(candidate.kind);
        let (parsed, variables_text) = if candidate.extension == "json" {
            let Some(parsed) = parse_json_pou(&candidate.raw_content, pou_type) else {
                continue;
            };
            (parsed, None)
        } else {
            let Some(language) = PouLanguage::from_extension(&candidate.extension) else {
                continue;
            };
            match parse_pou_from_string(
                &candidate.raw_content,
                language,
                pou_type,
                &function_blocks,
            ) {
                Ok(parsed) => (parsed, None),
                Err(reason) => {
                    let relative = candidate.relative_path.to_string_lossy();
                    if language.is_textual() {
                        warnings.push(format!(
                            "POU \"{}\" ({relative}) could not be fully parsed: {reason} Its variable declarations were preserved as raw text — open the POU's variables editor in code view, fix the declaration, and save.",
                            candidate.name
                        ));
                    } else {
                        warnings.push(format!(
                            "POU \"{}\" ({relative}) could not be fully parsed and was loaded with partial data: {reason}",
                            candidate.name
                        ));
                    }
                    let fallback = create_fallback_pou(
                        &candidate.raw_content,
                        language,
                        pou_type,
                        &candidate.name,
                    );
                    (fallback.pou, Some(fallback.variables_text))
                }
            }
        };
        let name = if parsed.name.is_empty() {
            candidate.name.clone()
        } else {
            parsed.name.clone()
        };
        files.push(PouFile {
            name,
            kind: candidate.kind,
            language: Some(parsed.body.language),
            relative_path: candidate.relative_path,
            absolute_path: candidate.absolute_path,
            parsed: Some(parsed),
            variables_text,
            raw_content: candidate.raw_content,
        });
    }

    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok((files, warnings))
}

fn scan_pou_directory(
    root: &Path,
    directory: &Path,
    kind: PouKind,
    files: &mut Vec<RawPouCandidate>,
) -> Result<(), ProjectError> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            scan_pou_directory(root, &path, kind, files)?;
            continue;
        }
        if !entry.file_type()?.is_file() {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(
            extension.as_str(),
            "st" | "il" | "ld" | "fbd" | "py" | "cpp" | "json"
        ) {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown")
            .to_owned();
        files.push(RawPouCandidate {
            name,
            kind,
            extension,
            relative_path: path.strip_prefix(root).unwrap_or(&path).to_path_buf(),
            absolute_path: path.clone(),
            raw_content: fs::read_to_string(path)?,
        });
    }
    Ok(())
}

fn pou_type(kind: PouKind) -> PouType {
    match kind {
        PouKind::Program => PouType::Program,
        PouKind::Function => PouType::Function,
        PouKind::FunctionBlock => PouType::FunctionBlock,
    }
}

fn parse_json_pou(content: &str, fallback_type: PouType) -> Option<ParsedPou> {
    let value = fold_legacy_variable_aliases(serde_json::from_str::<Value>(content).ok()?);
    if value.get("type").is_some() && value.get("data").is_some() {
        let data = value.get("data")?;
        let pou_type = value
            .get("type")
            .and_then(Value::as_str)
            .and_then(parse_pou_type)
            .unwrap_or(fallback_type);
        let body = parse_json_body(data.get("body")?)?;
        let variables = data
            .get("variables")
            .cloned()
            .and_then(|variables| serde_json::from_value::<Vec<PlcVariable>>(variables).ok())
            .unwrap_or_default();
        return Some(ParsedPou {
            name: data
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            pou_type,
            interface: PouInterface {
                return_type: data
                    .get("returnType")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                variables,
            },
            body,
            documentation: data
                .get("documentation")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        });
    }

    let object = value.as_object()?;
    let body = parse_json_body(object.get("body")?)?;
    let interface = object.get("interface").and_then(Value::as_object);
    let variables = interface
        .and_then(|interface| interface.get("variables"))
        .cloned()
        .and_then(|variables| serde_json::from_value::<Vec<PlcVariable>>(variables).ok())
        .unwrap_or_default();
    Some(ParsedPou {
        name: object
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        pou_type: object
            .get("pouType")
            .and_then(Value::as_str)
            .and_then(parse_pou_type)
            .unwrap_or(fallback_type),
        interface: PouInterface {
            return_type: interface
                .and_then(|interface| interface.get("returnType"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            variables,
        },
        body,
        documentation: object
            .get("documentation")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

fn parse_json_body(value: &Value) -> Option<PouBody> {
    let language = value
        .get("language")
        .and_then(Value::as_str)
        .and_then(parse_pou_language)?;
    Some(PouBody {
        language,
        value: value
            .get("value")
            .cloned()
            .unwrap_or(Value::String(String::new())),
    })
}

fn parse_pou_type(value: &str) -> Option<PouType> {
    match value.to_ascii_lowercase().as_str() {
        "program" => Some(PouType::Program),
        "function" => Some(PouType::Function),
        "function-block" => Some(PouType::FunctionBlock),
        _ => None,
    }
}

fn parse_pou_language(value: &str) -> Option<PouLanguage> {
    match value.to_ascii_lowercase().as_str() {
        "st" => Some(PouLanguage::St),
        "il" => Some(PouLanguage::Il),
        "ld" => Some(PouLanguage::Ld),
        "fbd" => Some(PouLanguage::Fbd),
        "python" => Some(PouLanguage::Python),
        "cpp" => Some(PouLanguage::Cpp),
        _ => None,
    }
}

fn fold_legacy_variable_aliases(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(fold_legacy_variable_aliases)
                .collect(),
        ),
        Value::Object(object) => {
            let is_legacy_alias_bound = object.get("location").and_then(Value::as_str).is_some()
                && object
                    .get("alias")
                    .and_then(Value::as_str)
                    .is_some_and(|alias| !alias.is_empty());
            let alias = object
                .get("alias")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let mut migrated = serde_json::Map::new();
            for (key, child) in object {
                if is_legacy_alias_bound && key == "alias" {
                    continue;
                }
                if is_legacy_alias_bound && key == "location" {
                    migrated.insert(key, Value::String(alias.clone().unwrap_or_default()));
                } else {
                    migrated.insert(key, fold_legacy_variable_aliases(child));
                }
            }
            Value::Object(migrated)
        }
        other => other,
    }
}

fn default_project_document() -> ProjectDocument {
    ProjectDocument {
        meta: ProjectMeta {
            name: String::new(),
            kind: ProjectKind::PlcProject,
            extra: serde_json::Map::new(),
        },
        data: json!({
            "pous": [],
            "dataTypes": [],
            "libraries": [],
            "configuration": {
                "resource": {
                    "tasks": [],
                    "instances": [],
                    "globalVariables": []
                }
            }
        }),
        extra: serde_json::Map::new(),
    }
}

fn default_device_configuration() -> Value {
    json!({
        "deviceBoard": "OpenPLC Simulator",
        "communicationPort": "",
        "runtimeIpAddress": "",
        "allowInsecureRuntimeTls": false,
        "vendorScreenData": {},
        "vendorScreenDataByBoard": {},
        "selectedPlatformOptions": {}
    })
}

fn read_json_directory(
    directory: &Path,
    kind: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<Value>, ProjectError> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut paths = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_file()))
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        })
        .collect::<Vec<_>>();
    paths.sort();

    let mut values = Vec::new();
    for path in paths {
        match fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
            .filter(Value::is_object)
        {
            Some(value) => values.push(value),
            None => warnings.push(format!(
                "{} {} is malformed and was skipped.",
                kind,
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("unknown.json")
            )),
        }
    }
    Ok(values)
}

pub(crate) fn project_document_for_disk(project: &ProjectDocument) -> ProjectDocument {
    let mut project = project.clone();
    if let Some(data) = project.data.as_object_mut() {
        data.remove("servers");
        data.remove("remoteDevices");
    }
    project
}

fn read_json_or_default(
    path: &Path,
    default: Value,
    display_name: &str,
    warnings: &mut Vec<String>,
) -> Value {
    let Ok(content) = fs::read_to_string(path) else {
        return default;
    };
    if content.trim().is_empty() {
        return default;
    }
    match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(_) => {
            warnings.push(format!(
                "{display_name} is malformed and could not be read. Using defaults."
            ));
            default
        }
    }
}

fn create_project_json(request: &NewProjectRequest) -> Value {
    let resource = match request.kind {
        ProjectKind::PlcProject => json!({
            "tasks": [{
                "name": "task0",
                "triggering": "Cyclic",
                "interval": request.interval,
                "priority": 1
            }],
            "instances": [{
                "name": "instance0",
                "program": "main",
                "task": "task0"
            }],
            "globalVariables": []
        }),
        ProjectKind::PlcLibrary => json!({
            "tasks": [],
            "instances": [],
            "globalVariables": []
        }),
    };
    json!({
        "meta": {
            "name": request.name,
            "type": request.kind
        },
        "data": {
            "pous": [],
            "dataTypes": [],
            "libraries": [],
            "configuration": {
                "resource": resource
            }
        }
    })
}

fn default_main_pou(language: Language) -> String {
    match language {
        Language::St | Language::Il => {
            "PROGRAM main\n  VAR\n  END_VAR\n\n\n\nEND_PROGRAM".to_owned()
        }
        Language::Ld => {
            let body = serde_json::to_string_pretty(&json!({
                "name": "main",
                "rungs": []
            }))
            .expect("static ladder JSON must serialize");
            format!("PROGRAM main\n  VAR\n  END_VAR\n\n{body}\nEND_PROGRAM")
        }
        Language::Fbd => {
            let body = serde_json::to_string_pretty(&json!({
                "name": "main",
                "rung": {
                    "comment": "",
                    "edges": [],
                    "nodes": []
                }
            }))
            .expect("static FBD JSON must serialize");
            format!("PROGRAM main\n  VAR\n  END_VAR\n\n{body}\nEND_PROGRAM")
        }
    }
}

fn write_json(path: PathBuf, value: &Value) -> Result<(), ProjectError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_file::write(&path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn namespace(name: &str) -> String {
    let mut result = String::new();
    let mut separator = false;
    for character in name.trim().chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if separator && !result.is_empty() {
                result.push('_');
            }
            separator = false;
            result.push(character);
        } else {
            separator = true;
        }
    }
    if result.is_empty() {
        return "lib".to_owned();
    }
    if result.as_bytes()[0].is_ascii_digit() {
        result.insert(0, '_');
    }
    result
}

fn iso_timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("RFC3339 formatting must succeed")
}

fn migrate_legacy_history(legacy_path: &Path, rust_path: &Path) -> Result<(), ProjectError> {
    if rust_path.exists() || !legacy_path.is_file() {
        return Ok(());
    }
    let bytes = match fs::read(legacy_path) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(()),
    };
    if bytes.is_empty() || serde_json::from_slice::<Vec<RecentProject>>(&bytes).is_err() {
        return Ok(());
    }
    if let Some(parent) = rust_path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_file::write(rust_path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_app_history_migrates_once_without_sharing_write_state() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp
            .path()
            .join("open-plc-editor/User/History/projects.json");
        let rust = temp
            .path()
            .join("open-plc-editor-r/User/History/projects.json");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        let projects = vec![RecentProject {
            name: "Demo".to_owned(),
            path: PathBuf::from("/tmp/demo"),
            project_file_path: PathBuf::from("/tmp/demo/project.json"),
            created_at: "2026-09-03T00:00:00Z".to_owned(),
            last_opened_at: "2026-09-03T00:00:00Z".to_owned(),
        }];
        fs::write(&legacy, serde_json::to_vec(&projects).unwrap()).unwrap();

        migrate_legacy_history(&legacy, &rust).unwrap();
        assert_eq!(fs::read(&rust).unwrap(), fs::read(&legacy).unwrap());

        fs::write(&rust, b"[]").unwrap();
        migrate_legacy_history(&legacy, &rust).unwrap();
        assert_eq!(fs::read(&rust).unwrap(), b"[]");
    }

    #[test]
    fn namespace_matches_editor_rules() {
        assert_eq!(namespace("My  Cool Lib"), "my_cool_lib");
        assert_eq!(namespace("Sensor-Tools"), "sensor_tools");
        assert_eq!(namespace("123foo"), "_123foo");
        assert_eq!(namespace(""), "lib");
    }

    #[test]
    fn default_st_program_matches_editor_shape() {
        assert_eq!(
            default_main_pou(Language::St),
            "PROGRAM main\n  VAR\n  END_VAR\n\n\n\nEND_PROGRAM"
        );
    }

    #[test]
    fn default_graphical_programs_match_editor_serializer() {
        assert_eq!(
            default_main_pou(Language::Ld),
            "PROGRAM main\n  VAR\n  END_VAR\n\n{\n  \"name\": \"main\",\n  \"rungs\": []\n}\nEND_PROGRAM"
        );
        assert_eq!(
            default_main_pou(Language::Fbd),
            "PROGRAM main\n  VAR\n  END_VAR\n\n{\n  \"name\": \"main\",\n  \"rung\": {\n    \"comment\": \"\",\n    \"edges\": [],\n    \"nodes\": []\n  }\n}\nEND_PROGRAM"
        );
    }

    #[test]
    fn fresh_plc_project_matches_openplc_file_shape() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("MyProject");
        let service = ProjectService {
            history_path: temp.path().join("User/History/projects.json"),
        };
        let request = NewProjectRequest {
            kind: ProjectKind::PlcProject,
            name: "MyProject".to_owned(),
            path: root.clone(),
            language: Language::St,
            interval: "T#50ms".to_owned(),
        };
        service.create(&request).unwrap();

        for directory in [
            "devices",
            "devices/servers",
            "devices/remote",
            "pous/functions",
            "pous/function-blocks",
            "pous/programs",
        ] {
            assert!(root.join(directory).is_dir(), "missing {directory}");
        }

        let project: Value =
            serde_json::from_slice(&fs::read(root.join("project.json")).unwrap()).unwrap();
        let resource = &project["data"]["configuration"]["resource"];
        assert_eq!(resource["tasks"][0]["name"], "task0");
        assert_eq!(resource["tasks"][0]["triggering"], "Cyclic");
        assert_eq!(resource["tasks"][0]["interval"], "T#50ms");
        assert_eq!(resource["instances"][0]["name"], "instance0");
        assert_eq!(resource["instances"][0]["program"], "main");
        assert_eq!(resource["instances"][0]["task"], "task0");
        assert_eq!(resource["globalVariables"], json!([]));

        let config: Value =
            serde_json::from_slice(&fs::read(root.join("devices/configuration.json")).unwrap())
                .unwrap();
        assert_eq!(config["deviceBoard"], "OpenPLC Simulator");
        assert_eq!(config["communicationPort"], "");
        assert_eq!(config["runtimeIpAddress"], "");
        assert_eq!(config["vendorScreenData"], json!({}));
        assert_eq!(config["vendorScreenDataByBoard"], json!({}));
        assert_eq!(config["selectedPlatformOptions"], json!({}));
        assert_eq!(
            serde_json::from_slice::<Value>(
                &fs::read(root.join("devices/pin-mapping.json")).unwrap()
            )
            .unwrap(),
            json!({})
        );
        assert_eq!(
            fs::read_to_string(root.join("pous/programs/main.st")).unwrap(),
            "PROGRAM main\n  VAR\n  END_VAR\n\n\n\nEND_PROGRAM"
        );
        assert!(!root.join("library.json").exists());
    }

    #[test]
    fn debug_variable_metadata_round_trips_like_openplc() {
        let root = PathBuf::from("/tmp/debug-metadata");
        let mut document = ProjectDocument {
            meta: ProjectMeta {
                name: "DebugMetadata".to_owned(),
                kind: ProjectKind::PlcProject,
                extra: serde_json::Map::new(),
            },
            data: json!({
                "debugVariables": {
                    "global": ["gWatch"],
                    "pous": {"main": ["x"]}
                },
                "configuration": {"resource": {
                    "tasks": [],
                    "instances": [],
                    "globalVariables": [{
                        "name": "gWatch",
                        "type": {"definition":"base-type","value":"BOOL"},
                        "debug": false
                    }]
                }}
            }),
            extra: serde_json::Map::new(),
        };
        let parsed = ParsedPou {
            name: "main".to_owned(),
            pou_type: PouType::Program,
            interface: PouInterface {
                return_type: None,
                variables: vec![PlcVariable {
                    name: "x".to_owned(),
                    class: Some(crate::plc_model::VariableClass::Local),
                    variable_type: crate::plc_model::PlcVariableType::base("INT"),
                    location: String::new(),
                    initial_value: None,
                    documentation: String::new(),
                    debug: Some(false),
                }],
            },
            body: PouBody::text(PouLanguage::St, ""),
            documentation: String::new(),
        };
        let mut pous = vec![PouFile {
            name: "main".to_owned(),
            kind: PouKind::Program,
            language: Some(PouLanguage::St),
            relative_path: PathBuf::from("pous/programs/main.st"),
            absolute_path: root.join("pous/programs/main.st"),
            parsed: Some(parsed),
            variables_text: None,
            raw_content: String::new(),
        }];

        restore_debug_flags(&mut document, &mut pous);
        assert_eq!(
            pous[0].parsed.as_ref().unwrap().interface.variables[0].debug,
            Some(true)
        );
        assert_eq!(
            document.data["configuration"]["resource"]["globalVariables"][0]["debug"],
            true
        );

        let mut project = OpenProject {
            root,
            project: document,
            pous,
            warnings: Vec::new(),
            device_configuration: json!({}),
            device_pin_mapping: json!({}),
            library_manifest: None,
        };
        sync_debug_variable_metadata(&mut project);
        assert_eq!(
            project.project.data["debugVariables"],
            json!({"global":["gWatch"],"pous":{"main":["x"]}})
        );
    }

    #[test]
    fn fresh_library_matches_openplc_file_shape() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Sensor Tools");
        let service = ProjectService {
            history_path: temp.path().join("User/History/projects.json"),
        };
        let request = NewProjectRequest {
            kind: ProjectKind::PlcLibrary,
            name: "Sensor Tools".to_owned(),
            path: root.clone(),
            language: Language::St,
            interval: "T#20ms".to_owned(),
        };
        service.create(&request).unwrap();

        let project: Value =
            serde_json::from_slice(&fs::read(root.join("project.json")).unwrap()).unwrap();
        assert_eq!(project["meta"]["type"], "plc-library");
        let resource = &project["data"]["configuration"]["resource"];
        assert_eq!(resource["tasks"], json!([]));
        assert_eq!(resource["instances"], json!([]));
        assert_eq!(resource["globalVariables"], json!([]));
        assert!(root.join("devices/configuration.json").is_file());
        assert!(root.join("devices/pin-mapping.json").is_file());
        assert!(root.join("pous/programs").is_dir());
        assert!(fs::read_dir(root.join("pous/programs"))
            .unwrap()
            .next()
            .is_none());

        let manifest = fs::read_to_string(root.join("library.json")).unwrap();
        assert!(manifest.ends_with('\n'));
        assert_eq!(
            serde_json::from_str::<Value>(&manifest).unwrap(),
            json!({
                "name": "Sensor Tools",
                "displayName": "Sensor Tools",
                "version": "0.1.0",
                "namespace": "sensor_tools",
                "description": ""
            })
        );
    }
}
