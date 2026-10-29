use open_rust_plc_compiler::backend::{generate_cpp_with_libraries, CppFile, DebugMap};
use open_rust_plc_compiler::frontend::parse;
use open_rust_plc_compiler::md5::md5_hex;
use open_rust_plc_compiler::transpile_to_st;

use crate::compiler_adapter::project_to_compiler_ir;
use crate::library_manager::LibraryManager;
use crate::project::OpenProject;

#[derive(Debug, Clone)]
pub struct GeneratedProject {
    pub program_st: String,
    pub md5: String,
    pub header_code: String,
    pub cpp_files: Vec<CppFile>,
    pub debug_map: DebugMap,
}

pub fn generate_project(
    project: &OpenProject,
    source_label: &str,
) -> Result<GeneratedProject, String> {
    let manager = LibraryManager::new()?;
    generate_project_with_manager(project, source_label, &manager)
}

fn generate_project_with_manager(
    project: &OpenProject,
    source_label: &str,
    manager: &LibraryManager,
) -> Result<GeneratedProject, String> {
    let ir = project_to_compiler_ir(project)?;
    let transpiled = transpile_to_st(&ir);
    if !transpiled.errors.is_empty() {
        return Err(transpiled.errors.join("\n"));
    }
    let program_st = transpiled
        .program_st
        .ok_or_else(|| "Failed to generate Structured Text".to_owned())?;
    let ast = parse(&program_st, source_label)
        .map_err(|errors| join_errors(errors.into_iter().map(|error| error.to_string())))?;
    let libraries = manager.load_project_libraries(project)?;
    if !libraries.missing.is_empty() {
        let missing = libraries
            .missing
            .iter()
            .map(|library| match library.version.as_deref() {
                Some(version) => format!("{}@{version}", library.name),
                None => library.name.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Missing enabled libraries: {missing}. Install them or remove them from the project library list."
        ));
    }
    let mut generated = generate_cpp_with_libraries(&ast, &libraries.archives, &program_st)
        .map_err(|error| error.to_string())?;
    let md5 = md5_hex(program_st.as_bytes());
    generated.debug_map.md5 = md5.clone();
    Ok(GeneratedProject {
        program_st,
        md5,
        header_code: generated.header_code,
        cpp_files: generated.cpp_files,
        debug_map: generated.debug_map,
    })
}

