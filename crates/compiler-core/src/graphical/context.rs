use crate::{
    TranspileBodyLanguage, TranspileDataType, TranspilePou, TranspilePouKind, TranspileProject,
    TranspileVariableClass, TranspileVariableType,
};

use super::connection_types::{BlockInfos, BlockIo, TypeContext};

pub fn build_type_context(pou: &TranspilePou, project: &TranspileProject) -> TypeContext {
    let mut context = TypeContext::default();

    for variable in &pou.interface.variables {
        context.variable_types.insert(
            variable.name.clone(),
            format_variable_type(&variable.variable_type),
        );
    }
    if pou.pou_type == TranspilePouKind::Function {
        context.function_name = Some(pou.name.clone());
        context.function_return_type =
            Some(format_return_type(pou.interface.return_type.as_deref()));
    }

    // User POUs are resolvable as blocks before library variants, matching
    // OpenPLC's `buildTypeContext` precedence.
    for project_pou in &project.pous {
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        for variable in &project_pou.interface.variables {
            let io = BlockIo {
                name: variable.name.clone(),
                variable_type: format_variable_type(&variable.variable_type),
            };
            match variable.class.unwrap_or(TranspileVariableClass::Local) {
                TranspileVariableClass::Input => inputs.push(io),
                TranspileVariableClass::Output => outputs.push(io),
                TranspileVariableClass::InOut => {
                    inputs.push(io.clone());
                    outputs.push(io);
                }
                TranspileVariableClass::External
                | TranspileVariableClass::Local
                | TranspileVariableClass::Temp => {}
            }
        }
        if project_pou.pou_type == TranspilePouKind::Function {
            outputs.push(BlockIo {
                name: "OUT".to_owned(),
                variable_type: format_return_type(project_pou.interface.return_type.as_deref()),
            });
        }
        context.block_infos.insert(
            project_pou.name.clone(),
            BlockInfos {
                name: project_pou.name.clone(),
                function_block: project_pou.pou_type != TranspilePouKind::Function,
                extensible: false,
                inputs,
                outputs,
            },
        );
    }

    for data_type in &project.data_types {
        if let TranspileDataType::Structure {
            name, variables, ..
        } = data_type
        {
            context.structure_fields.insert(
                name.clone(),
                variables
                    .iter()
                    .map(|variable| {
                        (
                            variable.name.clone(),
                            format_variable_type(&variable.variable_type),
                        )
                    })
                    .collect(),
            );
        }
    }

    collect_embedded_block_signatures(project, &mut context);
    context
}

fn collect_embedded_block_signatures(project: &TranspileProject, context: &mut TypeContext) {
    for pou in &project.pous {
        match pou.body.language {
            TranspileBodyLanguage::Ld => {
                for rung in pou
                    .body
                    .value
                    .get("rungs")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(nodes) = rung.get("nodes").and_then(serde_json::Value::as_array) {
                        collect_block_nodes(nodes, context);
                    }
                }
            }
            TranspileBodyLanguage::Fbd => {
                if let Some(nodes) = pou
                    .body
                    .value
                    .get("rung")
                    .and_then(|rung| rung.get("nodes"))
                    .and_then(serde_json::Value::as_array)
                {
                    collect_block_nodes(nodes, context);
                }
            }
            _ => {}
        }
    }
}

