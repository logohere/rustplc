//! Pure Rust project-IR → IEC Structured Text frontend for the pinned OpenPLC Editor.
//! Graphical LD/FBD walkers are ported separately; this slice preserves the
//! textual POU, data-type, configuration, value, and dependency-order rules.

pub mod backend;
pub mod frontend;
pub mod graphical;
pub mod library;
pub mod library_compile;
pub mod md5;
pub mod stdlib;
mod types;

use std::collections::{HashMap, HashSet};

pub use types::*;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TranspileResult {
    pub program_st: Option<String>,
    pub pou_names: Vec<String>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

pub fn transpile_to_st(project: &TranspileProject) -> TranspileResult {
    let mut result = TranspileResult::default();
    let mut pieces = Vec::new();

    match generate_data_types(project) {
        Ok(data_types) if !data_types.is_empty() => pieces.push(data_types),
        Ok(_) => {}
        Err(error) => result.errors.push(error),
    }

    for index in emission_order(project) {
        let pou = &project.pous[index];
        let generated = if pou.body.language.textual() {
            generate_textual_pou(pou, project, 2).map(|text| (text, Vec::new()))
        } else if matches!(
            pou.body.language,
            TranspileBodyLanguage::Ld | TranspileBodyLanguage::Fbd
        ) {
            generate_graphical_pou(pou, project)
        } else {
            Err(format!("language not supported: {:?}", pou.body.language))
        };
        match generated {
            Ok((text, warnings)) => {
                pieces.push(text);
                result.pou_names.push(pou.name.clone());
                result.warnings.extend(warnings);
            }
            Err(error) => result.errors.push(format!(
                "POU \"{}\" ({:?} body): {error}",
                pou.name, pou.body.language
            )),
        }
    }

    match generate_configuration(project) {
        Ok(configuration) => pieces.push(configuration),
        Err(error) => result.errors.push(error),
    }

    if result.pou_names.is_empty() && !result.errors.is_empty() {
        return result;
    }
    result.program_st = Some(pieces.concat());
    result
}

pub fn generate_textual_pou(
    pou: &TranspilePou,
    project: &TranspileProject,
    indent: usize,
) -> Result<String, String> {
    if !pou.body.language.textual() {
        return Err(format!(
            "generateTextualPou called with non-textual body \"{:?}\"",
            pou.body.language
        ));
    }

    let keyword = pou.pou_type.keyword();
    let mut output = format!("{keyword} {}", pou.name);
    if pou.pou_type == TranspilePouKind::Function {
        output.push_str(" : ");
        output.push_str(&format_return_type(pou.interface.return_type.as_deref()));
    }
    output.push('\n');

    let interface = compute_interface(&pou.interface.variables);
    if interface.is_empty() {
        return Err(format!("No variable defined in \"{}\" POU", pou.name));
    }

    for entry in interface {
        output.push_str("  ");
        output.push_str(entry.keyword);
        output.push('\n');
        for variable in entry.variables {
            output.push_str("    ");
            output.push_str(&variable.name);
            output.push(' ');
            if !variable.location.is_empty() {
                output.push_str("AT ");
                output.push_str(&variable.location);
                output.push(' ');
            }
            let type_text = format_variable_type(&variable.variable_type);
            output.push_str(": ");
            output.push_str(&type_text);
            if let Some(value) = variable
                .initial_value
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                output.push_str(" := ");
                output.push_str(&compute_value(
                    project,
                    value,
                    &declared_type_name(variable),
                ));
            }
            output.push_str(";\n");
        }
        output.push_str("  END_VAR\n");
    }
    output.push('\n');

    let Some(body) = pou.body.value.as_str() else {
        return Err(format!("No body defined in \"{}\" POU", pou.name));
    };
    if body.is_empty() {
        return Err(format!("No body defined in \"{}\" POU", pou.name));
    }
    output.push_str(&reindent_text(body, indent));
    output.push_str("END_");
    output.push_str(keyword);
    output.push_str("\n\n");
    Ok(output)
}

