mod data_type;
mod device;
mod graphical;
mod library_manager;
mod manifest;
mod monaco;
mod remote_device;
mod remote_runtime;
mod resource;
mod server;
mod user_management;
mod variables;
mod vendor_screen;

use dioxus::prelude::*;

use crate::app::{AppStore, TabTarget};
use crate::project::{OpenProject, PouFile};

use data_type::DataTypeEditor;
use device::DeviceConfigurationEditor;
use graphical::GraphicalEditor;
use library_manager::LibraryManagerEditor;
use manifest::ManifestEditor;
use monaco::MonacoEditor;
use remote_device::RemoteDeviceEditor;
use resource::ResourceEditor;
use server::ServerEditor;
use user_management::UserManagementEditor;
use variables::VariablesEditor;
use vendor_screen::VendorScreenEditor;

#[component]
pub fn EditorPanel(project: OpenProject) -> Element {
    let store = use_context::<AppStore>();
    let selected = (store.selected_tab)();
    let tab = selected
        .as_ref()
        .and_then(|selected| (store.tabs)().into_iter().find(|tab| &tab.name == selected));

    rsx! {
        div {
            id: "editorPanel",
            class: "relative flex min-h-0 flex-[69] grow flex-col overflow-hidden rounded-lg border-2 border-neutral-200 bg-white px-4 py-4 dark:border-neutral-800 dark:bg-neutral-950",
            if let Some(tab) = tab {
                match &tab.target {
                    TabTarget::Manifest => rsx! { ManifestEditor {} },
                    TabTarget::Resource => rsx! { ResourceEditor {} },
                    TabTarget::DeviceConfiguration => rsx! { DeviceConfigurationEditor {} },
                    TabTarget::LibraryManager => rsx! { LibraryManagerEditor {} },
                    TabTarget::UserManagement => rsx! { UserManagementEditor {} },
                    TabTarget::VendorScreen(name) => rsx! { VendorScreenEditor { key: "{tab.name}", screen_name: name.clone() } },
                    TabTarget::Server(name) => rsx! { ServerEditor { key: "{tab.name}", tab_name: name.clone() } },
                    TabTarget::RemoteDevice(name) => rsx! { RemoteDeviceEditor { key: "{tab.name}", tab_name: name.clone() } },
                    TabTarget::DataType(name) => rsx! { DataTypeEditor { key: "{tab.name}", tab_name: name.clone() } },
                    TabTarget::Pou(name) => {
                        let file = project.pous.iter().find(|file| &file.name == name).cloned();
                        if let Some(file) = file {
                            rsx! { PouEditor { key: "{tab.name}", file } }
                        } else {
                            VNode::empty()
                        }
                    }
                }
            } else {
                p {
                    class: "mx-auto my-auto flex cursor-default select-none flex-col items-center gap-2 font-display text-xl font-medium",
                    "No tabs open"
                }
            }
        }
    }
}

#[component]
fn PouEditor(file: PouFile) -> Element {
    let Some(parsed) = file.parsed.clone() else {
        return VNode::empty();
    };

    rsx! {
        div { class: "flex h-full w-full flex-1",
            div { class: "flex h-full w-full flex-1 flex-col gap-1",
                div { class: "relative flex h-[25%] min-h-[20%] w-full flex-col gap-4 overflow-auto",
                    VariablesEditor {
                        name: parsed.name.clone(),
                        pou_type: parsed.pou_type,
                        variables: parsed.interface.variables.clone(),
                        documentation: parsed.documentation.clone(),
                        return_type: parsed.interface.return_type.clone(),
                        variables_text: file.variables_text.clone()
                    }
                }
                div { class: "flex h-[1px] w-full bg-brand-light" }
                div { class: "mt-4 min-h-0 flex-1 grow rounded-md",
                    if parsed.body.language.is_graphical() {
                        GraphicalEditor {
                            language: parsed.body.language,
                            pou_name: parsed.name.clone(),
                        }
                    } else {
                        MonacoEditor {
                            key: "{parsed.name}",
                            language: parsed.body.language,
                        }
                    }
                }
            }
        }
    }
}
