use dioxus::prelude::*;

use crate::app::{AppStore, ConsoleEntry};
use crate::library_manager::{project_library_refs, InstalledLibrary, LibraryManager};
use crate::project::ProjectService;
use crate::public_library_catalog::PublicLibrary;

#[component]
pub fn LibraryManagerEditor() -> Element {
    let mut store = use_context::<AppStore>();
    let mut page = use_signal(|| "system".to_owned());
    let mut selected = use_signal(|| None::<String>);
    let filter = use_signal(String::new);
    let mut refresh = use_signal(|| 0u64);
    let _refresh_tick = refresh();
    let mut catalog_open = use_signal(|| false);
    let catalog_query = use_signal(String::new);
    let catalog_rows = use_signal(Vec::<PublicLibrary>::new);
    let catalog_error = use_signal(|| None::<String>);
    let catalog_loading = use_signal(|| false);

    let manager = LibraryManager::new();
    let installed = manager
        .as_ref()
        .ok()
        .and_then(|manager| manager.list_installed().ok())
        .unwrap_or_default();
    let archives = manager
        .as_ref()
        .ok()
        .and_then(|manager| manager.load_all().ok())
        .unwrap_or_default();
    let refs = (store.project)()
        .as_ref()
        .and_then(|project| project_library_refs(project).ok())
        .unwrap_or_default();
    let selected_name = selected()
        .filter(|name| installed.iter().any(|library| &library.name == name))
        .or_else(|| installed.first().map(|library| library.name.clone()));

    rsx! {
        div { class: "flex h-full min-h-0 w-full flex-col gap-4",
            div { class: "flex shrink-0 items-center justify-between border-b border-neutral-200 pb-2 dark:border-neutral-800",
                div {
                    h2 { class: "font-display text-lg font-semibold text-neutral-1000 dark:text-white", "Library Manager" }
                    p { class: "font-caption text-xs text-neutral-500", "Manage system libraries and per-project enablement." }
                }
                div { role: "tablist", class: "flex rounded-md border border-neutral-200 p-1 dark:border-neutral-700",
                    button {
                        r#type: "button",
                        role: "tab",
                        "aria-selected": page() == "system",
                        class: if page() == "system" { "rounded px-3 py-1.5 font-caption text-xs font-medium bg-brand text-white" } else { "rounded px-3 py-1.5 font-caption text-xs font-medium text-neutral-700 dark:text-neutral-300" },
                        onclick: move |_| page.set("system".to_owned()),
                        "System Libraries"
                    }
                    button {
                        r#type: "button",
                        role: "tab",
                        "aria-selected": page() == "project",
                        class: if page() == "project" { "rounded px-3 py-1.5 font-caption text-xs font-medium bg-brand text-white" } else { "rounded px-3 py-1.5 font-caption text-xs font-medium text-neutral-700 dark:text-neutral-300" },
                        onclick: move |_| page.set("project".to_owned()),
                        "Project Libraries"
                    }
                }
            }
            if let Err(error) = manager.as_ref() {
                div { role: "alert", class: "rounded border border-red-400 bg-red-50 p-3 font-caption text-xs text-red-700 dark:bg-red-950/30 dark:text-red-300", "{error}" }
            } else if page() == "system" {
                div { class: "flex min-h-0 flex-1 gap-4 overflow-hidden",
                    div { class: "flex w-1/2 min-w-[300px] flex-col gap-3 overflow-hidden",
                        div { class: "flex items-center justify-between",
                            h3 { class: "font-display text-sm font-medium", "Installed Libraries" }
                            div { class: "flex gap-1",
                                button {
                                    r#type: "button",
                                    class: "rounded-md border border-neutral-200 px-2 py-1 font-caption text-xs font-medium hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-900",
                                    title: "Browse public libraries",
                                    onclick: move |_| {
                                        catalog_open.set(true);
                                        request_catalog(catalog_query(), catalog_rows, catalog_error, catalog_loading);
                                    },
                                    "Browse Catalog"
                                }
                                button {
                                    r#type: "button",
                                    class: "rounded-md border border-neutral-200 px-2 py-1 font-caption text-xs font-medium hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-900",
                                    title: "Add .stlib, .lib, or .library",
                                    onclick: move |_| {
                                        let Some(path) = rfd::FileDialog::new()
                                            .set_title("Import PLC library")
                                            .add_filter("PLC library", &["stlib", "lib", "library"])
                                            .pick_file()
                                        else { return; };
                                        match store.install_library(&path) {
                                            Ok(name) => {
                                                selected.set(Some(name));
                                                refresh += 1;
                                            }
                                            Err(error) => push_console_error(&mut store, error),
                                        }
                                    },
                                    "+ Add from file"
                                }
                                {
                                    let selected_row = selected_name
                                        .as_ref()
                                        .and_then(|name| installed.iter().find(|library| &library.name == name))
                                        .cloned();
                                    let can_uninstall = selected_row.as_ref().is_some_and(|library| !library.bundled);
                                    let uninstall_name = selected_row.as_ref().map(|library| library.name.clone()).unwrap_or_default();
                                    rsx! {
                                        button {
                                            r#type: "button",
                                            disabled: !can_uninstall,
                                            class: "rounded-md border border-neutral-200 px-2 py-1 font-caption text-xs font-medium hover:bg-neutral-100 disabled:pointer-events-none disabled:opacity-30 dark:border-neutral-700 dark:hover:bg-neutral-900",
                                            title: if can_uninstall { "Uninstall selected library" } else { "Bundled libraries cannot be uninstalled" },
                                            onclick: move |_| {
                                                if uninstall_name.is_empty() { return; }
                                                match store.uninstall_library(&uninstall_name) {
                                                    Ok(()) => {
                                                        selected.set(None);
                                                        refresh += 1;
                                                    }
                                                    Err(error) => push_console_error(&mut store, error),
                                                }
                                            },
                                            "− Uninstall"
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "min-h-0 flex-1 overflow-y-auto rounded-md border border-neutral-200 dark:border-neutral-700",
                            for library in installed.iter() {
                                {
                                    let name = library.name.clone();
                                    let display = display_name(library);
                                    let version = library.version.clone();
                                    let metadata = library_metadata(library);
                                    let active = selected_name.as_deref() == Some(library.name.as_str());
                                    rsx! {
                                        button {
                                            key: "{library.name}",
                                            r#type: "button",
                                            "aria-selected": active,
                                            class: if active { "flex w-full items-center justify-between border-b border-neutral-100 bg-brand/20 px-3 py-2 text-left dark:border-neutral-800" } else { "flex w-full items-center justify-between border-b border-neutral-100 px-3 py-2 text-left hover:bg-neutral-100 dark:border-neutral-800 dark:hover:bg-neutral-900" },
                                            onclick: move |_| selected.set(Some(name.clone())),
                                            div { class: "min-w-0",
                                                div { class: "truncate font-caption text-xs font-medium text-neutral-950 dark:text-white", "{display}" }
                                                div { class: "text-[11px] text-neutral-500", "v{version}{metadata}" }
                                            }
                                            if library.bundled {
                                                span { class: "rounded-full bg-[#0464fb26] px-2 py-0.5 text-[10px] font-medium text-brand", "bundled" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "h-full w-px bg-neutral-200 dark:bg-neutral-800" }
                    div { class: "flex w-1/2 min-w-[300px] flex-col gap-3 overflow-y-auto",
                        if let Some(name) = selected_name.as_ref() {
                            if let Some(row) = installed.iter().find(|library| &library.name == name) {
                                h3 { class: "font-display text-base font-medium", "{display_name(row)}" }
                                DetailRow { label: "Identifier", value: row.name.clone() }
                                DetailRow { label: "Version", value: row.version.clone() }
                                DetailRow { label: "Source", value: if row.bundled { "Bundled with RustPLC / STruC++".to_owned() } else if row.origin == "codesys" { "CODESYS import (.lib / .library)".to_owned() } else { "STruC++ archive (.stlib)".to_owned() } }
                                if !row.installed_at.is_empty() { DetailRow { label: "Installed", value: row.installed_at.clone() } }
                                if let Some(description) = row.description.clone() { DetailRow { label: "Description", value: description } }
                                if let Some(archive) = archives.iter().find(|archive| archive.manifest.name == row.name) {
                                    div { class: "mt-2 grid grid-cols-2 gap-2 lg:grid-cols-4",
                                        CountCard { label: "Functions", count: archive.manifest.functions.len() }
                                        CountCard { label: "Function Blocks", count: archive.manifest.function_blocks.len() }
                                        CountCard { label: "Types", count: archive.manifest.types.len() }
                                        CountCard { label: "Globals", count: archive.manifest.globals.len() }
                                    }
                                    div { class: "mt-2 rounded-md border border-neutral-200 p-3 dark:border-neutral-700",
                                        div { class: "mb-2 font-caption text-xs font-medium", "Contents" }
                                        for name in archive.manifest.functions.iter().map(|item| format!("FUNCTION · {}", item.name))
                                            .chain(archive.manifest.function_blocks.iter().map(|item| format!("FUNCTION BLOCK · {}", item.name)))
                                            .chain(archive.manifest.types.iter().map(|item| format!("TYPE · {}", item.name)))
                                            .take(80) {
                                            div { class: "truncate py-0.5 font-caption text-[11px] text-neutral-600 dark:text-neutral-400", "{name}" }
                                        }
                                    }
                                }
                            }
                        } else {
                            div { class: "m-auto font-caption text-xs text-neutral-500", "Select a library to view details." }
                        }
                    }
                }
            } else {
                ProjectLibrariesPane { installed: installed.clone(), refs, filter, refresh }
            }
            if catalog_open() {
                CatalogBrowser {
                    query: catalog_query,
                    rows: catalog_rows,
                    error: catalog_error,
                    loading: catalog_loading,
                    installed: installed.clone(),
                    refresh,
                    onclose: move |_| catalog_open.set(false)
                }
            }
        }
    }
}

#[component]
fn CatalogBrowser(
    mut query: Signal<String>,
    rows: Signal<Vec<PublicLibrary>>,
    error: Signal<Option<String>>,
    loading: Signal<bool>,
    installed: Vec<InstalledLibrary>,
    mut refresh: Signal<u64>,
    onclose: EventHandler<MouseEvent>,
) -> Element {
    let installed_names = installed
        .iter()
        .map(|library| library.name.to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();
    rsx! {
        div { class: "fixed inset-0 z-[1000] flex items-center justify-center bg-black/30 p-6",
            div {
                role: "dialog",
                "aria-modal": "true",
                "aria-label": "Browse Public Libraries",
                class: "flex h-[600px] w-[680px] max-w-[calc(100vw-2rem)] flex-col rounded-lg border border-neutral-200 bg-white p-5 shadow-2xl dark:border-neutral-700 dark:bg-neutral-950",
                div { class: "mb-3 flex items-start justify-between",
                    div {
                        h3 { class: "font-display text-lg font-semibold", "Browse Public Libraries" }
                        p { class: "font-caption text-xs text-neutral-500", "Community-published OpenPLC libraries." }
                    }
                    button {
                        r#type: "button",
                        class: "rounded px-2 py-1 text-sm hover:bg-neutral-100 dark:hover:bg-neutral-900",
                        onclick: move |event| onclose.call(event),
                        "Close"
                    }
                }
                form {
                    class: "mb-3 flex gap-2",
                    onsubmit: move |event| {
                        event.prevent_default();
                        request_catalog(query(), rows, error, loading);
                    },
                    input {
                        r#type: "search",
                        value: "{query}",
                        placeholder: "Search name, author, description...",
                        class: "h-9 min-w-0 flex-1 rounded-md border border-neutral-200 bg-transparent px-3 font-caption text-xs outline-none focus:border-brand dark:border-neutral-700",
                        oninput: move |event| query.set(event.value())
                    }
                    button {
                        r#type: "submit",
                        disabled: loading(),
                        class: "rounded-md bg-brand px-3 py-1 font-caption text-xs font-medium text-white disabled:opacity-50",
                        if loading() { "Loading..." } else { "Search" }
                    }
                }
                if let Some(message) = error() {
                    div {
                        role: "alert",
                        class: "mb-3 rounded-md border border-red-300 bg-red-50 p-2 font-caption text-xs text-red-700 dark:bg-red-950/30 dark:text-red-300",
                        "{message}"
                    }
                }
                div { class: "min-h-0 flex-1 overflow-y-auto rounded-md border border-neutral-200 dark:border-neutral-700",
                    if loading() && rows().is_empty() {
                        div { class: "flex h-full items-center justify-center font-caption text-xs text-neutral-500", "Loading public catalog..." }
                    } else if rows().is_empty() {
                        div { class: "flex h-full items-center justify-center font-caption text-xs text-neutral-500", "No public libraries found." }
                    } else {
                        for library in rows() {
                            {
                                let already_installed = installed_names.contains(&library.name.to_ascii_lowercase());
                                let install_library = library.clone();
                                let id = library.id.clone();
                                let name = library.name.clone();
                                let version = library.version.clone();
                                let display = library.display_name.clone();
                                let author = library.author_handle.clone();
                                let description = library.description.clone().unwrap_or_default();
                                let downloads = library.downloads_count;
                                let metadata = format!("{name} - v{version} - @{author} - {downloads} downloads");
                                let mut error_signal = error;
                                let mut loading_signal = loading;
                                rsx! {
                                    div {
                                        key: "{id}",
                                        class: "flex items-center justify-between gap-3 border-b border-neutral-100 px-3 py-2 last:border-b-0 dark:border-neutral-800",
                                        div { class: "min-w-0 flex-1",
                                            div { class: "flex items-center gap-2",
                                                span { class: "truncate font-caption text-xs font-medium", "{display}" }
                                                if already_installed {
                                                    span { class: "rounded-full bg-[#0464fb26] px-2 py-0.5 text-[10px] text-brand", "installed" }
                                                }
                                            }
                                            div { class: "font-caption text-[11px] text-neutral-500", "{metadata}" }
                                            if !description.is_empty() {
                                                div { class: "mt-1 line-clamp-2 font-caption text-[11px] text-neutral-600 dark:text-neutral-400", "{description}" }
                                            }
                                        }
                                        button {
                                            r#type: "button",
                                            disabled: loading(),
                                            class: "shrink-0 rounded-md border border-neutral-200 px-2 py-1 font-caption text-xs font-medium hover:bg-neutral-100 disabled:opacity-50 dark:border-neutral-700 dark:hover:bg-neutral-900",
                                            onclick: move |_| {
                                                loading_signal.set(true);
                                                error_signal.set(None);
                                                let install_library = install_library.clone();
                                                spawn(async move {
                                                    let task = tokio::task::spawn_blocking(move || {
                                                        LibraryManager::new()?.install_from_catalog(&install_library)
                                                    }).await;
                                                    match task {
                                                        Ok(Ok(_)) => refresh += 1,
                                                        Ok(Err(message)) => error_signal.set(Some(message)),
                                                        Err(join_error) => error_signal.set(Some(format!("Catalog install task failed: {join_error}"))),
                                                    }
                                                    loading_signal.set(false);
                                                });
                                            },
                                            if already_installed { "Reinstall" } else { "Install" }
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

fn request_catalog(
    query: String,
    mut rows: Signal<Vec<PublicLibrary>>,
    mut error: Signal<Option<String>>,
    mut loading: Signal<bool>,
) {
    loading.set(true);
    error.set(None);
    spawn(async move {
        let task = tokio::task::spawn_blocking(move || {
            LibraryManager::new()?.query_public_catalog(&query)
        })
        .await;
        match task {
            Ok(Ok(page)) => rows.set(page.libraries),
            Ok(Err(message)) => error.set(Some(message)),
            Err(join_error) => {
                error.set(Some(format!("Catalog request task failed: {join_error}")))
            }
        }
        loading.set(false);
    });
}

#[component]
fn ProjectLibrariesPane(
    installed: Vec<InstalledLibrary>,
    refs: Vec<crate::library_manager::ProjectLibraryRef>,
    mut filter: Signal<String>,
    mut refresh: Signal<u64>,
) -> Element {
    let mut store = use_context::<AppStore>();
    let service = use_context::<ProjectService>();
    let query = filter().trim().to_ascii_lowercase();
    let enabled_names = refs
        .iter()
        .map(|item| item.name.as_str())
        .collect::<Vec<_>>();
    let bundled = installed
        .iter()
        .filter(|library| library.bundled)
        .cloned()
        .collect::<Vec<_>>();
    let enabled = installed
        .iter()
        .filter(|library| !library.bundled && enabled_names.contains(&library.name.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let available = installed
        .iter()
        .filter(|library| {
            !library.bundled
                && !enabled_names.contains(&library.name.as_str())
                && (query.is_empty()
                    || library.name.to_ascii_lowercase().contains(&query)
                    || library
                        .display_name
                        .as_deref()
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .contains(&query))
        })
        .cloned()
        .collect::<Vec<_>>();
    let missing = refs
        .iter()
        .filter(|item| !installed.iter().any(|library| library.name == item.name))
        .cloned()
        .collect::<Vec<_>>();

    rsx! {
        div { class: "flex min-h-0 flex-1 gap-4 overflow-hidden",
            LibraryCard { title: "Available Libraries", subtitle: "Pick a user-installed library to add to this project.",
                input {
                    r#type: "search",
                    value: "{filter}",
                    placeholder: "Search libraries…",
                    class: "mb-2 w-full rounded-md border border-neutral-200 bg-transparent px-2 py-1.5 font-caption text-xs dark:border-neutral-700",
                    oninput: move |event| filter.set(event.value())
                }
                for library in available {
                    {
                        let name = library.name.clone();
                        let version = library.version.clone();
                        let row_service = service.clone();
                        rsx! { LibraryActionRow { library, action: "+", title: "Add to project", onclick: move |_| {
                            if let Err(error) = store.set_library_enabled(&row_service, &name, &version, true) { push_console_error(&mut store, error); }
                            refresh += 1;
                        } } }
                    }
                }
            }
            LibraryCard { title: "Project Libraries", subtitle: format!("{} bundled, {} added", bundled.len(), enabled.len()),
                if !missing.is_empty() {
                    div { role: "alert", class: "mb-2 rounded-md border border-yellow-400 bg-yellow-50 p-2 font-caption text-xs text-yellow-800 dark:bg-yellow-950/30 dark:text-yellow-200",
                        div { class: "font-medium", "Missing libraries" }
                        for item in missing {
                            {
                                let label = missing_label(&item);
                                rsx! { div { "{label}" } }
                            }
                        }
                    }
                }
                for library in bundled {
                    LibraryActionRow { library, action: "•", title: "Bundled — always enabled" }
                }
                for library in enabled {
                    {
                        let name = library.name.clone();
                        let version = library.version.clone();
                        let row_service = service.clone();
                        rsx! { LibraryActionRow { library, action: "−", title: "Remove from project", onclick: move |_| {
                            if let Err(error) = store.set_library_enabled(&row_service, &name, &version, false) { push_console_error(&mut store, error); }
                            refresh += 1;
                        } } }
                    }
                }
            }
        }
    }
}

#[component]
fn LibraryCard(title: String, subtitle: String, children: Element) -> Element {
    rsx! { div { class: "flex min-h-0 w-1/2 min-w-[280px] flex-col overflow-hidden rounded-md border border-neutral-200 dark:border-neutral-700",
        div { class: "border-b border-neutral-200 px-3 py-2 dark:border-neutral-700",
            h3 { class: "font-caption text-sm font-semibold", "{title}" }
            p { class: "text-[11px] text-neutral-500", "{subtitle}" }
        }
        div { class: "min-h-0 flex-1 overflow-y-auto p-3", {children} }
    } }
}

#[component]
fn LibraryActionRow(
    library: InstalledLibrary,
    action: String,
    title: String,
    onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    let display = display_name(&library);
    let version = library.version.clone();
    let metadata = if library.bundled { " - bundled" } else { "" };
    rsx! { div { class: "flex items-center justify-between border-b border-neutral-100 px-2 py-2 last:border-b-0 dark:border-neutral-800",
        div { class: "min-w-0",
            div { class: "truncate font-caption text-xs font-medium", "{display}" }
            div { class: "text-[11px] text-neutral-500", "v{version}{metadata}" }
        }
        if let Some(handler) = onclick {
            button { r#type: "button", title: "{title}", class: "h-7 w-7 rounded-md hover:bg-neutral-100 dark:hover:bg-neutral-900", onclick: move |event| handler.call(event), "{action}" }
        } else {
            span { title: "{title}", class: "rounded-full bg-[#0464fb26] px-2 py-0.5 text-[10px] font-medium text-brand", "always on" }
        }
    } }
}

#[component]
fn DetailRow(label: String, value: String) -> Element {
    rsx! { div { class: "flex flex-col gap-0.5", span { class: "text-xs font-medium text-neutral-500", "{label}" } span { class: "break-words font-caption text-xs", "{value}" } } }
}

#[component]
fn CountCard(label: String, count: usize) -> Element {
    rsx! { div { class: "rounded-md border border-neutral-200 p-2 dark:border-neutral-700", div { class: "text-[10px] text-neutral-500", "{label}" } div { class: "font-display text-lg font-semibold", "{count}" } } }
}

fn display_name(library: &InstalledLibrary) -> String {
    library
        .display_name
        .clone()
        .unwrap_or_else(|| library.name.clone())
}

fn library_metadata(library: &InstalledLibrary) -> &'static str {
    if library.bundled {
        " - bundled"
    } else if library.origin == "codesys" {
        " - CODESYS import"
    } else {
        ""
    }
}

fn missing_label(item: &crate::library_manager::ProjectLibraryRef) -> String {
    match item.version.as_deref() {
        Some(version) => format!("{} - v{version}", item.name),
        None => item.name.clone(),
    }
}

fn push_console_error(store: &mut AppStore, error: String) {
    store.console.with_mut(|console| {
        console.push(ConsoleEntry {
            level: "error",
            message: error,
        })
    });
}