pub fn generate_graphical_pou(
    pou: &TranspilePou,
    project: &TranspileProject,
) -> Result<(String, Vec<String>), String> {
    let type_context = graphical::build_type_context(pou, project);
    let emitted = match pou.body.language {
        TranspileBodyLanguage::Ld => {
            graphical::emit_ld_value_with_context(&pou.body.value, &type_context)?
        }
        TranspileBodyLanguage::Fbd => {
            graphical::emit_fbd_value_with_context(&pou.body.value, &type_context)?
        }
        other => {
            return Err(format!(
                "generateGraphicalPou called with non-graphical body: {other:?}"
            ))
        }
    };

    let keyword = pou.pou_type.keyword();
    let mut output = format!("{keyword} {}", pou.name);
    if pou.pou_type == TranspilePouKind::Function {
        output.push_str(" : ");
        output.push_str(
            &pou.interface
                .return_type
                .as_deref()
                .unwrap_or("BOOL")
                .to_ascii_uppercase(),
        );
    }
    output.push('\n');

    let mut variables = pou.interface.variables.clone();
    for synthetic in &emitted.synthetic_vars {
        let variable_type = if is_plc_base_type(&synthetic.variable_type) {
            TranspileVariableType::BaseType {
                value: synthetic.variable_type.to_ascii_uppercase(),
            }
        } else {
            TranspileVariableType::Derived {
                value: synthetic.variable_type.clone(),
            }
        };
        variables.push(TranspileVariable {
            name: synthetic.name.clone(),
            variable_type,
            class: Some(TranspileVariableClass::Local),
            location: String::new(),
            initial_value: None,
            documentation: String::new(),
        });
    }

    for entry in compute_interface(&variables) {
        output.push_str("  ");
        output.push_str(entry.keyword);
        output.push('\n');
        for variable in entry.variables {
            output.push_str("    ");
            output.push_str(&variable.name);
            output.push(' ');
            if !variable.location.is_empty() {
                output.push_str("AT ");
                output.push_str(&variable.location);
                output.push(' ');
            }
            output.push_str(": ");
            output.push_str(&format_variable_type(&variable.variable_type));
            if let Some(value) = variable
                .initial_value
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                output.push_str(" := ");
                output.push_str(&compute_value(
                    project,
                    value,
                    &declared_type_name(variable),
                ));
            }
            output.push_str(";\n");
        }
        output.push_str("  END_VAR\n");
    }
    output.push_str(&emitted.body_st);
    output.push_str("END_");
    output.push_str(keyword);
    output.push_str("\n\n");
    Ok((output, emitted.warnings))
}

pub fn generate_configuration(project: &TranspileProject) -> Result<String, String> {
    let mut tasks = project.configuration.tasks.clone();
    tasks.sort_by_key(|task| task.priority);

    let mut output = String::from("\nCONFIGURATION Config0\n");
    if !project.configuration.global_variables.is_empty() {
        output.push_str("  VAR_GLOBAL\n");
        for variable in &project.configuration.global_variables {
            output.push_str("    ");
            output.push_str(&variable.name);
            output.push(' ');
            if !variable.location.is_empty() {
                output.push_str("AT ");
                output.push_str(&variable.location);
                output.push(' ');
            }
            output.push_str(": ");
            output.push_str(&format_variable_type(&variable.variable_type));
            if let Some(value) = variable
                .initial_value
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                output.push_str(" := ");
                output.push_str(&compute_value(
                    project,
                    value,
                    &declared_type_name(variable),
                ));
            }
            output.push_str(";\n");
        }
        output.push_str("  END_VAR\n");
    }

    output.push_str("\n  RESOURCE Res0 ON PLC\n");
    for task in &tasks {
        output.push_str("    TASK ");
        output.push_str(&task.name);
        output.push('(');
        if task.triggering != TranspileTaskTrigger::Cyclic {
            let single = task.single.as_deref().unwrap_or_default();
            if single.is_empty() {
                return Err(format!(
                    "Source signal has to be defined for single task '{}' in resource 'Config0.Res0'.",
                    task.name
                ));
            }
            let single_keyword = if single.starts_with('[') && single.ends_with(']') {
                "MULTI"
            } else {
                "SINGLE"
            };
            output.push_str(single_keyword);
            output.push_str(" := ");
            output.push_str(single);
            output.push(',');
        }
        if let Some(interval) = task.interval.as_deref() {
            output.push_str("INTERVAL := ");
            output.push_str(interval);
            output.push(',');
        }
        output.push_str("PRIORITY := ");
        output.push_str(&task.priority.to_string());
        output.push_str(");\n");
    }

    for task in &tasks {
        for instance in &project.configuration.instances {
            if instance.task.as_deref() != Some(task.name.as_str()) {
                continue;
            }
            output.push_str("    PROGRAM ");
            output.push_str(&instance.name);
            output.push_str(" WITH ");
            output.push_str(&task.name);
            output.push_str(" : ");
            output.push_str(&instance.program);
            output.push_str(";\n");
        }
    }
    for instance in &project.configuration.instances {
        if instance
            .task
            .as_deref()
            .is_some_and(|task| !task.is_empty())
        {
            continue;
        }
        output.push_str("    PROGRAM ");
        output.push_str(&instance.name);
        output.push_str(" : ");
        output.push_str(&instance.program);
        output.push_str(";\n");
    }
    output.push_str("  END_RESOURCE\nEND_CONFIGURATION\n");
    Ok(output)
}

