use dioxus::prelude::*;
use serde_json::{json, Map, Value};

use crate::app::AppStore;
use crate::vendor_screen::{
    alias_is_unique, evaluate_visible, io_mapping_json, persistence_keys, regenerate_io_mapping,
};
use crate::vpp_package::VppPackageManager;

#[component]
pub fn VendorScreenEditor(screen_name: String) -> Element {
    let store = use_context::<AppStore>();
    let Some(project) = (store.project)() else {
        return rsx! { p { "No project is open." } };
    };
    let board = project
        .device_configuration
        .get("deviceBoard")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let manager = match VppPackageManager::new() {
        Ok(manager) => manager,
        Err(error) => return error_view(error),
    };
    let definition = match manager.screen_definition(&board, &screen_name) {
        Ok(definition) => definition,
        Err(error) => return error_view(error),
    };
    let module_system = manager.module_system_definition(&board).ok().flatten();
    let keys = persistence_keys(&definition);
    let mut owned =
        serde_json::from_str::<Value>(&store.current_content()).unwrap_or_else(|_| json!({}));
    if !owned.is_object() {
        owned = json!({});
    }
    let sections = definition
        .get("sections")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    rsx! {
        div { class: "flex h-full min-h-0 w-full flex-col gap-3 overflow-y-auto p-1",
            div { class: "flex shrink-0 items-start justify-between border-b border-neutral-200 pb-2 dark:border-neutral-800",
                div {
                    h2 { class: "font-display text-lg font-semibold", "{screen_name}" }
                    p { class: "font-caption text-xs text-neutral-500", "Vendor screen · {board}" }
                }
                span { class: "rounded-full bg-[#0464fb26] px-2 py-1 font-caption text-[10px] text-brand", "{keys.len()} persistence keys" }
            }
            for section in sections {
                {render_section(store, &project, module_system.as_ref(), &section, &owned)}
            }
        }
    }
}

fn render_section(
    store: AppStore,
    project: &crate::project::OpenProject,
    module_system: Option<&Value>,
    section: &Value,
    owned: &Value,
) -> Element {
    let id = section
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("section");
    let title = section.get("title").and_then(Value::as_str).unwrap_or(id);
    let layout = section
        .get("layout")
        .and_then(Value::as_str)
        .unwrap_or("form");
    let persistence = section
        .get("persistence")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(id);
    let stored = owned
        .get(persistence)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    match layout {
        "form" => render_form(store, title, persistence, section, stored),
        "module-slots" => render_module_slots(
            store,
            project,
            title,
            persistence,
            section,
            module_system,
            owned,
        ),
        "io-table" => render_io_table(store, project, title, persistence, owned),
        other => rsx! {
            section { class: "rounded-md border border-neutral-200 p-3 dark:border-neutral-700",
                h3 { class: "font-display text-sm font-medium", "{title}" }
                p { class: "mt-2 font-caption text-xs text-neutral-500", "Unsupported vendor layout {other:?}; preserving its persisted state without mutation." }
            }
        },
    }
}

