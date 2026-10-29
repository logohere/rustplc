use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use open_rust_plc_compat_core::lifecycle::PlcState;
use open_rust_plc_compat_core::program_api::ProgramApi;
use open_rust_plc_compat_core::program_build::{build, BuildSpec};
use open_rust_plc_compat_core::runtime_owner::{RuntimeOwner, StartResult, StopResult};
use open_rust_plc_compiler::backend::DebugMap;
use open_rust_plc_compiler::md5::defines_h;

use crate::debug_values::{
    decode_debug_value, encode_force_value, watched_debug_variables, DebugValue, DebugVariable,
};
use crate::generated_project::generate_project;
use crate::project::OpenProject;

pub const SIMULATOR_BOARD: &str = "OpenPLC Simulator";

#[derive(Debug, Clone)]
pub struct BuildResult {
    pub output: PathBuf,
    pub source_count: usize,
    pub source_bytes: u64,
    pub diagnostics: String,
}

#[derive(Clone)]
pub struct LocalPlcService {
    inner: Arc<LocalPlcInner>,
}

struct LocalPlcInner {
    owner: RuntimeOwner,
    built_library: Mutex<Option<PathBuf>>,
    debug_map: Mutex<Option<DebugMap>>,
    forced_debug_paths: Mutex<HashSet<String>>,
}

impl Default for LocalPlcService {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalPlcService {
    pub fn new() -> Self {
        let owner = RuntimeOwner::new();
        owner.set_switch_allows_run(true);
        Self {
            inner: Arc::new(LocalPlcInner {
                owner,
                built_library: Mutex::new(None),
                debug_map: Mutex::new(None),
                forced_debug_paths: Mutex::new(HashSet::new()),
            }),
        }
    }

    pub fn state(&self) -> Result<PlcState, String> {
        self.inner
            .owner
            .state()
            .map_err(|error| format!("runtime state error: {error:?}"))
    }

    pub fn debug_values(&self, project: &OpenProject) -> Result<Vec<DebugValue>, String> {
        let variables = self.debug_variables(project)?;
        variables
            .into_iter()
            .map(|variable| {
                self.verify_live_debug_size(&variable)?;
                let bytes = self
                    .inner
                    .owner
                    .debug()
                    .read_entry(variable.array, variable.element)
                    .map_err(|error| format!("read debug variable {}: {error:?}", variable.path))?;
                if bytes.len() != variable.size {
                    return Err(format!(
                        "debug value size mismatch for {}: manifest {}, runtime {}",
                        variable.path,
                        variable.size,
                        bytes.len()
                    ));
                }
                let display = decode_debug_value(&variable.type_name, &bytes)?;
                Ok(DebugValue { variable, display })
            })
            .collect()
    }

    pub fn force_debug_value(
        &self,
        project: &OpenProject,
        path: &str,
        input: &str,
    ) -> Result<(), String> {
        let variable = self.debug_variable(project, path)?;
        self.verify_live_debug_size(&variable)?;
        let bytes = encode_force_value(&variable.type_name, input)?;
        if bytes.len() != variable.size {
            return Err(format!(
                "debug force value for {} encoded to {} bytes; runtime requires {}",
                variable.path,
                bytes.len(),
                variable.size
            ));
        }
        self.inner
            .owner
            .debug()
            .queue_force(variable.array, variable.element, &bytes)
            .map_err(|error| format!("queue debug force for {}: {error:?}", variable.path))?;
        self.inner
            .forced_debug_paths
            .lock()
            .map_err(|_| "forced debug state lock poisoned".to_owned())?
            .insert(variable.path);
        Ok(())
    }

    pub fn unforce_debug_value(&self, project: &OpenProject, path: &str) -> Result<(), String> {
        let variable = self.debug_variable(project, path)?;
        self.verify_live_debug_size(&variable)?;
        self.inner
            .owner
            .debug()
            .queue_unforce(variable.array, variable.element)
            .map_err(|error| format!("queue debug unforce for {}: {error:?}", variable.path))?;
        self.inner
            .forced_debug_paths
            .lock()
            .map_err(|_| "forced debug state lock poisoned".to_owned())?
            .remove(&variable.path);
        Ok(())
    }

