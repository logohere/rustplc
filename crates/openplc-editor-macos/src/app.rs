use std::collections::HashMap;
use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use dioxus_desktop::use_window;
use open_rust_plc_compat_core::lifecycle::PlcState;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::icons::{
    ArrowIcon, FolderIcon, MagnifierIcon, OpenPLCIcon, PlusIcon, StickArrowIcon, VideoIcon,
};
use crate::library_manager::LibraryManager;
use crate::local_runtime::LocalPlcService;
use crate::new_project::ProjectModal;
use crate::plc_model::{PouLanguage, PouType};
use crate::pou_serializer::serialize_pou_to_text;
use crate::project::{
    project_document_for_disk, sync_debug_variable_metadata, OpenProject, PouFile, PouKind,
    ProjectKind, ProjectService, RecentProject,
};
use crate::protocol_config::{default_remote_device, default_server};
use crate::remote_debug::RemoteDebugSession;
use crate::runtime_v4_client::RuntimeV4Client;
use crate::save_modals::SaveChangesModals;
use crate::vendor_screen::{
    owned_slice, persistence_keys, replace_owned_slice, set_project_vendor_slice,
};
use crate::vpp_package::VppPackageManager;
use crate::workspace::WorkspaceScreen;

const OPENPLC_CSS: &str = include_str!("../assets/openplc.css");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    Start,
    Workspace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TabTarget {
    Pou(String),
    DataType(String),
    Server(String),
    RemoteDevice(String),
    Resource,
    DeviceConfiguration,
    LibraryManager,
    UserManagement,
    VendorScreen(String),
    Manifest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditorTab {
    pub name: String,
    pub target: TabTarget,
    pub kind: Option<PouKind>,
    pub language: Option<PouLanguage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConsoleEntry {
    pub level: &'static str,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BottomPanel {
    Console,
    Variables,
    Search,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SearchTarget {
    Pou(String),
    DataType(String),
    Resource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchHit {
    pub target: SearchTarget,
    pub category: String,
    pub label: String,
    pub context: String,
    pub matches: usize,
}

impl SearchHit {
    pub(crate) fn pou(
        name: String,
        category: &str,
        label: String,
        context: &str,
        matches: usize,
    ) -> Self {
        Self {
            target: SearchTarget::Pou(name),
            category: category.to_owned(),
            label,
            context: context.to_owned(),
            matches,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchResult {
    pub id: String,
    pub query: String,
    pub project_name: String,
    pub total_matches: usize,
    pub hits: Vec<SearchHit>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EditorDocument {
    pub content: String,
    pub clean: String,
    pub clean_pou_file: Option<PouFile>,
    pub structural_dirty: bool,
}

impl EditorDocument {
    fn new(content: String, clean_pou_file: Option<PouFile>) -> Self {
        Self {
            clean: content.clone(),
            content,
            clean_pou_file,
            structural_dirty: false,
        }
    }

    pub(crate) fn dirty(&self) -> bool {
        self.structural_dirty || self.content != self.clean
    }
}

#[derive(Clone, Copy)]
pub(crate) struct AppStore {
    pub(crate) screen: Signal<Screen>,
    pub(crate) project: Signal<Option<OpenProject>>,
    pub(crate) recent: Signal<Vec<RecentProject>>,
    pub(crate) create_project_open: Signal<bool>,
    pub(crate) tabs: Signal<Vec<EditorTab>>,
    pub(crate) selected_tab: Signal<Option<String>>,
    pub(crate) editor_documents: Signal<HashMap<String, EditorDocument>>,
    pub(crate) pending_close_tab: Signal<Option<String>>,
    pub(crate) project_close_modal: Signal<bool>,
    pub(crate) console: Signal<Vec<ConsoleEntry>>,
    pub(crate) bottom_panel: Signal<BottomPanel>,
    pub(crate) search_dialog_open: Signal<bool>,
    pub(crate) search_results: Signal<Vec<SearchResult>>,
    pub(crate) plc_state: Signal<PlcState>,
    pub(crate) remote_debug: Signal<Option<RemoteDebugSession>>,
    pub(crate) runtime_client: Signal<Option<RuntimeV4Client>>,
    pub(crate) runtime_connected: Signal<bool>,
    pub(crate) runtime_version: Signal<Option<String>>,
    pub(crate) is_compiling: Signal<bool>,
}

impl AppStore {
    pub(crate) fn load_project(&mut self, mut project: OpenProject) {
        self.clear_remote_debug();
        self.clear_runtime_connection();
        if let Ok(manager) = VppPackageManager::new() {
            for package_id in manager.verify_installed_signatures() {
                project.warnings.push(format!(
                    "VPP package {package_id} was removed because its signature or installed contents no longer verify."
                ));
            }
        }
        let mut console = project
            .warnings
            .iter()
            .cloned()
            .map(|message| ConsoleEntry {
                level: "warning",
                message,
            })
            .collect::<Vec<_>>();
        console.push(ConsoleEntry {
            level: "info",
            message: format!("Opened project {}", project.root.display()),
        });

        let default_tab = if project.project.meta.kind == ProjectKind::PlcLibrary {
            Some(EditorTab {
                name: "Manifest".to_owned(),
                target: TabTarget::Manifest,
                kind: None,
                language: None,
            })
        } else {
            project
                .pous
                .iter()
                .find(|file| {
                    file.name == "main"
                        && file
                            .parsed
                            .as_ref()
                            .is_some_and(|pou| pou.pou_type == PouType::Program)
                })
                .or_else(|| {
                    project.pous.iter().find(|file| {
                        file.parsed
                            .as_ref()
                            .is_some_and(|pou| pou.pou_type == PouType::Program)
                    })
                })
                .map(tab_for_pou)
        };

        let mut editor_documents = HashMap::new();
        if let Some(tab) = default_tab.as_ref() {
            editor_documents.insert(
                tab.name.clone(),
                EditorDocument::new(tab_buffer(&project, tab), tab_clean_pou(&project, tab)),
            );
        }
        self.tabs
            .set(default_tab.clone().into_iter().collect::<Vec<_>>());
        self.selected_tab
            .set(default_tab.as_ref().map(|tab| tab.name.clone()));
        self.editor_documents.set(editor_documents);
        self.console.set(console);
        self.bottom_panel.set(BottomPanel::Console);
        self.search_dialog_open.set(false);
        self.search_results.set(Vec::new());
        self.project.set(Some(project));
        self.screen.set(Screen::Workspace);
    }

    pub(crate) fn open_pou(&mut self, name: &str) {
        let Some(project) = (self.project)() else {
            return;
        };
        let Some(file) = project.pous.iter().find(|file| file.name == name) else {
            return;
        };
        let tab = tab_for_pou(file);
        self.tabs.with_mut(|tabs| {
            if !tabs.iter().any(|existing| existing.name == tab.name) {
                tabs.push(tab.clone());
            }
        });
        self.editor_documents.with_mut(|documents| {
            documents.entry(tab.name.clone()).or_insert_with(|| {
                EditorDocument::new(tab_buffer(&project, &tab), tab_clean_pou(&project, &tab))
            });
        });
        self.selected_tab.set(Some(tab.name.clone()));
    }

    pub(crate) fn open_data_type(&mut self, name: &str) {
        let Some(project) = (self.project)() else {
            return;
        };
        let Some(data_type) = project
            .project
            .data
            .get("dataTypes")
            .and_then(serde_json::Value::as_array)
            .and_then(|data_types| {
                data_types.iter().find(|data_type| {
                    data_type.get("name").and_then(serde_json::Value::as_str) == Some(name)
                })
            })
        else {
            return;
        };
        let tab = EditorTab {
            name: name.to_owned(),
            target: TabTarget::DataType(name.to_owned()),
            kind: None,
            language: None,
        };
        let content = serde_json::to_string_pretty(data_type).unwrap_or_default();
        self.tabs.with_mut(|tabs| {
            if !tabs.iter().any(|existing| existing.name == tab.name) {
                tabs.push(tab.clone());
            }
        });
        self.editor_documents.with_mut(|documents| {
            documents
                .entry(tab.name.clone())
                .or_insert_with(|| EditorDocument::new(content, None));
        });
        self.selected_tab.set(Some(tab.name));
    }

    pub(crate) fn open_server(&mut self, name: &str) {
        self.open_named_protocol_target("servers", name, false);
    }

    pub(crate) fn open_remote_device(&mut self, name: &str) {
        self.open_named_protocol_target("remoteDevices", name, true);
    }

    fn open_named_protocol_target(&mut self, key: &str, name: &str, remote: bool) {
        let Some(project) = (self.project)() else {
            return;
        };
        let Some(value) = project
            .project
            .data
            .get(key)
            .and_then(serde_json::Value::as_array)
            .and_then(|values| {
                values.iter().find(|value| {
                    value.get("name").and_then(serde_json::Value::as_str) == Some(name)
                })
            })
        else {
            return;
        };
        let tab = EditorTab {
            name: name.to_owned(),
            target: if remote {
                TabTarget::RemoteDevice(name.to_owned())
            } else {
                TabTarget::Server(name.to_owned())
            },
            kind: None,
            language: None,
        };
        let content = serde_json::to_string_pretty(value).unwrap_or_default();
        self.tabs.with_mut(|tabs| {
            if !tabs.iter().any(|existing| existing.name == tab.name) {
                tabs.push(tab.clone());
            }
        });
        self.editor_documents.with_mut(|documents| {
            documents
                .entry(tab.name.clone())
                .or_insert_with(|| EditorDocument::new(content, None));
        });
        self.selected_tab.set(Some(tab.name));
    }

    pub(crate) fn create_server(&mut self, protocol: &str) -> Option<String> {
        self.create_protocol_target("servers", protocol, false)
    }

    pub(crate) fn create_remote_device(&mut self, protocol: &str) -> Option<String> {
        self.create_protocol_target("remoteDevices", protocol, true)
    }

    fn create_protocol_target(
        &mut self,
        key: &str,
        protocol: &str,
        remote: bool,
    ) -> Option<String> {
        let project = (self.project)()?;
        let existing = project
            .project
            .data
            .get(key)
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let base = if remote { "RemoteDevice" } else { "Server" };
        let name = next_named_protocol(&existing, base);
        let value = if remote {
            default_remote_device(&name, protocol)?
        } else {
            default_server(&name, protocol)?
        };
        self.project.with_mut(|project| {
            let Some(project) = project else {
                return;
            };
            let Some(data) = project.project.data.as_object_mut() else {
                return;
            };
            let values = data
                .entry(key.to_owned())
                .or_insert_with(|| serde_json::Value::Array(Vec::new()));
            if let Some(values) = values.as_array_mut() {
                values.push(value.clone());
            }
        });
        let tab = EditorTab {
            name: name.clone(),
            target: if remote {
                TabTarget::RemoteDevice(name.clone())
            } else {
                TabTarget::Server(name.clone())
            },
            kind: None,
            language: None,
        };
        let content = serde_json::to_string_pretty(&value).unwrap_or_default();
        self.tabs.with_mut(|tabs| tabs.push(tab));
        self.editor_documents.with_mut(|documents| {
            let mut document = EditorDocument::new(content, None);
            document.structural_dirty = true;
            document.clean.clear();
            documents.insert(name.clone(), document);
        });
        self.selected_tab.set(Some(name.clone()));
        Some(name)
    }

    pub(crate) fn open_resource(&mut self) {
        let Some(project) = (self.project)() else {
            return;
        };
        let resource = project
            .project
            .data
            .get("configuration")
            .and_then(|configuration| configuration.get("resource"))
            .cloned()
            .unwrap_or_else(|| {
                serde_json::json!({
                    "tasks": [],
                    "instances": [],
                    "globalVariables": []
                })
            });
        let tab = EditorTab {
            name: "Resource".to_owned(),
            target: TabTarget::Resource,
            kind: None,
            language: None,
        };
        let content = serde_json::to_string_pretty(&resource).unwrap_or_default();
        self.tabs.with_mut(|tabs| {
            if !tabs.iter().any(|existing| existing.name == tab.name) {
                tabs.push(tab.clone());
            }
        });
        self.editor_documents.with_mut(|documents| {
            documents
                .entry(tab.name.clone())
                .or_insert_with(|| EditorDocument::new(content, None));
        });
        self.selected_tab.set(Some(tab.name));
    }

    pub(crate) fn open_device_configuration(&mut self) {
        let Some(project) = (self.project)() else {
            return;
        };
        let tab = EditorTab {
            name: "Configuration".to_owned(),
            target: TabTarget::DeviceConfiguration,
            kind: None,
            language: None,
        };
        let content = device_configuration_buffer(&project);
        self.tabs.with_mut(|tabs| {
            if !tabs.iter().any(|existing| existing.name == tab.name) {
                tabs.push(tab.clone());
            }
        });
        self.editor_documents.with_mut(|documents| {
            documents
                .entry(tab.name.clone())
                .or_insert_with(|| EditorDocument::new(content, None));
        });
        self.selected_tab.set(Some(tab.name));
    }

    pub(crate) fn open_library_manager(&mut self) {
        let Some(project) = (self.project)() else {
            return;
        };
        let tab = EditorTab {
            name: "Library Manager".to_owned(),
            target: TabTarget::LibraryManager,
            kind: None,
            language: None,
        };
        let content = tab_buffer(&project, &tab);
        self.tabs.with_mut(|tabs| {
            if !tabs.iter().any(|existing| existing.name == tab.name) {
                tabs.push(tab.clone());
            }
        });
        self.editor_documents.with_mut(|documents| {
            documents
                .entry(tab.name.clone())
                .or_insert_with(|| EditorDocument::new(content, None));
        });
        self.selected_tab.set(Some(tab.name));
    }

    pub(crate) fn open_user_management(&mut self) {
        let Some(project) = (self.project)() else {
            return;
        };
        let tab = EditorTab {
            name: "User Management".to_owned(),
            target: TabTarget::UserManagement,
            kind: None,
            language: None,
        };
        let content = tab_buffer(&project, &tab);
        self.tabs.with_mut(|tabs| {
            if !tabs.iter().any(|existing| existing.name == tab.name) {
                tabs.push(tab.clone());
            }
        });
        self.editor_documents.with_mut(|documents| {
            documents
                .entry(tab.name.clone())
                .or_insert_with(|| EditorDocument::new(content, None));
        });
        self.selected_tab.set(Some(tab.name));
    }

    pub(crate) fn open_vendor_screen(&mut self, screen_name: &str) -> Result<(), String> {
        let Some(project) = (self.project)() else {
            return Err("No project is open.".to_owned());
        };
        let tab = EditorTab {
            name: screen_name.to_owned(),
            target: TabTarget::VendorScreen(screen_name.to_owned()),
            kind: None,
            language: None,
        };
        let content = vendor_screen_buffer(&project, screen_name)?;
        self.tabs.with_mut(|tabs| {
            if !tabs.iter().any(|existing| existing.name == tab.name) {
                tabs.push(tab.clone());
            }
        });
        self.editor_documents.with_mut(|documents| {
            documents
                .entry(tab.name.clone())
                .or_insert_with(|| EditorDocument::new(content, None));
        });
        self.selected_tab.set(Some(tab.name));
        Ok(())
    }

    pub(crate) fn install_library(&mut self, path: &Path) -> Result<String, String> {
        let manager = LibraryManager::new()?;
        let installed = manager.install_library(path)?;
        self.console.with_mut(|console| {
            console.push(ConsoleEntry {
                level: "info",
                message: format!("Installed library {}@{}", installed.name, installed.version),
            });
        });
        Ok(installed.name)
    }

    pub(crate) fn uninstall_library(&mut self, name: &str) -> Result<(), String> {
        LibraryManager::new()?.uninstall(name)?;
        self.console.with_mut(|console| {
            console.push(ConsoleEntry {
                level: "info",
                message: format!("Uninstalled library {name}"),
            });
        });
        Ok(())
    }

    pub(crate) fn set_library_enabled(
        &mut self,
        service: &ProjectService,
        name: &str,
        version: &str,
        enabled: bool,
    ) -> Result<(), String> {
        let Some(mut project) = (self.project)() else {
            return Err("No project is open.".to_owned());
        };
        if LibraryManager::new()?.is_bundled(name)? {
            if enabled {
                return Ok(());
            }
            return Err(format!("Bundled library '{name}' is always enabled."));
        }
        let data = project
            .project
            .data
            .as_object_mut()
            .ok_or_else(|| "project data must be a JSON object".to_owned())?;
        let libraries = data
            .entry("libraries")
            .or_insert_with(|| serde_json::Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| "project data.libraries must be an array".to_owned())?;

        if enabled {
            if !libraries.iter().any(|library| {
                library.get("name").and_then(serde_json::Value::as_str) == Some(name)
            }) {
                libraries.push(serde_json::json!({"name": name, "version": version}));
            }
        } else {
            libraries.retain(|library| {
                library.get("name").and_then(serde_json::Value::as_str) != Some(name)
            });
        }

        let project_on_disk = project_document_for_disk(&project.project);
        let mut project_json = serde_json::to_string_pretty(&project_on_disk)
            .map_err(|error| format!("serialize project.json: {error}"))?;
        project_json.push('\n');
        service
            .save_project_file(&project.root, "project.json", &project_json)
            .map_err(|error| error.to_string())?;
        self.project.set(Some(project));
        self.console.with_mut(|console| {
            console.push(ConsoleEntry {
                level: "info",
                message: format!(
                    "{} library {name}@{version}",
                    if enabled { "Enabled" } else { "Disabled" }
                ),
            });
        });
        Ok(())
    }

    pub(crate) fn create_data_type(&mut self, derivation: &str) -> Option<String> {
        let base = match derivation {
            "array" => "Array",
            "enumerated" => "Enumerated",
            "structure" => "Structure",
            _ => return None,
        };
        let project = (self.project)()?;
        let existing = project
            .project
            .data
            .get("dataTypes")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut suffix = 1usize;
        let name = loop {
            let candidate = if suffix == 1 {
                base.to_owned()
            } else {
                format!("{base}_{suffix}")
            };
            if !existing.iter().any(|data_type| {
                data_type
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|name| name.eq_ignore_ascii_case(&candidate))
            }) {
                break candidate;
            }
            suffix += 1;
        };
        let data_type = default_data_type_value(&name, derivation)?;
        self.project.with_mut(|project| {
            let Some(project) = project else {
                return;
            };
            let Some(data_types) = project
                .project
                .data
                .get_mut("dataTypes")
                .and_then(serde_json::Value::as_array_mut)
            else {
                return;
            };
            data_types.push(data_type.clone());
        });
        let tab = EditorTab {
            name: name.clone(),
            target: TabTarget::DataType(name.clone()),
            kind: None,
            language: None,
        };
        let content = serde_json::to_string_pretty(&data_type).unwrap_or_default();
        self.tabs.with_mut(|tabs| tabs.push(tab));
        self.editor_documents.with_mut(|documents| {
            let mut document = EditorDocument::new(content, None);
            document.structural_dirty = true;
            document.clean.clear();
            documents.insert(name.clone(), document);
        });
        self.selected_tab.set(Some(name.clone()));
        Some(name)
    }

    pub(crate) fn select_tab(&mut self, name: &str) {
        if !(self.tabs)().iter().any(|tab| tab.name == name) {
            return;
        }
        self.selected_tab.set(Some(name.to_owned()));
    }

    pub(crate) fn request_close_tab(&mut self, name: &str) -> bool {
        if self
            .editor_documents
            .read()
            .get(name)
            .is_some_and(EditorDocument::dirty)
        {
            self.pending_close_tab.set(Some(name.to_owned()));
            return false;
        }
        self.force_close_tab(name);
        true
    }

    pub(crate) fn force_close_tab(&mut self, name: &str) {
        self.tabs
            .with_mut(|tabs| tabs.retain(|tab| tab.name != name));
        self.editor_documents.with_mut(|documents| {
            documents.remove(name);
        });
        if (self.selected_tab)().as_deref() == Some(name) {
            let next = (self.tabs)().last().cloned();
            if let Some(tab) = next {
                self.select_tab(&tab.name);
            } else {
                self.selected_tab.set(None);
            }
        }
    }

    pub(crate) fn discard_tab(&mut self, name: &str) {
        let clean = self.editor_documents.read().get(name).cloned();
        let tab = (self.tabs)().into_iter().find(|tab| tab.name == name);
        let (Some(clean), Some(tab)) = (clean, tab) else {
            self.force_close_tab(name);
            return;
        };
        match tab.target {
            TabTarget::Pou(_) => {
                if let Some(clean_file) = clean.clean_pou_file.clone() {
                    self.project.with_mut(|project| {
                        let Some(project) = project else {
                            return;
                        };
                        if let Some(file) = project
                            .pous
                            .iter_mut()
                            .find(|file| file.name == clean_file.name)
                        {
                            *file = clean_file.clone();
                        }
                    });
                }
            }
            TabTarget::DataType(original_name) => {
                self.project.with_mut(|project| {
                    let Some(project) = project else {
                        return;
                    };
                    let Some(data_types) = project
                        .project
                        .data
                        .get_mut("dataTypes")
                        .and_then(serde_json::Value::as_array_mut)
                    else {
                        return;
                    };
                    if clean.clean.trim().is_empty() {
                        data_types.retain(|data_type| {
                            data_type.get("name").and_then(serde_json::Value::as_str)
                                != Some(original_name.as_str())
                        });
                    } else if let Ok(clean_value) =
                        serde_json::from_str::<serde_json::Value>(&clean.clean)
                    {
                        if let Some(index) = data_types.iter().position(|data_type| {
                            data_type.get("name").and_then(serde_json::Value::as_str)
                                == Some(original_name.as_str())
                        }) {
                            data_types[index] = clean_value;
                        }
                    }
                });
            }
            TabTarget::Server(original_name) => {
                restore_named_protocol_target(
                    &mut self.project,
                    "servers",
                    &original_name,
                    &clean.clean,
                );
            }
            TabTarget::RemoteDevice(original_name) => {
                restore_named_protocol_target(
                    &mut self.project,
                    "remoteDevices",
                    &original_name,
                    &clean.clean,
                );
            }
            TabTarget::Resource => {
                self.project.with_mut(|project| {
                    let Some(project) = project else {
                        return;
                    };
                    let Ok(resource) = serde_json::from_str::<serde_json::Value>(&clean.clean)
                    else {
                        return;
                    };
                    if let Some(configuration) = project
                        .project
                        .data
                        .get_mut("configuration")
                        .and_then(serde_json::Value::as_object_mut)
                    {
                        configuration.insert("resource".to_owned(), resource);
                    }
                });
            }
            TabTarget::VendorScreen(screen_name) => {
                if let Some(project) = (self.project)() {
                    if let Ok((_, keys)) = vendor_screen_definition_and_keys(&project, &screen_name)
                    {
                        if let Ok(slice) = serde_json::from_str::<serde_json::Value>(&clean.clean) {
                            self.project.with_mut(|project| {
                                if let Some(project) = project {
                                    set_project_vendor_slice(project, &keys, &slice);
                                }
                            });
                        }
                    }
                }
            }
            TabTarget::LibraryManager | TabTarget::UserManagement => {}
            TabTarget::DeviceConfiguration => {
                self.project.with_mut(|project| {
                    let Some(project) = project else {
                        return;
                    };
                    let Ok(buffer) = serde_json::from_str::<serde_json::Value>(&clean.clean) else {
                        return;
                    };
                    if let Some(configuration) = buffer.get("configuration").cloned() {
                        project.device_configuration = configuration;
                    }
                    if let Some(pin_mapping) = buffer.get("pinMapping").cloned() {
                        project.device_pin_mapping = pin_mapping;
                    }
                });
            }
            TabTarget::Manifest => {
                self.project.with_mut(|project| {
                    if let Some(project) = project {
                        project.library_manifest = Some(clean.clean.clone());
                    }
                });
            }
        }
        self.force_close_tab(name);
    }

    pub(crate) fn current_content(&self) -> String {
        let Some(selected) = (self.selected_tab)() else {
            return String::new();
        };
        self.editor_documents
            .read()
            .get(&selected)
            .map(|document| document.content.clone())
            .unwrap_or_default()
    }

    pub(crate) fn set_current_content(&mut self, content: String) {
        let Some(selected) = (self.selected_tab)() else {
            return;
        };
        self.editor_documents.with_mut(|documents| {
            if let Some(document) = documents.get_mut(&selected) {
                document.content = content;
            }
        });
    }

    pub(crate) fn mark_current_dirty(&mut self) {
        let Some(selected) = (self.selected_tab)() else {
            return;
        };
        self.editor_documents.with_mut(|documents| {
            if let Some(document) = documents.get_mut(&selected) {
                document.structural_dirty = true;
            }
        });
    }

    pub(crate) fn any_dirty(&self) -> bool {
        self.editor_documents
            .read()
            .values()
            .any(EditorDocument::dirty)
    }

    pub(crate) fn save_tab(
        &mut self,
        service: &ProjectService,
        selected: &str,
    ) -> Result<(), String> {
        let Some(project) = (self.project)() else {
            return Ok(());
        };
        let Some(tab) = (self.tabs)().into_iter().find(|tab| tab.name == selected) else {
            return Ok(());
        };
        let content = self
            .editor_documents
            .read()
            .get(selected)
            .map(|document| document.content.clone())
            .unwrap_or_default();
        let mut saved_key = selected.to_owned();

        match &tab.target {
            TabTarget::VendorScreen(screen_name) => {
                save_vendor_screen_slice(self, service, &project, screen_name, &content)?;
            }
            TabTarget::LibraryManager | TabTarget::UserManagement => {}
            TabTarget::Manifest => {
                service
                    .save_project_file(&project.root, "library.json", &content)
                    .map_err(|error| error.to_string())?;
                self.project.with_mut(|project| {
                    if let Some(project) = project {
                        project.library_manifest = Some(content.clone());
                    }
                });
            }
            TabTarget::Pou(pou_name) => {
                let Some(file) = project.pous.iter().find(|file| file.name == *pou_name) else {
                    return Ok(());
                };
                let Some(mut parsed) = file.parsed.clone() else {
                    return Ok(());
                };
                if parsed.body.language.is_graphical() {
                    parsed.body.value = serde_json::from_str(&content)
                        .map_err(|error| format!("Invalid graphical POU JSON: {error}"))?;
                } else {
                    parsed.body.value = serde_json::Value::String(content.clone());
                }
                let serialized = serialize_pou_to_text(&parsed, file.variables_text.as_deref());
                service
                    .save_text(file, &serialized)
                    .map_err(|error| error.to_string())?;
                self.project.with_mut(|project| {
                    let Some(project) = project else {
                        return;
                    };
                    if let Some(file) = project.pous.iter_mut().find(|file| file.name == *pou_name)
                    {
                        file.parsed = Some(parsed.clone());
                        file.raw_content = serialized.clone();
                    }
                });
            }
            TabTarget::Server(original_name) => {
                let new_name = save_named_protocol_target(
                    &mut self.project,
                    service,
                    &project.root,
                    "servers",
                    "devices/servers",
                    original_name,
                    &content,
                    false,
                )?;
                if new_name != *original_name {
                    retarget_named_protocol_tab(
                        self,
                        selected,
                        &new_name,
                        TabTarget::Server(new_name.clone()),
                    );
                    saved_key = new_name;
                }
            }
            TabTarget::RemoteDevice(original_name) => {
                let new_name = save_named_protocol_target(
                    &mut self.project,
                    service,
                    &project.root,
                    "remoteDevices",
                    "devices/remote",
                    original_name,
                    &content,
                    true,
                )?;
                if new_name != *original_name {
                    retarget_named_protocol_tab(
                        self,
                        selected,
                        &new_name,
                        TabTarget::RemoteDevice(new_name.clone()),
                    );
                    saved_key = new_name;
                }
            }
            TabTarget::Resource => {
                let resource = serde_json::from_str::<serde_json::Value>(&content)
                    .map_err(|error| format!("Invalid resource JSON: {error}"))?;
                let object = resource
                    .as_object()
                    .ok_or_else(|| "Resource must be a JSON object.".to_owned())?;
                for key in ["tasks", "instances", "globalVariables"] {
                    if !object.get(key).is_some_and(serde_json::Value::is_array) {
                        return Err(format!("Resource field {key:?} must be an array."));
                    }
                }
                self.project.with_mut(|project| {
                    let Some(project) = project else {
                        return;
                    };
                    if let Some(configuration) = project
                        .project
                        .data
                        .get_mut("configuration")
                        .and_then(serde_json::Value::as_object_mut)
                    {
                        configuration.insert("resource".to_owned(), resource.clone());
                    }
                });
            }
            TabTarget::DeviceConfiguration => {
                let (configuration, pin_mapping, configuration_text, pin_mapping_text) =
                    serialize_device_configuration_buffer(&content)?;
                service
                    .save_project_file(
                        &project.root,
                        "devices/configuration.json",
                        &configuration_text,
                    )
                    .map_err(|error| error.to_string())?;
                service
                    .save_project_file(&project.root, "devices/pin-mapping.json", &pin_mapping_text)
                    .map_err(|error| error.to_string())?;
                self.project.with_mut(|project| {
                    let Some(project) = project else {
                        return;
                    };
                    project.device_configuration = configuration.clone();
                    project.device_pin_mapping = pin_mapping.clone();
                });
            }
            TabTarget::DataType(original_name) => {
                let data_type = serde_json::from_str::<serde_json::Value>(&content)
                    .map_err(|error| format!("Invalid data type JSON: {error}"))?;
                let object = data_type
                    .as_object()
                    .ok_or_else(|| "Data type must be a JSON object.".to_owned())?;
                let new_name = object
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| "Data type name cannot be empty.".to_owned())?
                    .to_owned();
                let derivation = object
                    .get("derivation")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "Data type derivation is missing.".to_owned())?;
                if !matches!(derivation, "array" | "enumerated" | "structure") {
                    return Err(format!("Unsupported data type derivation {derivation:?}."));
                }
                let duplicate = project
                    .project
                    .data
                    .get("dataTypes")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|candidate| {
                        candidate
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|name| {
                                !name.eq_ignore_ascii_case(original_name)
                                    && name.eq_ignore_ascii_case(&new_name)
                            })
                    });
                if duplicate {
                    return Err(format!("Data type {new_name:?} already exists."));
                }
                self.project.with_mut(|project| {
                    let Some(project) = project else {
                        return;
                    };
                    let Some(data_types) = project
                        .project
                        .data
                        .get_mut("dataTypes")
                        .and_then(serde_json::Value::as_array_mut)
                    else {
                        return;
                    };
                    if let Some(index) = data_types.iter().position(|candidate| {
                        candidate.get("name").and_then(serde_json::Value::as_str)
                            == Some(original_name.as_str())
                    }) {
                        data_types[index] = data_type.clone();
                    }
                });
                if new_name != *original_name {
                    self.tabs.with_mut(|tabs| {
                        if let Some(tab) = tabs.iter_mut().find(|tab| tab.name == selected) {
                            tab.name = new_name.clone();
                            tab.target = TabTarget::DataType(new_name.clone());
                        }
                    });
                    self.editor_documents.with_mut(|documents| {
                        if let Some(document) = documents.remove(selected) {
                            documents.insert(new_name.clone(), document);
                        }
                    });
                    if (self.selected_tab)().as_deref() == Some(selected) {
                        self.selected_tab.set(Some(new_name.clone()));
                    }
                    saved_key = new_name;
                }
            }
        }
        self.project.with_mut(|project| {
            if let Some(project) = project {
                sync_debug_variable_metadata(project);
            }
        });
        if let Some(updated_project) = (self.project)() {
            let project_on_disk = project_document_for_disk(&updated_project.project);
            let mut project_json = serde_json::to_string_pretty(&project_on_disk)
                .map_err(|error| format!("serialize project.json: {error}"))?;
            project_json.push('\n');
            service
                .save_project_file(&updated_project.root, "project.json", &project_json)
                .map_err(|error| error.to_string())?;
        }
        let clean_pou_file = (self.project)()
            .as_ref()
            .and_then(|project| tab_clean_pou(project, &tab));
        self.editor_documents.with_mut(|documents| {
            if let Some(document) = documents.get_mut(&saved_key) {
                document.clean = document.content.clone();
                document.clean_pou_file = clean_pou_file;
                document.structural_dirty = false;
            }
        });
        self.console.with_mut(|console| {
            console.push(ConsoleEntry {
                level: "info",
                message: format!("Saved {saved_key}"),
            });
        });
        Ok(())
    }

    pub(crate) fn save_all(&mut self, service: &ProjectService) -> Result<(), String> {
        let dirty = self
            .editor_documents
            .read()
            .iter()
            .filter_map(|(name, document)| document.dirty().then_some(name.clone()))
            .collect::<Vec<_>>();
        for name in dirty {
            self.save_tab(service, &name)?;
        }
        Ok(())
    }

    pub(crate) fn request_close_project(&mut self) -> bool {
        if self.any_dirty() {
            self.project_close_modal.set(true);
            return false;
        }
        self.clear_project();
        true
    }

    pub(crate) fn clear_remote_debug(&mut self) {
        if let Some(session) = (self.remote_debug)() {
            let _ = session.disconnect();
        }
        self.remote_debug.set(None);
    }

    pub(crate) fn clear_runtime_connection(&mut self) {
        if let Some(client) = (self.runtime_client)() {
            let _ = client.logout();
        }
        self.runtime_client.set(None);
        self.runtime_connected.set(false);
        self.runtime_version.set(None);
    }

    pub(crate) fn clear_project(&mut self) {
        self.clear_remote_debug();
        self.clear_runtime_connection();
        self.project.set(None);
        self.tabs.set(Vec::new());
        self.selected_tab.set(None);
        self.editor_documents.set(HashMap::new());
        self.pending_close_tab.set(None);
        self.project_close_modal.set(false);
        self.console.set(Vec::new());
        self.bottom_panel.set(BottomPanel::Console);
        self.search_dialog_open.set(false);
        self.search_results.set(Vec::new());
        self.screen.set(Screen::Start);
    }
}

fn default_data_type_value(name: &str, derivation: &str) -> Option<serde_json::Value> {
    match derivation {
        "array" => Some(serde_json::json!({
            "name": name,
            "derivation": "array",
            "baseType": { "definition": "base-type", "value": "BOOL" },
            "initialValue": "",
            "dimensions": []
        })),
        "enumerated" => Some(serde_json::json!({
            "name": name,
            "derivation": "enumerated",
            "values": [],
            "initialValue": ""
        })),
        "structure" => Some(serde_json::json!({
            "name": name,
            "derivation": "structure",
            "variable": []
        })),
        _ => None,
    }
}

#[cfg(test)]
mod data_type_tests {
    use serde_json::json;

    use super::default_data_type_value;

    #[test]
    fn data_type_creation_defaults_match_the_pinned_editor() {
        assert_eq!(
            default_data_type_value("IntArray", "array").unwrap(),
            json!({
                "name":"IntArray",
                "derivation":"array",
                "baseType":{"definition":"base-type","value":"BOOL"},
                "initialValue":"",
                "dimensions":[]
            })
        );
        assert_eq!(
            default_data_type_value("Colors", "enumerated").unwrap(),
            json!({
                "name":"Colors",
                "derivation":"enumerated",
                "values":[],
                "initialValue":""
            })
        );
        assert_eq!(
            default_data_type_value("Point", "structure").unwrap(),
            json!({"name":"Point","derivation":"structure","variable":[]})
        );
        assert!(default_data_type_value("Bad", "pointer").is_none());
    }
}

fn serialize_device_configuration_buffer(
    content: &str,
) -> Result<(serde_json::Value, serde_json::Value, String, String), String> {
    let buffer = serde_json::from_str::<serde_json::Value>(content)
        .map_err(|error| format!("Invalid device configuration JSON: {error}"))?;
    let configuration = buffer
        .get("configuration")
        .cloned()
        .filter(serde_json::Value::is_object)
        .ok_or_else(|| "Device configuration must be an object.".to_owned())?;
    let board = configuration
        .get("deviceBoard")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Device board cannot be empty.".to_owned())?;
    let pin_mapping =
        normalize_pin_mapping_value(buffer.get("pinMapping").cloned().unwrap_or_default(), board);
    let mut configuration_text = serde_json::to_string_pretty(&configuration)
        .map_err(|error| format!("serialize device configuration: {error}"))?;
    configuration_text.push('\n');
    let mut pin_mapping_text = serde_json::to_string_pretty(&pin_mapping)
        .map_err(|error| format!("serialize pin mapping: {error}"))?;
    pin_mapping_text.push('\n');
    Ok((
        configuration,
        pin_mapping,
        configuration_text,
        pin_mapping_text,
    ))
}

fn normalize_pin_mapping_value(value: serde_json::Value, active_board: &str) -> serde_json::Value {
    if let Some(pins) = value.as_array() {
        return serde_json::json!({ active_board: pins });
    }
    if value.is_object() {
        value
    } else {
        serde_json::json!({})
    }
}

fn device_configuration_buffer(project: &OpenProject) -> String {
    let board = project
        .device_configuration
        .get("deviceBoard")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("OpenPLC Simulator");
    let pin_mapping = normalize_pin_mapping_value(project.device_pin_mapping.clone(), board);
    serde_json::to_string_pretty(&serde_json::json!({
        "configuration": project.device_configuration,
        "pinMapping": pin_mapping,
    }))
    .unwrap_or_default()
}

#[cfg(test)]
mod device_buffer_tests {
    use serde_json::json;

    use super::serialize_device_configuration_buffer;

    #[test]
    fn device_buffer_serialization_writes_canonical_two_file_shapes() {
        let content = serde_json::to_string(&json!({
            "configuration": {
                "deviceBoard":"Vendor Board",
                "communicationPort":"/dev/cu.usbmodem1",
                "runtimeIpAddress":"",
                "selectedPlatformOptions":{"cpu":"m4"}
            },
            "pinMapping": [
                {"pin":"D0","pinType":"digitalInput","address":"%IX0.0","alias":"start"}
            ]
        }))
        .unwrap();
        let (configuration, pin_mapping, configuration_text, pin_mapping_text) =
            serialize_device_configuration_buffer(&content).unwrap();
        assert_eq!(configuration["deviceBoard"], "Vendor Board");
        assert_eq!(pin_mapping["Vendor Board"].as_array().unwrap().len(), 1);
        assert!(configuration_text.ends_with('\n'));
        assert!(pin_mapping_text.ends_with('\n'));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&configuration_text).unwrap(),
            configuration
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&pin_mapping_text).unwrap(),
            pin_mapping
        );
    }

    #[test]
    fn device_buffer_rejects_an_empty_board_name() {
        let content = serde_json::to_string(&json!({
            "configuration":{"deviceBoard":"","communicationPort":""},
            "pinMapping":{}
        }))
        .unwrap();
        assert!(serialize_device_configuration_buffer(&content)
            .unwrap_err()
            .contains("cannot be empty"));
    }
}