pub fn generate_data_types(project: &TranspileProject) -> Result<String, String> {
    if project.data_types.is_empty() {
        return Ok(String::new());
    }
    let by_name: HashMap<&str, &TranspileDataType> = project
        .data_types
        .iter()
        .map(|data_type| (data_type.name(), data_type))
        .collect();
    let mut computed = HashSet::new();
    let mut body = String::new();
    for data_type in &project.data_types {
        generate_data_type(
            project,
            data_type.name(),
            &by_name,
            &mut computed,
            &mut body,
        )?;
    }
    Ok(format!("TYPE\n{body}END_TYPE\n\n"))
}

fn generate_data_type(
    project: &TranspileProject,
    name: &str,
    by_name: &HashMap<&str, &TranspileDataType>,
    computed: &mut HashSet<String>,
    body: &mut String,
) -> Result<(), String> {
    if !computed.insert(name.to_owned()) {
        return Ok(());
    }
    let Some(data_type) = by_name.get(name).copied() else {
        return Ok(());
    };

    let mut line = format!("  {name} : ");
    match data_type {
        TranspileDataType::DirectlyDerived {
            base_type,
            initial_value,
            ..
        } => {
            if by_name.contains_key(base_type.as_str()) {
                generate_data_type(project, base_type, by_name, computed, body)?;
                line.push_str(base_type);
            } else {
                line.push_str(&base_type.to_ascii_uppercase());
            }
            append_data_type_initial(project, &mut line, initial_value.as_deref(), name);
        }
        TranspileDataType::Enumerated {
            values,
            initial_value,
            ..
        } => {
            line.push('(');
            line.push_str(&values.join(", "));
            line.push(')');
            append_data_type_initial(project, &mut line, initial_value.as_deref(), name);
        }
        TranspileDataType::Array {
            dimensions,
            base_type,
            initial_value,
            ..
        } => {
            let resolved = if by_name.contains_key(base_type.as_str()) {
                generate_data_type(project, base_type, by_name, computed, body)?;
                base_type.clone()
            } else {
                base_type.to_ascii_uppercase()
            };
            line.push_str("ARRAY [");
            line.push_str(&dimensions.join(","));
            line.push_str("] OF ");
            line.push_str(&resolved);
            append_data_type_initial(project, &mut line, initial_value.as_deref(), name);
        }
        TranspileDataType::Structure {
            variables,
            initial_value,
            ..
        } => {
            line.push_str("STRUCT");
            for variable in variables {
                let type_text = match &variable.variable_type {
                    TranspileVariableType::Derived { value }
                    | TranspileVariableType::UserDataType { value } => {
                        if by_name.contains_key(value.as_str()) {
                            generate_data_type(project, value, by_name, computed, body)?;
                        }
                        value.clone()
                    }
                    other => format_variable_type(other),
                };
                line.push_str("\n    ");
                line.push_str(&variable.name);
                line.push_str(" : ");
                line.push_str(&type_text);
                if let Some(value) = variable
                    .initial_value
                    .as_deref()
                    .filter(|value| !value.is_empty())
                {
                    line.push_str(" := ");
                    line.push_str(&compute_value(project, value, &type_text));
                }
                line.push(';');
            }
            line.push_str("\n  END_STRUCT");
            append_data_type_initial(project, &mut line, initial_value.as_deref(), name);
        }
    }
    line.push_str(";\n");
    body.push_str(&line);
    Ok(())
}

fn append_data_type_initial(
    project: &TranspileProject,
    output: &mut String,
    value: Option<&str>,
    type_name: &str,
) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        output.push_str(" := ");
        output.push_str(&compute_value(project, value, type_name));
    }
}