    fn debug_variables(&self, project: &OpenProject) -> Result<Vec<DebugVariable>, String> {
        let map = self
            .inner
            .debug_map
            .lock()
            .map_err(|_| "debug map lock poisoned".to_owned())?
            .clone()
            .ok_or_else(|| "Build the project before using the debugger.".to_owned())?;
        let forced = self
            .inner
            .forced_debug_paths
            .lock()
            .map_err(|_| "forced debug state lock poisoned".to_owned())?
            .clone();
        watched_debug_variables(project, &map, &forced)
    }

    fn debug_variable(&self, project: &OpenProject, path: &str) -> Result<DebugVariable, String> {
        self.debug_variables(project)?
            .into_iter()
            .find(|variable| variable.path.eq_ignore_ascii_case(path))
            .ok_or_else(|| format!("Unknown watched debug variable path: {path}"))
    }

    fn verify_live_debug_size(&self, variable: &DebugVariable) -> Result<(), String> {
        let live = self
            .inner
            .owner
            .debug()
            .entry_size(variable.array, variable.element)
            .map_err(|error| format!("resolve debug variable {}: {error:?}", variable.path))?;
        if live == variable.size {
            Ok(())
        } else {
            Err(format!(
                "debug map/runtime size mismatch for {}: manifest {}, runtime {}",
                variable.path, variable.size, live
            ))
        }
    }

    pub fn build_project(&self, project: &OpenProject) -> Result<BuildResult, String> {
        if self.state()? == PlcState::Running {
            return Err("The PLC must be stopped before continuing.".to_owned());
        }

        let file_name = project
            .root
            .join("build")
            .join(SIMULATOR_BOARD)
            .join("src")
            .join("program.st");
        let file_label = file_name.to_string_lossy();
        let generated = generate_project(project, &file_label)?;
        let debug_map = generated.debug_map.clone();

        let board_dir = project.root.join("build").join(SIMULATOR_BOARD);
        let source_dir = board_dir.join("src");
        let staging_dir = board_dir.join("src.staging");
        prepare_board_directory(&board_dir, &staging_dir)?;
        fs::write(staging_dir.join("program.st"), &generated.program_st)
            .map_err(|error| format!("write program.st: {error}"))?;
        fs::write(staging_dir.join("generated.hpp"), &generated.header_code)
            .map_err(|error| format!("write generated.hpp: {error}"))?;
        fs::write(staging_dir.join("defines.h"), defines_h(&generated.md5))
            .map_err(|error| format!("write defines.h: {error}"))?;
        fs::write(
            staging_dir.join("debug_map.json"),
            serde_json::to_string_pretty(&generated.debug_map)
                .map_err(|error| format!("serialize debug_map.json: {error}"))?,
        )
        .map_err(|error| format!("write debug_map.json: {error}"))?;
        for file in generated.cpp_files {
            fs::write(staging_dir.join(file.name), file.content)
                .map_err(|error| format!("write generated C++: {error}"))?;
        }
        promote_source_tree(&source_dir, &staging_dir)?;

        let support = runtime_support()?;
        let output = board_dir.join(if cfg!(target_os = "macos") {
            "libplc.dylib"
        } else {
            "libplc.so"
        });
        let report = build(&BuildSpec {
            compiler: cpp_compiler(),
            generated_dir: source_dir,
            runtime_include: support.include,
            runtime_shim: support.runtime_shim,
            reset_shim: support.reset_shim,
            output: output.clone(),
        })
        .map_err(|error| error.to_string())?;

        *self
            .inner
            .built_library
            .lock()
            .map_err(|_| "runtime build path lock poisoned".to_owned())? = Some(output.clone());
        *self
            .inner
            .debug_map
            .lock()
            .map_err(|_| "debug map lock poisoned".to_owned())? = Some(debug_map);
        self.inner
            .forced_debug_paths
            .lock()
            .map_err(|_| "forced debug state lock poisoned".to_owned())?
            .clear();

        Ok(BuildResult {
            output,
            source_count: report.source_count,
            source_bytes: report.source_bytes,
            diagnostics: report.diagnostics,
        })
    }

