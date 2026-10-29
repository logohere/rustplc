use std::collections::BTreeMap;
use std::io::{Cursor, Write};

use open_rust_plc_compiler::md5::defines_h;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::generated_project::GeneratedProject;
use crate::project::OpenProject;
use crate::protocol_config::generate_runtime_confs;
use crate::vpp_package::{runtime_v4_plugin_files, VppPackageManager};

const RUNTIME_HEADERS: &[(&str, &str)] = &[
    (
        "iec_enum.hpp",
        include_str!("../../compiler-core/runtime/include/iec_enum.hpp"),
    ),
    (
        "iec_fault.hpp",
        include_str!("../../compiler-core/runtime/include/iec_fault.hpp"),
    ),
    (
        "iec_std_lib.hpp",
        include_str!("../../compiler-core/runtime/include/iec_std_lib.hpp"),
    ),
    (
        "iec_string.hpp",
        include_str!("../../compiler-core/runtime/include/iec_string.hpp"),
    ),
    (
        "iec_pointer.hpp",
        include_str!("../../compiler-core/runtime/include/iec_pointer.hpp"),
    ),
    (
        "iec_types.hpp",
        include_str!("../../compiler-core/runtime/include/iec_types.hpp"),
    ),
    (
        "iec_struct.hpp",
        include_str!("../../compiler-core/runtime/include/iec_struct.hpp"),
    ),
    (
        "iec_memory.hpp",
        include_str!("../../compiler-core/runtime/include/iec_memory.hpp"),
    ),
    (
        "iec_ptr.hpp",
        include_str!("../../compiler-core/runtime/include/iec_ptr.hpp"),
    ),
    (
        "debug_table.hpp",
        include_str!("../../compiler-core/runtime/include/debug_table.hpp"),
    ),
    (
        "iec_located.hpp",
        include_str!("../../compiler-core/runtime/include/iec_located.hpp"),
    ),
    (
        "debug_dispatch.hpp",
        include_str!("../../compiler-core/runtime/include/debug_dispatch.hpp"),
    ),
    (
        "iec_time.hpp",
        include_str!("../../compiler-core/runtime/include/iec_time.hpp"),
    ),
    (
        "iec_var.hpp",
        include_str!("../../compiler-core/runtime/include/iec_var.hpp"),
    ),
    (
        "iec_global.hpp",
        include_str!("../../compiler-core/runtime/include/iec_global.hpp"),
    ),
    (
        "iec_char.hpp",
        include_str!("../../compiler-core/runtime/include/iec_char.hpp"),
    ),
    (
        "iec_tod.hpp",
        include_str!("../../compiler-core/runtime/include/iec_tod.hpp"),
    ),
    (
        "iec_dt.hpp",
        include_str!("../../compiler-core/runtime/include/iec_dt.hpp"),
    ),
    (
        "iec_subrange.hpp",
        include_str!("../../compiler-core/runtime/include/iec_subrange.hpp"),
    ),
    (
        "iec_date.hpp",
        include_str!("../../compiler-core/runtime/include/iec_date.hpp"),
    ),
    (
        "iec_traits.hpp",
        include_str!("../../compiler-core/runtime/include/iec_traits.hpp"),
    ),
    (
        "iec_retain.hpp",
        include_str!("../../compiler-core/runtime/include/iec_retain.hpp"),
    ),
    (
        "iec_array.hpp",
        include_str!("../../compiler-core/runtime/include/iec_array.hpp"),
    ),
    (
        "iec_wstring.hpp",
        include_str!("../../compiler-core/runtime/include/iec_wstring.hpp"),
    ),
];

