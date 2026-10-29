use dioxus::prelude::*;

use crate::app::{AppStore, ConsoleEntry};
use crate::plc_model::{PlcVariable, PlcVariableType, PouType, VariableClass};
use crate::pou_serializer::generate_iec_variables_to_string;
use crate::variable_parser::parse_iec_string_to_variables;

#[component]
pub fn VariablesEditor(
    name: String,
    pou_type: PouType,
    variables: Vec<PlcVariable>,
    documentation: String,
    return_type: Option<String>,
    variables_text: Option<String>,
) -> Element {
    let mut store = use_context::<AppStore>();
    let initial_code = variables_text
        .clone()
        .unwrap_or_else(|| generate_iec_variables_to_string(&variables));
    let mut code_mode = use_signal(move || variables_text.is_some());
    let mut code = use_signal(move || initial_code);
    let return_type_label = return_type.clone().unwrap_or_else(|| "BOOL".to_owned());

    let function_blocks = (store.project)()
        .map(|project| {
            project
                .pous
                .iter()
                .filter_map(|file| file.parsed.as_ref())
                .filter(|pou| pou.pou_type == PouType::FunctionBlock)
                .map(|pou| pou.name.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let function_blocks_for_toggle = function_blocks.clone();
    let function_blocks_for_blur = function_blocks;

    rsx! {
        div { class: "h-full w-full overflow-auto",
            div { class: "mb-2 flex h-7 items-center justify-between",
                div { class: "flex items-center gap-2",
                    span { class: "font-caption text-xs font-semibold text-neutral-1000 dark:text-white", "Variables" }
                    span { class: "font-caption text-[10px] text-neutral-500", "{name}" }
                    if pou_type == PouType::Function {
                        span { class: "font-caption text-[10px] text-neutral-700 dark:text-neutral-300",
                            "Return: {return_type_label}"
                        }
                    }
                }
                div { class: "flex items-center gap-2",
                    if !code_mode() {
                        button {
                            r#type: "button",
                            class: "flex h-7 w-7 items-center justify-center rounded-md bg-brand text-white hover:bg-brand-medium-dark",
                            title: "Add variable",
                            onclick: {
                                let name = name.clone();
                                move |_| {
                                    store.project.with_mut(|project| {
                                        let Some(project) = project else { return; };
                                        let Some(file) = project.pous.iter_mut().find(|file| file.name == name) else { return; };
                                        let Some(parsed) = file.parsed.as_mut() else { return; };
                                        parsed.interface.variables.push(PlcVariable {
                                            name: unique_variable_name(&parsed.interface.variables),
                                            class: Some(VariableClass::Local),
                                            variable_type: PlcVariableType::base("BOOL"),
                                            location: String::new(),
                                            initial_value: None,
                                            documentation: String::new(),
                                            debug: Some(false),
                                        });
                                        file.variables_text = None;
                                    });
                                    store.mark_current_dirty();
                                }
                            },
                            "+"
                        }
                    }
                    button {
                        r#type: "button",
                        class: "h-7 rounded-lg bg-neutral-100 px-3 font-caption text-[10px] font-medium text-brand-light dark:bg-neutral-900",
                        onclick: {
                            let name = name.clone();
                            move |_| {
                                if code_mode() {
                                    if commit_code(&mut store, &name, &(code)(), &function_blocks_for_toggle) {
                                        code_mode.set(false);
                                    }
                                } else {
                                    let current = current_variables(&store, &name);
                                    code.set(generate_iec_variables_to_string(&current));
                                    code_mode.set(true);
                                }
                            }
                        },
                        if code_mode() { "Table" } else { "Code" }
                    }
                }
            }
            if code_mode() {
                textarea {
                    class: "h-[calc(100%-30px)] min-h-[90px] w-full resize-none overflow-auto whitespace-pre rounded-md border border-neutral-200 bg-neutral-50 p-2 font-mono text-xs text-neutral-850 outline-none focus:border-brand dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-300",
                    value: "{code}",
                    oninput: move |event| {
                        code.set(event.value());
                        store.mark_current_dirty();
                    },
                    onblur: {
                        let name = name.clone();
                        move |_| {
                            let _ = commit_code(&mut store, &name, &(code)(), &function_blocks_for_blur);
                        }
                    }
                }
            } else {
                VariableTable { pou_name: name.clone() }
            }
            if !documentation.is_empty() {
                p { class: "mt-2 font-caption text-[10px] text-neutral-500", "{documentation}" }
            }
        }
    }
}

#[component]
fn VariableTable(pou_name: String) -> Element {
    let mut store = use_context::<AppStore>();
    let variables = current_variables(&store, &pou_name);

    rsx! {
        table { class: "w-full table-fixed border-collapse font-caption text-[10px]",
            thead {
                tr { class: "border-b border-neutral-200 text-left text-neutral-600 dark:border-neutral-800 dark:text-neutral-400",
                    th { class: "w-[17%] px-2 py-1", "Name" }
                    th { class: "w-[13%] px-2 py-1", "Class" }
                    th { class: "w-[17%] px-2 py-1", "Type" }
                    th { class: "w-[17%] px-2 py-1", "Location" }
                    th { class: "w-[17%] px-2 py-1", "Initial Value" }
                    th { class: "w-[8%] px-2 py-1 text-center", "Debug" }
                    th { class: "w-[6%] px-2 py-1", "" }
                }
            }
            tbody {
                for (index, variable) in variables.iter().enumerate() {
                    tr { key: "{index}-{variable.name}", class: "border-b border-neutral-100 text-neutral-850 dark:border-neutral-900 dark:text-neutral-300",
                        td { class: "px-1 py-1",
                            input {
                                class: "w-full rounded border border-transparent bg-transparent px-1 py-0.5 outline-none focus:border-brand",
                                value: "{variable.name}",
                                oninput: {
                                    let pou_name = pou_name.clone();
                                    move |event| update_variable(&mut store, &pou_name, index, |variable| variable.name = event.value())
                                }
                            }
                        }
                        td { class: "px-1 py-1",
                            select {
                                class: "w-full rounded border border-transparent bg-transparent px-1 py-0.5 outline-none focus:border-brand dark:bg-neutral-950",
                                value: variable.class.map(variable_class_value).unwrap_or("local"),
                                onchange: {
                                    let pou_name = pou_name.clone();
                                    move |event| {
                                        if let Some(class) = parse_variable_class(&event.value()) {
                                            update_variable(&mut store, &pou_name, index, |variable| variable.class = Some(class));
                                        }
                                    }
                                },
                                option { value: "local", "Local" }
                                option { value: "input", "Input" }
                                option { value: "output", "Output" }
                                option { value: "inOut", "InOut" }
                                option { value: "external", "External" }
                                option { value: "temp", "Temp" }
                            }
                        }
                        td { class: "px-1 py-1",
                            input {
                                class: "w-full rounded border border-transparent bg-transparent px-1 py-0.5 outline-none focus:border-brand",
                                value: "{variable.variable_type.value}",
                                oninput: {
                                    let pou_name = pou_name.clone();
                                    move |event| update_variable(&mut store, &pou_name, index, |variable| variable.variable_type = PlcVariableType::base(event.value().to_ascii_uppercase()))
                                }
                            }
                        }
                        td { class: "px-1 py-1",
                            input {
                                class: "w-full rounded border border-transparent bg-transparent px-1 py-0.5 outline-none focus:border-brand",
                                value: "{variable.location}",
                                oninput: {
                                    let pou_name = pou_name.clone();
                                    move |event| update_variable(&mut store, &pou_name, index, |variable| variable.location = event.value())
                                }
                            }
                        }
                        td { class: "px-1 py-1",
                            input {
                                class: "w-full rounded border border-transparent bg-transparent px-1 py-0.5 outline-none focus:border-brand",
                                value: "{variable.initial_value.clone().unwrap_or_default()}",
                                oninput: {
                                    let pou_name = pou_name.clone();
                                    move |event| {
                                        let value = event.value();
                                        update_variable(&mut store, &pou_name, index, |variable| {
                                            variable.initial_value = (!value.is_empty()).then_some(value.clone())
                                        });
                                    }
                                }
                            }
                        }
                        td { class: "px-1 py-1 text-center",
                            input {
                                r#type: "checkbox",
                                checked: variable.debug.unwrap_or(false),
                                title: "Watch this variable while debugging",
                                onchange: {
                                    let pou_name = pou_name.clone();
                                    move |event| {
                                        let checked = event.checked();
                                        update_variable(&mut store, &pou_name, index, |variable| {
                                            variable.debug = Some(checked);
                                        });
                                    }
                                }
                            }
                        }
                        td { class: "px-1 py-1 text-right",
                            button {
                                r#type: "button",
                                class: "rounded px-2 py-0.5 text-red-500 hover:bg-red-50 dark:hover:bg-red-950/40",
                                title: "Delete variable",
                                onclick: {
                                    let pou_name = pou_name.clone();
                                    move |_| {
                                        store.project.with_mut(|project| {
                                            let Some(project) = project else { return; };
                                            let Some(file) = project.pous.iter_mut().find(|file| file.name == pou_name) else { return; };
                                            let Some(parsed) = file.parsed.as_mut() else { return; };
                                            if index < parsed.interface.variables.len() {
                                                parsed.interface.variables.remove(index);
                                                file.variables_text = None;
                                            }
                                        });
                                        store.mark_current_dirty();
                                    }
                                },
                                "−"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn current_variables(store: &AppStore, pou_name: &str) -> Vec<PlcVariable> {
    (store.project)()
        .and_then(|project| {
            project
                .pous
                .iter()
                .find(|file| file.name == pou_name)
                .and_then(|file| file.parsed.as_ref())
                .map(|pou| pou.interface.variables.clone())
        })
        .unwrap_or_default()
}

fn update_variable(
    store: &mut AppStore,
    pou_name: &str,
    index: usize,
    update: impl FnOnce(&mut PlcVariable),
) {
    store.project.with_mut(|project| {
        let Some(project) = project else {
            return;
        };
        let Some(file) = project.pous.iter_mut().find(|file| file.name == pou_name) else {
            return;
        };
        let Some(parsed) = file.parsed.as_mut() else {
            return;
        };
        let Some(variable) = parsed.interface.variables.get_mut(index) else {
            return;
        };
        update(variable);
        file.variables_text = None;
    });
    store.mark_current_dirty();
}

fn commit_code(
    store: &mut AppStore,
    pou_name: &str,
    code: &str,
    function_blocks: &[String],
) -> bool {
    match parse_iec_string_to_variables(code, function_blocks) {
        Ok(variables) => {
            store.project.with_mut(|project| {
                let Some(project) = project else {
                    return;
                };
                let Some(file) = project.pous.iter_mut().find(|file| file.name == pou_name) else {
                    return;
                };
                let Some(parsed) = file.parsed.as_mut() else {
                    return;
                };
                parsed.interface.variables = variables;
                file.variables_text = None;
            });
            store.mark_current_dirty();
            true
        }
        Err(error) => {
            store.project.with_mut(|project| {
                let Some(project) = project else {
                    return;
                };
                let Some(file) = project.pous.iter_mut().find(|file| file.name == pou_name) else {
                    return;
                };
                file.variables_text = Some(code.to_owned());
            });
            store.console.with_mut(|console| {
                console.push(ConsoleEntry {
                    level: "error",
                    message: error,
                })
            });
            false
        }
    }
}

fn unique_variable_name(variables: &[PlcVariable]) -> String {
    let mut index = variables.len() + 1;
    loop {
        let candidate = format!("Variable{index}");
        if !variables.iter().any(|variable| variable.name == candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn variable_class_value(class: VariableClass) -> &'static str {
    match class {
        VariableClass::Input => "input",
        VariableClass::Output => "output",
        VariableClass::InOut => "inOut",
        VariableClass::External => "external",
        VariableClass::Local => "local",
        VariableClass::Temp => "temp",
        VariableClass::Global => "global",
    }
}

fn parse_variable_class(value: &str) -> Option<VariableClass> {
    match value {
        "input" => Some(VariableClass::Input),
        "output" => Some(VariableClass::Output),
        "inOut" => Some(VariableClass::InOut),
        "external" => Some(VariableClass::External),
        "local" => Some(VariableClass::Local),
        "temp" => Some(VariableClass::Temp),
        "global" => Some(VariableClass::Global),
        _ => None,
    }
}
