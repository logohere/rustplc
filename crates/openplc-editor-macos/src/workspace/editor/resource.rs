use dioxus::prelude::*;
use serde_json::{json, Value};

use crate::app::AppStore;

const BASE_TYPES: &[&str] = &[
    "BOOL", "SINT", "INT", "DINT", "LINT", "USINT", "UINT", "UDINT", "ULINT", "REAL", "LREAL",
    "BYTE", "WORD", "DWORD", "LWORD", "TIME", "DATE", "TOD", "DT", "STRING", "WSTRING",
];

#[component]
pub fn ResourceEditor() -> Element {
    let mut store = use_context::<AppStore>();
    let mut config_code_mode = use_signal(|| false);
    let mut config_code = use_signal(String::new);
    let mut config_error = use_signal(|| None::<String>);
    let content = store.current_content();
    let Ok(resource) = serde_json::from_str::<Value>(&content) else {
        return rsx! {
            div { class: "flex h-full w-full items-center justify-center rounded-lg border border-red-200 bg-red-50 p-4 font-caption text-xs text-red-600 dark:border-red-900 dark:bg-red-950/20 dark:text-red-400",
                "Invalid Resource JSON."
            }
        };
    };

    let resource_for_tasks_code = resource.clone();
    let resource_for_instances_code = resource.clone();
    rsx! {
        div { class: "flex h-full w-full flex-col gap-8 overflow-y-auto pr-1",
            GlobalVariablesEditor { resource: resource.clone() }
            if config_code_mode() {
                ConfigurationCodeEditor {
                    code: config_code,
                    error: config_error,
                    on_table: move |_| {
                        match parse_resource_configuration(&config_code()) {
                            Ok((tasks, instances)) => {
                                mutate_resource(&mut store, |resource| {
                                    if let Some(object) = resource.as_object_mut() {
                                        object.insert("tasks".to_owned(), Value::Array(tasks));
                                        object.insert("instances".to_owned(), Value::Array(instances));
                                    }
                                });
                                config_error.set(None);
                                config_code_mode.set(false);
                            }
                            Err(message) => config_error.set(Some(message)),
                        }
                    }
                }
            } else {
                TasksEditor {
                    resource: resource.clone(),
                    on_code: move |_| {
                        config_code.set(serialize_resource_configuration(&resource_for_tasks_code));
                        config_error.set(None);
                        config_code_mode.set(true);
                    }
                }
                InstancesEditor {
                    resource,
                    on_code: move |_| {
                        config_code.set(serialize_resource_configuration(&resource_for_instances_code));
                        config_error.set(None);
                        config_code_mode.set(true);
                    }
                }
            }
        }
    }
}

