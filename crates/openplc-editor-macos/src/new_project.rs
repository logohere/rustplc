use dioxus::prelude::*;

use crate::app::AppStore;
use crate::icons::{
    ArrowButtonIcon, BookIcon, CloseIcon, FolderIcon, LanguageIcon, LanguageIconKind, PathIcon,
    TimerIcon,
};
use crate::project::{Language, NewProjectRequest, ProjectKind, ProjectService};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModalStep {
    One,
    Two,
    Three,
}

#[derive(Clone, Copy, PartialEq)]
struct NewProjectState {
    step: Signal<ModalStep>,
    kind: Signal<Option<ProjectKind>>,
    name: Signal<String>,
    path: Signal<String>,
    language: Signal<Option<Language>>,
    interval: Signal<String>,
    interval_open: Signal<bool>,
    error: Signal<Option<String>>,
}

#[component]
pub fn ProjectModal() -> Element {
    let state = NewProjectState {
        step: use_signal(|| ModalStep::One),
        kind: use_signal(|| None),
        name: use_signal(String::new),
        path: use_signal(String::new),
        language: use_signal(|| None),
        interval: use_signal(|| "T#20ms".to_owned()),
        interval_open: use_signal(|| false),
        error: use_signal(|| None),
    };
    use_context_provider(|| state);
    let mut store = use_context::<AppStore>();

    rsx! {
        div { class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm transition-opacity" }
        div {
            class: "box fixed inset-0 z-50 m-auto flex h-[400px] w-[525px] flex-col justify-between gap-4 rounded-lg bg-white p-6 outline-none dark:bg-neutral-950",
            button {
                class: "absolute right-4 top-4 disabled:pointer-events-none",
                onclick: move |_| store.create_project_open.set(false),
                CloseIcon {}
            }
            match (state.step)() {
                ModalStep::One => rsx! { Step1 {} },
                ModalStep::Two => rsx! { Step2 {} },
                ModalStep::Three => rsx! { Step3 {} },
            }
        }
        if (state.interval_open)() {
            IntervalModal {}
        }
    }
}

#[component]
fn Step1() -> Element {
    let mut state = use_context::<NewProjectState>();
    let mut store = use_context::<AppStore>();
    let selected = (state.kind)();
    let project_selected = selected == Some(ProjectKind::PlcProject);
    let library_selected = selected == Some(ProjectKind::PlcLibrary);

    let project_class = if project_selected {
        "flex h-10 w-40 items-center justify-center rounded-md border-2 border-blue-300 bg-blue-300 text-white dark:border-neutral-600 dark:bg-neutral-600"
    } else {
        "flex h-10 w-40 items-center justify-center rounded-md border-2 border-transparent bg-gray-200 text-black hover:border-blue-500 hover:dark:border-neutral-600"
    };
    let library_class = if library_selected {
        "flex h-10 w-40 items-center justify-center rounded-md border-2 border-blue-300 bg-blue-300 text-white dark:border-neutral-600 dark:bg-neutral-600"
    } else {
        "flex h-10 w-40 items-center justify-center rounded-md border-2 border-transparent bg-gray-200 text-black hover:border-blue-500 hover:dark:border-neutral-600"
    };

    rsx! {
        div { class: "relative flex select-none items-center justify-center pb-12 pt-2",
            div { class: "z-10 flex h-12 w-12 items-center justify-center rounded-full border-2 border-blue-500 bg-white text-blue-500 dark:bg-neutral-950", "1" }
            div { class: "h-[2px] w-12 bg-gray-500" }
            div { class: "z-10 flex h-12 w-12 items-center justify-center rounded-full border-2 bg-neutral-100 text-center font-medium text-neutral-1000 dark:bg-neutral-850 dark:text-neutral-100", "2" }
            div { class: "h-[2px] w-12 bg-gray-500" }
            div { class: "z-10 flex h-12 w-12 items-center justify-center rounded-full border-2 bg-neutral-100 text-center font-medium text-neutral-1000 dark:bg-neutral-850 dark:text-neutral-100", "3" }
        }
        div { class: "flex flex-grow flex-col justify-between",
            div {
                h2 { class: "mb-8 select-none text-center text-lg font-semibold text-neutral-1000 dark:text-white", "What type of project will you be working on? *" }
                div { class: "flex w-full justify-around",
                    button {
                        r#type: "button",
                        class: project_class,
                        onclick: move |_| state.kind.set(Some(ProjectKind::PlcProject)),
                        FolderIcon {}
                        " PLC Project"
                    }
                    button {
                        r#type: "button",
                        class: library_class,
                        onclick: move |_| state.kind.set(Some(ProjectKind::PlcLibrary)),
                        BookIcon {}
                        " Library"
                    }
                }
            }
            div { class: "flex flex-row justify-center space-x-4",
                button {
                    r#type: "button",
                    class: "h-8 w-52 items-center rounded-lg bg-neutral-100 text-center font-medium text-neutral-1000 dark:bg-neutral-850 dark:text-neutral-100",
                    onclick: move |_| store.create_project_open.set(false),
                    "Cancel"
                }
                button {
                    r#type: "button",
                    class: if selected.is_some() { "h-8 w-52 items-center rounded-lg bg-brand text-center font-medium text-white" } else { "h-8 w-52 cursor-not-allowed items-center rounded-lg bg-brand text-center font-medium text-white opacity-50" },
                    disabled: selected.is_none(),
                    onclick: move |_| state.step.set(ModalStep::Two),
                    "Next"
                }
            }
        }
    }
}

#[component]
fn Step2() -> Element {
    let mut state = use_context::<NewProjectState>();
    let is_library = (state.kind)() == Some(ProjectKind::PlcLibrary);
    let valid = !(state.name)().trim().is_empty() && !(state.path)().trim().is_empty();
    let service = use_context::<ProjectService>();
    let mut store = use_context::<AppStore>();

    let submit = move |_| {
        if !valid {
            return;
        }
        if is_library {
            create_project(&service, &mut store, state, Language::St);
        } else {
            state.step.set(ModalStep::Three);
        }
    };

    rsx! {
        div { class: "relative flex select-none items-center justify-center pt-2",
            div { class: "z-10 flex h-12 w-12 items-center justify-center rounded-full border-2 border-blue-500 bg-blue-500 font-bold text-white", "1" }
            div { class: "h-[2px] w-12 bg-blue-300" }
            div { class: "z-10 flex h-12 w-12 items-center justify-center rounded-full border-2 border-blue-500 bg-white text-blue-500 dark:bg-neutral-950", "2" }
            if !is_library {
                div { class: "h-[2px] w-12 bg-gray-500" }
                div { class: "z-10 flex h-12 w-12 items-center justify-center rounded-full border-2 bg-neutral-100 text-center font-medium text-neutral-1000 dark:bg-neutral-850 dark:text-neutral-100", "3" }
            }
        }
        div { class: "flex flex-grow flex-col justify-between",
            div { class: "flex flex-col items-center justify-center",
                h2 { class: "mb-2 select-none text-center text-lg font-semibold text-neutral-1000 dark:text-white", "Give a name for the project: *" }
                div { class: "relative h-10 w-64",
                    input {
                        id: "project-name",
                        class: "flex h-[40px] w-64 truncate rounded-lg border border-neutral-300 bg-inherit px-2 py-0 text-sm font-medium leading-tight text-neutral-950 outline-none focus:border-brand dark:border-neutral-850 dark:bg-neutral-900 dark:text-white",
                        autofocus: true,
                        placeholder: "Project Name",
                        value: "{state.name}",
                        oninput: move |event| state.name.set(event.value()),
                    }
                }
            }
            div { class: "flex flex-col items-center",
                h2 { class: "mb-2 select-none text-center text-lg font-semibold text-neutral-1000 dark:text-white", "Choose an empty directory for your project: *" }
                div { class: "relative h-20 w-64",
                    div {
                        class: "group flex h-10 w-full cursor-pointer items-center justify-center rounded-md border border-gray-300 p-2",
                        onclick: move |_| {
                            if let Some(path) = rfd::FileDialog::new().set_title("Choose project directory").pick_folder() {
                                state.path.set(path.display().to_string());
                                state.error.set(None);
                            }
                        },
                        PathIcon {}
                        input {
                            r#type: "text",
                            id: "project-path",
                            class: "flex w-full cursor-pointer truncate bg-inherit px-2 py-0 text-sm font-medium leading-tight text-neutral-950 outline-none dark:text-white",
                            placeholder: if (state.path)().is_empty() { "Choose path" } else { "" },
                            value: "{state.path}",
                            readonly: true,
                        }
                    }
                    if let Some(error) = (state.error)() {
                        div { class: "absolute bottom-0 left-0 w-full text-sm text-red-600", style: "height:20px", "{error}" }
                    }
                }
            }
            div { class: "mt-4 flex flex-row justify-center space-x-4",
                button {
                    r#type: "button",
                    class: "h-8 w-52 items-center rounded-lg bg-neutral-100 text-center font-medium text-neutral-1000 dark:bg-neutral-850 dark:text-neutral-100",
                    onclick: move |_| state.step.set(ModalStep::One),
                    "Prev"
                }
                button {
                    r#type: "button",
                    disabled: !valid,
                    class: if valid { "h-8 w-52 rounded-lg bg-brand text-white" } else { "h-8 w-52 cursor-not-allowed rounded-lg bg-brand text-white opacity-50" },
                    onclick: submit,
                    if is_library { "Create Library" } else { "Next" }
                }
            }
        }
    }
}

#[component]
fn Step3() -> Element {
    let mut state = use_context::<NewProjectState>();
    let service = use_context::<ProjectService>();
    let mut store = use_context::<AppStore>();
    let language = (state.language)();
    let mut language_open = use_signal(|| false);

    rsx! {
        div { class: "relative flex select-none items-center justify-center pt-2",
            div { class: "z-10 flex h-12 w-12 items-center justify-center rounded-full border-2 border-blue-500 bg-blue-500 font-bold text-white", "1" }
            div { class: "h-[2px] w-12 bg-blue-300" }
            div { class: "z-10 flex h-12 w-12 items-center justify-center rounded-full border-2 border-blue-500 bg-blue-500 font-bold text-white", "2" }
            div { class: "h-[2px] w-12 bg-blue-300" }
            div { class: "z-10 flex h-12 w-12 items-center justify-center rounded-full border-2 border-blue-500 bg-white text-blue-500 dark:bg-neutral-950", "3" }
        }
        div { class: "flex flex-grow flex-col justify-between",
            div { class: "flex flex-col items-center justify-center",
                h2 { class: "select-none text-center text-lg font-semibold text-neutral-900 dark:text-white", "Choose the language for the base program:" }
                div { id: "pou-language-form-container", class: "flex w-full max-w-sm flex-col items-center rounded-md p-4",
                    div { class: "relative w-64",
                        button {
                            r#type: "button",
                            aria_label: "pou-language",
                            class: "flex h-[40px] w-64 items-center justify-between gap-2 rounded-md border border-neutral-300 bg-white px-3 py-2 font-medium text-neutral-700 shadow-sm outline-none transition-colors duration-200 ease-in-out hover:border-neutral-400 dark:border-neutral-600 dark:bg-neutral-900 dark:text-neutral-300",
                            onclick: move |_| language_open.toggle(),
                            span { if let Some(language) = language { "{language.label()}" } else { "Select a language" } }
                        }
                        if language_open() {
                            div { class: "absolute top-[44px] z-50 w-64 rounded-md bg-white shadow-lg dark:bg-neutral-900",
                                LanguageOption { label: "Ladder Diagram", language: Language::Ld, icon: LanguageIconKind::Ld, state, language_open }
                                LanguageOption { label: "Structured Text", language: Language::St, icon: LanguageIconKind::St, state, language_open }
                                LanguageOption { label: "Instruction List", language: Language::Il, icon: LanguageIconKind::Il, state, language_open }
                                LanguageOption { label: "Functional Block Diagram", language: Language::Fbd, icon: LanguageIconKind::Fbd, state, language_open }
                                div { class: "pointer-events-none relative flex w-full cursor-not-allowed items-center px-2 py-[9px] opacity-30 outline-none",
                                    span { class: "flex items-center gap-2 font-caption text-cp-sm font-medium text-neutral-850 dark:text-neutral-300",
                                        LanguageIcon { kind: LanguageIconKind::Sfc }
                                        "Sequential Functional Chart"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "flex flex-col items-center justify-between gap-1",
                h2 { class: "select-none text-center text-lg font-semibold text-neutral-1000 dark:text-white", "Define the time for the cyclic task:" }
                button {
                    r#type: "button",
                    class: "group flex h-10 w-64 cursor-pointer items-center justify-center rounded-md border border-gray-300 p-2 hover:bg-gray-300 active:bg-gray-400",
                    onclick: move |_| state.interval_open.set(true),
                    TimerIcon {}
                    span { class: "text-sm font-medium text-gray-400 group-hover:text-neutral-1000 dark:text-white", "{state.interval}" }
                }
            }
            div { class: "mt-4 flex flex-row justify-center space-x-4",
                button {
                    r#type: "button",
                    class: "h-8 w-52 items-center rounded-lg bg-neutral-100 text-center font-medium text-neutral-1000 dark:bg-neutral-850 dark:text-neutral-100",
                    onclick: move |_| state.step.set(ModalStep::Two),
                    "Prev"
                }
                button {
                    r#type: "button",
                    class: if language.is_some() { "h-8 w-52 items-center rounded-lg bg-blue-500 text-center font-medium text-white" } else { "h-8 w-52 cursor-not-allowed items-center rounded-lg bg-blue-500 text-center font-medium text-white opacity-50" },
                    disabled: language.is_none(),
                    onclick: move |_| {
                        if let Some(language) = (state.language)() {
                            create_project(&service, &mut store, state, language);
                        }
                    },
                    "Create Project"
                }
            }
        }
    }
}

#[component]
fn LanguageOption(
    label: &'static str,
    language: Language,
    icon: LanguageIconKind,
    state: NewProjectState,
    mut language_open: Signal<bool>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "relative flex w-full cursor-pointer items-center px-2 py-[9px] outline-none hover:bg-neutral-100 dark:hover:bg-neutral-900",
            onclick: move |_| {
                state.language.set(Some(language));
                language_open.set(false);
            },
            span { class: "flex items-center gap-2 font-caption text-cp-sm font-medium text-neutral-850 dark:text-neutral-300",
                LanguageIcon { kind: icon }
                "{label}"
            }
        }
    }
}

#[derive(Clone, Copy, Default)]
struct IntervalValues {
    day: u32,
    hour: u32,
    min: u32,
    sec: u32,
    ms: u32,
    us: u32,
}

#[component]
fn IntervalModal() -> Element {
    let mut state = use_context::<NewProjectState>();
    let mut values = use_signal(|| parse_interval(&(state.interval)()));

    rsx! {
        div { class: "fixed inset-0 z-[110] bg-black/50 backdrop-blur-sm transition-opacity" }
        div { class: "box fixed inset-0 z-[111] m-auto flex max-h-56 w-fit select-none flex-col justify-between gap-2 rounded-lg bg-white p-8 outline-none dark:bg-neutral-950",
            h2 { class: "text-xl font-medium text-neutral-950 dark:text-white", "Set interval" }
            div { class: "flex h-24 w-full gap-2",
                IntervalField { label: "day", value: values().day, on_change: move |value| values.with_mut(|v| v.day = value) }
                IntervalField { label: "hour", value: values().hour, on_change: move |value| values.with_mut(|v| v.hour = value) }
                IntervalField { label: "min", value: values().min, on_change: move |value| values.with_mut(|v| v.min = value) }
                IntervalField { label: "sec", value: values().sec, on_change: move |value| values.with_mut(|v| v.sec = value) }
                IntervalField { label: "ms", value: values().ms, on_change: move |value| values.with_mut(|v| v.ms = value) }
                IntervalField { label: "us", value: values().us, on_change: move |value| values.with_mut(|v| v.us = value) }
            }
            div { class: "flex h-8 w-full justify-evenly gap-7",
                button {
                    class: "h-full w-[236px] rounded-lg bg-neutral-100 text-center font-medium text-neutral-1000 dark:bg-neutral-850 dark:text-neutral-100",
                    onclick: move |_| state.interval_open.set(false),
                    "Cancel"
                }
                button {
                    class: "h-full w-[236px] rounded-lg bg-brand text-center font-medium text-white",
                    onclick: move |_| {
                        state.interval.set(format_interval(values()));
                        state.interval_open.set(false);
                    },
                    "Ok"
                }
            }
        }
    }
}

#[component]
fn IntervalField(label: &'static str, value: u32, on_change: EventHandler<u32>) -> Element {
    rsx! {
        div { class: "flex w-full flex-col justify-center gap-1 font-caption text-xs",
            label { r#for: label, class: "w-full text-neutral-950 dark:text-white", "{label}" }
            div { class: "flex gap-1",
                input {
                    id: label,
                    r#type: "number",
                    class: "h-[26px] w-16 rounded-lg border border-neutral-300 bg-white p-2 text-center text-xs text-neutral-700 outline-none ring-brand focus:border-brand focus:ring-2 dark:border-neutral-850 dark:bg-neutral-900 dark:text-neutral-100",
                    value: if value == 0 { String::new() } else { value.to_string() },
                    oninput: move |event| on_change.call(event.value().parse().unwrap_or(0)),
                }
                div { class: "flex flex-col gap-0.5",
                    div {
                        class: "flex h-3 w-[17px] cursor-pointer items-center justify-center rounded-[2px] bg-neutral-200 p-0.5 transition-all duration-75 hover:bg-neutral-500 active:bg-neutral-600",
                        onclick: move |_| on_change.call(value.saturating_add(1)),
                        ArrowButtonIcon { down: false }
                    }
                    div {
                        class: "flex h-3 w-[17px] cursor-pointer items-center justify-center rounded-[2px] bg-neutral-200 p-0.5 transition-all duration-75 hover:bg-neutral-500 active:bg-neutral-600",
                        onclick: move |_| on_change.call(value.saturating_sub(1)),
                        ArrowButtonIcon { down: true }
                    }
                }
            }
        }
    }
}

fn create_project(
    service: &ProjectService,
    store: &mut AppStore,
    mut state: NewProjectState,
    language: Language,
) {
    let Some(kind) = (state.kind)() else {
        return;
    };
    let request = NewProjectRequest {
        kind,
        name: (state.name)().trim().to_owned(),
        path: (state.path)().trim().into(),
        language,
        interval: (state.interval)(),
    };
    match service.create(&request) {
        Ok(project) => {
            store.load_project(project);
            store
                .recent
                .set(service.recent_projects().unwrap_or_default());
            store.create_project_open.set(false);
        }
        Err(error) => state.error.set(Some(error.to_string())),
    }
}

fn parse_interval(value: &str) -> IntervalValues {
    let mut result = IntervalValues::default();
    let mut digits = String::new();
    let mut index = 0;
    let input = value.strip_prefix("T#").unwrap_or(value).as_bytes();
    while index < input.len() {
        let ch = input[index] as char;
        if ch.is_ascii_digit() {
            digits.push(ch);
            index += 1;
            continue;
        }
        let number = digits.parse::<u32>().unwrap_or(0);
        digits.clear();
        let rest = &value[value.len() - (input.len() - index)..];
        if rest.starts_with("min") {
            result.min = number;
            index += 3;
        } else if rest.starts_with("ms") {
            result.ms = number;
            index += 2;
        } else if rest.starts_with("us") {
            result.us = number;
            index += 2;
        } else if ch == 'd' {
            result.day = number;
            index += 1;
        } else if ch == 'h' {
            result.hour = number;
            index += 1;
        } else if ch == 's' {
            result.sec = number;
            index += 1;
        } else {
            index += 1;
        }
    }
    result
}

fn format_interval(values: IntervalValues) -> String {
    let mut output = String::from("T#");
    if values.day > 0 {
        output.push_str(&format!("{}d", values.day));
    }
    if values.hour > 0 {
        output.push_str(&format!("{}h", values.hour));
    }
    if values.min > 0 {
        output.push_str(&format!("{}min", values.min));
    }
    if values.sec > 0 {
        output.push_str(&format!("{}s", values.sec));
    }
    if values.ms > 0 {
        output.push_str(&format!("{}ms", values.ms));
    }
    if values.us > 0 {
        output.push_str(&format!("{}us", values.us));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{format_interval, parse_interval, IntervalValues};

    #[test]
    fn interval_round_trip_matches_openplc_shape() {
        let parsed = parse_interval("T#1d2h3min4s5ms6us");
        assert_eq!(parsed.day, 1);
        assert_eq!(parsed.hour, 2);
        assert_eq!(parsed.min, 3);
        assert_eq!(parsed.sec, 4);
        assert_eq!(parsed.ms, 5);
        assert_eq!(parsed.us, 6);
        assert_eq!(format_interval(parsed), "T#1d2h3min4s5ms6us");
        assert_eq!(format_interval(IntervalValues::default()), "T#");
    }
}