    pub fn start(&self) -> Result<StartResult, String> {
        let path = self
            .inner
            .built_library
            .lock()
            .map_err(|_| "runtime build path lock poisoned".to_owned())?
            .clone()
            .ok_or_else(|| "Build the project before starting the simulator.".to_owned())?;
        if self.state()? == PlcState::Error {
            let _ = self.inner.owner.stop();
        }
        let program = ProgramApi::load(&path)
            .map_err(|error| format!("load generated program: {error:?}"))?;
        // SAFETY: build_project validates this exact library through ProgramApi
        // before atomically promoting it to the path stored above.
        unsafe { self.inner.owner.start_program(Arc::new(program)) }
            .map_err(|error| format!("start PLC: {error:?}"))
    }

    pub fn stop(&self) -> Result<StopResult, String> {
        let result = self
            .inner
            .owner
            .stop()
            .map_err(|error| format!("stop PLC: {error:?}"))?;
        self.inner
            .forced_debug_paths
            .lock()
            .map_err(|_| "forced debug state lock poisoned".to_owned())?
            .clear();
        Ok(result)
    }
}

struct RuntimeSupport {
    include: PathBuf,
    runtime_shim: PathBuf,
    reset_shim: PathBuf,
}

fn runtime_support() -> Result<RuntimeSupport, String> {
    if let Some(resources) = bundle_resources() {
        let support = RuntimeSupport {
            include: resources.join("compiler-runtime/include"),
            runtime_shim: resources.join("compiler-runtime/runtime_v4_entry.cpp"),
            reset_shim: resources.join("compiler-runtime/strucpp_program_reset.cpp"),
        };
        if support.include.join("iec_types.hpp").is_file()
            && support.runtime_shim.is_file()
            && support.reset_shim.is_file()
        {
            return Ok(support);
        }
    }

    let editor_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = editor_manifest
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "cannot resolve workspace root".to_owned())?;
    let support = RuntimeSupport {
        include: workspace.join("crates/compiler-core/runtime/include"),
        runtime_shim: workspace.join("crates/compiler-core/runtime/runtime_v4_entry.cpp"),
        reset_shim: workspace.join("crates/compat-core/native/strucpp_program_reset.cpp"),
    };
    if !support.include.join("iec_types.hpp").is_file()
        || !support.runtime_shim.is_file()
        || !support.reset_shim.is_file()
    {
        return Err("OpenPLC compiler runtime resources are missing.".to_owned());
    }
    Ok(support)
}

fn bundle_resources() -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let resources = contents.join("Resources");
    resources.is_dir().then_some(resources)
}

fn cpp_compiler() -> PathBuf {
    if let Some(cxx) = env::var_os("CXX").filter(|value| !value.is_empty()) {
        return PathBuf::from(cxx);
    }
    let clang = PathBuf::from("/usr/bin/clang++");
    if clang.is_file() {
        clang
    } else {
        PathBuf::from("c++")
    }
}

fn prepare_board_directory(board_dir: &Path, staging_dir: &Path) -> Result<(), String> {
    if board_dir.exists() {
        let metadata = fs::symlink_metadata(board_dir)
            .map_err(|error| format!("inspect build directory: {error}"))?;
        if !metadata.file_type().is_dir() {
            return Err("build/OpenPLC Simulator is not a directory".to_owned());
        }
    } else {
        fs::create_dir_all(board_dir)
            .map_err(|error| format!("create build directory: {error}"))?;
    }
    if staging_dir.exists() {
        let metadata = fs::symlink_metadata(staging_dir)
            .map_err(|error| format!("inspect build staging directory: {error}"))?;
        if !metadata.file_type().is_dir() {
            return Err("build source staging path is not a directory".to_owned());
        }
        fs::remove_dir_all(staging_dir)
            .map_err(|error| format!("remove stale build staging directory: {error}"))?;
    }
    fs::create_dir_all(staging_dir)
        .map_err(|error| format!("create build source staging directory: {error}"))
}

