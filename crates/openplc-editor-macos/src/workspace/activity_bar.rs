use dioxus::prelude::*;
use open_rust_plc_compat_core::lifecycle::PlcState;

use crate::app::{AppStore, ConsoleEntry};
use crate::icons::MagnifierIcon;
use crate::local_runtime::LocalPlcService;
use crate::plc_model::PouLanguage;
use crate::project::{ProjectKind, ProjectService};
use crate::workspace_icons::{DownloadIcon, ExitIcon, FilesIcon, OpenPlcPlayIcon, OpenPlcStopIcon};

#[component]
pub fn WorkspaceActivityBar() -> Element {
    let project_service = use_context::<ProjectService>();
    let runtime = use_context::<LocalPlcService>();
    let control_project_service = project_service.clone();
    let exit_project_service = project_service;
    let control_runtime = runtime.clone();
    let exit_runtime = runtime;
    let mut store = use_context::<AppStore>();
    let plc_state = (store.plc_state)();
    let is_compiling = (store.is_compiling)();
    let project = (store.project)();
    let is_program_project = project
        .as_ref()
        .is_some_and(|project| project.project.meta.kind == ProjectKind::PlcProject);
    let graphical_language = (store.selected_tab)().and_then(|selected| {
        project
            .as_ref()?
            .pous
            .iter()
            .find(|file| file.name == selected)?
            .parsed
            .as_ref()
            .map(|pou| pou.body.language)
            .filter(|language| language.is_graphical())
    });

    rsx! {
        div { class: "sidebar-scroll my-5 flex min-h-0 w-full flex-1 flex-col items-center gap-5 overflow-y-auto",
            SidebarButton { label: "Explorer", active: true,
                FilesIcon {}
            }
            SidebarButton {
                label: "Search",
                active: false,
                onclick: move |_| store.search_dialog_open.set(true),
                MagnifierIcon {}
            }
            if let Some(language) = graphical_language {
                hr { class: "w-1/2 stroke-neutral-850 stroke-[1.5px]" }
                GraphicalToolbox { language }
            }
            hr { class: "w-1/2 stroke-neutral-850 stroke-[1.5px]" }
            if is_program_project {
                ActionButton {
                    label: if is_compiling { "Compiling…" } else { "Use Start to build and run" },
                    disabled: true,
                    DownloadIcon {}
                }
                ActionButton {
                    label: if plc_state == PlcState::Running { "Stop Simulator" } else { "Start Simulator" },
                    disabled: is_compiling,
                    onclick: move |_| {
                        if is_compiling {
                            return;
                        }
                        if plc_state == PlcState::Running {
                            match control_runtime.stop() {
                                Ok(_) => {
                                    let state = control_runtime.state().unwrap_or(PlcState::Stopped);
                                    store.plc_state.set(state);
                                    push_log(&mut store, "info", "Simulator stopped.");
                                }
                                Err(error) => push_log(&mut store, "error", error),
                            }
                            return;
                        }

                        let saved = store.save_all(&control_project_service);
                        if let Err(error) = saved {
                            push_log(&mut store, "error", error);
                            return;
                        }
                        let Some(project) = (store.project)() else {
                            return;
                        };

                        store.is_compiling.set(true);
                        push_log(&mut store, "info", "Build process started");
                        match control_runtime.build_project(&project) {
                            Ok(report) => {
                                if !report.diagnostics.trim().is_empty() {
                                    push_log(&mut store, "info", report.diagnostics.trim());
                                }
                                push_log(
                                    &mut store,
                                    "info",
                                    format!(
                                        "Build successful: {} sources, {} bytes → {}",
                                        report.source_count,
                                        report.source_bytes,
                                        report.output.display()
                                    ),
                                );
                                match control_runtime.start() {
                                    Ok(_) => {
                                        let state = control_runtime.state().unwrap_or(PlcState::Error);
                                        store.plc_state.set(state);
                                        if state == PlcState::Running {
                                            push_log(&mut store, "info", "Simulator started.");
                                        } else {
                                            push_log(
                                                &mut store,
                                                "error",
                                                format!("Simulator did not enter RUNNING state: {state:?}"),
                                            );
                                        }
                                    }
                                    Err(error) => {
                                        store.plc_state.set(control_runtime.state().unwrap_or(PlcState::Error));
                                        push_log(&mut store, "error", error);
                                    }
                                }
                            }
                            Err(error) => {
                                store.plc_state.set(control_runtime.state().unwrap_or(PlcState::Error));
                                push_log(&mut store, "error", format!("Build error: {error}"));
                            }
                        }
                        store.is_compiling.set(false);
                    },
                    if plc_state == PlcState::Running {
                        OpenPlcStopIcon {}
                    } else {
                        OpenPlcPlayIcon {}
                    }
                }
            }
        }
        div { class: "flex h-7 w-full shrink-0 flex-col gap-6 pb-10",
            button {
                class: "flex h-8 w-8 self-center items-center justify-center rounded text-neutral-500 transition-colors duration-150 hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800",
                title: "Exit",
                "aria-label": "Exit",
                onclick: move |_| {
                    if exit_runtime.state().ok() == Some(PlcState::Running) {
                        if let Err(error) = exit_runtime.stop() {
                            push_log(&mut store, "error", error);
                            return;
                        }
                        store.plc_state.set(PlcState::Stopped);
                    }
                    if store.request_close_project() {
                        store.recent.set(exit_project_service.recent_projects().unwrap_or_default());
                    }
                },
                ExitIcon {}
            }
        }
    }
}