fn format_return_type(return_type: Option<&str>) -> String {
    let return_type = return_type
        .filter(|value| !value.is_empty())
        .unwrap_or("BOOL");
    if is_plc_base_type(return_type) {
        return_type.to_ascii_uppercase()
    } else {
        return_type.to_owned()
    }
}

fn format_variable_type(variable_type: &TranspileVariableType) -> String {
    match variable_type {
        TranspileVariableType::BaseType { value } => value.to_ascii_uppercase(),
        TranspileVariableType::Derived { value }
        | TranspileVariableType::UserDataType { value } => value.clone(),
        TranspileVariableType::Array {
            dimensions,
            base_type,
        } => format!(
            "ARRAY [{}] OF {}",
            dimensions.join(","),
            base_type.to_ascii_uppercase()
        ),
    }
}

fn declared_type_name(variable: &TranspileVariable) -> String {
    match &variable.variable_type {
        TranspileVariableType::Derived { value }
        | TranspileVariableType::UserDataType { value } => value.clone(),
        other => format_variable_type(other),
    }
}

fn compute_value(project: &TranspileProject, value: &str, variable_type: &str) -> String {
    match resolve_base_type(project, variable_type).as_deref() {
        Some("STRING") if !value.starts_with('\'') && !value.ends_with('\'') => {
            format!("'{value}'")
        }
        Some("WSTRING") if !value.starts_with('"') && !value.ends_with('"') => {
            format!("\"{value}\"")
        }
        _ => value.to_owned(),
    }
}

fn resolve_base_type(project: &TranspileProject, name: &str) -> Option<String> {
    let upper = name.to_ascii_uppercase();
    if is_type_hierarchy_name(&upper) {
        return Some(upper);
    }
    let data_type = project
        .data_types
        .iter()
        .find(|data_type| data_type.name() == name)?;
    match data_type {
        TranspileDataType::Array { base_type, .. }
        | TranspileDataType::DirectlyDerived { base_type, .. } => {
            resolve_base_type(project, base_type)
        }
        TranspileDataType::Enumerated { .. } | TranspileDataType::Structure { .. } => {
            Some(name.to_owned())
        }
    }
}

fn is_plc_base_type(name: &str) -> bool {
    matches!(
        name.to_ascii_uppercase().as_str(),
        "BOOL"
            | "SINT"
            | "INT"
            | "DINT"
            | "LINT"
            | "USINT"
            | "UINT"
            | "UDINT"
            | "ULINT"
            | "REAL"
            | "LREAL"
            | "TIME"
            | "DATE"
            | "TOD"
            | "DT"
            | "STRING"
            | "BYTE"
            | "WORD"
            | "DWORD"
            | "LWORD"
    )
}

fn is_type_hierarchy_name(name: &str) -> bool {
    is_plc_base_type(name) || name == "__XWORD"
}

struct InterfaceEntry<'a> {
    keyword: &'static str,
    variables: Vec<&'a TranspileVariable>,
}

fn compute_interface(variables: &[TranspileVariable]) -> Vec<InterfaceEntry<'_>> {
    let mut grouped: Vec<(
        &'static str,
        Vec<&TranspileVariable>,
        Vec<&TranspileVariable>,
    )> = Vec::new();
    for variable in variables {
        let keyword = class_keyword(variable.class.unwrap_or(TranspileVariableClass::Local));
        let bucket = if let Some(index) = grouped
            .iter()
            .position(|(existing, _, _)| *existing == keyword)
        {
            &mut grouped[index]
        } else {
            grouped.push((keyword, Vec::new(), Vec::new()));
            grouped.last_mut().expect("bucket inserted")
        };
        if variable.location.is_empty() {
            bucket.1.push(variable);
        } else {
            bucket.2.push(variable);
        }
    }
    let mut output = Vec::new();
    for (keyword, unlocated, located) in grouped {
        if !unlocated.is_empty() {
            output.push(InterfaceEntry {
                keyword,
                variables: unlocated,
            });
        }
        if !located.is_empty() {
            output.push(InterfaceEntry {
                keyword,
                variables: located,
            });
        }
    }
    output
}

fn class_keyword(class: TranspileVariableClass) -> &'static str {
    match class {
        TranspileVariableClass::Input => "VAR_INPUT",
        TranspileVariableClass::Output => "VAR_OUTPUT",
        TranspileVariableClass::InOut => "VAR_IN_OUT",
        TranspileVariableClass::External => "VAR_EXTERNAL",
        TranspileVariableClass::Local => "VAR",
        TranspileVariableClass::Temp => "VAR_TEMP",
    }
}

