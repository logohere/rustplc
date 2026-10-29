use dioxus::prelude::*;

use crate::app::AppStore;
use crate::icons::WarningIcon;
use crate::project::ProjectService;

#[component]
pub fn SaveChangesModals() -> Element {
    let store = use_context::<AppStore>();
    rsx! {
        if let Some(file_name) = (store.pending_close_tab)() {
            SaveChangesFileModal { file_name }
        }
        if (store.project_close_modal)() {
            SaveChangesProjectModal {}
        }
    }
}

#[component]
fn SaveChangesFileModal(file_name: String) -> Element {
    let service = use_context::<ProjectService>();
    let mut store = use_context::<AppStore>();
    let save_name = file_name.clone();
    let discard_name = file_name.clone();

    rsx! {
        ModalBackdrop {}
        div {
            class: "box fixed inset-0 z-[151] m-auto flex h-[420px] w-[340px] select-none flex-col items-center justify-evenly rounded-lg bg-white outline-none dark:bg-neutral-950",
            div { class: "flex h-[350px] select-none flex-col items-center gap-6",
                WarningIcon {}
                div {
                    p { class: "text-sm w-full text-center font-bold text-gray-600 dark:text-neutral-100",
                        strong { "{file_name}" }
                        " has unsaved changes. Do you want to save before closing?"
                    }
                }
                div { class: "flex w-[300px] flex-col text-sm",
                    div { class: "mb-6 flex flex-col gap-2",
                        button {
                            r#type: "button",
                            class: "w-full rounded-lg bg-brand px-4 py-2 text-center font-medium text-white",
                            onclick: move |_| {
                                match store.save_tab(&service, &save_name) {
                                    Ok(()) => {
                                        store.force_close_tab(&save_name);
                                        store.pending_close_tab.set(None);
                                    }
                                    Err(error) => {
                                        store.console.with_mut(|console| console.push(crate::app::ConsoleEntry {
                                            level: "error",
                                            message: error,
                                        }));
                                    }
                                }
                            },
                            "Save"
                        }
                        button {
                            r#type: "button",
                            class: "w-full rounded-lg bg-neutral-100 px-4 py-2 text-center font-medium text-neutral-1000 dark:bg-neutral-850 dark:text-neutral-100",
                            onclick: move |_| {
                                store.discard_tab(&discard_name);
                                store.pending_close_tab.set(None);
                            },
                            "Don't Save"
                        }
                    }
                    button {
                        r#type: "button",
                        class: "w-full rounded-lg bg-neutral-100 px-4 py-2 text-center font-medium text-neutral-1000 dark:bg-neutral-850 dark:text-neutral-100",
                        onclick: move |_| store.pending_close_tab.set(None),
                        "Cancel"
                    }
                }
            }
        }
    }
}

#[component]
fn SaveChangesProjectModal() -> Element {
    let service = use_context::<ProjectService>();
    let mut store = use_context::<AppStore>();

    rsx! {
        ModalBackdrop {}
        div {
            class: "box fixed inset-0 z-[151] m-auto flex h-[420px] w-[340px] select-none flex-col items-center justify-evenly rounded-lg bg-white outline-none dark:bg-neutral-950",
            div { class: "flex h-[350px] select-none flex-col items-center gap-6",
                WarningIcon {}
                div {
                    p { class: "text-sm w-full text-center font-bold text-gray-600 dark:text-neutral-100",
                        "There are unsaved changes in your "
                        strong { "project" }
                        ". Do you want to save before closing?"
                    }
                }
                div { class: "flex w-[300px] flex-col text-sm",
                    div { class: "mb-6 flex flex-col gap-2",
                        button {
                            r#type: "button",
                            class: "w-full rounded-lg bg-brand px-4 py-2 text-center font-medium text-white",
                            onclick: move |_| {
                                match store.save_all(&service) {
                                    Ok(()) => store.clear_project(),
                                    Err(error) => {
                                        store.console.with_mut(|console| console.push(crate::app::ConsoleEntry {
                                            level: "error",
                                            message: error,
                                        }));
                                    }
                                }
                            },
                            "Save and close"
                        }
                        button {
                            r#type: "button",
                            class: "w-full rounded-lg bg-neutral-100 px-4 py-2 text-center font-medium text-neutral-1000 dark:bg-neutral-850 dark:text-neutral-100",
                            onclick: move |_| store.clear_project(),
                            "Close without saving"
                        }
                    }
                    button {
                        r#type: "button",
                        class: "w-full rounded-lg bg-neutral-100 px-4 py-2 text-center font-medium text-neutral-1000 dark:bg-neutral-850 dark:text-neutral-100",
                        onclick: move |_| store.project_close_modal.set(false),
                        "Cancel"
                    }
                }
            }
        }
    }
}

#[component]
fn ModalBackdrop() -> Element {
    rsx! {
        div { class: "fixed inset-0 z-[150] bg-black/50 backdrop-blur-sm transition-opacity" }
    }
}
