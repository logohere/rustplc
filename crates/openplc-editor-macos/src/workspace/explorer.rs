use dioxus::prelude::*;

use crate::app::{AppStore, EditorTab, TabTarget};
use crate::icons::{FolderIcon, LanguageIcon, LanguageIconKind};
use crate::library_manager::{project_library_refs, LibraryManager};
use crate::plc_model::PouLanguage;
use crate::project::{OpenProject, PouFile, PouKind, ProjectKind, ProjectService};
use crate::runtime_v4_client::is_user_management_capable_runtime;
use crate::vpp_package::VppPackageManager;
use crate::workspace_icons::{FunctionBlockIcon, FunctionIcon, PlcIcon, ProgramIcon};

#[component]
pub fn Explorer(project: OpenProject) -> Element {
    rsx! {
        div { class: "h-full w-full",
            div { id: "project-container", class: "flex h-full w-full flex-col pr-2",
                div { id: "project-actions-container", class: "relative z-10 my-3 flex w-full justify-normal gap-2 pl-2",
                    div {
                        id: "project-name-container",
                        class: "flex h-8 w-full flex-1 cursor-default select-none items-center justify-start gap-1 rounded-lg bg-neutral-100 px-1.5 py-[1px] dark:bg-brand-dark",
                        div { class: "flex-shrink-0", FolderIcon {} }
                        span {
                            id: "project-name",
                            class: "w-full cursor-pointer overflow-hidden text-ellipsis whitespace-nowrap rounded-lg px-2 py-1 text-xs font-medium text-neutral-1000 dark:text-neutral-50",
                            title: "Edit name project",
                            "{project.name()}"
                        }
                    }
                }
                div { id: "project-tree-container", class: "mb-1 flex h-full w-full flex-col overflow-auto",
                    ProjectTreeRoot { label: project.name().to_owned(),
                        if project.project.meta.kind == ProjectKind::PlcLibrary {
                            ManifestLeaf {}
                        }
                        PouBranch { project: project.clone(), kind: PouKind::Function }
                        PouBranch { project: project.clone(), kind: PouKind::FunctionBlock }
                        if project.project.meta.kind == ProjectKind::PlcProject {
                            PouBranch { project: project.clone(), kind: PouKind::Program }
                        }
                        DataTypesBranch { project: project.clone() }
                        if project.project.meta.kind == ProjectKind::PlcProject {
                            LibrariesBranch { project: project.clone() }
                            SingletonBranch { label: "Resource" }
                            ProtocolBranch { project: project.clone(), remote: false }
                            ProtocolBranch { project: project.clone(), remote: true }
                            DeviceBranch {}
                            VendorScreens { project: project.clone() }
                        }
                    }
                }
                div { class: "h-[1px] w-full bg-neutral-200 dark:bg-neutral-850" }
                div { class: "min-h-[100px] p-2",
                    p { class: "font-caption text-xs font-medium text-neutral-1000 dark:text-white", "Info" }
                }
            }
        }
    }
}

#[component]
fn ProjectTreeRoot(label: String, children: Element) -> Element {
    let mut open = use_signal(|| true);
    rsx! {
        div { class: "select-none",
            ul { class: "list-none p-0",
                li {
                    class: "flex cursor-pointer flex-row items-center py-1 pl-3 hover:bg-slate-50 dark:hover:bg-neutral-900",
                    onclick: move |_| open.toggle(),
                    TreeArrow { open: open() }
                    PlcIcon {}
                    span {
                        class: if open() { "ml-1 truncate font-caption text-xs font-medium text-neutral-1000 dark:text-white" } else { "ml-1 truncate font-caption text-xs font-normal text-neutral-850 dark:text-neutral-300" },
                        "{label}"
                    }
                }
                if open() {
                    ul { class: "list-none p-0", {children} }
                }
            }
        }
    }
}