#[component]
fn GraphicalToolbox(language: PouLanguage) -> Element {
    let (mime, tools): (&'static str, &[(&'static str, &'static str, &'static str)]) =
        match language {
            PouLanguage::Ld => (
                "application/reactflow/ladder-blocks",
                &[
                    ("block", "Block", "▣"),
                    ("coil", "Coil", "( )"),
                    ("contact", "Contact", "││"),
                ],
            ),
            PouLanguage::Fbd => (
                "application/reactflow/fbd-blocks",
                &[
                    ("block", "Block", "▣"),
                    ("input-variable", "Input Variable", "→"),
                    ("output-variable", "Output Variable", "←"),
                    ("connector", "Connector", "◀"),
                    ("continuation", "Continuation", "▶"),
                    ("comment", "Comment", "T"),
                ],
            ),
            _ => return VNode::empty(),
        };

    rsx! {
        for (node_type, label, glyph) in tools.iter().copied() {
            GraphicalToolboxButton { mime, node_type, label, glyph }
        }
    }
}

#[component]
fn GraphicalToolboxButton(
    mime: &'static str,
    node_type: &'static str,
    label: &'static str,
    glyph: &'static str,
) -> Element {
    rsx! {
        div {
            class: "flex h-8 w-8 cursor-grab select-none items-center justify-center rounded-md text-neutral-600 transition-colors hover:bg-neutral-100 active:cursor-grabbing dark:text-neutral-300 dark:hover:bg-neutral-900",
            draggable: true,
            title: label,
            "aria-label": label,
            ondragstart: move |event| {
                let transfer = event.data_transfer();
                let _ = transfer.set_data(mime, node_type);
                let _ = transfer.set_data("text/plain", node_type);
                transfer.set_effect_allowed("move");
            },
            span { class: "font-mono text-[11px] font-semibold", "{glyph}" }
        }
    }
}

fn push_log(store: &mut AppStore, level: &'static str, message: impl Into<String>) {
    store.console.with_mut(|console| {
        console.push(ConsoleEntry {
            level,
            message: message.into(),
        });
    });
}

#[component]
fn SidebarButton(
    label: &'static str,
    active: bool,
    onclick: Option<EventHandler<MouseEvent>>,
    children: Element,
) -> Element {
    let class = if active {
        "relative flex h-8 w-8 items-center justify-center rounded bg-blue-500/20 text-blue-500 transition-colors duration-150 dark:text-blue-400"
    } else {
        "relative flex h-8 w-8 items-center justify-center rounded text-neutral-500 transition-colors duration-150 hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800"
    };
    rsx! {
        button {
            class,
            title: label,
            "aria-label": label,
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
fn ActionButton(
    label: String,
    disabled: bool,
    onclick: Option<EventHandler<MouseEvent>>,
    children: Element,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            disabled,
            title: "{label}",
            "aria-label": "{label}",
            class: if disabled {
                "flex h-8 w-full cursor-not-allowed items-center justify-center opacity-50"
            } else {
                "flex h-8 w-full cursor-pointer items-center justify-center"
            },
            onclick: move |event| {
                if !disabled {
                    if let Some(handler) = &onclick {
                        handler.call(event);
                    }
                }
            },
            div { class: "rounded-md p-[5px] transition-all duration-150 hover:bg-brand-medium dark:hover:bg-neutral-900",
                {children}
            }
        }
    }
}