fn reindent_text(text: &str, spaces: usize) -> String {
    let mut lines = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    let Some(first) = lines.iter().find(|line| !line.trim().is_empty()) else {
        return String::new();
    };
    let source_indent = first
        .chars()
        .take_while(|character| *character == ' ')
        .count();
    let prefix = " ".repeat(spaces.saturating_sub(source_indent));
    let mut output = String::new();
    for line in lines {
        if line.is_empty() {
            output.push('\n');
        } else {
            output.push_str(&prefix);
            output.push_str(&line);
            output.push('\n');
        }
    }
    output
}

fn emission_order(project: &TranspileProject) -> Vec<usize> {
    let index_by_name = project
        .pous
        .iter()
        .enumerate()
        .map(|(index, pou)| (pou.name.clone(), index))
        .collect::<HashMap<_, _>>();
    let names = index_by_name.keys().cloned().collect::<HashSet<_>>();
    let mut emitted = HashSet::new();
    let mut order = Vec::new();

    fn visit(
        index: usize,
        project: &TranspileProject,
        index_by_name: &HashMap<String, usize>,
        names: &HashSet<String>,
        emitted: &mut HashSet<String>,
        order: &mut Vec<usize>,
    ) {
        let pou = &project.pous[index];
        if !emitted.insert(pou.name.clone()) {
            return;
        }
        for dependency in extract_dependencies(pou, names) {
            if let Some(dependency_index) = index_by_name.get(&dependency).copied() {
                visit(
                    dependency_index,
                    project,
                    index_by_name,
                    names,
                    emitted,
                    order,
                );
            }
        }
        order.push(index);
    }

    for index in 0..project.pous.len() {
        visit(
            index,
            project,
            &index_by_name,
            &names,
            &mut emitted,
            &mut order,
        );
    }
    order
}

fn extract_dependencies(pou: &TranspilePou, names: &HashSet<String>) -> Vec<String> {
    let mut dependencies = Vec::new();
    for variable in &pou.interface.variables {
        let Some(type_name) = variable.variable_type.derived_name() else {
            continue;
        };
        if type_name != pou.name
            && names.contains(type_name)
            && !dependencies.iter().any(|name| name == type_name)
        {
            dependencies.push(type_name.to_owned());
        }
    }
    if let Some(body) = pou.body.value.as_str() {
        let upper = body.to_ascii_uppercase();
        for name in names {
            if name == &pou.name {
                continue;
            }
            if identifier_reference(&upper, &name.to_ascii_uppercase())
                && !dependencies.iter().any(|dependency| dependency == name)
            {
                dependencies.push(name.clone());
            }
        }
    }
    dependencies
}