fn promote_source_tree(source_dir: &Path, staging_dir: &Path) -> Result<(), String> {
    if source_dir.exists() {
        let metadata = fs::symlink_metadata(source_dir)
            .map_err(|error| format!("inspect build source directory: {error}"))?;
        if !metadata.file_type().is_dir() {
            return Err("build source path is not a directory".to_owned());
        }
        fs::remove_dir_all(source_dir)
            .map_err(|error| format!("remove previous build source directory: {error}"))?;
    }
    fs::rename(staging_dir, source_dir)
        .map_err(|error| format!("promote generated source directory: {error}"))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use open_rust_plc_compat_core::lifecycle::PlcState;
    use open_rust_plc_compat_core::runtime_owner::{StartResult, StopResult};
    use serde_json::json;

    use super::{runtime_support, LocalPlcService, SIMULATOR_BOARD};
    use crate::plc_model::{
        ParsedPou, PlcVariable, PlcVariableType, PouBody, PouInterface, PouLanguage, PouType,
        VariableClass,
    };
    use crate::project::{
        OpenProject, PouFile, PouKind, ProjectDocument, ProjectKind, ProjectMeta,
    };

    #[test]
    fn simulator_build_target_matches_openplc_editor() {
        assert_eq!(SIMULATOR_BOARD, "OpenPLC Simulator");
    }

    #[test]
    fn development_runtime_support_is_complete() {
        let support = runtime_support().unwrap();
        assert!(support.include.join("iec_types.hpp").is_file());
        assert!(support.runtime_shim.is_file());
        assert!(support.reset_shim.is_file());
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "invokes the host C++ compiler and runs a real generated PLC"]
    fn builds_starts_and_stops_through_the_app_runtime_service() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("RuntimeProject");
        std::fs::create_dir_all(&root).unwrap();
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
        let project = OpenProject {
            root: root.clone(),
            project: ProjectDocument {
                meta: ProjectMeta {
                    name: "RuntimeProject".to_owned(),
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

        let service = LocalPlcService::new();
        let build = service.build_project(&project).unwrap();
        assert!(build.output.is_file());
        let debug_map: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                root.join("build")
                    .join(SIMULATOR_BOARD)
                    .join("src")
                    .join("debug_map.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(debug_map["version"], 2);
        assert!(debug_map["md5"].as_str().is_some_and(|md5| !md5.is_empty()));
        assert_eq!(debug_map["leaves"][0]["path"], "INSTANCE0.X");
        assert_eq!(debug_map["leaves"][0]["type"], "INT");
        assert_eq!(debug_map["leaves"][0]["size"], 2);
        assert_eq!(debug_map["leaves"][0]["arrayIdx"], 0);
        assert_eq!(debug_map["leaves"][0]["elemIdx"], 0);
        assert_eq!(service.start().unwrap(), StartResult::Started);
        assert_eq!(service.state().unwrap(), PlcState::Running);
        std::thread::sleep(std::time::Duration::from_millis(30));
        let initial = service.debug_values(&project).unwrap();
        assert_eq!(initial.len(), 1);
        assert_eq!(initial[0].variable.path, "INSTANCE0.X");
        assert_eq!(initial[0].variable.type_name, "INT");
        assert!(!initial[0].variable.forced);
        assert!(initial[0].display.parse::<i16>().unwrap() > 0);

        service
            .force_debug_value(&project, "instance0.x", "123")
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        let forced = service.debug_values(&project).unwrap();
        assert_eq!(forced[0].display, "123");
        assert!(forced[0].variable.forced);

        service
            .unforce_debug_value(&project, "INSTANCE0.X")
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        let released = loop {
            let released = service.debug_values(&project).unwrap();
            if !released[0].variable.forced && released[0].display != "123" {
                break released;
            }
            assert!(
                Instant::now() < deadline,
                "released variable did not resume scan updates"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(!released[0].variable.forced);
        assert_ne!(released[0].display, "123");

        assert_eq!(service.stop().unwrap(), StopResult::Stopped);
        assert_eq!(service.state().unwrap(), PlcState::Stopped);
    }
}