fn vendor_screen_definition_and_keys(
    project: &OpenProject,
    screen_name: &str,
) -> Result<(serde_json::Value, Vec<String>), String> {
    let board = project
        .device_configuration
        .get("deviceBoard")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Device board is not configured.".to_owned())?;
    let definition = VppPackageManager::new()?.screen_definition(board, screen_name)?;
    let keys = persistence_keys(&definition);
    Ok((definition, keys))
}

fn vendor_screen_buffer(project: &OpenProject, screen_name: &str) -> Result<String, String> {
    let (_, keys) = vendor_screen_definition_and_keys(project, screen_name)?;
    let vendor_data = project
        .device_configuration
        .get("vendorScreenData")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    serde_json::to_string_pretty(&owned_slice(&vendor_data, &keys))
        .map_err(|error| format!("serialize vendor screen state: {error}"))
}

fn save_vendor_screen_slice(
    store: &mut AppStore,
    service: &ProjectService,
    project: &OpenProject,
    screen_name: &str,
    content: &str,
) -> Result<(), String> {
    let (_, keys) = vendor_screen_definition_and_keys(project, screen_name)?;
    let slice = serde_json::from_str::<serde_json::Value>(content)
        .map_err(|error| format!("Invalid vendor screen state: {error}"))?;
    if !slice.is_object() {
        return Err("Vendor screen state must be a JSON object.".to_owned());
    }
    let path = project.root.join("devices/configuration.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    let mut disk = serde_json::from_str::<serde_json::Value>(&raw)
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
    let Some(disk_object) = disk.as_object_mut() else {
        return Err("devices/configuration.json must contain a JSON object.".to_owned());
    };
    let vendor_data = disk_object
        .entry("vendorScreenData".to_owned())
        .or_insert_with(|| serde_json::json!({}));
    replace_owned_slice(vendor_data, &keys, &slice);
    let mut text = serde_json::to_string_pretty(&disk)
        .map_err(|error| format!("serialize devices/configuration.json: {error}"))?;
    text.push('\n');
    service
        .save_project_file(&project.root, "devices/configuration.json", &text)
        .map_err(|error| error.to_string())?;
    store.project.with_mut(|project| {
        if let Some(project) = project {
            set_project_vendor_slice(project, &keys, &slice);
        }
    });
    Ok(())
}

fn named_protocol_buffer(project: &OpenProject, key: &str, name: &str) -> String {
    project
        .project
        .data
        .get(key)
        .and_then(serde_json::Value::as_array)
        .and_then(|values| {
            values
                .iter()
                .find(|value| value.get("name").and_then(serde_json::Value::as_str) == Some(name))
        })
        .and_then(|value| serde_json::to_string_pretty(value).ok())
        .unwrap_or_default()
}

fn tab_for_pou(file: &crate::project::PouFile) -> EditorTab {
    EditorTab {
        name: file.name.clone(),
        target: TabTarget::Pou(file.name.clone()),
        kind: Some(file.kind),
        language: file.language,
    }
}

fn tab_buffer(project: &OpenProject, tab: &EditorTab) -> String {
    match &tab.target {
        TabTarget::LibraryManager | TabTarget::UserManagement => String::new(),
        TabTarget::VendorScreen(name) => vendor_screen_buffer(project, name).unwrap_or_default(),
        TabTarget::Manifest => project.library_manifest.clone().unwrap_or_default(),
        TabTarget::Resource => project
            .project
            .data
            .get("configuration")
            .and_then(|configuration| configuration.get("resource"))
            .and_then(|resource| serde_json::to_string_pretty(resource).ok())
            .unwrap_or_default(),
        TabTarget::DeviceConfiguration => device_configuration_buffer(project),
        TabTarget::Server(name) => named_protocol_buffer(project, "servers", name),
        TabTarget::RemoteDevice(name) => named_protocol_buffer(project, "remoteDevices", name),
        TabTarget::DataType(name) => project
            .project
            .data
            .get("dataTypes")
            .and_then(serde_json::Value::as_array)
            .and_then(|data_types| {
                data_types.iter().find(|data_type| {
                    data_type.get("name").and_then(serde_json::Value::as_str) == Some(name)
                })
            })
            .and_then(|data_type| serde_json::to_string_pretty(data_type).ok())
            .unwrap_or_default(),
        TabTarget::Pou(name) => project
            .pous
            .iter()
            .find(|file| file.name == *name)
            .and_then(|file| file.parsed.as_ref())
            .map(|pou| {
                if pou.body.language.is_graphical() {
                    serde_json::to_string_pretty(&pou.body.value).unwrap_or_default()
                } else {
                    pou.body.value.as_str().unwrap_or_default().to_owned()
                }
            })
            .unwrap_or_default(),
    }
}

fn tab_clean_pou(project: &OpenProject, tab: &EditorTab) -> Option<PouFile> {
    let TabTarget::Pou(name) = &tab.target else {
        return None;
    };
    project.pous.iter().find(|file| file.name == *name).cloned()
}

pub fn App() -> Element {
    let service = match ProjectService::new() {
        Ok(service) => service,
        Err(error) => {
            return rsx! {
                div { class: "flex h-screen w-screen items-center justify-center bg-white p-8 dark:bg-neutral-1000",
                    div { role: "alert", class: "max-w-lg rounded-lg border border-red-300 bg-red-50 p-6 text-red-800 dark:border-red-800 dark:bg-red-950/30 dark:text-red-200",
                        h1 { class: "font-display text-xl font-semibold", "RustPLC Editor could not start" }
                        p { class: "mt-2 font-caption text-sm", "Application data storage is unavailable: {error}" }
                        p { class: "mt-2 font-caption text-xs text-red-600 dark:text-red-300", "No project files were modified." }
                    }
                }
            };
        }
    };
    let service = use_context_provider(|| service);
    let recent_service = service.clone();
    use_context_provider(LocalPlcService::new);
    let store = AppStore {
        screen: use_signal(|| Screen::Start),
        project: use_signal(|| None),
        recent: use_signal(move || recent_service.recent_projects().unwrap_or_default()),
        create_project_open: use_signal(|| false),
        tabs: use_signal(Vec::new),
        selected_tab: use_signal(|| None),
        editor_documents: use_signal(HashMap::new),
        pending_close_tab: use_signal(|| None),
        project_close_modal: use_signal(|| false),
        console: use_signal(Vec::new),
        bottom_panel: use_signal(|| BottomPanel::Console),
        search_dialog_open: use_signal(|| false),
        search_results: use_signal(Vec::new),
        plc_state: use_signal(|| PlcState::Stopped),
        remote_debug: use_signal(|| None),
        runtime_client: use_signal(|| None),
        runtime_connected: use_signal(|| false),
        runtime_version: use_signal(|| None),
        is_compiling: use_signal(|| false),
    };
    use_context_provider(|| store);

    rsx! {
        style { dangerous_inner_html: OPENPLC_CSS }
        AppLayout {}
    }
}

#[component]
fn AppLayout() -> Element {
    let store = use_context::<AppStore>();
    let current = (store.screen)();
    rsx! {
        div { class: "h-full bg-neutral-50 text-gray-500 dark:bg-neutral-950 dark:text-gray-400 block",
            TitleBar {}
            main { class: "absolute bottom-0 left-0 right-0 flex overflow-hidden top-[--oplc-title-bar-height]",
                if current == Screen::Start {
                    StartScreen {}
                } else {
                    WorkspaceScreen {}
                }
            }
            SaveChangesModals {}
        }
    }
}

#[component]
fn TitleBar() -> Element {
    rsx! {
        div { id: "title-bar", class: "oplc-titlebar-container",
            div { class: "oplc-titlebar-content dark:bg-neutral-950",
                TitleBarLeftSlot {}
                TitleBarCenterSlot {}
                TitleBarRightSlot {}
            }
        }
    }
}

#[component]
fn TitleBarLeftSlot() -> Element {
    // Pinned OpenPLC behavior on darwin native application: an empty left slot.
    rsx! { div { class: "flex items-center justify-start gap-1 px-4 py-0.5" } }
}

#[component]
fn TitleBarCenterSlot() -> Element {
    let window = use_window();
    rsx! {
        div {
            class: "flex flex-1 items-center justify-center gap-2 oplc-titlebar-drag-region",
            onmousedown: move |_| window.drag(),
            OpenPLCIcon {}
            span { class: "font-caption text-xs font-normal", "RustPLC Editor" }
        }
    }
}

#[component]
fn TitleBarRightSlot() -> Element {
    // Pinned OpenPLC behavior on darwin native application: Windows controls are not rendered here.
    rsx! { div { id: "title-bar-right-slot", class: "flex items-center justify-end" } }
}

#[component]
fn StartScreen() -> Element {
    let service = use_context::<ProjectService>();
    let mut store = use_context::<AppStore>();
    let search_filter_value = use_signal(String::new);

    let handle_open_project = move |_| {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Select a PLC project to open")
            .pick_folder()
        else {
            return;
        };
        if let Ok(project) = service.open(path) {
            store.load_project(project);
            store
                .recent
                .set(service.recent_projects().unwrap_or_default());
        }
    };

    let close_window = use_window();
    rsx! {
        StartSideContent {
            MenuRoot {
                MenuSection {
                    MenuItem {
                        ghosted: false,
                        onclick: move |_| store.create_project_open.set(true),
                        PlusIcon {}
                        " New Project"
                    }
                    MenuItem {
                        ghosted: true,
                        onclick: handle_open_project,
                        FolderIcon {}
                        " Open"
                    }
                    MenuItem { ghosted: true,
                        VideoIcon {}
                        " Tutorials"
                    }
                }
                MenuDivider {}
                MenuSection {
                    MenuItem {
                        ghosted: true,
                        onclick: move |_| close_window.close(),
                        StickArrowIcon {}
                        " Exit"
                    }
                }
            }
        }
        StartMainContent {
            ProjectFilterBar { search_filter_value }
            DisplayRecentProjects { search_name_filter_value: search_filter_value() }
        }
        if (store.create_project_open)() {
            ProjectModal {}
        }
    }
}

#[component]
fn StartSideContent(children: Element) -> Element {
    rsx! {
        div { class: "relative top-[63%] ml-16 min-w-[240px] 2xl:top-2/3", {children} }
    }
}

#[component]
fn StartMainContent(children: Element) -> Element {
    rsx! {
        div {
            class: "my-4 mb-2 h-full w-full max-w-[778px] min-[1328px]:max-w-5xl min-[1575px]:max-w-7xl min-[1822px]:max-w-[1520px] 3xl:max-w-[1532px] 4xl:max-w-[1990px]",
            {children}
        }
    }
}

#[component]
fn MenuRoot(children: Element) -> Element {
    rsx! {
        div {
            class: "flex h-fit min-h-[372px] w-[240px] max-w-[240px] select-none flex-col justify-start bg-transparent text-xl text-neutral-1000 dark:text-white",
            {children}
        }
    }
}

#[component]
fn MenuSection(children: Element) -> Element {
    rsx! { div { class: "flex flex-col gap-2", {children} } }
}

#[component]
fn MenuDivider() -> Element {
    rsx! { hr { class: "my-4 w-48 stroke-neutral-200 stroke-[1.5px]" } }
}

#[component]
fn MenuItem(
    ghosted: bool,
    onclick: Option<EventHandler<MouseEvent>>,
    children: Element,
) -> Element {
    let classes = if ghosted {
        // Upstream builds this with tailwind-merge; use the already-merged form
        // so conflicting base/hover/focus background and font classes cannot
        // resolve differently when the compiled CSS order changes.
        "flex h-12 w-48 items-center gap-3 rounded-md px-5 py-3 font-caption text-xl bg-transparent focus:bg-transparent text-neutral-1000 hover:text-brand hover:bg-transparent dark:text-white justify-start hover:opacity-90 font-medium"
    } else {
        "flex h-12 w-48 items-center gap-3 rounded-md bg-brand px-5 py-3 font-caption text-xl font-normal text-white hover:bg-brand-medium-dark focus:bg-brand-medium"
    };
    rsx! {
        button {
            r#type: "button",
            class: classes,
            onclick: move |event| {
                if let Some(handler) = &onclick {
                    handler.call(event);
                }
            },
            {children}
        }
    }
}

#[component]
fn ProjectFilterBar(mut search_filter_value: Signal<String>) -> Element {
    let mut store = use_context::<AppStore>();
    let mut project_filter = use_signal(|| "Recent".to_owned());
    let mut dropdown_open = use_signal(|| false);

    let mut select_filter = move |filter: &'static str| {
        project_filter.set(filter.to_owned());
        dropdown_open.set(false);
        let mut projects = (store.recent)();
        if filter == "Name" {
            projects.sort_by(|a, b| a.name.cmp(&b.name));
        } else {
            projects.sort_by(|a, b| b.last_opened_at.cmp(&a.last_opened_at));
        }
        store.recent.set(projects);
    };

    rsx! {
        div {
            id: "project-filter-bar",
            class: "relative mb-4 flex w-full items-center justify-between gap-4 pr-10 xl:pr-10 3xl:pr-10 4xl:pr-0",
            div {
                id: "project-filter-dropdown-container",
                class: "relative flex h-12 min-w-52 shrink-0 select-none items-center justify-around gap-3 rounded-md border border-neutral-100 bg-white py-[6px] pl-4 dark:border-neutral-800 dark:bg-neutral-900",
                div { id: "project-filter-dropdown-label",
                    span { class: "cursor-default whitespace-nowrap font-caption text-base font-normal text-neutral-700 dark:text-neutral-500", "Order by" }
                }
                div { class: "relative h-full w-full max-w-28 flex-1",
                    button {
                        id: "project-filter-dropdown-button",
                        r#type: "button",
                        class: "group flex h-full w-full max-w-28 flex-1 items-center justify-around rounded-md",
                        onclick: move |_| dropdown_open.toggle(),
                        span { class: "whitespace-nowrap font-caption text-base font-normal text-black dark:text-white", "{project_filter}" }
                        ArrowIcon {}
                    }
                    if dropdown_open() {
                        div {
                            id: "project-filter-dropdown-options",
                            class: "absolute left-0 top-[calc(100%+15px)] z-[10] w-full rounded-md border border-neutral-100 bg-white dark:border-neutral-800 dark:bg-neutral-900",
                            role: "menu",
                            button {
                                id: "project-filter-dropdown-item-recent",
                                class: "flex h-9 w-full flex-col justify-center rounded-t-md bg-white pl-2 text-left font-caption text-base font-normal text-black outline-none hover:cursor-pointer hover:bg-neutral-50 dark:bg-neutral-900 dark:text-white hover:dark:bg-neutral-700",
                                onclick: move |_| select_filter("Recent"),
                                span { "Recent" }
                            }
                            button {
                                id: "project-filter-dropdown-item-name",
                                class: "flex h-9 w-full flex-col justify-center rounded-b-md bg-white pl-2 text-left font-caption text-base font-normal text-black outline-none hover:cursor-pointer hover:bg-neutral-50 dark:bg-neutral-900 dark:text-white hover:dark:bg-neutral-700",
                                onclick: move |_| select_filter("Name"),
                                span { "Name" }
                            }
                        }
                    }
                }
            }
            div {
                id: "project-filter-search-container",
                class: "mr-2 flex h-12 w-[704px] flex-grow items-center gap-4 rounded-lg border-[1.5px] border-neutral-100 bg-white px-8 font-caption text-base dark:border-neutral-800 dark:bg-neutral-900 xl:mr-4",
                div { id: "project-filter-search-label", MagnifierIcon {} }
                input {
                    id: "project-filter-search-input",
                    r#type: "text",
                    placeholder: "Search for a project",
                    "aria-label": "Search recent projects",
                    autocomplete: "off",
                    spellcheck: "false",
                    class: "h-full w-full bg-inherit text-black outline-none placeholder:select-none placeholder:font-caption placeholder:font-normal dark:text-white",
                    value: "{search_filter_value}",
                    oninput: move |event| search_filter_value.set(event.value()),
                }
            }
        }
    }
}