fn collect_block_nodes(nodes: &[serde_json::Value], context: &mut TypeContext) {
    for node in nodes {
        if node.get("type").and_then(serde_json::Value::as_str) != Some("block") {
            continue;
        }
        let Some(variant) = node
            .get("data")
            .and_then(|data| data.get("variant"))
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };
        let Some(name) = variant.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if context.block_infos.contains_key(name) {
            continue;
        }

        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        for variable in variant
            .get("variables")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(variable) = variable.as_object() else {
                continue;
            };
            let Some(variable_name) = variable.get("name").and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let Some(variable_type) = variable
                .get("type")
                .and_then(|value| value.get("value"))
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let io = BlockIo {
                name: variable_name.to_owned(),
                variable_type: normalize_block_type(variable_type),
            };
            match variable.get("class").and_then(serde_json::Value::as_str) {
                Some("input") => inputs.push(io),
                Some("output") => outputs.push(io),
                Some("inOut") | Some("inout") => {
                    inputs.push(io.clone());
                    outputs.push(io);
                }
                _ => {}
            }
        }

        context.block_infos.insert(
            name.to_owned(),
            BlockInfos {
                name: name.to_owned(),
                function_block: matches!(
                    variant.get("type").and_then(serde_json::Value::as_str),
                    Some("function-block") | Some("function-block-instance")
                ),
                extensible: variant
                    .get("extensible")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                inputs,
                outputs,
            },
        );
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

fn format_return_type(return_type: Option<&str>) -> String {
    let value = return_type
        .filter(|value| !value.is_empty())
        .unwrap_or("BOOL");
    if is_base_type(value) {
        value.to_ascii_uppercase()
    } else {
        value.to_owned()
    }
}

fn normalize_block_type(value: &str) -> String {
    let upper = value.to_ascii_uppercase();
    if upper.starts_with("ANY") || is_base_type(&upper) || upper == "__XWORD" {
        upper
    } else {
        value.to_owned()
    }
}

fn is_base_type(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "BOOL"
            | "BYTE"
            | "WORD"
            | "DWORD"
            | "LWORD"
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
            | "WSTRING"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TranspileBody, TranspilePouInterface, TranspileVariable, TranspileVariableType};
    use serde_json::json;

    #[test]
    fn embedded_generic_block_and_pou_types_are_resolved() {
        let function = TranspilePou {
            name: "Scale".to_owned(),
            pou_type: TranspilePouKind::Function,
            documentation: String::new(),
            interface: TranspilePouInterface {
                return_type: Some("DINT".to_owned()),
                variables: vec![TranspileVariable {
                    name: "value".to_owned(),
                    variable_type: TranspileVariableType::BaseType {
                        value: "DINT".to_owned(),
                    },
                    class: Some(TranspileVariableClass::Input),
                    location: String::new(),
                    initial_value: None,
                    documentation: String::new(),
                }],
            },
            body: TranspileBody::text(TranspileBodyLanguage::St, "Scale := value;"),
        };
        let main = TranspilePou {
            name: "main".to_owned(),
            pou_type: TranspilePouKind::Program,
            documentation: String::new(),
            interface: TranspilePouInterface {
                return_type: None,
                variables: vec![TranspileVariable {
                    name: "x".to_owned(),
                    variable_type: TranspileVariableType::BaseType {
                        value: "DINT".to_owned(),
                    },
                    class: Some(TranspileVariableClass::Local),
                    location: String::new(),
                    initial_value: None,
                    documentation: String::new(),
                }],
            },
            body: TranspileBody {
                language: TranspileBodyLanguage::Ld,
                value: json!({"rungs":[{"nodes":[{
                    "id":"add","type":"block","position":{"x":0,"y":0},
                    "data":{"variant":{"name":"ADD","type":"function","variables":[
                        {"name":"OUT","class":"output","type":{"value":"ANY_NUM"}},
                        {"name":"IN1","class":"input","type":{"value":"ANY_NUM"}}
                    ]}}
                }],"edges":[]}]}),
            },
        };
        let project = TranspileProject {
            pous: vec![main.clone(), function],
            ..Default::default()
        };
        let context = build_type_context(&main, &project);
        assert_eq!(
            context.variable_types.get("x").map(String::as_str),
            Some("DINT")
        );
        assert_eq!(
            context
                .block_infos
                .get("Scale")
                .and_then(|info| info.outputs.first())
                .map(|output| output.variable_type.as_str()),
            Some("DINT")
        );
        assert_eq!(
            context
                .block_infos
                .get("ADD")
                .and_then(|info| info.outputs.first())
                .map(|output| output.variable_type.as_str()),
            Some("ANY_NUM")
        );
    }
}