#[component]
fn PouBranch(project: OpenProject, kind: PouKind) -> Element {
    let mut store = use_context::<AppStore>();
    let mut files = project
        .pous
        .iter()
        .filter(|file| file.kind == kind)
        .cloned()
        .collect::<Vec<_>>();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    let has_files = !files.is_empty();
    let mut open = use_signal(move || has_files);

    rsx! {
        li { "aria-expanded": open(), class: "cursor-pointer aria-expanded:cursor-default",
            div {
                class: "flex w-full cursor-pointer flex-row items-center gap-1 py-1 pl-[18px] hover:bg-slate-50 dark:hover:bg-neutral-900",
                onclick: move |_| {
                    if has_files {
                        open.toggle();
                    }
                },
                if has_files { TreeArrow { open: open() } } else { div { class: "w-[22px]" } }
                div { class: "h-5 w-5",
                    match kind {
                        PouKind::Program => rsx! { ProgramIcon {} },
                        PouKind::Function => rsx! { FunctionIcon {} },
                        PouKind::FunctionBlock => rsx! { FunctionBlockIcon {} },
                    }
                }
                span {
                    class: if open() { "truncate font-caption text-xs font-medium text-neutral-1000 dark:text-white" } else { "truncate font-caption text-xs font-normal text-neutral-850 dark:text-neutral-300" },
                    "{kind.label()}"
                }
            }
            if open() {
                ul { class: "list-none p-0",
                    for file in files {
                        {
                            let name = file.name.clone();
                            rsx! {
                                PouLeaf {
                                    key: "{name}",
                                    file,
                                    onclick: move |_| store.open_pou(&name)
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
fn PouLeaf(file: PouFile, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        li {
            class: "ml-4 flex w-full cursor-pointer flex-row items-center py-1 pl-2 hover:bg-slate-50 dark:hover:bg-neutral-900",
            onclick: move |event| onclick.call(event),
            div { class: "w-[22px]" }
            if let Some(language) = file.language {
                LanguageBadge { language }
            }
            span { class: "ml-1 truncate font-caption text-xs font-normal text-neutral-850 dark:text-neutral-300", "{file.name}" }
        }
    }
}

#[component]
fn ManifestLeaf() -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {
        li {
            class: "ml-4 flex w-full cursor-pointer flex-row items-center py-1 pl-2 hover:bg-slate-50 dark:hover:bg-neutral-900",
            onclick: move |_| {
                let tab = EditorTab {
                    name: "Manifest".to_owned(),
                    target: TabTarget::Manifest,
                    kind: None,
                    language: None,
                };
                store.tabs.with_mut(|tabs| {
                    if !tabs.iter().any(|candidate| candidate.name == tab.name) {
                        tabs.push(tab.clone());
                    }
                });
                store.select_tab("Manifest");
            },
            div { class: "w-[22px]" }
            span { class: "flex h-5 w-5 items-center justify-center rounded bg-brand text-[8px] font-bold text-white", "JSON" }
            span { class: "ml-1 truncate font-caption text-xs font-normal text-neutral-850 dark:text-neutral-300", "Manifest" }
        }
    }
}

#[component]
fn DataTypesBranch(project: OpenProject) -> Element {
    let mut store = use_context::<AppStore>();
    let data_types = project
        .project
        .data
        .get("dataTypes")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let has_data_types = !data_types.is_empty();
    let mut open = use_signal(move || has_data_types);
    let mut creating = use_signal(|| false);
    rsx! {
        li { class: "cursor-pointer",
            div {
                class: "flex w-full cursor-pointer flex-row items-center gap-1 py-1 pl-[18px] pr-1 hover:bg-slate-50 dark:hover:bg-neutral-900",
                onclick: move |_| open.toggle(),
                if has_data_types || creating() { TreeArrow { open: open() } } else { div { class: "w-[22px]" } }
                div { class: "flex h-5 w-5 items-center justify-center rounded bg-brand-light text-[9px] font-bold text-brand-dark", "T" }
                span { class: "min-w-0 flex-1 truncate font-caption text-xs font-normal text-neutral-850 dark:text-neutral-300", "Data Types" }
                button {
                    r#type: "button",
                    class: "flex h-5 w-5 items-center justify-center rounded text-sm font-medium text-brand hover:bg-neutral-100 dark:hover:bg-neutral-800",
                    title: "Create data type",
                    onclick: move |event| {
                        event.stop_propagation();
                        creating.toggle();
                        open.set(true);
                    },
                    "+"
                }
            }
            if open() {
                ul { class: "list-none p-0",
                    if creating() {
                        li { class: "ml-8 flex gap-1 py-1 pl-4",
                            for (derivation, label) in [("array", "Array"), ("enumerated", "Enum"), ("structure", "Struct")] {
                                button {
                                    key: "{derivation}",
                                    r#type: "button",
                                    class: "rounded border border-neutral-200 px-2 py-1 font-caption text-[10px] text-neutral-600 hover:border-brand hover:text-brand dark:border-neutral-800 dark:text-neutral-300",
                                    onclick: move |event| {
                                        event.stop_propagation();
                                        if store.create_data_type(derivation).is_some() {
                                            creating.set(false);
                                        }
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                    for data_type in data_types.iter() {
                        if let Some(name) = data_type.get("name").and_then(serde_json::Value::as_str) {
                            {
                                let name = name.to_owned();
                                rsx! {
                                    li {
                                        key: "{name}",
                                        class: "ml-4 flex w-full cursor-pointer flex-row items-center py-1 pl-8 font-caption text-xs text-neutral-850 hover:bg-slate-50 dark:text-neutral-300 dark:hover:bg-neutral-900",
                                        onclick: move |event| {
                                            event.stop_propagation();
                                            store.open_data_type(&name);
                                        },
                                        "{name}"
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
fn LibrariesBranch(project: OpenProject) -> Element {
    let mut store = use_context::<AppStore>();
    let service = use_context::<ProjectService>();
    let (installed, manager_error) =
        match LibraryManager::new().and_then(|manager| manager.list_installed()) {
            Ok(installed) => (installed, None),
            Err(error) => (Vec::new(), Some(error)),
        };
    let (refs, refs_error) = match project_library_refs(&project) {
        Ok(refs) => (refs, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    let has_rows = !installed.is_empty()
        || !refs.is_empty()
        || manager_error.is_some()
        || refs_error.is_some();
    let mut open = use_signal(move || has_rows);

    rsx! {
        li { class: "cursor-pointer",
            div {
                class: "flex w-full cursor-pointer flex-row items-center gap-1 py-1 pl-[18px] pr-1 hover:bg-slate-50 dark:hover:bg-neutral-900",
                onclick: move |_| open.toggle(),
                if has_rows { TreeArrow { open: open() } } else { div { class: "w-[22px]" } }
                div { class: "flex h-5 w-5 items-center justify-center rounded bg-brand-light text-[9px] font-bold text-brand-dark", "L" }
                span { class: "min-w-0 flex-1 truncate font-caption text-xs font-normal text-neutral-850 dark:text-neutral-300", "Libraries" }
                button {
                    r#type: "button",
                    class: "flex h-5 w-5 items-center justify-center rounded text-sm font-medium text-brand hover:bg-neutral-100 dark:hover:bg-neutral-800",
                    title: "Open Library Manager",
                    onclick: move |event| {
                        event.stop_propagation();
                        store.open_library_manager();
                    },
                    "…"
                }
            }
            if open() {
                ul { class: "list-none p-0",
                    if let Some(error) = manager_error.as_ref() {
                        li { class: "ml-4 flex w-full flex-row items-center py-1 pl-8 font-caption text-xs text-neutral-850 dark:text-neutral-300", "{error}" }
                    }
                    if let Some(error) = refs_error.as_ref() {
                        li { class: "ml-4 flex w-full flex-row items-center py-1 pl-8 font-caption text-xs text-neutral-850 dark:text-neutral-300", "{error}" }
                    }
                    for library in installed.iter() {
                        {
                            let name = library.name.clone();
                            let version = library.version.clone();
                            let enabled = library.bundled || refs.iter().any(|library_ref| library_ref.name == name);
                            let row_service = service.clone();
                            rsx! {
                                li {
                                    key: "{name}",
                                    class: "ml-4 flex w-full flex-row items-center gap-1 py-1 pl-8 pr-1 font-caption text-xs text-neutral-850 hover:bg-slate-50 dark:text-neutral-300 dark:hover:bg-neutral-900",
                                    span { class: "min-w-0 flex-1 truncate", "{name} {version}" }
                                    if library.bundled {
                                        span { title: "Bundled — always enabled", class: "flex h-5 items-center justify-center rounded px-1 text-[9px] font-medium text-brand", "always" }
                                    } else {
                                        button {
                                            r#type: "button",
                                            class: "flex h-5 w-5 items-center justify-center rounded text-sm font-medium text-brand hover:bg-neutral-100 dark:hover:bg-neutral-800",
                                            title: if enabled { "Disable library" } else { "Enable library" },
                                            onclick: move |event| {
                                                event.stop_propagation();
                                                if let Err(error) = store.set_library_enabled(&row_service, &name, &version, !enabled) {
                                                    store.console.with_mut(|console| {
                                                        console.push(crate::app::ConsoleEntry { level: "error", message: error });
                                                    });
                                                }
                                            },
                                            if enabled { "✓" } else { "+" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    for library_ref in refs.iter().filter(|library_ref| !installed.iter().any(|library| library.name == library_ref.name)) {
                        {
                            let name = library_ref.name.clone();
                            let version = library_ref.version.clone().unwrap_or_default();
                            let row_service = service.clone();
                            rsx! {
                                li {
                                    key: "missing-{name}",
                                    class: "ml-4 flex w-full flex-row items-center gap-1 py-1 pl-8 pr-1 font-caption text-xs text-neutral-850 dark:text-neutral-300",
                                    span { class: "min-w-0 flex-1 truncate", "{name} {version} (missing)" }
                                    button {
                                        r#type: "button",
                                        class: "flex h-5 w-5 items-center justify-center rounded text-sm font-medium text-brand hover:bg-neutral-100 dark:hover:bg-neutral-800",
                                        title: "Remove missing library from project",
                                        onclick: move |event| {
                                            event.stop_propagation();
                                            if let Err(error) = store.set_library_enabled(&row_service, &name, &version, false) {
                                                store.console.with_mut(|console| {
                                                    console.push(crate::app::ConsoleEntry {
                                                        level: "error",
                                                        message: error,
                                                    });
                                                });
                                            }
                                        },
                                        "×"
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
fn DeviceBranch() -> Element {
    let mut store = use_context::<AppStore>();
    let mut open = use_signal(|| true);
    let show_user_management = (store.runtime_connected)()
        && is_user_management_capable_runtime((store.runtime_version)().as_deref());
    rsx! {
        li { class: "cursor-pointer",
            div {
                class: "flex w-full cursor-pointer flex-row items-center gap-1 py-1 pl-[18px] hover:bg-slate-50 dark:hover:bg-neutral-900",
                onclick: move |_| open.toggle(),
                TreeArrow { open: open() }
                div { class: "flex h-5 w-5 items-center justify-center rounded bg-brand-light text-[9px] font-bold text-brand-dark", "D" }
                span { class: "truncate font-caption text-xs font-normal text-neutral-850 dark:text-neutral-300", "Device" }
            }
            if open() {
                ul { class: "list-none p-0",
                    li {
                        class: "ml-4 flex w-full cursor-pointer flex-row items-center py-1 pl-8 font-caption text-xs text-neutral-850 hover:bg-slate-50 dark:text-neutral-300 dark:hover:bg-neutral-900",
                        onclick: move |event| {
                            event.stop_propagation();
                            store.open_device_configuration();
                        },
                        "Configuration"
                    }
                    if show_user_management {
                        li {
                            class: "ml-4 flex w-full cursor-pointer flex-row items-center py-1 pl-8 font-caption text-xs text-neutral-850 hover:bg-slate-50 dark:text-neutral-300 dark:hover:bg-neutral-900",
                            onclick: move |event| {
                                event.stop_propagation();
                                store.open_user_management();
                            },
                            "User Management"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn VendorScreens(project: OpenProject) -> Element {
    let mut store = use_context::<AppStore>();
    let board = project
        .device_configuration
        .get("deviceBoard")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let screens = VppPackageManager::new()
        .ok()
        .map(|manager| manager.screen_names(board))
        .unwrap_or_default();
    rsx! {
        for screen in screens {
            {
                let screen_name = screen.clone();
                rsx! {
                    li {
                        key: "vendor-{screen}",
                        class: "ml-4 flex w-full cursor-pointer flex-row items-center gap-1 py-1 pl-8 font-caption text-xs text-neutral-850 hover:bg-slate-50 dark:text-neutral-300 dark:hover:bg-neutral-900",
                        onclick: move |event| {
                            event.stop_propagation();
                            if let Err(error) = store.open_vendor_screen(&screen_name) {
                                store.console.with_mut(|console| console.push(crate::app::ConsoleEntry { level: "error", message: error }));
                            }
                        },
                        span { class: "flex h-5 w-5 items-center justify-center rounded bg-brand-light text-[9px] font-bold text-brand-dark", "V" }
                        span { class: "truncate", "{screen}" }
                    }
                }
            }
        }
    }
}

#[component]
fn SingletonBranch(label: &'static str) -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {
        li {
            class: "flex w-full cursor-pointer flex-row items-center gap-1 py-1 pl-[18px] hover:bg-slate-50 dark:hover:bg-neutral-900",
            onclick: move |_| {
                if label == "Resource" {
                    store.open_resource();
                }
            },
            div { class: "w-[22px]" }
            div { class: "flex h-5 w-5 items-center justify-center rounded bg-brand-light text-[9px] font-bold text-brand-dark", "•" }
            span { class: "truncate font-caption text-xs font-normal text-neutral-850 dark:text-neutral-300", "{label}" }
        }
    }
}

#[component]
fn TreeArrow(open: bool) -> Element {
    rsx! {
        svg {
            view_box: "0 0 28 28",
            fill: "none",
            xmlns: "http://www.w3.org/2000/svg",
            class: if open { "mr-[6px] h-4 w-4 -rotate-90 stroke-brand transition-all" } else { "mr-[6px] h-4 w-4 rotate-180 stroke-brand-light transition-all" },
            path { d: "M17.5002 21L10.5002 14L17.5002 7", stroke: "currentColor", stroke_width: "1.5", stroke_linecap: "round", stroke_linejoin: "round" }
        }
    }
}

#[component]
pub(crate) fn LanguageBadge(language: PouLanguage) -> Element {
    let kind = match language {
        PouLanguage::Ld => Some(LanguageIconKind::Ld),
        PouLanguage::St => Some(LanguageIconKind::St),
        PouLanguage::Il => Some(LanguageIconKind::Il),
        PouLanguage::Fbd => Some(LanguageIconKind::Fbd),
        PouLanguage::Python | PouLanguage::Cpp => None,
    };
    if let Some(kind) = kind {
        rsx! { LanguageIcon { kind } }
    } else {
        let text = match language {
            PouLanguage::Python => "PY",
            PouLanguage::Cpp => "C++",
            _ => "",
        };
        rsx! { span { class: "flex h-5 w-5 items-center justify-center rounded bg-neutral-200 text-[8px] font-bold text-brand-dark", "{text}" } }
    }
}

#[component]
fn ProtocolBranch(project: OpenProject, remote: bool) -> Element {
    let mut store = use_context::<AppStore>();
    let key = if remote { "remoteDevices" } else { "servers" };
    let label = if remote { "Remote Devices" } else { "Servers" };
    let icon = if remote { "R" } else { "S" };
    let values = project
        .project
        .data
        .get(key)
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let has_values = !values.is_empty();
    let mut open = use_signal(move || has_values);
    let mut creating = use_signal(|| false);
    rsx! {
        li { class: "cursor-pointer",
            div {
                class: "flex w-full cursor-pointer flex-row items-center gap-1 py-1 pl-[18px] pr-1 hover:bg-slate-50 dark:hover:bg-neutral-900",
                onclick: move |_| open.toggle(),
                if has_values || creating() { TreeArrow { open: open() } } else { div { class: "w-[22px]" } }
                div { class: "flex h-5 w-5 items-center justify-center rounded bg-brand-light text-[9px] font-bold text-brand-dark", "{icon}" }
                span { class: "min-w-0 flex-1 truncate font-caption text-xs font-normal text-neutral-850 dark:text-neutral-300", "{label}" }
                button {
                    r#type: "button",
                    class: "flex h-5 w-5 items-center justify-center rounded text-sm font-medium text-brand hover:bg-neutral-100 dark:hover:bg-neutral-800",
                    title: if remote { "Create remote device" } else { "Create server" },
                    onclick: move |event| {
                        event.stop_propagation();
                        creating.toggle();
                        open.set(true);
                    },
                    "+"
                }
            }
            if open() {
                ul { class: "list-none p-0",
                    if creating() {
                        li { class: "ml-8 flex flex-wrap gap-1 py-1 pl-4",
                            if remote {
                                for (protocol, protocol_label) in [("modbus-tcp", "Modbus"), ("ethercat", "EtherCAT")] {
                                    button {
                                        key: "remote-create-{protocol}", r#type:"button",
                                        class:"rounded border border-neutral-200 px-2 py-1 font-caption text-[9px] text-neutral-600 hover:border-brand hover:text-brand dark:border-neutral-800 dark:text-neutral-300",
                                        onclick: move |event| { event.stop_propagation(); if store.create_remote_device(protocol).is_some() { creating.set(false); } },
                                        "{protocol_label}"
                                    }
                                }
                            } else {
                                for (protocol, protocol_label) in [("modbus-tcp", "Modbus"), ("s7comm", "S7Comm"), ("opcua", "OPC-UA")] {
                                    button {
                                        key: "server-create-{protocol}", r#type:"button",
                                        class:"rounded border border-neutral-200 px-2 py-1 font-caption text-[9px] text-neutral-600 hover:border-brand hover:text-brand dark:border-neutral-800 dark:text-neutral-300",
                                        onclick: move |event| { event.stop_propagation(); if store.create_server(protocol).is_some() { creating.set(false); } },
                                        "{protocol_label}"
                                    }
                                }
                            }
                        }
                    }
                    for value in values.iter() {
                        if let Some(name) = value.get("name").and_then(serde_json::Value::as_str) {
                            {
                                let name = name.to_owned();
                                let protocol = value.get("protocol").and_then(serde_json::Value::as_str).unwrap_or("unknown").to_owned();
                                rsx! {
                                    li {
                                        key:"{label}-{name}",
                                        class:"ml-4 flex w-full cursor-pointer items-center gap-2 py-1 pl-8 pr-2 font-caption text-xs text-neutral-850 hover:bg-slate-50 dark:text-neutral-300 dark:hover:bg-neutral-900",
                                        onclick: move |event| { event.stop_propagation(); if remote { store.open_remote_device(&name); } else { store.open_server(&name); } },
                                        span { class:"min-w-0 flex-1 truncate", "{name}" }
                                        span { class:"rounded bg-neutral-100 px-1 py-0.5 font-mono text-[8px] text-neutral-500 dark:bg-neutral-800", "{protocol}" }
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