#[component]
fn DisplayRecentProjects(search_name_filter_value: String) -> Element {
    let service = use_context::<ProjectService>();
    let mut store = use_context::<AppStore>();
    let mut actions_for = use_signal(|| None::<PathBuf>);

    let recent = (store.recent)();
    let filter = search_name_filter_value.to_lowercase();
    let projects: Vec<_> = recent
        .into_iter()
        .filter(|project| filter.is_empty() || project.name.to_lowercase().contains(&filter))
        .collect();

    rsx! {
        section { class: "flex h-[52%] w-full select-none flex-col pr-9 2xl:h-3/5 3xl:h-3/4 4xl:h-4/5 4xl:pr-0",
            h2 { class: "mb-6 flex cursor-default justify-start font-caption text-xl font-medium text-neutral-1000 dark:text-white", "Projects" }
            div { class: "scroll-area flex h-auto w-full flex-wrap gap-[25px] overflow-y-auto",
                for project in projects {
                    {
                        let project_path = project.path.clone();
                        let open_path = project.path.clone();
                        let remove_path = project.path.clone();
                        let project_name = project.name.clone();
                        let last_modified = compare_last_opened(&project.last_opened_at);
                        let open_service = service.clone();
                        let remove_service = service.clone();
                        rsx! {
                            div { key: "{project.path.display()}", class: "group relative",
                                File {
                                    project_name,
                                    project_path: project.path.display().to_string(),
                                    last_modified,
                                    onopen: move |_| {
                                        if let Ok(opened) = open_service.open(&open_path) {
                                            store.load_project(opened);
                                            store.recent.set(open_service.recent_projects().unwrap_or_default());
                                        }
                                    }
                                }
                                button {
                                    "aria-label": "Project actions",
                                    class: "absolute right-2 top-10 rounded p-1 hover:bg-black/20 focus:outline-none dark:hover:bg-black/40",
                                    title: "More actions",
                                    onclick: move |event| {
                                        event.stop_propagation();
                                        if actions_for().as_ref() == Some(&project_path) {
                                            actions_for.set(None);
                                        } else {
                                            actions_for.set(Some(project_path.clone()));
                                        }
                                    },
                                    svg { class: "h-4 w-4 text-white", view_box: "0 0 16 16", fill: "currentColor", "aria-hidden": "true",
                                        path { d: "M8 9.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3ZM1.5 9.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Zm13 0a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z" }
                                    }
                                }
                                if actions_for().as_ref() == Some(&project.path) {
                                    div { class: "absolute right-2 top-16 z-[60] min-w-[180px] overflow-hidden rounded-md border border-neutral-200 bg-white py-1 shadow-lg dark:border-neutral-700 dark:bg-neutral-900",
                                        button {
                                            class: "flex w-full cursor-pointer select-none items-center gap-2 px-3 py-1.5 text-xs text-neutral-700 outline-none hover:bg-neutral-100 dark:text-neutral-300 dark:hover:bg-neutral-800",
                                            onclick: move |event| {
                                                event.stop_propagation();
                                                if remove_service.remove_recent_project(&remove_path).is_ok() {
                                                    store.recent.set(remove_service.recent_projects().unwrap_or_default());
                                                }
                                                actions_for.set(None);
                                            },
                                            svg { class: "h-3.5 w-3.5 text-neutral-500", view_box: "0 0 16 16", fill: "currentColor",
                                                path { d: "M3.72 3.72a.75.75 0 0 1 1.06 0L8 6.94l3.22-3.22a.75.75 0 1 1 1.06 1.06L9.06 8l3.22 3.22a.75.75 0 1 1-1.06 1.06L8 9.06l-3.22 3.22a.75.75 0 0 1-1.06-1.06L6.94 8 3.72 4.78a.75.75 0 0 1 0-1.06Z" }
                                            }
                                            "Remove from list"
                                        }
                                        button {
                                            class: "flex w-full cursor-pointer select-none items-center gap-2 px-3 py-1.5 text-xs text-red-600 outline-none hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950/40",
                                            onclick: move |event| {
                                                event.stop_propagation();
                                                actions_for.set(None);
                                            },
                                            svg { class: "h-3.5 w-3.5", view_box: "0 0 16 16", fill: "currentColor",
                                                path { d: "M11 1.75V3h2.25a.75.75 0 0 1 0 1.5H2.75a.75.75 0 0 1 0-1.5H5V1.75C5 .784 5.784 0 6.75 0h2.5C10.216 0 11 .784 11 1.75ZM6.5 1.75V3h3V1.75a.25.25 0 0 0-.25-.25h-2.5a.25.25 0 0 0-.25.25ZM4.005 5.073a.75.75 0 0 1 .673.627l.79 5.532a.75.75 0 0 0 .742.643h3.58a.75.75 0 0 0 .742-.643l.79-5.532a.75.75 0 0 1 1.49.214l-.79 5.532A2.25 2.25 0 0 1 9.79 13.5H6.21a2.25 2.25 0 0 1-2.23-1.928l-.79-5.532a.75.75 0 0 1 .626-.867Z" }
                                            }
                                            "Delete project"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn File(
    project_name: String,
    project_path: String,
    last_modified: String,
    onopen: EventHandler<()>,
) -> Element {
    let accessible_name = format!("Open project {project_name}");
    rsx! {
        div {
            title: "file-root",
            id: "folder-root",
            role: "button",
            tabindex: "0",
            "aria-label": "{accessible_name}",
            class: "relative flex h-[160px] w-[224px] overflow-hidden outline-none focus:ring-2 focus:ring-brand",
            onclick: move |_| onopen.call(()),
            onkeydown: move |event| {
                if event.key() == Key::Enter || event.key() == Key::Character(" ".to_owned()) {
                    event.prevent_default();
                    onopen.call(());
                }
            },
            p {
                id: "{project_name}",
                class: "absolute bottom-4 left-3 flex cursor-pointer flex-col gap-[1px] text-white",
                span { id: "{project_name}", class: "overflow-hidden overflow-ellipsis whitespace-nowrap text-sm font-bold", "{project_name}" }
                span { id: "{project_name}", class: "overflow-hidden overflow-ellipsis whitespace-nowrap text-cp-sm", "{project_path}" }
                span { id: "{last_modified}", class: "overflow-hidden overflow-ellipsis whitespace-nowrap text-[10px] opacity-40", "{last_modified}" }
            }
            svg { "aria-label": "folder-shape", view_box: "0 0 224 160", xmlns: "http://www.w3.org/2000/svg",
                title { "folder-shape" }
                path {
                    id: "folder-path",
                    class: "cursor-pointer fill-brand-medium hover:fill-brand-medium-dark",
                    d: "M224 153.143V33.5238C224 29.7367 220.991 26.6667 217.28 26.6667H136.217C135.027 26.6667 133.858 26.3443 132.831 25.7326L91.1693 0.934087C90.1415 0.322343 88.9731 0 87.7833 0H6.72C3.00865 0 0 3.07005 0 6.85715V153.143C0 156.93 3.00864 160 6.71999 160H217.28C220.991 160 224 156.93 224 153.143Z",
                }
            }
        }
    }
}

fn compare_last_opened(last_opened_at: &str) -> String {
    let Ok(last) = OffsetDateTime::parse(last_opened_at, &Rfc3339) else {
        return String::new();
    };
    let difference = OffsetDateTime::now_utc() - last;
    let minutes = difference.whole_minutes().max(0);
    let hours = difference.whole_hours().max(0);
    let days = difference.whole_days().max(0);
    if minutes < 60 {
        if minutes == 1 {
            "modified 1 minute ago".to_owned()
        } else {
            format!("modified {minutes} minutes ago")
        }
    } else if hours < 24 {
        if hours == 1 {
            "modified 1 hour ago".to_owned()
        } else {
            format!("modified {hours} hours ago")
        }
    } else if days == 1 {
        "modified 1 day ago".to_owned()
    } else {
        format!("modified {days} days ago")
    }
}

fn next_named_protocol(values: &[serde_json::Value], base: &str) -> String {
    if !values.iter().any(|value| {
        value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|name| name.eq_ignore_ascii_case(base))
    }) {
        return base.to_owned();
    }
    let mut suffix = 1usize;
    loop {
        let candidate = format!("{base}{suffix}");
        if !values.iter().any(|value| {
            value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|name| name.eq_ignore_ascii_case(&candidate))
        }) {
            return candidate;
        }
        suffix += 1;
    }
}

fn restore_named_protocol_target(
    project_signal: &mut Signal<Option<OpenProject>>,
    key: &str,
    original_name: &str,
    clean: &str,
) {
    project_signal.with_mut(|project| {
        let Some(project) = project else {
            return;
        };
        let Some(data) = project.project.data.as_object_mut() else {
            return;
        };
        let values = data
            .entry(key.to_owned())
            .or_insert_with(|| serde_json::Value::Array(Vec::new()));
        let Some(values) = values.as_array_mut() else {
            return;
        };
        if clean.trim().is_empty() {
            values.retain(|value| {
                value.get("name").and_then(serde_json::Value::as_str) != Some(original_name)
            });
            return;
        }
        let Ok(clean_value) = serde_json::from_str::<serde_json::Value>(clean) else {
            return;
        };
        if let Some(index) = values.iter().position(|value| {
            value.get("name").and_then(serde_json::Value::as_str) == Some(original_name)
        }) {
            values[index] = clean_value;
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn save_named_protocol_target(
    project_signal: &mut Signal<Option<OpenProject>>,
    service: &ProjectService,
    project_root: &std::path::Path,
    key: &str,
    folder: &str,
    original_name: &str,
    content: &str,
    remote: bool,
) -> Result<String, String> {
    let value = serde_json::from_str::<serde_json::Value>(content)
        .map_err(|error| format!("Invalid protocol JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Protocol configuration must be a JSON object.".to_owned())?;
    let new_name = object
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "Protocol name cannot be empty.".to_owned())?
        .to_owned();
    let protocol = object
        .get("protocol")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Protocol configuration is missing protocol.".to_owned())?;
    let supported = if remote {
        matches!(
            protocol,
            "modbus-tcp" | "ethercat" | "ethernet-ip" | "profinet"
        )
    } else {
        matches!(protocol, "modbus-tcp" | "s7comm" | "opcua" | "ethernet-ip")
    };
    if !supported {
        return Err(format!("Unsupported protocol {protocol:?}."));
    }
    let duplicate = project_signal
        .read()
        .as_ref()
        .and_then(|project| project.project.data.get(key))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .any(|candidate| {
            candidate
                .get("name")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|name| {
                    !name.eq_ignore_ascii_case(original_name)
                        && name.eq_ignore_ascii_case(&new_name)
                })
        });
    if duplicate {
        return Err(format!("Protocol item {new_name:?} already exists."));
    }

    let mut serialized = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
    serialized.push('\n');
    service
        .save_project_file(
            project_root,
            &format!("{folder}/{new_name}.json"),
            &serialized,
        )
        .map_err(|error| error.to_string())?;
    if !new_name.eq(original_name) {
        service
            .delete_project_file(project_root, &format!("{folder}/{original_name}.json"))
            .map_err(|error| error.to_string())?;
    }

    project_signal.with_mut(|project| {
        let Some(project) = project else {
            return;
        };
        let Some(data) = project.project.data.as_object_mut() else {
            return;
        };
        let values = data
            .entry(key.to_owned())
            .or_insert_with(|| serde_json::Value::Array(Vec::new()));
        let Some(values) = values.as_array_mut() else {
            return;
        };
        if let Some(index) = values.iter().position(|candidate| {
            candidate.get("name").and_then(serde_json::Value::as_str) == Some(original_name)
        }) {
            values[index] = value.clone();
        } else {
            values.push(value.clone());
        }
    });
    Ok(new_name)
}

fn retarget_named_protocol_tab(
    store: &mut AppStore,
    old_name: &str,
    new_name: &str,
    target: TabTarget,
) {
    store.tabs.with_mut(|tabs| {
        if let Some(tab) = tabs.iter_mut().find(|tab| tab.name == old_name) {
            tab.name = new_name.to_owned();
            tab.target = target;
        }
    });
    store.editor_documents.with_mut(|documents| {
        if let Some(document) = documents.remove(old_name) {
            documents.insert(new_name.to_owned(), document);
        }
    });
    if (store.selected_tab)().as_deref() == Some(old_name) {
        store.selected_tab.set(Some(new_name.to_owned()));
    }
}