pub fn compose_runtime_v4_files(
    generated: &GeneratedProject,
    project: &OpenProject,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut files = BTreeMap::new();
    files.insert(
        "program.st".to_owned(),
        generated.program_st.as_bytes().to_vec(),
    );
    files.insert(
        "generated.hpp".to_owned(),
        generated.header_code.as_bytes().to_vec(),
    );
    files.insert(
        "defines.h".to_owned(),
        defines_h(&generated.md5).into_bytes(),
    );
    files.insert(
        "debug-map.json".to_owned(),
        serde_json::to_vec_pretty(&generated.debug_map)
            .map_err(|error| format!("serialize debug-map.json: {error}"))?,
    );
    files.insert("c_blocks.h".to_owned(), b"// Empty file\n".to_vec());
    let confs = generate_runtime_confs(project, &generated.debug_map)?;
    for (path, content) in [
        ("conf/modbus_slave.json", confs.modbus_slave),
        ("conf/modbus_master.json", confs.modbus_master),
        ("conf/s7comm.json", confs.s7comm),
        ("conf/opcua.json", confs.opcua),
    ] {
        if let Some(content) = content {
            files.insert(path.to_owned(), content.into_bytes());
        }
    }
    files.insert(
        "conf/ethercat.json".to_owned(),
        confs.ethercat.unwrap_or_default().into_bytes(),
    );

    for file in &generated.cpp_files {
        files.insert(file.name.clone(), file.content.as_bytes().to_vec());
    }
    if let Some(board_name) = project
        .device_configuration
        .get("deviceBoard")
        .and_then(serde_json::Value::as_str)
    {
        if let Ok(manager) = VppPackageManager::new() {
            if let Some(board) = manager.board(board_name) {
                let vendor_screen_data = project
                    .device_configuration
                    .get("vendorScreenData")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                let pins = if let Some(array) = project.device_pin_mapping.as_array() {
                    array.clone()
                } else {
                    project
                        .device_pin_mapping
                        .get(board_name)
                        .and_then(serde_json::Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                };
                for (path, content) in runtime_v4_plugin_files(&board, &vendor_screen_data, &pins)?
                {
                    files.insert(path, content);
                }
            }
        }
    }
    for (name, content) in RUNTIME_HEADERS {
        files.insert(
            format!("strucpp_runtime/include/{name}"),
            content.as_bytes().to_vec(),
        );
    }
    Ok(files)
}

pub fn build_runtime_v4_zip(
    generated: &GeneratedProject,
    project: &OpenProject,
) -> Result<Vec<u8>, String> {
    let files = compose_runtime_v4_files(generated, project)?;
    let cursor = Cursor::new(Vec::<u8>::new());
    let mut zip = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (path, content) in files {
        zip.start_file(path, options)
            .map_err(|error| format!("start runtime bundle zip entry: {error}"))?;
        zip.write_all(&content)
            .map_err(|error| format!("write runtime bundle zip entry: {error}"))?;
    }
    let cursor = zip
        .finish()
        .map_err(|error| format!("finish runtime bundle zip: {error}"))?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};
    use std::path::PathBuf;

    use serde_json::json;
    use zip::ZipArchive;

    use super::{build_runtime_v4_zip, compose_runtime_v4_files};
    use crate::generated_project::GeneratedProject;
    use crate::project::{OpenProject, ProjectDocument, ProjectKind, ProjectMeta};
    use open_rust_plc_compiler::backend::{CppFile, DebugArray, DebugLeaf, DebugMap};

    fn project_fixture() -> OpenProject {
        OpenProject {
            root: PathBuf::from("/tmp/runtime-bundle"),
            project: ProjectDocument {
                meta: ProjectMeta {
                    name: "RuntimeBundle".to_owned(),
                    kind: ProjectKind::PlcProject,
                    extra: Default::default(),
                },
                data: json!({
                    "configuration":{"resource":{"instances":[],"tasks":[],"globalVariables":[]}},
                    "servers":[],
                    "remoteDevices":[]
                }),
                extra: Default::default(),
            },
            pous: Vec::new(),
            warnings: Vec::new(),
            device_configuration: json!({}),
            device_pin_mapping: json!({}),
            library_manifest: None,
        }
    }

    fn fixture() -> GeneratedProject {
        GeneratedProject {
            program_st: "PROGRAM main\nEND_PROGRAM\n".to_owned(),
            md5: "0123456789abcdef0123456789abcdef".to_owned(),
            header_code: "// header\n".to_owned(),
            cpp_files: vec![
                CppFile {
                    name: "configuration.cpp".to_owned(),
                    content: "// config\n".to_owned(),
                },
                CppFile {
                    name: "generated_debug.cpp".to_owned(),
                    content: "// debug\n".to_owned(),
                },
            ],
            debug_map: DebugMap {
                version: 2,
                md5: "0123456789abcdef0123456789abcdef".to_owned(),
                type_tags: Default::default(),
                arrays: vec![DebugArray { index: 0, count: 1 }],
                leaves: vec![DebugLeaf {
                    array_idx: 0,
                    elem_idx: 0,
                    path: "INSTANCE0.X".to_owned(),
                    type_name: "INT".to_owned(),
                    size: 2,
                }],
            },
        }
    }

    #[test]
    fn bundle_file_map_matches_runtime_v4_required_layout() {
        let files = compose_runtime_v4_files(&fixture(), &project_fixture()).unwrap();
        for required in [
            "program.st",
            "generated.hpp",
            "configuration.cpp",
            "generated_debug.cpp",
            "defines.h",
            "debug-map.json",
            "c_blocks.h",
            "conf/ethercat.json",
            "strucpp_runtime/include/iec_std_lib.hpp",
            "strucpp_runtime/include/debug_dispatch.hpp",
        ] {
            assert!(files.contains_key(required), "missing {required}");
        }
        assert!(String::from_utf8_lossy(&files["defines.h"])
            .contains("0123456789abcdef0123456789abcdef"));
        assert!(files["conf/ethercat.json"].is_empty());
    }

    #[test]
    fn runtime_v4_zip_contains_the_same_root_layout() {
        let bytes = build_runtime_v4_zip(&fixture(), &project_fixture()).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let names = archive
            .file_names()
            .map(|name| name.unwrap().into_owned())
            .collect::<Vec<_>>();
        assert!(names.contains(&"generated.hpp".to_owned()));
        assert!(names.contains(&"configuration.cpp".to_owned()));
        assert!(names.contains(&"strucpp_runtime/include/iec_types.hpp".to_owned()));
        let mut defines = String::new();
        archive
            .by_name("defines.h")
            .unwrap()
            .read_to_string(&mut defines)
            .unwrap();
        assert!(defines.contains("PROGRAM_MD5"));
    }
}