#[component]
fn GlobalVariablesEditor(resource: Value) -> Element {
    let mut store = use_context::<AppStore>();
    let mut selected = use_signal(|| None::<usize>);
    let mut code_mode = use_signal(|| false);
    let mut code = use_signal(String::new);
    let mut parse_error = use_signal(|| None::<String>);
    let rows = resource
        .get("globalVariables")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let row_count = rows.len();
    let rows_for_add = rows.clone();
    let rows_for_code = rows.clone();
    let user_types = user_data_type_names(&store);

    rsx! {
        div { class: "flex w-full flex-col gap-2",
            SectionHeader {
                title: "Global Variables",
                on_add: move |_| {
                    let name = next_name(&rows_for_add, "GlobalVar");
                    mutate_resource(&mut store, |resource| {
                        if let Some(rows) = ensure_array(resource, "globalVariables") { rows.push(default_global_variable(&name)); }
                    });
                    selected.set(Some(row_count));
                },
                on_remove: move |_| remove_selected(&mut store, "globalVariables", selected),
                on_up: move |_| move_row(&mut store, "globalVariables", selected, -1),
                on_down: move |_| move_row(&mut store, "globalVariables", selected, 1),
                on_code: move |_| {
                    if code_mode() {
                        match parse_iec_variables(&code()) {
                            Ok(variables) => {
                                mutate_resource(&mut store, |resource| {
                                    if let Some(object) = resource.as_object_mut() {
                                        object.insert("globalVariables".to_owned(), Value::Array(variables));
                                    }
                                });
                                parse_error.set(None);
                                code_mode.set(false);
                            }
                            Err(message) => parse_error.set(Some(message)),
                        }
                    } else {
                        code.set(serialize_iec_variables(&rows_for_code));
                        parse_error.set(None);
                        code_mode.set(true);
                    }
                },
                code_mode: code_mode(),
                can_remove: selected().is_some(),
                can_up: selected().is_some_and(|index| index > 0),
                can_down: selected().is_some_and(|index| index + 1 < row_count),
            }
            if code_mode() {
                div { class: "flex min-h-72 flex-col gap-2",
                    textarea {
                        class: "min-h-72 w-full flex-1 resize-y rounded-lg border border-neutral-200 bg-neutral-50 p-3 font-mono text-xs leading-5 outline-none focus:border-brand dark:border-neutral-800 dark:bg-neutral-1000",
                        value: "{code}",
                        oninput: move |event| code.set(event.value()),
                    }
                    if let Some(message) = parse_error() {
                        p { class: "font-caption text-xs text-red-500", "Error: {message}" }
                    }
                }
            } else {
            div { class: "overflow-x-auto rounded-lg border border-neutral-200 dark:border-neutral-800",
                table { class: "w-full min-w-[980px] table-fixed border-collapse font-caption text-xs",
                    thead { class: "bg-neutral-50 dark:bg-neutral-900",
                        tr {
                            th { class: "w-[16%] px-2 py-2 text-left font-medium", "Name" }
                            th { class: "w-[15%] px-2 py-2 text-left font-medium", "Type" }
                            th { class: "w-[14%] px-2 py-2 text-left font-medium", "Location" }
                            th { class: "w-[15%] px-2 py-2 text-left font-medium", "Initial Value" }
                            th { class: "w-[30%] px-2 py-2 text-left font-medium", "Documentation" }
                            th { class: "w-[10%] px-2 py-2 text-center font-medium", "Debug" }
                        }
                    }
                    tbody {
                        for (index, row) in rows.iter().enumerate() {
                            {
                                let name = text(row, "name");
                                let definition = row.get("type").and_then(|value| value.get("definition")).and_then(Value::as_str).unwrap_or("base-type").to_owned();
                                let type_value = row.get("type").and_then(|value| value.get("value")).and_then(Value::as_str).unwrap_or("DINT").to_owned();
                                let location = text(row, "location");
                                let initial = text(row, "initialValue");
                                let documentation = text(row, "documentation");
                                let debug = row.get("debug").and_then(Value::as_bool).unwrap_or(false);
                                rsx! {
                                    tr {
                                        key: "global-{index}",
                                        class: if selected() == Some(index) { "bg-brand/5" } else { "border-t border-neutral-100 dark:border-neutral-900" },
                                        onclick: move |_| selected.set(Some(index)),
                                        td { class: "p-1",
                                            CellInput { value: name, oninput: move |value| set_row_string(&mut store, "globalVariables", index, "name", value) }
                                        }
                                        td { class: "p-1",
                                            select {
                                                class: "h-8 w-full bg-transparent px-1 text-xs outline-none",
                                                value: "{definition}|{type_value}",
                                                onchange: move |event| {
                                                    if let Some((definition, value)) = event.value().split_once('|') {
                                                        set_row_type(&mut store, "globalVariables", index, definition, value);
                                                    }
                                                },
                                                optgroup { label: "Base Types",
                                                    for base in BASE_TYPES {
                                                        option { key: "global-{index}-base-{base}", value: "base-type|{base}", "{base}" }
                                                    }
                                                }
                                                if !user_types.is_empty() {
                                                    optgroup { label: "User Data Types",
                                                        for user_type in user_types.iter() {
                                                            option { key: "global-{index}-user-{user_type}", value: "user-data-type|{user_type}", "{user_type}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        td { class: "p-1", CellInput { value: location, placeholder: "%IX0.0", oninput: move |value| set_row_string(&mut store, "globalVariables", index, "location", value) } }
                                        td { class: "p-1", CellInput { value: initial, oninput: move |value| set_row_string(&mut store, "globalVariables", index, "initialValue", value) } }
                                        td { class: "p-1", CellInput { value: documentation, oninput: move |value| set_row_string(&mut store, "globalVariables", index, "documentation", value) } }
                                        td { class: "p-1 text-center",
                                            input {
                                                r#type: "checkbox",
                                                checked: debug,
                                                title: "Watch this global while debugging",
                                                onchange: move |event| set_row_bool(&mut store, "globalVariables", index, "debug", event.checked()),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if rows.is_empty() {
                            tr { td { colspan: "6", class: "px-3 py-7 text-center text-neutral-500", "No global variables." } }
                        }
                    }
                }
            }
            }
        }
    }
}

#[component]
fn TasksEditor(resource: Value, on_code: EventHandler<MouseEvent>) -> Element {
    let mut store = use_context::<AppStore>();
    let mut selected = use_signal(|| None::<usize>);
    let rows = resource
        .get("tasks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let row_count = rows.len();
    let rows_for_add = rows.clone();

    rsx! {
        div { class: "flex w-full flex-col gap-2",
            SectionHeader {
                title: "Tasks",
                on_add: move |_| {
                    let name = if rows_for_add.is_empty() { "Task0".to_owned() } else { next_name(&rows_for_add, "Task") };
                    mutate_resource(&mut store, |resource| {
                        if let Some(rows) = ensure_array(resource, "tasks") { rows.push(default_task(&name)); }
                    });
                    selected.set(Some(row_count));
                },
                on_remove: move |_| remove_selected(&mut store, "tasks", selected),
                on_up: move |_| move_row(&mut store, "tasks", selected, -1),
                on_down: move |_| move_row(&mut store, "tasks", selected, 1),
                on_code: move |event| on_code.call(event),
                code_mode: false,
                can_remove: selected().is_some(),
                can_up: selected().is_some_and(|index| index > 0),
                can_down: selected().is_some_and(|index| index + 1 < row_count),
            }
            div { class: "overflow-hidden rounded-lg border border-neutral-200 dark:border-neutral-800",
                table { class: "w-full table-fixed border-collapse font-caption text-xs",
                    thead { class: "bg-neutral-50 dark:bg-neutral-900",
                        tr {
                            th { class: "w-[24%] px-2 py-2 text-left font-medium", "Name" }
                            th { class: "w-[24%] px-2 py-2 text-left font-medium", "Triggering" }
                            th { class: "w-[32%] px-2 py-2 text-left font-medium", "Interval / Single" }
                            th { class: "w-[20%] px-2 py-2 text-left font-medium", "Priority" }
                        }
                    }
                    tbody {
                        for (index, row) in rows.iter().enumerate() {
                            {
                                let name = text(row, "name");
                                let triggering = text(row, "triggering");
                                let interval = text(row, "interval");
                                let priority = row.get("priority").and_then(Value::as_i64).unwrap_or(0);
                                rsx! {
                                    tr {
                                        key: "task-{index}",
                                        class: if selected() == Some(index) { "bg-brand/5" } else { "border-t border-neutral-100 dark:border-neutral-900" },
                                        onclick: move |_| selected.set(Some(index)),
                                        td { class: "p-1", CellInput { value: name, oninput: move |value| set_row_string(&mut store, "tasks", index, "name", value) } }
                                        td { class: "p-1",
                                            select {
                                                class: "h-8 w-full bg-transparent px-1 text-xs outline-none",
                                                value: "{triggering}",
                                                onchange: move |event| set_row_string(&mut store, "tasks", index, "triggering", event.value()),
                                                option { value: "Cyclic", "Cyclic" }
                                                option { value: "Interrupt", "Interrupt" }
                                            }
                                        }
                                        td { class: "p-1", CellInput { value: interval, placeholder: "T#20ms", oninput: move |value| set_row_string(&mut store, "tasks", index, "interval", value) } }
                                        td { class: "p-1",
                                            input {
                                                r#type: "number",
                                                min: "0",
                                                class: "h-8 w-full bg-transparent px-2 font-mono text-xs outline-none",
                                                value: "{priority}",
                                                oninput: move |event| {
                                                    if let Ok(priority) = event.value().parse::<i64>() {
                                                        set_row_i64(&mut store, "tasks", index, "priority", priority);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if rows.is_empty() {
                            tr { td { colspan: "4", class: "px-3 py-7 text-center text-neutral-500", "No tasks." } }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn InstancesEditor(resource: Value, on_code: EventHandler<MouseEvent>) -> Element {
    let mut store = use_context::<AppStore>();
    let mut selected = use_signal(|| None::<usize>);
    let rows = resource
        .get("instances")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let tasks = resource
        .get("tasks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|task| task.get("name").and_then(Value::as_str).map(str::to_owned))
        .collect::<Vec<_>>();
    let programs = program_names(&store);
    let row_count = rows.len();
    let rows_for_add = rows.clone();
    let programs_for_add = programs.clone();
    let tasks_for_add = tasks.clone();

    rsx! {
        div { class: "flex w-full flex-col gap-2 pb-4",
            SectionHeader {
                title: "Instances",
                on_add: move |_| {
                    let name = if rows_for_add.is_empty() { "instance0".to_owned() } else { next_name(&rows_for_add, "instance") };
                    let program = programs_for_add.first().cloned().unwrap_or_default();
                    let task = tasks_for_add.first().cloned().unwrap_or_default();
                    mutate_resource(&mut store, |resource| {
                        if let Some(rows) = ensure_array(resource, "instances") { rows.push(default_instance(&name, &program, &task)); }
                    });
                    selected.set(Some(row_count));
                },
                on_remove: move |_| remove_selected(&mut store, "instances", selected),
                on_up: move |_| move_row(&mut store, "instances", selected, -1),
                on_down: move |_| move_row(&mut store, "instances", selected, 1),
                on_code: move |event| on_code.call(event),
                code_mode: false,
                can_remove: selected().is_some(),
                can_up: selected().is_some_and(|index| index > 0),
                can_down: selected().is_some_and(|index| index + 1 < row_count),
            }
            div { class: "overflow-hidden rounded-lg border border-neutral-200 dark:border-neutral-800",
                table { class: "w-full table-fixed border-collapse font-caption text-xs",
                    thead { class: "bg-neutral-50 dark:bg-neutral-900",
                        tr {
                            th { class: "w-[34%] px-2 py-2 text-left font-medium", "Name" }
                            th { class: "w-[33%] px-2 py-2 text-left font-medium", "Program" }
                            th { class: "w-[33%] px-2 py-2 text-left font-medium", "Task" }
                        }
                    }
                    tbody {
                        for (index, row) in rows.iter().enumerate() {
                            {
                                let name = text(row, "name");
                                let program = text(row, "program");
                                let task = text(row, "task");
                                rsx! {
                                    tr {
                                        key: "instance-{index}",
                                        class: if selected() == Some(index) { "bg-brand/5" } else { "border-t border-neutral-100 dark:border-neutral-900" },
                                        onclick: move |_| selected.set(Some(index)),
                                        td { class: "p-1", CellInput { value: name, oninput: move |value| set_row_string(&mut store, "instances", index, "name", value) } }
                                        td { class: "p-1",
                                            select {
                                                class: "h-8 w-full bg-transparent px-1 text-xs outline-none",
                                                value: "{program}",
                                                onchange: move |event| set_row_string(&mut store, "instances", index, "program", event.value()),
                                                option { value: "", "" }
                                                for program_name in programs.iter() {
                                                    option { key: "instance-{index}-program-{program_name}", value: "{program_name}", "{program_name}" }
                                                }
                                            }
                                        }
                                        td { class: "p-1",
                                            select {
                                                class: "h-8 w-full bg-transparent px-1 text-xs outline-none",
                                                value: "{task}",
                                                onchange: move |event| set_row_string(&mut store, "instances", index, "task", event.value()),
                                                option { value: "", "" }
                                                for task_name in tasks.iter() {
                                                    option { key: "instance-{index}-task-{task_name}", value: "{task_name}", "{task_name}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if rows.is_empty() {
                            tr { td { colspan: "3", class: "px-3 py-7 text-center text-neutral-500", "No instances." } }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ConfigurationCodeEditor(
    mut code: Signal<String>,
    error: Signal<Option<String>>,
    on_table: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "flex min-h-96 w-full flex-col gap-2",
            div { class: "flex h-8 items-center justify-between",
                span { class: "font-caption text-sm font-medium text-neutral-950 dark:text-neutral-100", "Text editor" }
                button {
                    r#type: "button",
                    class: "h-7 rounded-md bg-brand px-3 font-caption text-[10px] font-medium text-white",
                    title: "Apply configuration text and return to tables",
                    onclick: move |event| on_table.call(event),
                    "Table"
                }
            }
            textarea {
                class: "min-h-80 w-full flex-1 resize-y rounded-lg border border-neutral-200 bg-neutral-50 p-3 font-mono text-xs leading-5 outline-none focus:border-brand dark:border-neutral-800 dark:bg-neutral-1000",
                value: "{code}",
                oninput: move |event| code.set(event.value()),
            }
            if let Some(message) = error() {
                p { class: "font-caption text-xs text-red-500", "Error: {message}" }
            }
        }
    }
}

#[component]
fn SectionHeader(
    title: &'static str,
    on_add: EventHandler<MouseEvent>,
    on_remove: EventHandler<MouseEvent>,
    on_up: EventHandler<MouseEvent>,
    on_down: EventHandler<MouseEvent>,
    on_code: EventHandler<MouseEvent>,
    code_mode: bool,
    can_remove: bool,
    can_up: bool,
    can_down: bool,
) -> Element {
    rsx! {
        div { class: "flex h-8 items-center justify-between",
            span { class: "font-caption text-sm font-medium text-neutral-950 dark:text-neutral-100", "{title}" }
            div { class: "flex items-center gap-1",
                if !code_mode {
                    ActionButton { title: "Add row", label: "+", disabled: false, onclick: on_add }
                    ActionButton { title: "Remove row", label: "−", disabled: !can_remove, onclick: on_remove }
                    ActionButton { title: "Move row up", label: "↑", disabled: !can_up, onclick: on_up }
                    ActionButton { title: "Move row down", label: "↓", disabled: !can_down, onclick: on_down }
                }
                button {
                    r#type: "button",
                    class: if code_mode {
                        "h-7 rounded-md bg-brand px-2 font-caption text-[10px] font-medium text-white"
                    } else {
                        "h-7 rounded-md px-2 font-caption text-[10px] font-medium text-brand hover:bg-neutral-100 dark:hover:bg-neutral-900"
                    },
                    title: if code_mode { "Apply text and return to table" } else { "Edit as IEC text" },
                    onclick: move |event| on_code.call(event),
                    if code_mode { "Table" } else { "Code" }
                }
            }
        }
    }
}

#[component]
fn ActionButton(
    title: &'static str,
    label: &'static str,
    disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "flex h-7 w-7 items-center justify-center rounded-md text-sm font-medium text-brand transition-colors hover:bg-neutral-100 disabled:cursor-not-allowed disabled:opacity-30 dark:hover:bg-neutral-900",
            title,
            disabled,
            onclick: move |event| onclick.call(event),
            "{label}"
        }
    }
}

#[component]
fn CellInput(
    value: String,
    #[props(default = "")] placeholder: &'static str,
    oninput: EventHandler<String>,
) -> Element {
    rsx! {
        input {
            class: "h-8 w-full bg-transparent px-2 font-mono text-xs outline-none",
            value: "{value}",
            placeholder,
            oninput: move |event| oninput.call(event.value()),
        }
    }
}

fn mutate_resource(store: &mut AppStore, mutate: impl FnOnce(&mut Value)) {
    let content = store.current_content();
    let Ok(mut resource) = serde_json::from_str::<Value>(&content) else {
        return;
    };
    mutate(&mut resource);
    if let Ok(content) = serde_json::to_string_pretty(&resource) {
        store.set_current_content(content);
    }
}

fn ensure_array<'a>(resource: &'a mut Value, key: &str) -> Option<&'a mut Vec<Value>> {
    if !resource.is_object() {
        *resource = json!({});
    }
    let object = resource.as_object_mut()?;
    if !object.get(key).is_some_and(Value::is_array) {
        object.insert(key.to_owned(), Value::Array(Vec::new()));
    }
    object.get_mut(key).and_then(Value::as_array_mut)
}

fn remove_selected(store: &mut AppStore, key: &str, mut selected: Signal<Option<usize>>) {
    let Some(index) = selected() else {
        return;
    };
    mutate_resource(store, |resource| {
        let Some(rows) = ensure_array(resource, key) else {
            return;
        };
        if index < rows.len() {
            rows.remove(index);
        }
    });
    selected.set(index.checked_sub(1));
}

fn move_row(store: &mut AppStore, key: &str, mut selected: Signal<Option<usize>>, delta: isize) {
    let Some(index) = selected() else {
        return;
    };
    let next = index as isize + delta;
    if next < 0 {
        return;
    }
    let next = next as usize;
    let mut moved = false;
    mutate_resource(store, |resource| {
        let Some(rows) = ensure_array(resource, key) else {
            return;
        };
        if index < rows.len() && next < rows.len() {
            rows.swap(index, next);
            moved = true;
        }
    });
    if moved {
        selected.set(Some(next));
    }
}

fn set_row_string(store: &mut AppStore, key: &str, index: usize, field: &str, value: String) {
    mutate_resource(store, |resource| {
        if let Some(row) = ensure_array(resource, key)
            .and_then(|rows| rows.get_mut(index))
            .and_then(Value::as_object_mut)
        {
            row.insert(field.to_owned(), Value::String(value));
        }
    });
}

fn set_row_bool(store: &mut AppStore, key: &str, index: usize, field: &str, value: bool) {
    mutate_resource(store, |resource| {
        if let Some(row) = ensure_array(resource, key)
            .and_then(|rows| rows.get_mut(index))
            .and_then(Value::as_object_mut)
        {
            row.insert(field.to_owned(), Value::Bool(value));
        }
    });
}

fn set_row_i64(store: &mut AppStore, key: &str, index: usize, field: &str, value: i64) {
    mutate_resource(store, |resource| {
        if let Some(row) = ensure_array(resource, key)
            .and_then(|rows| rows.get_mut(index))
            .and_then(Value::as_object_mut)
        {
            row.insert(field.to_owned(), Value::Number(value.into()));
        }
    });
}

fn set_row_type(store: &mut AppStore, key: &str, index: usize, definition: &str, value: &str) {
    mutate_resource(store, |resource| {
        if let Some(row) = ensure_array(resource, key)
            .and_then(|rows| rows.get_mut(index))
            .and_then(Value::as_object_mut)
        {
            row.insert(
                "type".to_owned(),
                json!({ "definition": definition, "value": value }),
            );
        }
    });
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn next_name(rows: &[Value], base: &str) -> String {
    let names = rows
        .iter()
        .filter_map(|row| row.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    if !names.contains(&base) {
        return base.to_owned();
    }
    let mut suffix = 1usize;
    loop {
        let candidate = format!("{base}{suffix}");
        if !names.iter().any(|name| *name == candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

fn user_data_type_names(store: &AppStore) -> Vec<String> {
    let mut names = (store.project)()
        .and_then(|project| {
            project
                .project
                .data
                .get("dataTypes")
                .and_then(Value::as_array)
                .cloned()
        })
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.get("name").and_then(Value::as_str).map(str::to_owned))
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn program_names(store: &AppStore) -> Vec<String> {
    let mut names = (store.project)()
        .map(|project| {
            project
                .pous
                .iter()
                .filter_map(|file| file.parsed.as_ref())
                .filter(|pou| pou.pou_type == crate::plc_model::PouType::Program)
                .map(|pou| pou.name.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn default_global_variable(name: &str) -> Value {
    json!({
        "name": name,
        "type": { "definition": "base-type", "value": "DINT" },
        "class": "global",
        "location": "",
        "initialValue": "",
        "documentation": "",
        "debug": false
    })
}

fn default_task(name: &str) -> Value {
    json!({
        "name": name,
        "triggering": "Cyclic",
        "interval": "T#20ms",
        "priority": 0
    })
}

fn default_instance(name: &str, program: &str, task: &str) -> Value {
    json!({ "name": name, "program": program, "task": task })
}

fn serialize_resource_configuration(resource: &Value) -> String {
    let mut tasks = resource
        .get("tasks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let instances = resource
        .get("instances")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if tasks.is_empty() && instances.is_empty() {
        return "(* No tasks or program instances declared. *)".to_owned();
    }
    tasks.sort_by_key(|task| task.get("priority").and_then(Value::as_i64).unwrap_or(1));
    let mut out = "CONFIGURATION Config0\n\tRESOURCE Res0 ON PLC\n\n".to_owned();
    for task in &tasks {
        let name = task.get("name").and_then(Value::as_str).unwrap_or_default();
        let interval = task
            .get("interval")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("T#20ms");
        let priority = task.get("priority").and_then(Value::as_i64).unwrap_or(1);
        out.push_str(&format!(
            "\t\tTASK {name}(INTERVAL := {interval}, PRIORITY := {priority});\n"
        ));
    }
    if !tasks.is_empty() {
        out.push('\n');
    }
    for instance in &instances {
        let name = instance
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let task = instance
            .get("task")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let program = instance
            .get("program")
            .and_then(Value::as_str)
            .unwrap_or_default();
        out.push_str(&format!("\t\tPROGRAM {name} WITH {task} : {program};\n"));
    }
    out.push_str("\tEND_RESOURCE\nEND_CONFIGURATION");
    out
}

fn parse_resource_configuration(input: &str) -> Result<(Vec<Value>, Vec<Value>), String> {
    let mut tasks = Vec::new();
    let mut instances = Vec::new();
    let mut task_names = std::collections::HashSet::<String>::new();
    let mut instance_names = std::collections::HashSet::<String>::new();
    let mut instance_lines = Vec::<(String, String, usize)>::new();

    for (index, raw_line) in input.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_inline_iec_comment(raw_line).trim().to_owned();
        if line.is_empty() {
            continue;
        }
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("TASK") {
            if !line.trim_end().ends_with(';') {
                return Err(format!(
                    "Syntax error on line {line_number}: missing semicolon."
                ));
            }
            let open = line.find('(').ok_or_else(|| {
                format!("Syntax error on line {line_number}: task parameters need parentheses.")
            })?;
            let close = line
                .rfind(')')
                .filter(|close| *close > open)
                .ok_or_else(|| {
                    format!("Syntax error on line {line_number}: task parameters need parentheses.")
                })?;
            let prefix = line[..open].trim();
            let name = prefix
                .strip_prefix("TASK")
                .or_else(|| prefix.strip_prefix("task"))
                .map(str::trim)
                .filter(|name| valid_resource_identifier(name))
                .ok_or_else(|| format!("Invalid TASK name on line {line_number}."))?;
            if name.eq_ignore_ascii_case("task") {
                return Err(format!(
                    "Invalid task name on line {line_number}: {name:?} is reserved."
                ));
            }
            let name_key = name.to_ascii_lowercase();
            if !task_names.insert(name_key) {
                return Err(format!(
                    "Duplicate TASK name on line {line_number}: {name:?} was already declared."
                ));
            }
            let mut interval = String::new();
            let mut priority = 1i64;
            for parameter in line[open + 1..close]
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                let (key, value) = parameter.split_once(":=").ok_or_else(|| {
                    format!("Syntax error on line {line_number}: missing := in task parameter.")
                })?;
                match key.trim().to_ascii_uppercase().as_str() {
                    "INTERVAL" => interval = value.trim().to_owned(),
                    "PRIORITY" => {
                        priority = value.trim().parse::<i64>().map_err(|_| {
                            format!(
                                "Invalid PRIORITY on line {line_number}: {:?}.",
                                value.trim()
                            )
                        })?;
                    }
                    _ => {}
                }
            }
            tasks.push(json!({
                "name": name,
                "triggering": "Cyclic",
                "interval": interval,
                "priority": priority
            }));
            continue;
        }
        if upper.starts_with("PROGRAM") {
            if !line.trim_end().ends_with(';') {
                return Err(format!(
                    "Syntax error on line {line_number}: missing semicolon."
                ));
            }
            let declaration = line.trim_end_matches(';').trim();
            let after_program = declaration
                .get("PROGRAM".len()..)
                .map(str::trim)
                .ok_or_else(|| format!("Syntax error on line {line_number}."))?;
            let with_index = find_ascii_case(after_program, " WITH ")
                .ok_or_else(|| format!("Syntax error on line {line_number}: missing WITH."))?;
            let name = after_program[..with_index].trim();
            if !valid_resource_identifier(name) {
                return Err(format!(
                    "Invalid PROGRAM instance name on line {line_number}."
                ));
            }
            let after_with = after_program[with_index + " WITH ".len()..].trim();
            let (task, program) = after_with
                .split_once(':')
                .ok_or_else(|| format!("Syntax error on line {line_number}: missing colon."))?;
            let task = task.trim();
            let program = program.trim();
            if !valid_resource_identifier(task) || !valid_resource_identifier(program) {
                return Err(format!(
                    "Invalid PROGRAM declaration on line {line_number}."
                ));
            }
            let name_key = name.to_ascii_lowercase();
            if !instance_names.insert(name_key) {
                return Err(format!(
                    "Duplicate PROGRAM name on line {line_number}: {name:?} was already declared."
                ));
            }
            instances.push(json!({ "name": name, "task": task, "program": program }));
            instance_lines.push((name.to_owned(), task.to_owned(), line_number));
            continue;
        }
        if upper.starts_with("CONFIGURATION")
            || upper.starts_with("RESOURCE")
            || upper.starts_with("END_RESOURCE")
            || upper.starts_with("END_CONFIGURATION")
        {
            continue;
        }
        return Err(format!(
            "Syntax error on line {line_number}: {line:?}. Unrecognized declaration."
        ));
    }

    for (instance, task, line) in instance_lines {
        if !task_names.contains(&task.to_ascii_lowercase()) {
            return Err(format!(
                "Task {task:?} referenced in PROGRAM {instance:?} on line {line} is not declared above."
            ));
        }
    }
    Ok((tasks, instances))
}

fn serialize_iec_variables(variables: &[Value]) -> String {
    if variables.is_empty() {
        return "  VAR\n  END_VAR".to_owned();
    }
    const ORDER: &[(&str, &str)] = &[
        ("global", "VAR_GLOBAL"),
        ("external", "VAR_EXTERNAL"),
        ("input", "VAR_INPUT"),
        ("output", "VAR_OUTPUT"),
        ("inOut", "VAR_IN_OUT"),
        ("local", "VAR"),
        ("temp", "VAR_TEMP"),
    ];
    let mut out = String::new();
    for (class, header) in ORDER {
        let group = variables
            .iter()
            .filter(|variable| {
                variable
                    .get("class")
                    .and_then(Value::as_str)
                    .unwrap_or("global")
                    .eq_ignore_ascii_case(class)
            })
            .collect::<Vec<_>>();
        if group.is_empty() {
            continue;
        }
        out.push_str(&format!("  {header}\n"));
        for variable in group {
            let name = variable
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let type_text = variable
                .get("type")
                .map(iec_variable_type_text)
                .unwrap_or_else(|| "DINT".to_owned());
            let mut line = format!("    {name} : {type_text}");
            if let Some(location) = variable
                .get("location")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                line.push_str(&format!(" AT {location}"));
            }
            if let Some(initial) = variable
                .get("initialValue")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                line.push_str(&format!(" := {initial}"));
            }
            line.push(';');
            if let Some(documentation) = variable
                .get("documentation")
                .and_then(Value::as_str)
                .map(|value| value.replace(['\r', '\n'], " "))
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
            {
                line.push_str(&format!(" (* {documentation} *)"));
            }
            out.push_str(&line);
            out.push('\n');
        }
        out.push_str("  END_VAR\n");
    }
    out.trim_end().to_owned()
}

fn parse_iec_variables(input: &str) -> Result<Vec<Value>, String> {
    let mut variables = Vec::new();
    let mut current_class: Option<&str> = None;
    for (index, raw_line) in input.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let upper = line.to_ascii_uppercase();
        current_class = match upper.as_str() {
            value if value.starts_with("VAR_GLOBAL") => Some("global"),
            value if value.starts_with("VAR_EXTERNAL") => Some("external"),
            value if value.starts_with("VAR_INPUT") => Some("input"),
            value if value.starts_with("VAR_OUTPUT") => Some("output"),
            value if value.starts_with("VAR_IN_OUT") => Some("inOut"),
            value if value.starts_with("VAR_TEMP") => Some("temp"),
            "VAR" => Some("local"),
            value if value.starts_with("END_VAR") => None,
            _ => current_class,
        };
        if upper.starts_with("VAR") || upper.starts_with("END_VAR") {
            continue;
        }
        let Some(class) = current_class else {
            continue;
        };
        let (declaration, documentation) = split_iec_documentation(line);
        let declaration = declaration.trim();
        if !declaration.ends_with(';') {
            return Err(format!(
                "Syntax error on line {line_number}: missing semicolon."
            ));
        }
        let declaration = declaration.trim_end_matches(';').trim();
        let (declaration, initial_value) = declaration
            .split_once(":=")
            .map(|(left, right)| (left.trim(), Some(right.trim().to_owned())))
            .unwrap_or((declaration, None));
        if class == "external" && initial_value.is_some() {
            return Err(format!(
                "Syntax error on line {line_number}: EXTERNAL variables cannot have initial values."
            ));
        }
        let (name, type_text, location) = parse_iec_variable_declaration(declaration)
            .map_err(|message| format!("Syntax error on line {line_number}: {message}"))?;
        if location.is_some() && matches!(class, "input" | "output" | "inOut" | "external" | "temp")
        {
            return Err(format!(
                "Syntax error on line {line_number}: AT is not allowed for {class} variables."
            ));
        }
        variables.push(json!({
            "name": name,
            "class": class,
            "type": parse_iec_variable_type(&type_text),
            "location": location.unwrap_or_default(),
            "initialValue": initial_value.unwrap_or_default(),
            "documentation": documentation.unwrap_or_default(),
            "debug": false
        }));
    }
    Ok(variables)
}

fn parse_iec_variable_declaration(
    declaration: &str,
) -> Result<(String, String, Option<String>), String> {
    let colon = declaration
        .find(':')
        .ok_or_else(|| "missing colon between name and type".to_owned())?;
    let left = declaration[..colon].trim();
    let right = declaration[colon + 1..].trim();
    if let Some(at) = find_ascii_case(left, " AT ") {
        let name = left[..at].trim();
        let location = left[at + " AT ".len()..].trim();
        if name.is_empty() || location.is_empty() || right.is_empty() {
            return Err("unrecognized declaration format".to_owned());
        }
        return Ok((name.to_owned(), right.to_owned(), Some(location.to_owned())));
    }
    if let Some(at) = find_ascii_case(right, " AT ") {
        let type_text = right[..at].trim();
        let location = right[at + " AT ".len()..].trim();
        if left.is_empty() || type_text.is_empty() || location.is_empty() {
            return Err("unrecognized declaration format".to_owned());
        }
        return Ok((
            left.to_owned(),
            type_text.to_owned(),
            Some(location.to_owned()),
        ));
    }
    if left.is_empty() || right.is_empty() {
        return Err("unrecognized declaration format".to_owned());
    }
    Ok((left.to_owned(), right.to_owned(), None))
}

fn parse_iec_variable_type(type_text: &str) -> Value {
    let trimmed = type_text.trim();
    let upper = trimmed.to_ascii_uppercase();
    if upper.starts_with("ARRAY") {
        if let (Some(open), Some(close)) = (trimmed.find('['), trimmed.find(']')) {
            let after = trimmed[close + 1..].trim();
            if let Some(base) = after
                .strip_prefix("OF ")
                .or_else(|| after.strip_prefix("of "))
                .map(str::trim)
            {
                let dimensions = trimmed[open + 1..close]
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|dimension| json!({ "dimension": dimension }))
                    .collect::<Vec<_>>();
                let base_upper = base.to_ascii_uppercase();
                let base_definition = if BASE_TYPES.contains(&base_upper.as_str()) {
                    "base-type"
                } else {
                    "user-data-type"
                };
                return json!({
                    "definition": "array",
                    "value": trimmed,
                    "data": {
                        "baseType": { "definition": base_definition, "value": base },
                        "dimensions": dimensions
                    }
                });
            }
        }
    }
    if BASE_TYPES.contains(&upper.as_str()) {
        json!({ "definition": "base-type", "value": upper })
    } else {
        json!({ "definition": "user-data-type", "value": trimmed })
    }
}

fn iec_variable_type_text(variable_type: &Value) -> String {
    if variable_type.get("definition").and_then(Value::as_str) == Some("array") {
        let dimensions = variable_type
            .get("data")
            .and_then(|data| data.get("dimensions"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|dimension| dimension.get("dimension").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(", ");
        let base = variable_type
            .get("data")
            .and_then(|data| data.get("baseType"))
            .and_then(|base| base.get("value"))
            .and_then(Value::as_str)
            .unwrap_or("BOOL");
        return format!("ARRAY[{dimensions}] OF {base}");
    }
    variable_type
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or("DINT")
        .to_owned()
}

fn strip_inline_iec_comment(line: &str) -> String {
    let mut output = line.to_owned();
    while let Some(start) = output.find("(*") {
        let Some(relative_end) = output[start + 2..].find("*)") else {
            break;
        };
        let end = start + 2 + relative_end + 2;
        output.replace_range(start..end, "");
    }
    output
}

fn split_iec_documentation(line: &str) -> (&str, Option<String>) {
    let Some(start) = line.find("(*") else {
        return (line, None);
    };
    let Some(relative_end) = line[start + 2..].find("*)") else {
        return (line, None);
    };
    let end = start + 2 + relative_end;
    let documentation = line[start + 2..end].trim().to_owned();
    (&line[..start], Some(documentation))
}

fn find_ascii_case(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn valid_resource_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

#[cfg(test)]
mod codec_tests {
    use serde_json::json;

    use super::{
        default_global_variable, default_instance, default_task, parse_iec_variables,
        parse_resource_configuration, serialize_iec_variables, serialize_resource_configuration,
    };

    #[test]
    fn resource_row_defaults_match_the_pinned_editor() {
        assert_eq!(
            default_global_variable("GlobalVar"),
            json!({
                "name":"GlobalVar",
                "type":{"definition":"base-type","value":"DINT"},
                "class":"global",
                "location":"",
                "initialValue":"",
                "documentation":"",
                "debug":false
            })
        );
        assert_eq!(
            default_task("Task0"),
            json!({"name":"Task0","triggering":"Cyclic","interval":"T#20ms","priority":0})
        );
        assert_eq!(
            default_instance("instance0", "", ""),
            json!({"name":"instance0","program":"","task":""})
        );
    }

    #[test]
    fn resource_configuration_text_matches_pinned_shape_and_round_trips() {
        let resource = json!({
            "tasks": [
                {"name":"slow","triggering":"Cyclic","interval":"T#100ms","priority":5},
                {"name":"fast","triggering":"Cyclic","interval":"T#10ms","priority":1}
            ],
            "instances": [{"name":"main_fast","task":"fast","program":"main"}],
            "globalVariables": []
        });
        let text = serialize_resource_configuration(&resource);
        assert_eq!(
            text,
            "CONFIGURATION Config0\n\tRESOURCE Res0 ON PLC\n\n\t\tTASK fast(INTERVAL := T#10ms, PRIORITY := 1);\n\t\tTASK slow(INTERVAL := T#100ms, PRIORITY := 5);\n\n\t\tPROGRAM main_fast WITH fast : main;\n\tEND_RESOURCE\nEND_CONFIGURATION"
        );
        let (tasks, instances) = parse_resource_configuration(&text).unwrap();
        assert_eq!(tasks[0]["name"], "fast");
        assert_eq!(tasks[0]["interval"], "T#10ms");
        assert_eq!(tasks[0]["priority"], 1);
        assert_eq!(
            instances[0],
            json!({"name":"main_fast","task":"fast","program":"main"})
        );
    }

    #[test]
    fn resource_configuration_text_rejects_duplicates_and_missing_tasks() {
        let duplicate = "TASK fast(INTERVAL := T#10ms, PRIORITY := 1);\nTASK FAST(INTERVAL := T#20ms, PRIORITY := 2);";
        assert!(parse_resource_configuration(duplicate)
            .unwrap_err()
            .contains("Duplicate TASK"));
        let missing = "PROGRAM main_fast WITH nope : main;";
        assert!(parse_resource_configuration(missing)
            .unwrap_err()
            .contains("not declared"));
    }

    #[test]
    fn global_variable_text_preserves_location_array_initial_and_documentation() {
        let variables = vec![
            json!({
                "name":"enabled","class":"global",
                "type":{"definition":"base-type","value":"BOOL"},
                "location":"%IX0.0","initialValue":"","documentation":"Enable input","debug":true
            }),
            json!({
                "name":"samples","class":"global",
                "type":{
                    "definition":"array","value":"ARRAY[0..3] OF INT",
                    "data":{"dimensions":[{"dimension":"0..3"}],"baseType":{"definition":"base-type","value":"INT"}}
                },
                "location":"","initialValue":"[1,2,3,4]","documentation":"","debug":false
            }),
        ];
        let text = serialize_iec_variables(&variables);
        assert!(text.contains("enabled : BOOL AT %IX0.0; (* Enable input *)"));
        assert!(text.contains("samples : ARRAY[0..3] OF INT := [1,2,3,4];"));
        let parsed = parse_iec_variables(&text).unwrap();
        assert_eq!(parsed[0]["name"], "enabled");
        assert_eq!(parsed[0]["location"], "%IX0.0");
        assert_eq!(parsed[0]["documentation"], "Enable input");
        assert_eq!(parsed[1]["type"]["definition"], "array");
        assert_eq!(
            parsed[1]["type"]["data"]["dimensions"][0]["dimension"],
            "0..3"
        );
        assert_eq!(parsed[1]["initialValue"], "[1,2,3,4]");
    }
}