fn join_errors(errors: impl IntoIterator<Item = String>) -> String {
    errors.into_iter().collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde_json::json;
    use tempfile::tempdir;

    use super::{generate_project, generate_project_with_manager};
    use crate::library_manager::LibraryManager;
    use crate::plc_model::{
        ParsedPou, PlcVariable, PlcVariableType, PouBody, PouInterface, PouLanguage, PouType,
        VariableClass,
    };
    use crate::project::{
        OpenProject, PouFile, PouKind, ProjectDocument, ProjectKind, ProjectMeta,
    };

    #[test]
    fn shared_generation_emits_program_md5_debug_map_and_split_cpp() {
        let main = ParsedPou {
            name: "main".to_owned(),
            pou_type: PouType::Program,
            interface: PouInterface {
                return_type: None,
                variables: vec![PlcVariable {
                    name: "x".to_owned(),
                    class: Some(VariableClass::Local),
                    variable_type: PlcVariableType::base("INT"),
                    location: String::new(),
                    initial_value: None,
                    documentation: String::new(),
                    debug: Some(true),
                }],
            },
            body: PouBody::text(PouLanguage::St, "x := x + 1;"),
            documentation: String::new(),
        };
        let root = PathBuf::from("/tmp/shared-generation");
        let project = OpenProject {
            root: root.clone(),
            project: ProjectDocument {
                meta: ProjectMeta {
                    name: "SharedGeneration".to_owned(),
                    kind: ProjectKind::PlcProject,
                    extra: serde_json::Map::new(),
                },
                data: json!({
                    "dataTypes": [],
                    "libraries": [],
                    "configuration": {"resource": {
                        "tasks": [{"name":"task0","triggering":"Cyclic","interval":"T#20ms","priority":1}],
                        "instances": [{"name":"instance0","program":"main","task":"task0"}],
                        "globalVariables": []
                    }}
                }),
                extra: serde_json::Map::new(),
            },
            pous: vec![PouFile {
                name: "main".to_owned(),
                kind: PouKind::Program,
                language: Some(PouLanguage::St),
                relative_path: PathBuf::from("pous/programs/main.st"),
                absolute_path: root.join("pous/programs/main.st"),
                parsed: Some(main),
                variables_text: None,
                raw_content: String::new(),
            }],
            warnings: Vec::new(),
            device_configuration: json!({}),
            device_pin_mapping: json!({}),
            library_manifest: None,
        };

        let generated = generate_project(&project, "shared-generation.st").unwrap();
        assert!(!generated.program_st.is_empty());
        assert_eq!(generated.md5.len(), 32);
        assert_eq!(generated.debug_map.md5, generated.md5);
        assert!(generated.header_code.contains("class Program_MAIN"));
        assert!(generated
            .cpp_files
            .iter()
            .any(|file| file.name == "configuration.cpp"));
        assert!(generated
            .cpp_files
            .iter()
            .any(|file| file.name == "generated_debug.cpp"));
    }

    #[test]
    fn project_generation_injects_enabled_closed_source_stlib() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("demo.stlib");
        fs::write(
            &source,
            r#"{
              "formatVersion":1,
              "manifest":{
                "name":"demo-lib","version":"1.0.0","namespace":"demo",
                "functions":[{"name":"ADDONE","returnType":"INT","parameters":[{"name":"A","type":"INT","direction":"input"}]}],
                "functionBlocks":[],"types":[],"globals":[],"headers":[],"isBuiltin":false
              },
              "chunks":[{"name":"ADDONE","kind":"function","header":"IEC_INT ADDONE(IEC_INT A);\n","cpp":"IEC_INT ADDONE(IEC_INT A) { return A + 1; }\n","deps":[]}],
              "dependencies":[]
            }"#,
        )
        .unwrap();
        let manager = LibraryManager::with_libraries_dir(temp.path().join("pool"));
        manager.install_stlib(&source).unwrap();

        let main = ParsedPou {
            name: "main".to_owned(),
            pou_type: PouType::Program,
            interface: PouInterface {
                return_type: None,
                variables: vec![PlcVariable {
                    name: "r".to_owned(),
                    class: Some(VariableClass::Local),
                    variable_type: PlcVariableType::base("INT"),
                    location: String::new(),
                    initial_value: None,
                    documentation: String::new(),
                    debug: Some(false),
                }],
            },
            body: PouBody::text(PouLanguage::St, "r := ADDONE(r);"),
            documentation: String::new(),
        };
        let root = temp.path().join("project");
        let project = OpenProject {
            root: root.clone(),
            project: ProjectDocument {
                meta: ProjectMeta {
                    name: "LibraryConsumer".to_owned(),
                    kind: ProjectKind::PlcProject,
                    extra: serde_json::Map::new(),
                },
                data: json!({
                    "dataTypes": [],
                    "libraries": [{"name":"demo-lib","version":"1.0.0"}],
                    "configuration": {"resource": {
                        "tasks": [{"name":"task0","triggering":"Cyclic","interval":"T#20ms","priority":1}],
                        "instances": [{"name":"instance0","program":"main","task":"task0"}],
                        "globalVariables": []
                    }}
                }),
                extra: serde_json::Map::new(),
            },
            pous: vec![PouFile {
                name: "main".to_owned(),
                kind: PouKind::Program,
                language: Some(PouLanguage::St),
                relative_path: PathBuf::from("pous/programs/main.st"),
                absolute_path: root.join("pous/programs/main.st"),
                parsed: Some(main),
                variables_text: None,
                raw_content: String::new(),
            }],
            warnings: Vec::new(),
            device_configuration: json!({}),
            device_pin_mapping: json!({}),
            library_manifest: None,
        };

        let generated =
            generate_project_with_manager(&project, "library-consumer.st", &manager).unwrap();
        assert!(generated.header_code.contains("IEC_INT ADDONE(IEC_INT A);"));
        let program = generated
            .cpp_files
            .iter()
            .find(|file| file.name == "pou_MAIN.cpp")
            .unwrap();
        assert!(program.content.contains("R = ADDONE(R);"));
        assert!(generated
            .cpp_files
            .iter()
            .any(|file| file.name == "lib_demo_lib_addone.cpp"));
    }
}
