use dioxus::prelude::*;

use crate::app::AppStore;
use crate::icons::{ArrowIcon, CloseIcon};
use crate::project::{OpenProject, PouKind};
use crate::workspace::explorer::LanguageBadge;
use crate::workspace_icons::{FunctionBlockIcon, FunctionIcon, PlcIcon, ProgramIcon};

#[component]
pub fn Navigation(project: OpenProject) -> Element {
    let mut store = use_context::<AppStore>();
    let tabs = (store.tabs)();
    let selected = (store.selected_tab)();

    rsx! {
        div { class: "h-[70px] w-full overflow-hidden rounded-lg border-2 border-neutral-200 bg-white dark:border-neutral-850 dark:bg-neutral-950",
            div { role: "tablist", class: "isolate flex border-none outline-none", "aria-label": "Tabs list",
                for tab in tabs {
                    {
                        let name_for_click = tab.name.clone();
                        let name_for_close = tab.name.clone();
                        let current = selected.as_deref() == Some(tab.name.as_str());
                        rsx! {
                            div {
                                key: "{tab.name}",
                                role: "tab",
                                class: if current {
                                    "aria-[current=page]:dark:bg-brand-dark group relative flex h-[30px] min-w-0 max-w-[160px] flex-1 cursor-pointer items-center justify-between overflow-hidden bg-neutral-100 text-start font-display text-xs font-normal text-neutral-1000 dark:bg-neutral-800 dark:text-white"
                                } else {
                                    "border-r border-neutral-300 opacity-[35%] aria-[current=page]:dark:bg-brand-dark group relative flex h-[30px] min-w-0 max-w-[160px] flex-1 cursor-pointer items-center justify-between overflow-hidden bg-neutral-100 text-start font-display text-xs font-normal text-neutral-1000 dark:bg-neutral-800 dark:text-white"
                                },
                                "aria-current": if current { "page" } else { "false" },
                                div {
                                    class: "flex h-full w-full items-center gap-1 px-3 py-2",
                                    onclick: move |_| store.select_tab(&name_for_click),
                                    if let Some(language) = tab.language {
                                        LanguageBadge { language }
                                    }
                                    span { class: "flex-grow overflow-hidden text-ellipsis whitespace-nowrap", "{tab.name}" }
                                    span { "aria-hidden": "true", class: if current { "absolute inset-x-0 top-0 z-50 h-[3px] bg-brand" } else { "absolute inset-x-0 top-0 z-50 h-[3px] bg-transparent" } }
                                }
                                button {
                                    class: "absolute right-2 z-[999] hidden h-4 w-4 rounded-sm stroke-brand p-[2px] hover:bg-neutral-300 group-hover:block dark:stroke-brand-light dark:hover:bg-neutral-700",
                                    onclick: move |event| {
                                        event.stop_propagation();
                                        store.request_close_tab(&name_for_close);
                                    },
                                    CloseIcon {}
                                }
                            }
                        }
                    }
                }
            }
            Breadcrumbs { project }
        }
    }
}

#[component]
fn Breadcrumbs(project: OpenProject) -> Element {
    let store = use_context::<AppStore>();
    let selected = (store.selected_tab)();
    let tab = selected
        .as_ref()
        .and_then(|selected| (store.tabs)().into_iter().find(|tab| &tab.name == selected));

    rsx! {
        ol { class: "flex h-1/2 cursor-default select-none items-center p-2",
            li {
                BreadcrumbItem { text: project.name().to_owned(), last: tab.is_none(),
                    PlcIcon {}
                }
            }
            if let Some(tab) = tab {
                if let Some(kind) = tab.kind {
                    li { BreadcrumbSeparator {} }
                    li {
                        BreadcrumbItem {
                            text: match kind {
                                PouKind::Program => "Program",
                                PouKind::Function => "Function",
                                PouKind::FunctionBlock => "Function Block",
                            }.to_owned(),
                            last: false,
                            match kind {
                                PouKind::Program => rsx! { ProgramIcon {} },
                                PouKind::Function => rsx! { FunctionIcon {} },
                                PouKind::FunctionBlock => rsx! { FunctionBlockIcon {} },
                            }
                        }
                    }
                }
                li { BreadcrumbSeparator {} }
                li {
                    BreadcrumbItem { text: tab.name.clone(), last: true,
                        if let Some(language) = tab.language {
                            LanguageBadge { language }
                        } else {
                            span { class: "font-mono text-[8px] font-bold text-brand", "JSON" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BreadcrumbItem(text: String, last: bool, children: Element) -> Element {
    rsx! {
        div { class: "flex items-center gap-1",
            {children}
            span {
                class: if last {
                    "font-caption text-[10px] font-semibold text-neutral-1000 dark:text-white"
                } else {
                    "font-caption text-[10px] font-medium text-neutral-850 dark:text-neutral-300"
                },
                "{text}"
            }
        }
    }
}

#[component]
fn BreadcrumbSeparator() -> Element {
    rsx! {
        div { class: "h-4 w-4 rotate-180",
            ArrowIcon {}
        }
    }
}
