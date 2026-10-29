mod activity_bar;
mod console;
mod debug_variables;
mod editor;
mod explorer;
mod navigation;
mod search;

use dioxus::prelude::*;

use crate::app::AppStore;

use activity_bar::WorkspaceActivityBar;
use console::ConsolePanel;
use editor::EditorPanel;
use explorer::Explorer;
use navigation::Navigation;
use search::SearchDialog;

#[component]
pub fn WorkspaceScreen() -> Element {
    let store = use_context::<AppStore>();
    let Some(project) = (store.project)() else {
        return VNode::empty();
    };

    rsx! {
        div { class: "flex h-full w-full flex-col overflow-hidden bg-brand-dark dark:bg-neutral-950",
            SearchDialog {}
            div { class: "flex min-h-0 flex-1 overflow-hidden",
                div {
                    class: "flex h-full w-14 shrink-0 flex-col justify-between border-t-inherit bg-brand-dark pb-10 dark:bg-neutral-950",
                    WorkspaceActivityBar {}
                }
                div {
                    class: "flex h-full w-full min-w-0 flex-1 flex-grow gap-1 overflow-hidden bg-neutral-100 p-2 !rounded-tl-lg dark:bg-neutral-900",
                    div {
                        id: "leftPanel",
                        class: "flex h-full w-[16%] min-w-[13%] max-w-lg flex-col overflow-auto rounded-lg border-2 border-neutral-200 bg-white dark:border-neutral-850 dark:bg-neutral-950",
                        Explorer { project: project.clone() }
                    }
                    div {
                        id: "workspaceHandle",
                        class: "z-[99] my-[2px] w-[4px] py-2 transition-colors duration-200"
                    }
                    div {
                        id: "workspacePanel",
                        class: "flex h-full min-h-0 min-w-0 flex-1 overflow-hidden",
                        div {
                            id: "workspaceContentPanel",
                            class: "flex h-full min-h-0 flex-1 grow flex-col gap-2 overflow-hidden",
                            if !(store.tabs)().is_empty() {
                                Navigation { project: project.clone() }
                            }
                            div { class: "flex h-full min-h-0 flex-1 flex-col gap-2 overflow-hidden",
                                EditorPanel { project: project.clone() }
                                ConsolePanel {}
                            }
                        }
                    }
                }
            }
        }
    }
}
