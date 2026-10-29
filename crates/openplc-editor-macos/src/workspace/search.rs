use dioxus::prelude::*;
use regex::{Regex, RegexBuilder};
use serde_json::Value;
use uuid::Uuid;

use crate::app::{AppStore, BottomPanel, SearchHit, SearchResult, SearchTarget};
use crate::project::OpenProject;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchScope {
    WholeProject,
    OnlyElements,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SearchFilters {
    data_types: bool,
    functions: bool,
    function_blocks: bool,
    programs: bool,
    configuration: bool,
}

impl Default for SearchFilters {
    fn default() -> Self {
        Self {
            data_types: true,
            functions: true,
            function_blocks: true,
            programs: true,
            configuration: true,
        }
    }
}

#[derive(Debug, Clone)]
struct SearchRequest {
    query: String,
    case_sensitive: bool,
    regular_expression: bool,
    scope: SearchScope,
    filters: SearchFilters,
}

#[component]
pub fn SearchDialog() -> Element {
    let mut store = use_context::<AppStore>();
    if !(store.search_dialog_open)() {
        return VNode::empty();
    }
    let mut query = use_signal(String::new);
    let mut case_sensitive = use_signal(|| false);
    let mut regular_expression = use_signal(|| false);
    let mut scope = use_signal(|| SearchScope::WholeProject);
    let mut filters = use_signal(SearchFilters::default);
    let error = use_signal(|| None::<String>);

    let Some(project) = (store.project)() else {
        return VNode::empty();
    };

    rsx! {
        div {
            class: "fixed inset-0 z-[1000] flex items-center justify-center bg-black/30",
            onclick: move |_| store.search_dialog_open.set(false),
            div {
                class: "w-[560px] rounded-xl border border-neutral-200 bg-white p-6 shadow-2xl dark:border-neutral-800 dark:bg-neutral-950",
                onclick: move |event| event.stop_propagation(),
                div { class: "mb-5 flex items-start justify-between",
                    div {
                        h2 { class: "text-xl font-medium text-neutral-950 dark:text-white", "Search in Project" }
                        p { class: "mt-1 font-caption text-xs text-neutral-500", "Search POUs, variables, data types and resource configuration." }
                    }
                    button {
                        r#type: "button", title: "Close search", "aria-label": "Close search",
                        class: "h-7 w-7 rounded text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-900",
                        onclick: move |_| store.search_dialog_open.set(false), "×"
                    }
                }
                label { class: "flex flex-col gap-1",
                    span { class: "font-caption text-sm font-medium text-neutral-950 dark:text-white", "Pattern to Search" }
                    input {
                        autofocus: true,
                        class: "h-9 rounded-lg border border-neutral-300 bg-transparent px-3 text-xs text-neutral-800 outline-none focus:border-brand dark:border-neutral-800 dark:text-neutral-100",
                        value: "{query}", placeholder: "Search",
                        oninput: move |event| query.set(event.value()),
                        onkeydown: {
                            let project = project.clone();
                            move |event| {
                                if event.key() == Key::Enter {
                                    run_search(&mut store, &project, SearchRequest {
                                        query: query(),
                                        case_sensitive: case_sensitive(),
                                        regular_expression: regular_expression(),
                                        scope: scope(),
                                        filters: filters(),
                                    }, error);
                                }
                            }
                        }
                    }
                }
                div { class: "mt-4 flex items-center gap-5 font-caption text-xs",
                    label { class: "flex items-center gap-2",
                        input { r#type: "checkbox", checked: case_sensitive(), onchange: move |event| case_sensitive.set(event.checked()) }
                        "Match case"
                    }
                    label { class: "flex items-center gap-2",
                        input { r#type: "checkbox", checked: regular_expression(), onchange: move |event| regular_expression.set(event.checked()) }
                        "Regular expression"
                    }
                }
                div { class: "mt-5 border-t border-neutral-200 pt-4 dark:border-neutral-800",
                    div { class: "mb-3 flex items-center gap-5 font-caption text-xs",
                        label { class: "flex items-center gap-2",
                            input {
                                r#type: "radio", name: "search-scope", checked: scope() == SearchScope::WholeProject,
                                onchange: move |_| scope.set(SearchScope::WholeProject)
                            }
                            "Whole project"
                        }
                        label { class: "flex items-center gap-2",
                            input {
                                r#type: "radio", name: "search-scope", checked: scope() == SearchScope::OnlyElements,
                                onchange: move |_| scope.set(SearchScope::OnlyElements)
                            }
                            "Only elements"
                        }
                    }
                    if scope() == SearchScope::OnlyElements {
                        div { class: "grid grid-cols-2 gap-2 rounded-lg bg-neutral-50 p-3 font-caption text-xs dark:bg-neutral-900",
                            ScopeToggle { label: "Data Types", checked: filters().data_types, onchange: move |value| filters.with_mut(|f| f.data_types = value) }
                            ScopeToggle { label: "Functions", checked: filters().functions, onchange: move |value| filters.with_mut(|f| f.functions = value) }
                            ScopeToggle { label: "Function Blocks", checked: filters().function_blocks, onchange: move |value| filters.with_mut(|f| f.function_blocks = value) }
                            ScopeToggle { label: "Programs", checked: filters().programs, onchange: move |value| filters.with_mut(|f| f.programs = value) }
                            ScopeToggle { label: "Configuration", checked: filters().configuration, onchange: move |value| filters.with_mut(|f| f.configuration = value) }
                        }
                    }
                }
                if let Some(message) = error() {
                    p { class: "mt-3 rounded bg-red-50 px-3 py-2 font-caption text-xs text-red-600 dark:bg-red-950/30 dark:text-red-400", "{message}" }
                }
                div { class: "mt-5 flex justify-end gap-2",
                    button {
                        r#type: "button", class: "h-9 rounded-md px-4 font-caption text-xs hover:bg-neutral-100 dark:hover:bg-neutral-900",
                        onclick: move |_| store.search_dialog_open.set(false), "Cancel"
                    }
                    button {
                        r#type: "button", class: "h-9 rounded-md bg-brand px-5 font-caption text-xs font-medium text-white hover:bg-brand-medium-dark",
                        onclick: {
                            let project = project.clone();
                            move |_| run_search(&mut store, &project, SearchRequest {
                                query: query(),
                                case_sensitive: case_sensitive(),
                                regular_expression: regular_expression(),
                                scope: scope(),
                                filters: filters(),
                            }, error)
                        },
                        "Search"
                    }
                }
            }
        }
    }
}

#[component]
fn ScopeToggle(label: &'static str, checked: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "flex items-center gap-2",
            input { r#type: "checkbox", checked, onchange: move |event| onchange.call(event.checked()) }
            "{label}"
        }
    }
}

fn run_search(
    store: &mut AppStore,
    project: &OpenProject,
    request: SearchRequest,
    mut error: Signal<Option<String>>,
) {
    if request.query.trim().is_empty() {
        error.set(Some("Please enter a search query to search.".to_owned()));
        return;
    }
    let active = if request.scope == SearchScope::WholeProject {
        SearchFilters::default()
    } else {
        request.filters
    };
    match search_project(
        project,
        &request.query,
        request.case_sensitive,
        request.regular_expression,
        active,
    ) {
        Ok(hits) if hits.is_empty() => {
            error.set(Some("No matches were found for your search.".to_owned()))
        }
        Ok(hits) => {
            let total_matches = hits.iter().map(|hit| hit.matches).sum();
            store.search_results.with_mut(|results| {
                results.push(SearchResult {
                    id: Uuid::new_v4().to_string(),
                    query: request.query.clone(),
                    project_name: project.name().to_owned(),
                    total_matches,
                    hits,
                });
            });
            store.bottom_panel.set(BottomPanel::Search);
            store.search_dialog_open.set(false);
            error.set(None);
        }
        Err(message) => error.set(Some(message)),
    }
}

#[component]
pub fn SearchResultsPanel() -> Element {
    let mut store = use_context::<AppStore>();
    let results = (store.search_results)();
    rsx! {
        div { class: "flex h-full min-h-0 w-full flex-col overflow-auto rounded-lg border border-neutral-200 p-2 dark:border-neutral-800",
            for result in results.iter().rev() {
                div { key: "{result.id}", class: "mb-2 rounded-lg border border-neutral-200 dark:border-neutral-800",
                    div { class: "flex items-center justify-between border-b border-neutral-200 px-3 py-2 dark:border-neutral-800",
                        p { class: "font-caption text-sm font-medium text-neutral-950 dark:text-white",
                            "{result.query}"
                            span { class: "ml-1 text-xs font-normal", "— {result.total_matches} matches" }
                        }
                        button {
                            r#type: "button", title: "Remove search result", "aria-label": "Remove search result",
                            class: "h-6 w-6 rounded text-brand hover:bg-neutral-100 hover:text-red-500 dark:hover:bg-neutral-900",
                            onclick: {
                                let id = result.id.clone();
                                move |_| store.search_results.with_mut(|items| items.retain(|item| item.id != id))
                            }, "×"
                        }
                    }
                    div { class: "px-2 py-1",
                        p { class: "px-2 py-1 font-caption text-xs font-semibold text-neutral-700 dark:text-neutral-300", "{result.project_name}" }
                        for (index, hit) in result.hits.iter().enumerate() {
                            SearchHitRow { key: "{result.id}-{index}", hit: hit.clone() }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SearchHitRow(hit: SearchHit) -> Element {
    let mut store = use_context::<AppStore>();
    let target = hit.target.clone();
    rsx! {
        button {
            r#type: "button",
            class: "flex w-full items-start gap-2 rounded px-2 py-1.5 text-left hover:bg-neutral-50 dark:hover:bg-neutral-900",
            onclick: move |_| match &target {
                SearchTarget::Pou(name) => store.open_pou(name),
                SearchTarget::DataType(name) => store.open_data_type(name),
                SearchTarget::Resource => store.open_resource(),
            },
            span { class: "mt-0.5 w-24 shrink-0 font-caption text-[10px] font-medium uppercase text-brand", "{hit.category}" }
            div { class: "min-w-0 flex-1",
                p { class: "truncate font-caption text-xs font-medium text-neutral-850 dark:text-neutral-200", "{hit.label}" }
                if !hit.context.is_empty() {
                    p { class: "truncate font-mono text-[10px] text-neutral-500", "{hit.context}" }
                }
            }
            span { class: "rounded bg-neutral-100 px-1.5 py-0.5 font-mono text-[9px] text-neutral-500 dark:bg-neutral-850", "{hit.matches}" }
        }
    }
}

fn pou_type_label(pou_type: crate::plc_model::PouType) -> &'static str {
    match pou_type {
        crate::plc_model::PouType::Program => "Program",
        crate::plc_model::PouType::Function => "Function",
        crate::plc_model::PouType::FunctionBlock => "Function Block",
    }
}

fn search_project(
    project: &OpenProject,
    query: &str,
    case_sensitive: bool,
    regular_expression: bool,
    filters: SearchFilters,
) -> Result<Vec<SearchHit>, String> {
    let matcher = Matcher::new(query, case_sensitive, regular_expression)?;
    let mut hits = Vec::new();
    for file in &project.pous {
        let Some(pou) = file.parsed.as_ref() else {
            continue;
        };
        let allowed = match pou.pou_type {
            crate::plc_model::PouType::Program => filters.programs,
            crate::plc_model::PouType::Function => filters.functions,
            crate::plc_model::PouType::FunctionBlock => filters.function_blocks,
        };
        if !allowed {
            continue;
        }
        let name_count = matcher.count(&pou.name);
        if name_count > 0 {
            hits.push(SearchHit::pou(
                file.name.clone(),
                pou_type_label(pou.pou_type),
                pou.name.clone(),
                "name",
                name_count,
            ));
        }
        for variable in &pou.interface.variables {
            let count = matcher.count(&variable.name);
            if count > 0 {
                hits.push(SearchHit::pou(
                    file.name.clone(),
                    pou_type_label(pou.pou_type),
                    variable.name.clone(),
                    &format!("{} variable", pou.name),
                    count,
                ));
            }
        }
        if matches!(
            pou.body.language,
            crate::plc_model::PouLanguage::St | crate::plc_model::PouLanguage::Il
        ) {
            if let Some(body) = pou.body.value.as_str() {
                for (line_no, line) in body.lines().enumerate() {
                    let count = matcher.count(line);
                    if count > 0 {
                        hits.push(SearchHit::pou(
                            file.name.clone(),
                            pou_type_label(pou.pou_type),
                            pou.name.clone(),
                            &format!("line {}: {}", line_no + 1, line.trim()),
                            count,
                        ));
                    }
                }
            }
        }
    }
    if filters.data_types {
        for data_type in project
            .project
            .data
            .get("dataTypes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(name) = data_type.get("name").and_then(Value::as_str) else {
                continue;
            };
            let count = matcher.count(name);
            if count > 0 {
                hits.push(SearchHit {
                    target: SearchTarget::DataType(name.to_owned()),
                    category: "Data Type".to_owned(),
                    label: name.to_owned(),
                    context: data_type
                        .get("derivation")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    matches: count,
                });
            }
        }
    }
    if filters.configuration {
        let resource = project
            .project
            .data
            .get("configuration")
            .and_then(|v| v.get("resource"));
        for (key, category) in [
            ("globalVariables", "Global Variable"),
            ("tasks", "Task"),
            ("instances", "Instance"),
        ] {
            for item in resource
                .and_then(|r| r.get(key))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(name) = item.get("name").and_then(Value::as_str) else {
                    continue;
                };
                let count = matcher.count(name);
                if count > 0 {
                    hits.push(SearchHit {
                        target: SearchTarget::Resource,
                        category: category.to_owned(),
                        label: name.to_owned(),
                        context: "Resource".to_owned(),
                        matches: count,
                    });
                }
            }
        }
    }
    Ok(hits)
}

struct Matcher {
    literal: Option<String>,
    regex: Option<Regex>,
    case_sensitive: bool,
}

impl Matcher {
    fn new(query: &str, case_sensitive: bool, regular_expression: bool) -> Result<Self, String> {
        if regular_expression {
            let regex = RegexBuilder::new(query)
                .case_insensitive(!case_sensitive)
                .build()
                .map_err(|error| format!("Invalid regular expression: {error}"))?;
            Ok(Self {
                literal: None,
                regex: Some(regex),
                case_sensitive,
            })
        } else {
            Ok(Self {
                literal: Some(if case_sensitive {
                    query.to_owned()
                } else {
                    query.to_lowercase()
                }),
                regex: None,
                case_sensitive,
            })
        }
    }

    fn count(&self, text: &str) -> usize {
        if let Some(regex) = &self.regex {
            return regex.find_iter(text).count();
        }
        let Some(query) = &self.literal else {
            return 0;
        };
        if query.is_empty() {
            return 0;
        }
        let haystack = if self.case_sensitive {
            text.to_owned()
        } else {
            text.to_lowercase()
        };
        haystack.match_indices(query).count()
    }
}

#[cfg(test)]
mod tests {
    use super::{search_project, SearchFilters};
    use crate::plc_model::{
        ParsedPou, PlcVariable, PlcVariableType, PouBody, PouInterface, PouLanguage, PouType,
        VariableClass,
    };
    use crate::project::{
        OpenProject, PouFile, PouKind, ProjectDocument, ProjectKind, ProjectMeta,
    };
    use serde_json::json;
    use std::path::PathBuf;

    fn project() -> OpenProject {
        let root = PathBuf::from("/tmp/search-contract");
        let main = ParsedPou {
            name: "PressurePump".to_owned(),
            pou_type: PouType::Program,
            interface: PouInterface {
                return_type: None,
                variables: vec![PlcVariable {
                    name: "PressureSetpoint".to_owned(),
                    class: Some(VariableClass::Local),
                    variable_type: PlcVariableType::base("INT"),
                    location: String::new(),
                    initial_value: None,
                    documentation: String::new(),
                    debug: Some(false),
                }],
            },
            body: PouBody::text(PouLanguage::St, "PressureSetpoint := PressureSetpoint + 1;"),
            documentation: String::new(),
        };
        OpenProject {
            root: root.clone(),
            project: ProjectDocument {
                meta: ProjectMeta {
                    name: "SearchContract".to_owned(),
                    kind: ProjectKind::PlcProject,
                    extra: serde_json::Map::new(),
                },
                data: json!({"dataTypes":[{"name":"PressureType","derivation":"enumerated"}],"configuration":{"resource":{"tasks":[{"name":"PressureTask"}],"instances":[{"name":"PumpInstance"}],"globalVariables":[{"name":"PressureGlobal"}]}}}),
                extra: serde_json::Map::new(),
            },
            pous: vec![PouFile {
                name: "PressurePump".to_owned(),
                kind: PouKind::Program,
                language: Some(PouLanguage::St),
                relative_path: PathBuf::from("pous/programs/PressurePump.st"),
                absolute_path: root.join("pous/programs/PressurePump.st"),
                parsed: Some(main),
                variables_text: None,
                raw_content: String::new(),
            }],
            warnings: vec![],
            device_configuration: json!({}),
            device_pin_mapping: json!({}),
            library_manifest: None,
        }
    }

    #[test]
    fn search_matches_pou_variable_body_datatype_and_resource_contract() {
        let hits = search_project(
            &project(),
            "Pressure",
            false,
            false,
            SearchFilters::default(),
        )
        .unwrap();
        assert!(hits
            .iter()
            .any(|hit| hit.category == "Program" && hit.context == "name"));
        assert!(hits.iter().any(|hit| hit.label == "PressureSetpoint"));
        assert!(hits.iter().any(|hit| hit.context.starts_with("line 1:")));
        assert!(hits.iter().any(|hit| hit.category == "Data Type"));
        assert!(hits.iter().any(|hit| hit.category == "Global Variable"));
        assert!(hits.iter().any(|hit| hit.category == "Task"));
    }

    #[test]
    fn regex_and_scope_filters_are_enforced() {
        let filters = SearchFilters {
            programs: false,
            configuration: false,
            ..SearchFilters::default()
        };
        let hits =
            search_project(&project(), "Pressure(Type|Setpoint)", true, true, filters).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].category, "Data Type");
        assert!(search_project(&project(), "[", false, true, SearchFilters::default()).is_err());
    }
}