fn render_form(
    store: AppStore,
    title: &str,
    persistence: &str,
    section: &Value,
    stored: Map<String, Value>,
) -> Element {
    let fields = section
        .get("fields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut values = stored.clone();
    for field in &fields {
        if let Some(id) = field.get("id").and_then(Value::as_str) {
            if !values.contains_key(id) {
                if let Some(default) = field.get("default") {
                    values.insert(id.to_owned(), default.clone());
                }
            }
        }
    }
    rsx! {
        section { class: "rounded-md border border-neutral-200 p-3 dark:border-neutral-700",
            h3 { class: "mb-3 font-display text-sm font-medium", "{title}" }
            div { class: "flex flex-col gap-3",
                for field in fields {
                    if evaluate_visible(field.get("visible"), &values) {
                        {render_field(store, persistence, &stored, &field)}
                    }
                }
            }
        }
    }
}

fn render_field(
    store: AppStore,
    persistence: &str,
    stored: &Map<String, Value>,
    field: &Value,
) -> Element {
    let id = field
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let label = field
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or(&id)
        .to_owned();
    let kind = field
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("text")
        .to_owned();
    let current = stored
        .get(&id)
        .cloned()
        .or_else(|| field.get("default").cloned())
        .unwrap_or(Value::Null);
    let help = field
        .get("help")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let key = persistence.to_owned();
    let original = stored.clone();
    let input_class = "h-[30px] w-56 rounded-md border border-neutral-200 bg-transparent px-2 font-caption text-xs outline-none focus:border-brand dark:border-neutral-700";
    rsx! {
        div { class: "flex items-center gap-4",
            label { class: "w-40 shrink-0 font-caption text-xs", "{label}" }
            if kind == "boolean" {
                input {
                    r#type: "checkbox",
                    checked: current.as_bool().unwrap_or(false),
                    onchange: move |event| update_form_value(store, &key, &original, &id, Value::Bool(event.checked()))
                }
            } else if kind == "number" {
                input {
                    r#type: "number",
                    value: value_text(&current),
                    min: field.get("min").and_then(Value::as_f64),
                    max: field.get("max").and_then(Value::as_f64),
                    step: field.get("step").and_then(Value::as_f64),
                    class: input_class,
                    oninput: move |event| {
                        let value = event.value().parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or(Value::Null);
                        update_form_value(store, &key, &original, &id, value);
                    }
                }
            } else if kind == "select" {
                select {
                    value: value_text(&current),
                    class: input_class,
                    onchange: move |event| update_form_value(store, &key, &original, &id, Value::String(event.value())),
                    for option in field.get("options").and_then(Value::as_array).cloned().unwrap_or_default() {
                        {
                            let value = option.as_str().map(str::to_owned).or_else(|| option.get("value").and_then(Value::as_str).map(str::to_owned)).unwrap_or_default();
                            let label = option.as_str().map(str::to_owned).or_else(|| option.get("label").and_then(Value::as_str).map(str::to_owned)).unwrap_or_else(|| value.clone());
                            rsx! { option { value: "{value}", "{label}" } }
                        }
                    }
                }
            } else {
                input {
                    r#type: if kind == "password" { "password" } else { "text" },
                    value: value_text(&current),
                    placeholder: field.get("placeholder").and_then(Value::as_str).unwrap_or_default(),
                    class: input_class,
                    oninput: move |event| update_form_value(store, &key, &original, &id, Value::String(event.value()))
                }
            }
            if !help.is_empty() { span { title: "{help}", class: "cursor-help text-xs text-neutral-400", "ⓘ" } }
        }
    }
}

fn render_module_slots(
    store: AppStore,
    project: &crate::project::OpenProject,
    title: &str,
    persistence: &str,
    section: &Value,
    module_system: Option<&Value>,
    owned: &Value,
) -> Element {
    let modules = module_system
        .and_then(|value| value.get("modules"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let max_slots = section
        .get("maxSlots")
        .and_then(Value::as_u64)
        .or_else(|| {
            module_system
                .and_then(|value| value.get("maxSlots"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0) as usize;
    let current = owned.get(persistence).cloned().unwrap_or_else(|| json!({}));
    let slots = current
        .get("slots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rsx! {
        section { class: "rounded-md border border-neutral-200 p-3 dark:border-neutral-700",
            h3 { class: "mb-3 font-display text-sm font-medium", "{title}" }
            div { class: "grid grid-cols-1 gap-2 lg:grid-cols-2",
                for index in 0..max_slots {
                    {
                        let selected = slots.get(index).and_then(Value::as_str).unwrap_or_default().to_owned();
                        let key = persistence.to_owned();
                        let source = current.clone();
                        let project_for_change = project.clone();
                        let module_system_for_change = module_system.cloned();
                        let slot_label = format!("Slot {}", index + 1);
                        rsx! {
                            div { class: "flex items-center gap-2 rounded-md border border-neutral-100 p-2 dark:border-neutral-800",
                                span { class: "w-14 font-caption text-xs text-neutral-500", "{slot_label}" }
                                select {
                                    class: "h-[30px] min-w-0 flex-1 rounded-md border border-neutral-200 bg-transparent px-2 font-caption text-xs dark:border-neutral-700",
                                    value: "{selected}",
                                    onchange: move |event| {
                                        update_slot(store, &project_for_change, module_system_for_change.as_ref(), &key, &source, index, &event.value());
                                    },
                                    option { value: "", "Empty" }
                                    for module in modules.iter() {
                                        {
                                            let id = module.get("id").and_then(Value::as_str).unwrap_or_default();
                                            let name = module.get("name").and_then(Value::as_str).unwrap_or(id);
                                            rsx! { option { value: "{id}", "{name}" } }
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

fn render_io_table(
    store: AppStore,
    project: &crate::project::OpenProject,
    title: &str,
    persistence: &str,
    owned: &Value,
) -> Element {
    let entries = owned
        .get(persistence)
        .and_then(|value| value.get("entries"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rsx! {
        section { class: "rounded-md border border-neutral-200 p-3 dark:border-neutral-700",
            h3 { class: "mb-3 font-display text-sm font-medium", "{title}" }
            div { class: "overflow-x-auto rounded-md border border-neutral-200 dark:border-neutral-700",
                table { class: "w-full border-collapse font-caption text-xs",
                    thead { tr { class: "bg-neutral-50 text-left dark:bg-neutral-900",
                        th { class: "px-2 py-1.5", "Slot" } th { class: "px-2 py-1.5", "Module" } th { class: "px-2 py-1.5", "Channel" } th { class: "px-2 py-1.5", "Type" } th { class: "px-2 py-1.5", "IEC Address" } th { class: "px-2 py-1.5", "Alias" }
                    } }
                    tbody {
                        for entry in entries {
                            {
                                let slot = entry.get("slot").and_then(Value::as_u64).unwrap_or(0);
                                let channel = entry.get("channelName").and_then(Value::as_str).unwrap_or_default().to_owned();
                                let except = format!("{slot}:{channel}");
                                let alias = entry.get("alias").and_then(Value::as_str).unwrap_or_default().to_owned();
                                let module_name = entry.get("moduleName").and_then(Value::as_str).unwrap_or_default().to_owned();
                                let data_type = entry.get("dataType").and_then(Value::as_str).unwrap_or_default().to_owned();
                                let iec_address = entry.get("iecAddress").and_then(Value::as_str).unwrap_or_default().to_owned();
                                let original = owned.get(persistence).cloned().unwrap_or_else(|| json!({"entries":[]}));
                                let key = persistence.to_owned();
                                let project_for_alias = project.clone();
                                rsx! { tr { class: "border-t border-neutral-100 dark:border-neutral-800",
                                    td { class: "px-2 py-1.5", "{slot}" }
                                    td { class: "px-2 py-1.5", "{module_name}" }
                                    td { class: "px-2 py-1.5", "{channel}" }
                                    td { class: "px-2 py-1.5", "{data_type}" }
                                    td { class: "px-2 py-1.5 font-mono", "{iec_address}" }
                                    td { class: "px-2 py-1.5",
                                        input {
                                            value: "{alias}",
                                            class: "h-7 w-40 rounded border border-neutral-200 bg-transparent px-2 dark:border-neutral-700",
                                            oninput: move |event| update_alias(store, &project_for_alias, &key, &original, &except, &event.value())
                                        }
                                    }
                                } }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn update_form_value(
    store: AppStore,
    persistence: &str,
    original: &Map<String, Value>,
    id: &str,
    value: Value,
) {
    let mut next = original.clone();
    next.insert(id.to_owned(), value);
    update_persistence(store, persistence, Value::Object(next));
}

fn update_slot(
    store: AppStore,
    project: &crate::project::OpenProject,
    module_system: Option<&Value>,
    persistence: &str,
    original: &Value,
    index: usize,
    selected: &str,
) {
    let mut next = original.clone();
    if !next.is_object() {
        next = json!({});
    }
    let Some(object) = next.as_object_mut() else {
        return;
    };
    let slots = object
        .entry("slots".to_owned())
        .or_insert_with(|| json!([]));
    if !slots.is_array() {
        *slots = json!([]);
    }
    let Some(values) = slots.as_array_mut() else {
        return;
    };
    while values.len() <= index {
        values.push(Value::String(String::new()));
    }
    values[index] = Value::String(selected.to_owned());
    update_persistence(store, persistence, next.clone());

    let mut live = current_owned(store);
    if let Some(object) = live.as_object_mut() {
        object.insert(persistence.to_owned(), next);
    }
    let channels = regenerate_io_mapping(project, module_system, &live);
    update_persistence(
        store,
        "io-mapping",
        json!({"entries": io_mapping_json(&channels)}),
    );
}

fn update_alias(
    store: AppStore,
    project: &crate::project::OpenProject,
    persistence: &str,
    original: &Value,
    except_key: &str,
    alias: &str,
) {
    let live = current_owned(store);
    if !alias_is_unique(project, &live, alias, except_key) {
        return;
    }
    let mut next = original.clone();
    let Some(entries) = next.get_mut("entries").and_then(Value::as_array_mut) else {
        return;
    };
    for entry in entries {
        let key = format!(
            "{}:{}",
            entry.get("slot").and_then(Value::as_u64).unwrap_or(0),
            entry
                .get("channelName")
                .and_then(Value::as_str)
                .unwrap_or_default()
        );
        if key == except_key {
            if let Some(object) = entry.as_object_mut() {
                object.insert("alias".to_owned(), Value::String(alias.to_owned()));
            }
            break;
        }
    }
    update_persistence(store, persistence, next);
}

fn current_owned(store: AppStore) -> Value {
    serde_json::from_str(&store.current_content()).unwrap_or_else(|_| json!({}))
}

fn update_persistence(mut store: AppStore, key: &str, value: Value) {
    let mut owned = current_owned(store);
    if !owned.is_object() {
        owned = json!({});
    }
    if let Some(object) = owned.as_object_mut() {
        object.insert(key.to_owned(), value);
    } else {
        return;
    }
    if let Ok(content) = serde_json::to_string_pretty(&owned) {
        store.set_current_content(content);
    }
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        _ => String::new(),
    }
}

fn error_view(error: String) -> Element {
    rsx! { div { role: "alert", class: "m-auto rounded-md border border-red-300 bg-red-50 p-3 font-caption text-xs text-red-700 dark:bg-red-950/30 dark:text-red-300", "{error}" } }
}
