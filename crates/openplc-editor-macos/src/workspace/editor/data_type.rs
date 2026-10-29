use dioxus::prelude::*;
use serde_json::{json, Value};

use crate::app::AppStore;

const BASE_TYPES: &[&str] = &[
    "BOOL", "SINT", "INT", "DINT", "LINT", "USINT", "UINT", "UDINT", "ULINT", "REAL", "LREAL",
    "BYTE", "WORD", "DWORD", "LWORD", "TIME", "DATE", "TOD", "DT", "STRING", "WSTRING",
];

#[component]
pub fn DataTypeEditor(tab_name: String) -> Element {
    let mut store = use_context::<AppStore>();
    let content = store.current_content();
    let parsed = serde_json::from_str::<Value>(&content);
    let Ok(data_type) = parsed else {
        return rsx! {
            div { class: "flex h-full w-full items-center justify-center rounded-lg border border-red-200 bg-red-50 p-4 font-caption text-xs text-red-600 dark:border-red-900 dark:bg-red-950/20 dark:text-red-400",
                "Invalid data type JSON in {tab_name}."
            }
        };
    };
    let derivation = data_type
        .get("derivation")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let name = data_type
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    rsx! {
        div { class: "flex h-full w-full flex-col gap-4 overflow-hidden",
            div { class: "flex shrink-0 items-center gap-4 rounded-md bg-neutral-50 p-2 shadow-sm dark:border dark:border-neutral-800 dark:bg-neutral-1000",
                label { class: "font-caption text-xs font-medium text-neutral-950 dark:text-white", "Name:" }
                input {
                    class: "h-8 w-full max-w-[385px] rounded-lg border border-neutral-300 bg-white px-2 font-caption text-xs text-neutral-850 outline-none focus:border-brand dark:border-neutral-800 dark:bg-neutral-950 dark:text-neutral-100",
                    value: "{name}",
                    oninput: move |event| {
                        mutate_current(&mut store, |value| {
                            if let Some(object) = value.as_object_mut() {
                                object.insert("name".to_owned(), Value::String(event.value()));
                            }
                        });
                    }
                }
                span { class: "rounded bg-neutral-100 px-2 py-1 font-caption text-[10px] font-medium uppercase tracking-wide text-neutral-500 dark:bg-neutral-900", "{derivation}" }
            }
            div { class: "min-h-0 flex-1 overflow-auto",
                match derivation.as_str() {
                    "array" => rsx! { ArrayEditor { data_type: data_type.clone() } },
                    "enumerated" => rsx! { EnumeratedEditor { data_type: data_type.clone() } },
                    "structure" => rsx! { StructureEditor { data_type: data_type.clone() } },
                    _ => rsx! {
                        div { class: "rounded border border-amber-200 bg-amber-50 p-3 font-caption text-xs text-amber-700 dark:border-amber-900 dark:bg-amber-950/20 dark:text-amber-300",
                            "Unsupported data type derivation: {derivation}"
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn ArrayEditor(data_type: Value) -> Element {
    let mut store = use_context::<AppStore>();
    let mut selected = use_signal(|| None::<usize>);
    let dimensions = data_type
        .get("dimensions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let base_definition = data_type
        .get("baseType")
        .and_then(|value| value.get("definition"))
        .and_then(Value::as_str)
        .unwrap_or("base-type")
        .to_owned();
    let base_value = data_type
        .get("baseType")
        .and_then(|value| value.get("value"))
        .and_then(Value::as_str)
        .unwrap_or("BOOL")
        .to_owned();
    let initial = data_type
        .get("initialValue")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let user_types = user_data_type_names(&store, data_type.get("name").and_then(Value::as_str));
    let dimension_count = dimensions.len();

    rsx! {
        div { class: "flex h-full w-full flex-col gap-4",
            div { class: "grid grid-cols-2 gap-8",
                div { class: "flex items-center gap-3",
                    label { class: "w-20 font-caption text-xs font-medium", "Base Type" }
                    select {
                        class: "h-8 min-w-44 rounded-lg border border-neutral-300 bg-white px-2 font-caption text-xs outline-none focus:border-brand dark:border-neutral-800 dark:bg-neutral-950",
                        value: "{base_definition}|{base_value}",
                        onchange: move |event| {
                            if let Some((definition, value)) = event.value().split_once('|') {
                                mutate_current(&mut store, |data_type| {
                                    if let Some(object) = data_type.as_object_mut() {
                                        object.insert(
                                            "baseType".to_owned(),
                                            json!({ "definition": definition, "value": value }),
                                        );
                                    }
                                });
                            }
                        },
                        optgroup { label: "Base Types",
                            for base in BASE_TYPES {
                                option { key: "base-{base}", value: "base-type|{base}", "{base}" }
                            }
                        }
                        if !user_types.is_empty() {
                            optgroup { label: "User Data Types",
                                for user_type in user_types.iter() {
                                    option { key: "user-{user_type}", value: "user-data-type|{user_type}", "{user_type}" }
                                }
                            }
                        }
                    }
                }
                div { class: "flex items-center gap-3",
                    label { class: "w-24 font-caption text-xs font-medium", "Initial Value" }
                    input {
                        class: "h-8 min-w-44 flex-1 rounded-lg border border-neutral-300 bg-white px-2 font-mono text-xs outline-none focus:border-brand dark:border-neutral-800 dark:bg-neutral-950",
                        value: "{initial}",
                        oninput: move |event| {
                            mutate_current(&mut store, |data_type| {
                                if let Some(object) = data_type.as_object_mut() {
                                    object.insert("initialValue".to_owned(), Value::String(event.value()));
                                }
                            });
                        }
                    }
                }
            }
            div { class: "flex w-[620px] flex-col gap-2",
                TableHeader {
                    title: "Dimensions",
                    on_add: move |_| {
                        mutate_current(&mut store, |data_type| {
                            if let Some(rows) = ensure_array(data_type, "dimensions") { rows.push(json!({ "dimension": "" })); }
                        });
                        selected.set(Some(dimension_count));
                    },
                    on_remove: move |_| {
                        if let Some(index) = selected() {
                            mutate_current(&mut store, |data_type| {
                                let Some(rows) = ensure_array(data_type, "dimensions") else { return; };
                                if index < rows.len() { rows.remove(index); }
                            });
                            selected.set(index.checked_sub(1));
                        }
                    },
                    on_up: move |_| move_row(&mut store, "dimensions", selected, -1),
                    on_down: move |_| move_row(&mut store, "dimensions", selected, 1),
                    can_remove: selected().is_some(),
                    can_up: selected().is_some_and(|index| index > 0),
                    can_down: selected().is_some_and(|index| index + 1 < dimension_count),
                }
                div { class: "overflow-hidden rounded-lg border border-neutral-200 dark:border-neutral-800",
                    table { class: "w-full table-fixed border-collapse font-caption text-xs",
                        thead { class: "bg-neutral-50 dark:bg-neutral-900",
                            tr { th { class: "px-3 py-2 text-left font-medium", "Dimension" } }
                        }
                        tbody {
                            for (index, row) in dimensions.iter().enumerate() {
                                {
                                    let dimension = row.get("dimension").and_then(Value::as_str).unwrap_or_default().to_owned();
                                    rsx! { tr {
                                    key: "dimension-{index}",
                                    class: if selected() == Some(index) { "bg-brand/5" } else { "border-t border-neutral-100 dark:border-neutral-900" },
                                    onclick: move |_| selected.set(Some(index)),
                                    td { class: "p-1",
                                        input {
                                            class: "h-8 w-full bg-transparent px-2 font-mono text-xs outline-none",
                                            value: "{dimension}",
                                            placeholder: "0..9",
                                            oninput: move |event| {
                                                mutate_current(&mut store, |data_type| {
                                                    if let Some(row) = ensure_array(data_type, "dimensions")
                                                        .and_then(|rows| rows.get_mut(index))
                                                        .and_then(Value::as_object_mut)
                                                    {
                                                        row.insert("dimension".to_owned(), Value::String(event.value()));
                                                    }
                                                });
                                            }
                                        }
                                    }
                                } }
                                }
                            }
                            if dimensions.is_empty() {
                                tr { td { class: "px-3 py-6 text-center text-neutral-500", "No dimensions. Add one to define the array bounds." } }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EnumeratedEditor(data_type: Value) -> Element {
    let mut store = use_context::<AppStore>();
    let mut selected = use_signal(|| None::<usize>);
    let values = data_type
        .get("values")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let initial = data_type
        .get("initialValue")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let value_count = values.len();

    rsx! {
        div { class: "flex h-full w-full gap-8",
            div { class: "flex w-[620px] flex-col gap-2",
                TableHeader {
                    title: "Description",
                    on_add: move |_| {
                        mutate_current(&mut store, |data_type| {
                            if let Some(rows) = ensure_array(data_type, "values") { rows.push(json!({ "description": "" })); }
                        });
                        selected.set(Some(value_count));
                    },
                    on_remove: move |_| {
                        if let Some(index) = selected() {
                            mutate_current(&mut store, |data_type| {
                                let Some(rows) = ensure_array(data_type, "values") else { return; };
                                if index < rows.len() { rows.remove(index); }
                            });
                            selected.set(index.checked_sub(1));
                        }
                    },
                    on_up: move |_| move_row(&mut store, "values", selected, -1),
                    on_down: move |_| move_row(&mut store, "values", selected, 1),
                    can_remove: selected().is_some(),
                    can_up: selected().is_some_and(|index| index > 0),
                    can_down: selected().is_some_and(|index| index + 1 < value_count),
                }
                div { class: "overflow-hidden rounded-lg border border-neutral-200 dark:border-neutral-800",
                    table { class: "w-full table-fixed border-collapse font-caption text-xs",
                        tbody {
                            for (index, row) in values.iter().enumerate() {
                                {
                                    let description = row.get("description").and_then(Value::as_str).unwrap_or_default().to_owned();
                                    rsx! { tr {
                                    key: "enum-{index}",
                                    class: if selected() == Some(index) { "bg-brand/5" } else { "border-b border-neutral-100 dark:border-neutral-900" },
                                    onclick: move |_| selected.set(Some(index)),
                                    td { class: "p-1",
                                        input {
                                            class: "h-8 w-full bg-transparent px-2 font-mono text-xs outline-none",
                                            value: "{description}",
                                            placeholder: "Value",
                                            oninput: move |event| {
                                                mutate_current(&mut store, |data_type| {
                                                    if let Some(row) = ensure_array(data_type, "values")
                                                        .and_then(|rows| rows.get_mut(index))
                                                        .and_then(Value::as_object_mut)
                                                    {
                                                        row.insert("description".to_owned(), Value::String(event.value()));
                                                    }
                                                });
                                            }
                                        }
                                    }
                                } }
                                }
                            }
                            if values.is_empty() {
                                tr { td { class: "px-3 py-6 text-center text-neutral-500", "No enum values." } }
                            }
                        }
                    }
                }
            }
            div { class: "flex min-w-60 items-start gap-3",
                label { class: "pt-2 font-caption text-xs font-medium", "Initial Value:" }
                select {
                    class: "h-8 min-w-44 rounded-lg border border-neutral-300 bg-white px-2 font-caption text-xs outline-none focus:border-brand dark:border-neutral-800 dark:bg-neutral-950",
                    value: "{initial}",
                    onchange: move |event| {
                        mutate_current(&mut store, |data_type| {
                            if let Some(object) = data_type.as_object_mut() {
                                object.insert("initialValue".to_owned(), Value::String(event.value()));
                            }
                        });
                    },
                    option { value: "", "" }
                    for row in values.iter() {
                        if let Some(description) = row.get("description").and_then(Value::as_str).filter(|value| !value.is_empty()) {
                            option { key: "initial-{description}", value: "{description}", "{description}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StructureEditor(data_type: Value) -> Element {
    let mut store = use_context::<AppStore>();
    let mut selected = use_signal(|| None::<usize>);
    let variables = data_type
        .get("variable")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let current_name = data_type.get("name").and_then(Value::as_str);
    let user_types = user_data_type_names(&store, current_name);
    let variable_count = variables.len();
    let variables_for_add = variables.clone();

    rsx! {
        div { class: "flex h-full w-full flex-col gap-2 overflow-hidden",
            TableHeader {
                title: "Structure",
                on_add: move |_| {
                    let name = next_structure_variable_name(&variables_for_add, selected());
                    mutate_current(&mut store, |data_type| {
                        let Some(rows) = ensure_array(data_type, "variable") else { return; };
                        let insert_at = selected().map_or(rows.len(), |index| (index + 1).min(rows.len()));
                        rows.insert(insert_at, json!({
                            "name": name,
                            "type": { "definition": "base-type", "value": "DINT" },
                            "initialValue": { "simpleValue": { "value": "" } }
                        }));
                    });
                    selected.set(Some(selected().map_or(variable_count, |index| index + 1)));
                },
                on_remove: move |_| {
                    if let Some(index) = selected() {
                        mutate_current(&mut store, |data_type| {
                            let Some(rows) = ensure_array(data_type, "variable") else { return; };
                            if index < rows.len() { rows.remove(index); }
                        });
                        selected.set(index.checked_sub(1));
                    }
                },
                on_up: move |_| move_row(&mut store, "variable", selected, -1),
                on_down: move |_| move_row(&mut store, "variable", selected, 1),
                can_remove: selected().is_some(),
                can_up: selected().is_some_and(|index| index > 0),
                can_down: selected().is_some_and(|index| index + 1 < variable_count),
            }
            div { class: "min-h-0 flex-1 overflow-auto rounded-lg border border-neutral-200 dark:border-neutral-800",
                table { class: "w-full table-fixed border-collapse font-caption text-xs",
                    thead { class: "sticky top-0 z-10 bg-neutral-50 dark:bg-neutral-900",
                        tr {
                            th { class: "w-[20%] px-2 py-2 text-left font-medium", "Name" }
                            th { class: "w-[18%] px-2 py-2 text-left font-medium", "Kind" }
                            th { class: "w-[22%] px-2 py-2 text-left font-medium", "Type" }
                            th { class: "w-[20%] px-2 py-2 text-left font-medium", "Dimensions" }
                            th { class: "w-[20%] px-2 py-2 text-left font-medium", "Initial Value" }
                        }
                    }
                    tbody {
                        for (index, variable) in variables.iter().enumerate() {
                            {
                                let definition = variable
                                    .get("type")
                                    .and_then(|value| value.get("definition"))
                                    .and_then(Value::as_str)
                                    .unwrap_or("base-type")
                                    .to_owned();
                                let variable_name = variable
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned();
                                let type_value = structure_type_value(variable);
                                let dimensions = structure_dimensions(variable);
                                let initial = variable
                                    .get("initialValue")
                                    .and_then(|value| value.get("simpleValue"))
                                    .and_then(|value| value.get("value"))
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned();
                                rsx! {
                                    tr {
                                        key: "struct-{index}",
                                        class: if selected() == Some(index) { "bg-brand/5" } else { "border-t border-neutral-100 dark:border-neutral-900" },
                                        onclick: move |_| selected.set(Some(index)),
                                        td { class: "p-1",
                                            input {
                                                class: "h-8 w-full bg-transparent px-2 font-mono text-xs outline-none",
                                                value: "{variable_name}",
                                                oninput: move |event| set_structure_name(&mut store, index, event.value()),
                                            }
                                        }
                                        td { class: "p-1",
                                            select {
                                                class: "h-8 w-full bg-transparent px-1 text-xs outline-none",
                                                value: "{definition}",
                                                onchange: move |event| set_structure_definition(&mut store, index, &event.value()),
                                                option { value: "base-type", "Base" }
                                                option { value: "user-data-type", "User Type" }
                                                option { value: "array", "Array" }
                                            }
                                        }
                                        td { class: "p-1",
                                            if definition == "user-data-type" {
                                                select {
                                                    class: "h-8 w-full bg-transparent px-1 text-xs outline-none",
                                                    value: "{type_value}",
                                                    onchange: move |event| set_structure_type_value(&mut store, index, &event.value()),
                                                    for user_type in user_types.iter() {
                                                        option { key: "struct-user-{index}-{user_type}", value: "{user_type}", "{user_type}" }
                                                    }
                                                }
                                            } else {
                                                select {
                                                    class: "h-8 w-full bg-transparent px-1 text-xs outline-none",
                                                    value: "{type_value}",
                                                    onchange: move |event| set_structure_type_value(&mut store, index, &event.value()),
                                                    for base in BASE_TYPES {
                                                        option { key: "struct-base-{index}-{base}", value: "{base}", "{base}" }
                                                    }
                                                }
                                            }
                                        }
                                        td { class: "p-1",
                                            if definition == "array" {
                                                input {
                                                    class: "h-8 w-full bg-transparent px-2 font-mono text-xs outline-none",
                                                    value: "{dimensions}",
                                                    placeholder: "0..9, 0..3",
                                                    oninput: move |event| set_structure_dimensions(&mut store, index, &event.value()),
                                                }
                                            } else {
                                                span { class: "px-2 text-neutral-400", "—" }
                                            }
                                        }
                                        td { class: "p-1",
                                            input {
                                                class: "h-8 w-full bg-transparent px-2 font-mono text-xs outline-none",
                                                value: "{initial}",
                                                oninput: move |event| set_structure_initial(&mut store, index, event.value()),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if variables.is_empty() {
                            tr { td { colspan: "5", class: "px-3 py-8 text-center text-neutral-500", "No structure members." } }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TableHeader(
    title: &'static str,
    on_add: EventHandler<MouseEvent>,
    on_remove: EventHandler<MouseEvent>,
    on_up: EventHandler<MouseEvent>,
    on_down: EventHandler<MouseEvent>,
    can_remove: bool,
    can_up: bool,
    can_down: bool,
) -> Element {
    rsx! {
        div { class: "flex h-8 items-center justify-between",
            span { class: "font-caption text-xs font-medium", "{title}" }
            div { class: "flex items-center gap-1",
                TableButton { title: "Add row", label: "+", disabled: false, onclick: on_add }
                TableButton { title: "Remove row", label: "−", disabled: !can_remove, onclick: on_remove }
                TableButton { title: "Move row up", label: "↑", disabled: !can_up, onclick: on_up }
                TableButton { title: "Move row down", label: "↓", disabled: !can_down, onclick: on_down }
            }
        }
    }
}

#[component]
fn TableButton(
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

fn mutate_current(store: &mut AppStore, mutate: impl FnOnce(&mut Value)) {
    let content = store.current_content();
    let Ok(mut value) = serde_json::from_str::<Value>(&content) else {
        return;
    };
    mutate(&mut value);
    if let Ok(content) = serde_json::to_string_pretty(&value) {
        store.set_current_content(content);
    }
}

fn ensure_array<'a>(value: &'a mut Value, key: &str) -> Option<&'a mut Vec<Value>> {
    if !value.is_object() {
        *value = json!({});
    }
    let object = value.as_object_mut()?;
    if !object.get(key).is_some_and(Value::is_array) {
        object.insert(key.to_owned(), Value::Array(Vec::new()));
    }
    object.get_mut(key).and_then(Value::as_array_mut)
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
    mutate_current(store, |data_type| {
        let Some(rows) = ensure_array(data_type, key) else {
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

fn user_data_type_names(store: &AppStore, current: Option<&str>) -> Vec<String> {
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
        .filter(|name| current.is_none_or(|current| !name.eq_ignore_ascii_case(current)))
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn next_structure_variable_name(variables: &[Value], selected: Option<usize>) -> String {
    let base = selected
        .and_then(|index| variables.get(index))
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .unwrap_or("structureVar")
        .trim_end_matches(|character: char| character.is_ascii_digit() || character == '_')
        .to_owned();
    let mut candidate = base.clone();
    let mut suffix = 1usize;
    while variables
        .iter()
        .any(|value| value.get("name").and_then(Value::as_str) == Some(candidate.as_str()))
    {
        candidate = format!("{base}_{suffix}");
        suffix += 1;
    }
    candidate
}

fn set_structure_name(store: &mut AppStore, index: usize, name: String) {
    mutate_current(store, |data_type| {
        if let Some(row) = ensure_array(data_type, "variable")
            .and_then(|rows| rows.get_mut(index))
            .and_then(Value::as_object_mut)
        {
            row.insert("name".to_owned(), Value::String(name));
        }
    });
}

fn set_structure_definition(store: &mut AppStore, index: usize, definition: &str) {
    mutate_current(store, |data_type| {
        let Some(row) = ensure_array(data_type, "variable")
            .and_then(|rows| rows.get_mut(index))
            .and_then(Value::as_object_mut)
        else {
            return;
        };
        let type_value = match definition {
            "user-data-type" => json!({ "definition": "user-data-type", "value": "" }),
            "array" => json!({
                "definition": "array",
                "value": "ARRAY",
                "data": {
                    "dimensions": [{ "dimension": "0..0" }],
                    "baseType": { "definition": "base-type", "value": "BOOL" }
                }
            }),
            _ => json!({ "definition": "base-type", "value": "DINT" }),
        };
        row.insert("type".to_owned(), type_value);
    });
}

fn set_structure_type_value(store: &mut AppStore, index: usize, value: &str) {
    mutate_current(store, |data_type| {
        let Some(variable_type) = ensure_array(data_type, "variable")
            .and_then(|rows| rows.get_mut(index))
            .and_then(|row| row.get_mut("type"))
        else {
            return;
        };
        if variable_type.get("definition").and_then(Value::as_str) == Some("array") {
            if let Some(base_type) = variable_type
                .get_mut("data")
                .and_then(|data| data.get_mut("baseType"))
                .and_then(Value::as_object_mut)
            {
                base_type.insert(
                    "definition".to_owned(),
                    Value::String("base-type".to_owned()),
                );
                base_type.insert("value".to_owned(), Value::String(value.to_owned()));
            }
        } else if let Some(object) = variable_type.as_object_mut() {
            object.insert("value".to_owned(), Value::String(value.to_owned()));
        }
    });
}

fn set_structure_dimensions(store: &mut AppStore, index: usize, dimensions: &str) {
    let dimensions = dimensions
        .split(',')
        .map(str::trim)
        .filter(|dimension| !dimension.is_empty())
        .map(|dimension| json!({ "dimension": dimension }))
        .collect::<Vec<_>>();
    mutate_current(store, |data_type| {
        let Some(variable_type) = ensure_array(data_type, "variable")
            .and_then(|rows| rows.get_mut(index))
            .and_then(|row| row.get_mut("type"))
        else {
            return;
        };
        if let Some(data) = variable_type.get_mut("data").and_then(Value::as_object_mut) {
            data.insert("dimensions".to_owned(), Value::Array(dimensions));
        }
    });
}

fn set_structure_initial(store: &mut AppStore, index: usize, initial: String) {
    mutate_current(store, |data_type| {
        if let Some(row) = ensure_array(data_type, "variable")
            .and_then(|rows| rows.get_mut(index))
            .and_then(Value::as_object_mut)
        {
            row.insert(
                "initialValue".to_owned(),
                json!({ "simpleValue": { "value": initial } }),
            );
        }
    });
}

fn structure_type_value(variable: &Value) -> String {
    let Some(variable_type) = variable.get("type") else {
        return "DINT".to_owned();
    };
    if variable_type.get("definition").and_then(Value::as_str) == Some("array") {
        variable_type
            .get("data")
            .and_then(|data| data.get("baseType"))
            .and_then(|base_type| base_type.get("value"))
            .and_then(Value::as_str)
            .unwrap_or("BOOL")
            .to_owned()
    } else {
        variable_type
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("DINT")
            .to_owned()
    }
}

fn structure_dimensions(variable: &Value) -> String {
    variable
        .get("type")
        .and_then(|variable_type| variable_type.get("data"))
        .and_then(|data| data.get("dimensions"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|dimension| dimension.get("dimension").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(", ")
}