fn identifier_reference(haystack: &str, needle: &str) -> bool {
    for (offset, _) in haystack.match_indices(needle) {
        let before = haystack[..offset].chars().next_back();
        let after = haystack[offset + needle.len()..].chars().next();
        let boundary = |character: Option<char>| {
            character.is_none_or(|character| !character.is_ascii_alphanumeric() && character != '^')
        };
        if boundary(before) && boundary(after) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn local(name: &str, variable_type: TranspileVariableType) -> TranspileVariable {
        TranspileVariable {
            name: name.to_owned(),
            variable_type,
            class: Some(TranspileVariableClass::Local),
            location: String::new(),
            initial_value: None,
            documentation: String::new(),
        }
    }

    fn program(name: &str, body: &str) -> TranspilePou {
        TranspilePou {
            name: name.to_owned(),
            pou_type: TranspilePouKind::Program,
            documentation: String::new(),
            interface: TranspilePouInterface {
                return_type: None,
                variables: vec![local(
                    "x",
                    TranspileVariableType::BaseType {
                        value: "int".to_owned(),
                    },
                )],
            },
            body: TranspileBody::text(TranspileBodyLanguage::St, body),
        }
    }

    #[test]
    fn textual_pou_matches_openplc_emitter_shape() {
        let mut project = TranspileProject::default();
        let mut pou = program("main", "x := 1;");
        pou.interface.variables.push(TranspileVariable {
            name: "sensor".to_owned(),
            variable_type: TranspileVariableType::BaseType {
                value: "bool".to_owned(),
            },
            class: Some(TranspileVariableClass::Input),
            location: "%IX0.0".to_owned(),
            initial_value: Some("TRUE".to_owned()),
            documentation: String::new(),
        });
        project.pous.push(pou.clone());
        let text = generate_textual_pou(&pou, &project, 2).unwrap();
        assert_eq!(
            text,
            "PROGRAM main\n  VAR\n    x : INT;\n  END_VAR\n  VAR_INPUT\n    sensor AT %IX0.0 : BOOL := TRUE;\n  END_VAR\n\n  x := 1;\nEND_PROGRAM\n\n"
        );
    }

    #[test]
    fn quotes_string_alias_initial_values_like_openplc() {
        let project = TranspileProject {
            data_types: vec![TranspileDataType::DirectlyDerived {
                name: "Label".to_owned(),
                base_type: "STRING".to_owned(),
                initial_value: None,
            }],
            ..Default::default()
        };
        assert_eq!(compute_value(&project, "hello", "Label"), "'hello'");
        assert_eq!(compute_value(&project, "'hello'", "Label"), "'hello'");
    }

    #[test]
    fn emits_data_type_dependencies_before_dependents() {
        let project = TranspileProject {
            data_types: vec![
                TranspileDataType::DirectlyDerived {
                    name: "Outer".to_owned(),
                    base_type: "Inner".to_owned(),
                    initial_value: None,
                },
                TranspileDataType::DirectlyDerived {
                    name: "Inner".to_owned(),
                    base_type: "int".to_owned(),
                    initial_value: None,
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            generate_data_types(&project).unwrap(),
            "TYPE\n  Inner : INT;\n  Outer : Inner;\nEND_TYPE\n\n"
        );
    }

    #[test]
    fn emits_configuration_priority_and_task_binding() {
        let project = TranspileProject {
            configuration: TranspileConfiguration {
                tasks: vec![TranspileTask {
                    name: "task0".to_owned(),
                    priority: 1,
                    triggering: TranspileTaskTrigger::Cyclic,
                    interval: Some("T#20ms".to_owned()),
                    single: None,
                }],
                instances: vec![TranspileInstance {
                    name: "instance0".to_owned(),
                    program: "main".to_owned(),
                    task: Some("task0".to_owned()),
                }],
                global_variables: Vec::new(),
            },
            ..Default::default()
        };
        assert_eq!(
            generate_configuration(&project).unwrap(),
            "\nCONFIGURATION Config0\n\n  RESOURCE Res0 ON PLC\n    TASK task0(INTERVAL := T#20ms,PRIORITY := 1);\n    PROGRAM instance0 WITH task0 : main;\n  END_RESOURCE\nEND_CONFIGURATION\n"
        );
    }

    #[test]
    fn dependency_order_matches_openplc_textual_scan() {
        let fb = TranspilePou {
            name: "Blink".to_owned(),
            pou_type: TranspilePouKind::FunctionBlock,
            documentation: String::new(),
            interface: TranspilePouInterface {
                return_type: None,
                variables: vec![local(
                    "enabled",
                    TranspileVariableType::BaseType {
                        value: "BOOL".to_owned(),
                    },
                )],
            },
            body: TranspileBody::text(TranspileBodyLanguage::St, "enabled := TRUE;"),
        };
        let mut main = program("main", "blink1();");
        main.interface.variables.push(local(
            "blink1",
            TranspileVariableType::Derived {
                value: "Blink".to_owned(),
            },
        ));
        let project = TranspileProject {
            pous: vec![main, fb],
            ..Default::default()
        };
        assert_eq!(emission_order(&project), vec![1, 0]);
    }

    #[test]
    fn compiles_graphical_pou_through_shared_walker() {
        let project = TranspileProject {
            pous: vec![TranspilePou {
                name: "main".to_owned(),
                pou_type: TranspilePouKind::Program,
                documentation: String::new(),
                interface: TranspilePouInterface {
                    return_type: None,
                    variables: vec![local(
                        "x",
                        TranspileVariableType::BaseType {
                            value: "BOOL".to_owned(),
                        },
                    )],
                },
                body: TranspileBody {
                    language: TranspileBodyLanguage::Ld,
                    value: json!({"rungs": []}),
                },
            }],
            ..Default::default()
        };
        let result = transpile_to_st(&project);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let program = result.program_st.expect("graphical program ST");
        assert!(program.contains("PROGRAM main\n"));
        assert!(program.contains("  VAR\n    x : BOOL;\n  END_VAR\n"));
        assert!(program.contains("END_PROGRAM\n\n"));
    }
}
