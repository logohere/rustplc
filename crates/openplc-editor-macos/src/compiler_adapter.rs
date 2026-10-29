use open_rust_plc_compiler::{
    TranspileBody, TranspileBodyLanguage, TranspileConfiguration, TranspileDataType,
    TranspileInstance, TranspilePou, TranspilePouInterface, TranspilePouKind, TranspileProject,
    TranspileTask, TranspileTaskTrigger, TranspileVariable, TranspileVariableClass,
    TranspileVariableType,
};
use serde_json::Value;

use crate::plc_model::{
    PlcVariable, PlcVariableType, PouLanguage, PouType, VariableClass, VariableTypeDefinition,
};
use crate::project::OpenProject;

pub fn project_to_compiler_ir(project: &OpenProject) -> Result<TranspileProject, String> {
    let pous = project
        .pous
        .iter()
        .filter_map(|file| file.parsed.as_ref())
        .map(project_pou)
        .collect::<Result<Vec<_>, _>>()?;
    let data_types = project
        .project
        .data
        .get("dataTypes")
        .and_then(Value::as_array)
        .map(|data_types| {
            data_types
                .iter()
                .map(project_data_type)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let resource = project
        .project
        .data
        .get("configuration")
        .and_then(|value| value.get("resource"));
    let tasks = resource
        .and_then(|value| value.get("tasks"))
        .and_then(Value::as_array)
        .map(|tasks| {
            tasks
                .iter()
                .map(project_task)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let instances = resource
        .and_then(|value| value.get("instances"))
        .and_then(Value::as_array)
        .map(|instances| {
            instances
                .iter()
                .map(project_instance)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let global_variables = resource
        .and_then(|value| value.get("globalVariables"))
        .and_then(Value::as_array)
        .map(|variables| {
            variables
                .iter()
                .map(project_json_variable)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(TranspileProject {
        data_types,
        pous,
        configuration: TranspileConfiguration {
            tasks,
            instances,
            global_variables,
        },
    })
}

fn project_pou(pou: &crate::plc_model::ParsedPou) -> Result<TranspilePou, String> {
    Ok(TranspilePou {
        name: pou.name.clone(),
        pou_type: match pou.pou_type {
            PouType::Program => TranspilePouKind::Program,
            PouType::Function => TranspilePouKind::Function,
            PouType::FunctionBlock => TranspilePouKind::FunctionBlock,
        },
        documentation: pou.documentation.clone(),
        interface: TranspilePouInterface {
            return_type: (pou.pou_type == PouType::Function).then(|| {
                pou.interface
                    .return_type
                    .clone()
                    .unwrap_or_else(|| "BOOL".to_owned())
            }),
            variables: pou
                .interface
                .variables
                .iter()
                .map(project_variable)
                .collect::<Result<Vec<_>, _>>()?,
        },
        body: TranspileBody {
            language: match pou.body.language {
                PouLanguage::St => TranspileBodyLanguage::St,
                PouLanguage::Il => TranspileBodyLanguage::Il,
                PouLanguage::Ld => TranspileBodyLanguage::Ld,
                PouLanguage::Fbd => TranspileBodyLanguage::Fbd,
                PouLanguage::Python => TranspileBodyLanguage::Python,
                PouLanguage::Cpp => TranspileBodyLanguage::Cpp,
            },
            value: pou.body.value.clone(),
        },
    })
}

fn project_variable(variable: &PlcVariable) -> Result<TranspileVariable, String> {
    Ok(TranspileVariable {
        name: variable.name.clone(),
        variable_type: project_variable_type(&variable.variable_type)?,
        class: variable.class.map(normalize_variable_class),
        location: variable.location.clone(),
        initial_value: variable
            .initial_value
            .clone()
            .filter(|value| !value.is_empty()),
        documentation: variable.documentation.clone(),
    })
}

fn normalize_variable_class(class: VariableClass) -> TranspileVariableClass {
    match class {
        VariableClass::Input => TranspileVariableClass::Input,
        VariableClass::Output => TranspileVariableClass::Output,
        VariableClass::InOut => TranspileVariableClass::InOut,
        VariableClass::External => TranspileVariableClass::External,
        VariableClass::Local | VariableClass::Global => TranspileVariableClass::Local,
        VariableClass::Temp => TranspileVariableClass::Temp,
    }
}

fn project_variable_type(variable_type: &PlcVariableType) -> Result<TranspileVariableType, String> {
    Ok(match variable_type.definition {
        VariableTypeDefinition::BaseType => TranspileVariableType::BaseType {
            value: variable_type.value.clone(),
        },
        VariableTypeDefinition::Derived => TranspileVariableType::Derived {
            value: variable_type.value.clone(),
        },
        VariableTypeDefinition::UserDataType => TranspileVariableType::UserDataType {
            value: variable_type.value.clone(),
        },
        VariableTypeDefinition::Array => {
            let data = variable_type.data.as_ref().ok_or_else(|| {
                format!(
                    "array variable type {:?} is missing data",
                    variable_type.value
                )
            })?;
            TranspileVariableType::Array {
                dimensions: data
                    .dimensions
                    .iter()
                    .map(|dimension| dimension.dimension.clone())
                    .collect(),
                base_type: data.base_type.value.clone(),
            }
        }
    })
}

fn project_data_type(value: &Value) -> Result<TranspileDataType, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "data type entry is not an object".to_owned())?;
    let name = string_field(value, "name")?.to_owned();
    let derivation = string_field(value, "derivation")?;
    let initial_value = object
        .get("initialValue")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    match derivation {
        "array" => Ok(TranspileDataType::Array {
            name,
            dimensions: object
                .get("dimensions")
                .and_then(Value::as_array)
                .map(|dimensions| {
                    dimensions
                        .iter()
                        .map(|dimension| string_field(dimension, "dimension").map(str::to_owned))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default(),
            base_type: type_value(
                object
                    .get("baseType")
                    .ok_or_else(|| "array data type has no baseType".to_owned())?,
            )?,
            initial_value,
        }),
        "enumerated" => Ok(TranspileDataType::Enumerated {
            name,
            values: object
                .get("values")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .map(|value| string_field(value, "description").map(str::to_owned))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default(),
            initial_value,
        }),
        "structure" => Ok(TranspileDataType::Structure {
            name,
            variables: object
                .get("variable")
                .and_then(Value::as_array)
                .map(|variables| {
                    variables
                        .iter()
                        .map(project_structure_variable)
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default(),
            initial_value,
        }),
        "directly-derived" => Ok(TranspileDataType::DirectlyDerived {
            name,
            base_type: object
                .get("baseType")
                .and_then(Value::as_str)
                .ok_or_else(|| "directly-derived data type has no string baseType".to_owned())?
                .to_owned(),
            initial_value,
        }),
        other => Err(format!("unsupported data type derivation {other:?}")),
    }
}

fn project_structure_variable(value: &Value) -> Result<TranspileVariable, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "structure variable is not an object".to_owned())?;
    let variable_type = object
        .get("type")
        .ok_or_else(|| "structure variable has no type".to_owned())?;
    Ok(TranspileVariable {
        name: string_field(value, "name")?.to_owned(),
        variable_type: project_json_variable_type(variable_type)?,
        class: None,
        location: String::new(),
        initial_value: object
            .get("initialValue")
            .and_then(|value| value.get("simpleValue"))
            .and_then(|value| value.get("value"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        documentation: String::new(),
    })
}

fn project_json_variable(value: &Value) -> Result<TranspileVariable, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "variable is not an object".to_owned())?;
    let variable_type = object
        .get("type")
        .ok_or_else(|| "variable has no type".to_owned())?;
    Ok(TranspileVariable {
        name: string_field(value, "name")?.to_owned(),
        variable_type: project_json_variable_type(variable_type)?,
        class: object
            .get("class")
            .and_then(Value::as_str)
            .and_then(parse_variable_class),
        location: object
            .get("location")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        initial_value: object
            .get("initialValue")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        documentation: object
            .get("documentation")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

fn project_json_variable_type(value: &Value) -> Result<TranspileVariableType, String> {
    let definition = string_field(value, "definition")?;
    match definition {
        "base-type" => Ok(TranspileVariableType::BaseType {
            value: string_field(value, "value")?.to_owned(),
        }),
        "derived" => Ok(TranspileVariableType::Derived {
            value: string_field(value, "value")?.to_owned(),
        }),
        "user-data-type" => Ok(TranspileVariableType::UserDataType {
            value: string_field(value, "value")?.to_owned(),
        }),
        "array" => {
            let data = value
                .get("data")
                .ok_or_else(|| "array type has no data".to_owned())?;
            Ok(TranspileVariableType::Array {
                dimensions: data
                    .get("dimensions")
                    .and_then(Value::as_array)
                    .map(|dimensions| {
                        dimensions
                            .iter()
                            .map(|dimension| {
                                string_field(dimension, "dimension").map(str::to_owned)
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default(),
                base_type: type_value(
                    data.get("baseType")
                        .ok_or_else(|| "array type data has no baseType".to_owned())?,
                )?,
            })
        }
        other => Err(format!("unsupported variable type definition {other:?}")),
    }
}

fn project_task(value: &Value) -> Result<TranspileTask, String> {
    let triggering = string_field(value, "triggering")?;
    let interval = string_field(value, "interval")
        .unwrap_or_default()
        .to_owned();
    Ok(TranspileTask {
        name: string_field(value, "name")?.to_owned(),
        priority: value
            .get("priority")
            .and_then(Value::as_i64)
            .ok_or_else(|| "task priority is missing or not an integer".to_owned())?,
        triggering: match triggering {
            "Cyclic" => TranspileTaskTrigger::Cyclic,
            "Interrupt" => TranspileTaskTrigger::Interrupt,
            other => return Err(format!("unsupported task triggering mode {other:?}")),
        },
        interval: (triggering == "Cyclic").then_some(interval.clone()),
        single: (triggering != "Cyclic").then_some(interval),
    })
}

fn project_instance(value: &Value) -> Result<TranspileInstance, String> {
    Ok(TranspileInstance {
        name: string_field(value, "name")?.to_owned(),
        program: string_field(value, "program")?.to_owned(),
        task: value
            .get("task")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
    })
}

fn type_value(value: &Value) -> Result<String, String> {
    value
        .as_str()
        .or_else(|| value.get("value").and_then(Value::as_str))
        .map(str::to_owned)
        .ok_or_else(|| "type has no string value".to_owned())
}

fn string_field<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("field {field:?} is missing or not a string"))
}

fn parse_variable_class(value: &str) -> Option<TranspileVariableClass> {
    match value {
        "input" => Some(TranspileVariableClass::Input),
        "output" => Some(TranspileVariableClass::Output),
        "inOut" => Some(TranspileVariableClass::InOut),
        "external" => Some(TranspileVariableClass::External),
        "local" | "global" => Some(TranspileVariableClass::Local),
        "temp" => Some(TranspileVariableClass::Temp),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use open_rust_plc_compiler::transpile_to_st;
    use serde_json::json;

    use super::project_to_compiler_ir;
    use crate::plc_model::{ParsedPou, PouBody, PouInterface, PouLanguage, PouType};
    use crate::project::{
        OpenProject, PouFile, PouKind, ProjectDocument, ProjectKind, ProjectMeta,
    };

    #[test]
    fn adapts_real_open_project_into_textual_compiler_ir() {
        let main = ParsedPou {
            name: "main".to_owned(),
            pou_type: PouType::Program,
            interface: PouInterface {
                return_type: None,
                variables: vec![crate::plc_model::PlcVariable {
                    name: "x".to_owned(),
                    class: Some(crate::plc_model::VariableClass::Local),
                    variable_type: crate::plc_model::PlcVariableType::base("INT"),
                    location: String::new(),
                    initial_value: None,
                    documentation: String::new(),
                    debug: Some(false),
                }],
            },
            body: PouBody::text(PouLanguage::St, "x := 1;"),
            documentation: String::new(),
        };
        let project = OpenProject {
            root: PathBuf::from("/tmp/TestProject"),
            project: ProjectDocument {
                meta: ProjectMeta {
                    name: "TestProject".to_owned(),
                    kind: ProjectKind::PlcProject,
                    extra: serde_json::Map::new(),
                },
                data: json!({
                    "dataTypes": [],
                    "libraries": [],
                    "configuration": {
                        "resource": {
                            "tasks": [{
                                "name": "task0",
                                "triggering": "Cyclic",
                                "interval": "T#20ms",
                                "priority": 1
                            }],
                            "instances": [{
                                "name": "instance0",
                                "program": "main",
                                "task": "task0"
                            }],
                            "globalVariables": []
                        }
                    }
                }),
                extra: serde_json::Map::new(),
            },
            pous: vec![PouFile {
                name: "main".to_owned(),
                kind: PouKind::Program,
                language: Some(PouLanguage::St),
                relative_path: PathBuf::from("pous/programs/main.st"),
                absolute_path: PathBuf::from("/tmp/TestProject/pous/programs/main.st"),
                parsed: Some(main),
                variables_text: None,
                raw_content: String::new(),
            }],
            warnings: Vec::new(),
            device_configuration: json!({}),
            device_pin_mapping: json!({}),
            library_manifest: None,
        };

        let ir = project_to_compiler_ir(&project).unwrap();
        let result = transpile_to_st(&ir);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let program = result.program_st.unwrap();
        assert!(program.contains("PROGRAM main\n"));
        assert!(program.contains("  VAR\n    x : INT;\n  END_VAR\n"));
        assert!(program.contains("CONFIGURATION Config0"));
        assert!(program.contains("TASK task0(INTERVAL := T#20ms,PRIORITY := 1);"));
        assert!(program.contains("PROGRAM instance0 WITH task0 : main;"));
    }

    #[test]
    fn adapts_editor_data_types_into_the_owned_compiler_type_block() {
        let project = OpenProject {
            root: PathBuf::from("/tmp/DataTypesProject"),
            project: ProjectDocument {
                meta: ProjectMeta {
                    name: "DataTypesProject".to_owned(),
                    kind: ProjectKind::PlcProject,
                    extra: serde_json::Map::new(),
                },
                data: json!({
                    "dataTypes": [
                        {
                            "name": "Buffer",
                            "derivation": "array",
                            "baseType": {"definition":"base-type","value":"INT"},
                            "dimensions": [{"dimension":"0..9"}],
                            "initialValue": ""
                        },
                        {
                            "name": "Color",
                            "derivation": "enumerated",
                            "values": [{"description":"Red"},{"description":"Green"}],
                            "initialValue": "Red"
                        },
                        {
                            "name": "Packet",
                            "derivation": "structure",
                            "variable": [
                                {
                                    "name":"count",
                                    "type":{"definition":"base-type","value":"DINT"},
                                    "initialValue":{"simpleValue":{"value":"1"}}
                                },
                                {
                                    "name":"mask",
                                    "type":{
                                        "definition":"array",
                                        "value":"ARRAY",
                                        "data":{
                                            "dimensions":[{"dimension":"0..1"}],
                                            "baseType":{"definition":"base-type","value":"BOOL"}
                                        }
                                    },
                                    "initialValue":{"simpleValue":{"value":""}}
                                }
                            ]
                        }
                    ],
                    "configuration": {"resource": {"tasks": [], "instances": [], "globalVariables": []}}
                }),
                extra: serde_json::Map::new(),
            },
            pous: Vec::new(),
            warnings: Vec::new(),
            device_configuration: json!({}),
            device_pin_mapping: json!({}),
            library_manifest: None,
        };

        let ir = project_to_compiler_ir(&project).unwrap();
        let result = transpile_to_st(&ir);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let program = result.program_st.unwrap();
        assert!(
            program.contains("Buffer : ARRAY [0..9] OF INT;"),
            "{program}"
        );
        assert!(
            program.contains("Color : (Red, Green) := Red;"),
            "{program}"
        );
        assert!(program.contains("Packet : STRUCT"), "{program}");
        assert!(program.contains("count : DINT := 1;"), "{program}");
        assert!(
            program.contains("mask : ARRAY [0..1] OF BOOL;"),
            "{program}"
        );
    }

    #[test]
    fn adapts_resource_globals_tasks_and_instances_into_configuration_st() {
        let main = ParsedPou {
            name: "main".to_owned(),
            pou_type: PouType::Program,
            interface: PouInterface {
                return_type: None,
                variables: vec![crate::plc_model::PlcVariable {
                    name: "tick".to_owned(),
                    class: Some(crate::plc_model::VariableClass::Local),
                    variable_type: crate::plc_model::PlcVariableType::base("BOOL"),
                    location: String::new(),
                    initial_value: None,
                    documentation: String::new(),
                    debug: Some(false),
                }],
            },
            body: PouBody::text(PouLanguage::St, "tick := enabled;"),
            documentation: String::new(),
        };
        let project = OpenProject {
            root: PathBuf::from("/tmp/ResourceProject"),
            project: ProjectDocument {
                meta: ProjectMeta {
                    name: "ResourceProject".to_owned(),
                    kind: ProjectKind::PlcProject,
                    extra: serde_json::Map::new(),
                },
                data: json!({
                    "dataTypes": [],
                    "configuration": {"resource": {
                        "globalVariables": [
                            {
                                "name":"enabled",
                                "type":{"definition":"base-type","value":"BOOL"},
                                "class":"global",
                                "location":"%IX0.0",
                                "initialValue":"",
                                "documentation":"Enable input",
                                "debug":true
                            },
                            {
                                "name":"interrupt_signal",
                                "type":{"definition":"base-type","value":"BOOL"},
                                "class":"global",
                                "location":"",
                                "initialValue":"",
                                "documentation":"",
                                "debug":false
                            }
                        ],
                        "tasks": [
                            {
                                "name":"fast",
                                "triggering":"Cyclic",
                                "interval":"T#10ms",
                                "priority":2
                            },
                            {
                                "name":"irq",
                                "triggering":"Interrupt",
                                "interval":"interrupt_signal",
                                "priority":3
                            }
                        ],
                        "instances": [{
                            "name":"main_fast",
                            "program":"main",
                            "task":"fast"
                        }]
                    }}
                }),
                extra: serde_json::Map::new(),
            },
            pous: vec![PouFile {
                name: "main".to_owned(),
                kind: PouKind::Program,
                language: Some(PouLanguage::St),
                relative_path: PathBuf::from("pous/programs/main.st"),
                absolute_path: PathBuf::from("/tmp/ResourceProject/pous/programs/main.st"),
                parsed: Some(main),
                variables_text: None,
                raw_content: String::new(),
            }],
            warnings: Vec::new(),
            device_configuration: json!({}),
            device_pin_mapping: json!({}),
            library_manifest: None,
        };

        let ir = project_to_compiler_ir(&project).unwrap();
        let result = transpile_to_st(&ir);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let program = result.program_st.unwrap();
        assert!(program.contains("enabled AT %IX0.0 : BOOL;"), "{program}");
        assert!(
            program.contains("TASK fast(INTERVAL := T#10ms,PRIORITY := 2);"),
            "{program}"
        );
        assert!(
            program.contains("TASK irq(SINGLE := interrupt_signal,PRIORITY := 3);"),
            "{program}"
        );
        assert!(
            program.contains("PROGRAM main_fast WITH fast : main;"),
            "{program}"
        );
    }

    #[test]
    fn preserves_graphical_body_for_walker_stage() {
        let project = OpenProject {
            root: PathBuf::from("/tmp/LdProject"),
            project: ProjectDocument {
                meta: ProjectMeta {
                    name: "LdProject".to_owned(),
                    kind: ProjectKind::PlcProject,
                    extra: serde_json::Map::new(),
                },
                data: json!({
                    "dataTypes": [],
                    "configuration": {"resource": {"tasks": [], "instances": [], "globalVariables": []}}
                }),
                extra: serde_json::Map::new(),
            },
            pous: vec![PouFile {
                name: "main".to_owned(),
                kind: PouKind::Program,
                language: Some(PouLanguage::Ld),
                relative_path: PathBuf::from("pous/programs/main.ld"),
                absolute_path: PathBuf::from("/tmp/LdProject/pous/programs/main.ld"),
                parsed: Some(ParsedPou {
                    name: "main".to_owned(),
                    pou_type: PouType::Program,
                    interface: PouInterface {
                        return_type: None,
                        variables: vec![crate::plc_model::PlcVariable {
                            name: "x".to_owned(),
                            class: Some(crate::plc_model::VariableClass::Local),
                            variable_type: crate::plc_model::PlcVariableType::base("BOOL"),
                            location: String::new(),
                            initial_value: None,
                            documentation: String::new(),
                            debug: Some(false),
                        }],
                    },
                    body: PouBody {
                        language: PouLanguage::Ld,
                        value: json!({"name":"main","rungs":[]}),
                    },
                    documentation: String::new(),
                }),
                variables_text: None,
                raw_content: String::new(),
            }],
            warnings: Vec::new(),
            device_configuration: json!({}),
            device_pin_mapping: json!({}),
            library_manifest: None,
        };
        let ir = project_to_compiler_ir(&project).unwrap();
        assert_eq!(
            ir.pous[0].body.language,
            open_rust_plc_compiler::TranspileBodyLanguage::Ld
        );
        assert_eq!(ir.pous[0].body.value["rungs"], json!([]));
    }
}
