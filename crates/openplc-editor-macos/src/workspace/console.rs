use dioxus::prelude::*;

use crate::app::{AppStore, BottomPanel, ConsoleEntry};
use crate::workspace::debug_variables::DebugVariablesPanel;
use crate::workspace::search::SearchResultsPanel;

#[component]
pub fn ConsolePanel() -> Element {
    let mut store = use_context::<AppStore>();
    let active = (store.bottom_panel)();
    let logs = (store.console)();
    let has_search = !(store.search_results)().is_empty();

    rsx! {
        div {
            id: "consolePanel",
            class: "min-h-0 flex-[31] grow rounded-lg border-2 border-neutral-200 bg-white p-4 dark:border-neutral-800 dark:bg-neutral-950",
            div { class: "relative flex h-full min-h-0 w-full flex-col gap-2 overflow-hidden",
                div { class: "flex h-7 shrink-0 select-none items-center justify-between",
                    div { class: "flex h-7 select-none gap-2",
                        button {
                            r#type: "button",
                            class: if active == BottomPanel::Console {
                                "h-7 rounded-md bg-blue-500 px-3 text-xs font-medium text-white"
                            } else {
                                "h-7 rounded-md px-3 text-xs font-medium text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-900"
                            },
                            onclick: move |_| store.bottom_panel.set(BottomPanel::Console),
                            "Console"
                        }
                        button {
                            r#type: "button",
                            class: if active == BottomPanel::Variables {
                                "h-7 rounded-md bg-blue-500 px-3 text-xs font-medium text-white"
                            } else {
                                "h-7 rounded-md px-3 text-xs font-medium text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-900"
                            },
                            onclick: move |_| store.bottom_panel.set(BottomPanel::Variables),
                            "Variables"
                        }
                        if has_search {
                            button {
                                r#type: "button",
                                class: if active == BottomPanel::Search {
                                    "h-7 rounded-md bg-blue-500 px-3 text-xs font-medium text-white"
                                } else {
                                    "h-7 rounded-md px-3 text-xs font-medium text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-900"
                                },
                                onclick: move |_| store.bottom_panel.set(BottomPanel::Search),
                                "Search"
                            }
                        }
                    }
                    if active == BottomPanel::Console {
                        button {
                            r#type: "button",
                            class: "flex h-7 w-fit select-none items-center gap-1 rounded-lg bg-neutral-100 px-2 transition-colors hover:bg-neutral-200 dark:bg-neutral-850 dark:hover:bg-neutral-900",
                            title: "Clear console",
                            onclick: move |_| store.console.set(Vec::new()),
                            span { class: "font-caption text-cp-base font-normal", "Clear" }
                        }
                    }
                }
                if active == BottomPanel::Variables {
                    DebugVariablesPanel {}
                } else if active == BottomPanel::Search {
                    SearchResultsPanel {}
                } else {
                    div {
                        "aria-label": "Console",
                        class: "relative h-full w-full select-text overflow-auto text-cp-base font-semibold text-brand-dark focus:outline-none dark:text-neutral-50",
                        for (index, log) in logs.iter().enumerate() {
                            ConsoleLog { key: "{index}", entry: log.clone() }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ConsoleLog(entry: ConsoleEntry) -> Element {
    let class_for_message = match entry.level {
        "debug" => "text-neutral-500 dark:text-neutral-400",
        "warning" => "text-yellow-600",
        "error" => "text-red-500",
        "info" => "text-brand-medium dark:text-brand",
        _ => "text-[#011432] dark:text-white pl-2",
    };

    rsx! {
        if !entry.message.is_empty() {
            div { class: "group flex items-start gap-1",
                p {
                    class: "flex-1 whitespace-pre-wrap break-words font-mono font-normal {class_for_message}",
                    "{entry.message}"
                }
            }
        }
    }
}
