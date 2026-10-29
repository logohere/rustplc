use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use serde_json::Value;

use crate::arduino_defines::generate_arduino_defines;
use crate::arduino_vpp_config::generate_vpp_config;
use crate::bounded_process::run_output;
use crate::generated_project::generate_project;
use crate::project::OpenProject;
use crate::vpp_package::{apply_platform_options, InstalledVppBoard, VppPackageManager};

const GLOBAL_LIBRARIES: &[&str] = &[
    "Arduino_EdgeControl",
    "ArduinoJson",
    "Arduino_MachineControl",
    "ArduinoMqttClient",
    "AVR_PWM",
    "CAN",
    "CONTROLLINO",
    "DallasTemperature",
    "Ethernet",
    "megaAVR_PWM",
    "OneWire",
    "P1AM",
    "Portenta_H7_PWM",
    "PubSubClient",
    "RP2040_PWM",
    "SAMD_PWM",
    "SAMDUE_PWM",
    "STM32_CAN",
    "STM32_PWM",
    "WiFiNINA",
];

const ARDUINO_COMMAND_TIMEOUT: Duration = Duration::from_secs(600);
const ARDUINO_PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_ARDUINO_OUTPUT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ArduinoBuildResult {
    pub board: String,
    pub fqbn: String,
    pub build_dir: PathBuf,
    pub diagnostics: String,
}

pub fn build_project(
    project: &OpenProject,
    board_name: &str,
    upload: bool,
) -> Result<ArduinoBuildResult, String> {
    let manager = VppPackageManager::new()?;
    if manager
        .verify_installed_signatures()
        .iter()
        .any(|removed| removed == board_name)
    {
        return Err(format!(
            "VPP package for board {board_name:?} no longer verifies."
        ));
    }
    let board = manager
        .board(board_name)
        .ok_or_else(|| format!("Installed VPP board {board_name:?} was not found."))?;
    if board.device.target.target_type != "arduino-cli" {
        return Err(format!(
            "Board {board_name:?} is target type {:?}, not arduino-cli.",
            board.device.target.target_type
        ));
    }
    let platform = board
        .device
        .target
        .platform
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("Board {board_name:?} does not declare a platform/FQBN."))?;
    let selected = project
        .device_configuration
        .get("selectedPlatformOptions")
        .and_then(Value::as_object)
        .map(|values| {
            values
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_owned()))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let fqbn = apply_platform_options(platform, &board.device.target.platform_options, &selected);

    build_with_board(project, board_name, &board, &fqbn, upload)
}

fn build_with_board(
    project: &OpenProject,
    board_name: &str,
    board: &InstalledVppBoard,
    fqbn: &str,
    upload: bool,
) -> Result<ArduinoBuildResult, String> {
    let cli = ArduinoCli::discover()?;
    cli.ensure_config()?;
    cli.ensure_core(board)?;
    let mut diagnostics = String::new();
    cli.ensure_libraries(board, &mut diagnostics)?;

    let generated = generate_project(project, "arduino-upload.st")?;
    let pins = pins_for_board(project, board_name);
    let build_dir = project.root.join("build").join(board_name);
    prepare_firmware_tree(&build_dir, board, &generated, &pins, project)?;

    let properties = cli.toolchain_properties(fqbn, &build_dir)?;
    let precompiled =
        precompile_user_library(&build_dir, fqbn, &properties, board, &generated.md5)?;
    let compile_output = cli.compile(&build_dir, fqbn, board, &precompiled)?;
    diagnostics.push_str(&format_output(&compile_output));

    if upload {
        let port = project
            .device_configuration
            .get("communicationPort")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "No communication port specified for Arduino upload.".to_owned())?;
        let upload_output = cli.upload(&build_dir, fqbn, port)?;
        diagnostics.push_str(&format_output(&upload_output));
    }

    Ok(ArduinoBuildResult {
        board: board_name.to_owned(),
        fqbn: fqbn.to_owned(),
        build_dir,
        diagnostics,
    })
}

#[derive(Debug, Clone)]
struct ArduinoCli {
    binary: PathBuf,
    config: PathBuf,
}

impl ArduinoCli {
    fn discover() -> Result<Self, String> {
        let binary = discover_arduino_cli()?;
        let config = crate::app_paths::data_root()?
            .join("User")
            .join("arduino-cli.yaml");
        Ok(Self { binary, config })
    }

    fn ensure_config(&self) -> Result<(), String> {
        if self.config.is_file() {
            return Ok(());
        }
        let parent = self
            .config
            .parent()
            .ok_or_else(|| "Arduino CLI config path has no parent".to_owned())?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
        run_command(
            &self.binary,
            [
                OsString::from("config"),
                OsString::from("init"),
                OsString::from("--dest-file"),
                self.config.as_os_str().to_owned(),
            ],
            "initialize Arduino CLI configuration",
        )?;
        Ok(())
    }

    fn base_args(&self) -> [OsString; 2] {
        [
            OsString::from("--config-file"),
            self.config.as_os_str().to_owned(),
        ]
    }

    fn ensure_core(&self, board: &InstalledVppBoard) -> Result<(), String> {
        let Some(core) = board
            .device
            .target
            .core
            .as_deref()
            .filter(|value| !value.is_empty())
        else {
            return Ok(());
        };
        let installed = self.installed_cores()?;
        let version = board.device.target.core_version.as_deref();
        let acceptable = installed.get(core).is_some_and(|installed_version| {
            version.is_none_or(|version| version == installed_version)
        });
        if acceptable {
            return Ok(());
        }
        let core_ref =
            version.map_or_else(|| core.to_owned(), |version| format!("{core}@{version}"));
        let mut args = vec![
            OsString::from("core"),
            OsString::from("install"),
            OsString::from(core_ref),
        ];
        args.extend(self.base_args());
        run_command(&self.binary, args, "install Arduino core")?;
        Ok(())
    }

    fn installed_cores(&self) -> Result<BTreeMap<String, String>, String> {
        let mut args = vec![
            OsString::from("core"),
            OsString::from("list"),
            OsString::from("--format"),
            OsString::from("json"),
        ];
        args.extend(self.base_args());
        let output = run_command(&self.binary, args, "list Arduino cores")?;
        let value: Value = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("parse Arduino core list: {error}"))?;
        let mut cores = BTreeMap::new();
        for platform in value
            .get("platforms")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(id) = platform.get("id").and_then(Value::as_str) else {
                continue;
            };
            let Some(version) = platform.get("installed_version").and_then(Value::as_str) else {
                continue;
            };
            if !version.is_empty() {
                cores.insert(id.to_owned(), version.to_owned());
            }
        }
        Ok(cores)
    }

    fn ensure_libraries(
        &self,
        board: &InstalledVppBoard,
        diagnostics: &mut String,
    ) -> Result<(), String> {
        let installed = self.installed_libraries()?;
        let mut required = GLOBAL_LIBRARIES
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<BTreeSet<_>>();
        required.extend(board.device.hal.extra_arduino_libraries.iter().cloned());
        let missing = required
            .into_iter()
            .filter(|library| !installed.contains(library))
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(());
        }
        let mut args = vec![OsString::from("lib"), OsString::from("install")];
        args.extend(missing.iter().cloned().map(OsString::from));
        args.extend(self.base_args());
        match run_command(&self.binary, args, "install Arduino libraries") {
            Ok(output) => diagnostics.push_str(&format_output(&output)),
            Err(error) => {
                diagnostics.push_str(&format!(
                    "Warning: Arduino library provisioning failed for {}. Continuing to compile; the compiler is authoritative.\n{error}\n",
                    missing.join(", ")
                ));
            }
        }
        Ok(())
    }

    fn installed_libraries(&self) -> Result<BTreeSet<String>, String> {
        let mut args = vec![
            OsString::from("lib"),
            OsString::from("list"),
            OsString::from("--format"),
            OsString::from("json"),
        ];
        args.extend(self.base_args());
        let output = run_command(&self.binary, args, "list Arduino libraries")?;
        let value: Value = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("parse Arduino library list: {error}"))?;
        Ok(value
            .get("installed_libraries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.get("library"))
            .filter_map(|library| library.get("name"))
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect())
    }

    fn toolchain_properties(
        &self,
        fqbn: &str,
        build_dir: &Path,
    ) -> Result<ToolchainProperties, String> {
        let dummy = build_dir.join("show_properties_dummy");
        fs::create_dir_all(&dummy)
            .map_err(|error| format!("create {}: {error}", dummy.display()))?;
        fs::write(dummy.join("show_properties_dummy.ino"), "\n")
            .map_err(|error| format!("write dummy Arduino sketch: {error}"))?;
        let mut args = vec![
            OsString::from("compile"),
            OsString::from("--fqbn"),
            OsString::from(fqbn),
            OsString::from("--show-properties=expanded"),
            dummy.as_os_str().to_owned(),
        ];
        args.extend(self.base_args());
        let output = run_command(&self.binary, args, "resolve Arduino toolchain recipes")?;
        ToolchainProperties::parse(fqbn, &String::from_utf8_lossy(&output.stdout))
    }

    fn compile(
        &self,
        build_dir: &Path,
        fqbn: &str,
        board: &InstalledVppBoard,
        precompiled: &PrecompiledLibrary,
    ) -> Result<Output, String> {
        let source_dir = build_dir.join("src");
        let baremetal = build_dir.join("examples").join("Baremetal");
        let mut args = vec![
            OsString::from("compile"),
            OsString::from("-v"),
            OsString::from("-j"),
            OsString::from("0"),
        ];
        if !board.device.hal.compiler_flags.c_flags.is_empty() {
            args.push(OsString::from("--build-property"));
            args.push(OsString::from(format!(
                "compiler.c.extra_flags={}",
                board.device.hal.compiler_flags.c_flags.join(" ")
            )));
        }
        let mut cxx_flags = board.device.hal.compiler_flags.cxx_flags.clone();
        if board.device.target.core.as_deref().is_some_and(|core| {
            core.starts_with("arduino:avr") || core.starts_with("arduino:megaavr")
        }) {
            cxx_flags.push(format!(
                "-I{}",
                source_assets()?.join("avr-libstdcpp/include").display()
            ));
        }
        if !cxx_flags.is_empty() {
            args.push(OsString::from("--build-property"));
            args.push(OsString::from(format!(
                "compiler.cpp.extra_flags={}",
                cxx_flags.join(" ")
            )));
        }
        if !board.device.hal.compiler_flags.ld_flags.is_empty() {
            args.push(OsString::from("--build-property"));
            args.push(OsString::from(format!(
                "compiler.c.elf.extra_flags={}",
                board.device.hal.compiler_flags.ld_flags.join(" ")
            )));
        }
        args.extend([
            OsString::from("--library"),
            source_dir.as_os_str().to_owned(),
            OsString::from("--library"),
            precompiled.library_dir.as_os_str().to_owned(),
            OsString::from("--build-property"),
            OsString::from(format!(
                "compiler.libraries.ldflags=-L{} -lOpenPLCUserLib",
                precompiled.arch_dir.display()
            )),
        ]);
        if let Some(relative) = board.device.hal.precompiled_library.as_deref() {
            let path = package_relative(&board.package_path, relative)?;
            args.extend([OsString::from("--library"), path.as_os_str().to_owned()]);
        }
        if let Some(relative) = board.device.hal.libraries.as_deref() {
            let path = package_relative(&board.package_path, relative)?;
            args.extend([OsString::from("--library"), path.as_os_str().to_owned()]);
        }
        args.extend([
            OsString::from("--export-binaries"),
            OsString::from("--fqbn"),
            OsString::from(fqbn),
            baremetal.join("Baremetal.ino").as_os_str().to_owned(),
        ]);
        args.extend(self.base_args());
        run_command(&self.binary, args, "compile Arduino firmware")
    }

    fn upload(&self, build_dir: &Path, fqbn: &str, port: &str) -> Result<Output, String> {
        let baremetal = build_dir.join("examples").join("Baremetal");
        let mut args = vec![
            OsString::from("upload"),
            OsString::from("--port"),
            OsString::from(port),
            OsString::from("--fqbn"),
            OsString::from(fqbn),
            baremetal.as_os_str().to_owned(),
        ];
        args.extend(self.base_args());
        run_command(&self.binary, args, "upload Arduino firmware")
    }
}

#[derive(Debug)]
struct ToolchainProperties {
    values: BTreeMap<String, String>,
    recipe_cpp: String,
}

impl ToolchainProperties {
    fn parse(fqbn: &str, output: &str) -> Result<Self, String> {
        let values = output
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect::<BTreeMap<_, _>>();
        let recipe_cpp = values
            .get("recipe.cpp.o.pattern")
            .cloned()
            .ok_or_else(|| format!("Arduino toolchain for {fqbn:?} has no recipe.cpp.o.pattern"))?;
        for required in ["compiler.path", "compiler.ar.cmd"] {
            if !values.contains_key(required) {
                return Err(format!(
                    "Arduino toolchain for {fqbn:?} is missing {required}."
                ));
            }
        }
        Ok(Self { values, recipe_cpp })
    }
}

#[derive(Debug)]
struct PrecompiledLibrary {
    library_dir: PathBuf,
    arch_dir: PathBuf,
}

fn precompile_user_library(
    build_dir: &Path,
    fqbn: &str,
    properties: &ToolchainProperties,
    board: &InstalledVppBoard,
    project_md5: &str,
) -> Result<PrecompiledLibrary, String> {
    let source_dir = build_dir.join("src");
    let baremetal = build_dir.join("examples").join("Baremetal");
    let stash = build_dir.join("precompile").join("sources");
    let objects = build_dir.join("precompile").join("obj");
    fs::create_dir_all(&stash).map_err(|error| format!("create {}: {error}", stash.display()))?;
    fs::create_dir_all(&objects)
        .map_err(|error| format!("create {}: {error}", objects.display()))?;

    let mut source_names = Vec::new();
    for entry in fs::read_dir(&source_dir)
        .map_err(|error| format!("read {}: {error}", source_dir.display()))?
    {
        let entry = entry.map_err(|error| format!("read source entry: {error}"))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.ends_with(".cpp") || name_str == "arduino.cpp" {
            continue;
        }
        let target = stash.join(&name);
        if target.exists() {
            fs::remove_file(&target)
                .map_err(|error| format!("remove {}: {error}", target.display()))?;
        }
        fs::rename(entry.path(), &target)
            .map_err(|error| format!("move {} to precompile stash: {error}", name_str))?;
        source_names.push(name);
    }
    for entry in
        fs::read_dir(&stash).map_err(|error| format!("read {}: {error}", stash.display()))?
    {
        let entry = entry.map_err(|error| format!("read precompile entry: {error}"))?;
        if entry.path().extension().and_then(|value| value.to_str()) == Some("cpp")
            && !source_names.contains(&entry.file_name())
        {
            source_names.push(entry.file_name());
        }
    }
    source_names.sort();
    if source_names.is_empty() {
        return Err("No generated C++ sources were available for Arduino precompile.".to_owned());
    }

    let core_path = properties
        .values
        .get("build.core.path")
        .ok_or_else(|| format!("Arduino toolchain for {fqbn:?} did not report build.core.path."))?;
    let variant_path = properties.values.get("build.variant.path");
    let mut extra_include = board
        .device
        .hal
        .compiler_flags
        .cxx_flags
        .iter()
        .filter(|flag| flag.starts_with("-I"))
        .cloned()
        .collect::<Vec<_>>();
    let mut extra_non_include = board
        .device
        .hal
        .compiler_flags
        .cxx_flags
        .iter()
        .filter(|flag| !flag.starts_with("-I"))
        .cloned()
        .collect::<Vec<_>>();
    if board
        .device
        .target
        .core
        .as_deref()
        .is_some_and(|core| core.starts_with("arduino:avr") || core.starts_with("arduino:megaavr"))
    {
        extra_include.insert(
            0,
            format!(
                "-I{}",
                source_assets()?.join("avr-libstdcpp/include").display()
            ),
        );
    }
    let mut includes = extra_include;
    includes.push(format!("-I{core_path}"));
    if let Some(variant) = variant_path {
        includes.push(format!("-I{variant}"));
    }
    includes.push(format!("-I{}", source_dir.display()));
    includes.push(format!("-I{}", baremetal.display()));
    extra_non_include.insert(0, "-fno-rtti".to_owned());
    extra_non_include.insert(0, "-std=gnu++17".to_owned());

    let recipe = tokenize_recipe(&properties.recipe_cpp)?;
    let mut object_paths = Vec::new();
    for name in source_names {
        let source = stash.join(&name);
        let object = objects.join(Path::new(&name).with_extension("o"));
        let mut argv = substitute_placeholders(&recipe, &source, &object, &includes)?;
        argv.extend(extra_non_include.iter().cloned().map(OsString::from));
        ensure_response_file_stubs(&argv)?;
        run_argv(argv, &format!("precompile {}", source.display()))?;
        object_paths.push(object);
    }

    let archive = build_dir.join("precompile").join("libOpenPLCUserLib.a");
    let compiler_path = properties.values["compiler.path"].clone();
    let ar_name = properties.values["compiler.ar.cmd"].clone();
    let executable = PathBuf::from(format!("{compiler_path}{ar_name}"));
    let mut argv = vec![executable.into_os_string()];
    argv.extend(
        properties
            .values
            .get("compiler.ar.flags")
            .map(String::as_str)
            .unwrap_or("rcs")
            .split_whitespace()
            .map(OsString::from),
    );
    argv.extend(
        properties
            .values
            .get("compiler.ar.extra_flags")
            .map(String::as_str)
            .unwrap_or_default()
            .split_whitespace()
            .map(OsString::from),
    );
    argv.push(archive.as_os_str().to_owned());
    argv.extend(object_paths.iter().map(|path| path.as_os_str().to_owned()));
    run_argv(argv, "archive precompiled OpenPLC Arduino library")?;

    let arch_candidates = ["build.mcu", "build.architecture", "build.arch"]
        .iter()
        .filter_map(|key| properties.values.get(*key))
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if arch_candidates.is_empty() {
        return Err(format!(
            "Arduino toolchain for {fqbn:?} exposes no build.mcu/build.architecture/build.arch; cannot place the precompiled archive."
        ));
    }

    let staging = env::temp_dir().join(format!(
        "openplc-precompile-{}-{}",
        &project_md5[..project_md5.len().min(12)],
        std::process::id()
    ));
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .map_err(|error| format!("remove {}: {error}", staging.display()))?;
    }
    let library_dir = staging.join("OpenPLCUserLib");
    let library_src = library_dir.join("src");
    let arch_dir = library_src.join(&arch_candidates[0]);
    for arch in &arch_candidates {
        let candidate = library_src.join(arch);
        fs::create_dir_all(&candidate)
            .map_err(|error| format!("create {}: {error}", candidate.display()))?;
        fs::copy(&archive, candidate.join("libOpenPLCUserLib.a"))
            .map_err(|error| format!("copy precompiled archive: {error}"))?;
    }
    fs::write(
        library_dir.join("library.properties"),
        "name=OpenPLCUserLib\nversion=1.0.0\nauthor=RustPLC Editor\nmaintainer=RustPLC Editor\nsentence=Pre-compiled OpenPLC user code archive\nparagraph=Pre-compiled gnu++17 archive of generated PLC code.\ncategory=Other\narchitectures=*\nprecompiled=full\n",
    )
    .map_err(|error| format!("write precompiled library.properties: {error}"))?;
    fs::write(
        library_src.join("OpenPLCUserLib.h"),
        "// Auto-generated stub for OpenPLCUserLib.\n#pragma once\n",
    )
    .map_err(|error| format!("write OpenPLCUserLib.h: {error}"))?;

    Ok(PrecompiledLibrary {
        library_dir,
        arch_dir,
    })
}

fn prepare_firmware_tree(
    build_dir: &Path,
    board: &InstalledVppBoard,
    generated: &crate::generated_project::GeneratedProject,
    pins: &[Value],
    project: &OpenProject,
) -> Result<(), String> {
    if build_dir.exists() {
        fs::remove_dir_all(build_dir)
            .map_err(|error| format!("remove {}: {error}", build_dir.display()))?;
    }
    let source_dir = build_dir.join("src");
    let baremetal_dir = build_dir.join("examples").join("Baremetal");
    fs::create_dir_all(&source_dir)
        .map_err(|error| format!("create {}: {error}", source_dir.display()))?;
    fs::create_dir_all(&baremetal_dir)
        .map_err(|error| format!("create {}: {error}", baremetal_dir.display()))?;

    let assets = source_assets()?;
    copy_tree(&assets.join("arduino"), &source_dir)?;
    copy_tree(&assets.join("Baremetal"), &baremetal_dir)?;
    copy_tree(&runtime_headers()?, &source_dir)?;

    fs::write(source_dir.join("program.st"), &generated.program_st)
        .map_err(|error| format!("write program.st: {error}"))?;
    fs::write(source_dir.join("generated.hpp"), &generated.header_code)
        .map_err(|error| format!("write generated.hpp: {error}"))?;
    fs::write(
        source_dir.join("debug-map.json"),
        serde_json::to_vec_pretty(&generated.debug_map)
            .map_err(|error| format!("serialize debug-map.json: {error}"))?,
    )
    .map_err(|error| format!("write debug-map.json: {error}"))?;
    for file in &generated.cpp_files {
        fs::write(source_dir.join(&file.name), &file.content)
            .map_err(|error| format!("write {}: {error}", file.name))?;
    }
    fs::write(
        source_dir.join("defines.h"),
        generate_arduino_defines(&board.device, pins, &generated.program_st, &generated.md5),
    )
    .map_err(|error| format!("write defines.h: {error}"))?;
    fs::write(
        source_dir.join("vpp_config.h"),
        generate_vpp_config(project.device_configuration.get("vendorScreenData")),
    )
    .map_err(|error| format!("write vpp_config.h: {error}"))?;
    fs::write(source_dir.join("c_blocks.h"), "// Empty file\n")
        .map_err(|error| format!("write c_blocks.h: {error}"))?;

    let hal_source = board.device.hal.source.as_deref().ok_or_else(|| {
        format!(
            "Arduino VPP board {:?} has no hal.source",
            board.device.name
        )
    })?;
    let hal_source = package_relative(&board.package_path, hal_source)?;
    if !hal_source.is_file() {
        return Err(format!(
            "Board HAL source does not exist: {}",
            hal_source.display()
        ));
    }
    fs::copy(&hal_source, source_dir.join("arduino.cpp"))
        .map_err(|error| format!("copy board HAL {}: {error}", hal_source.display()))?;
    Ok(())
}

fn pins_for_board(project: &OpenProject, board: &str) -> Vec<Value> {
    project
        .device_pin_mapping
        .as_array()
        .cloned()
        .or_else(|| {
            project
                .device_pin_mapping
                .get(board)
                .and_then(Value::as_array)
                .cloned()
        })
        .unwrap_or_default()
}

fn source_assets() -> Result<PathBuf, String> {
    if let Some(resources) = bundle_resources() {
        let path = resources.join("sources");
        if path.join("Baremetal/Baremetal.ino").is_file() {
            return Ok(path);
        }
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/sources");
    path.join("Baremetal/Baremetal.ino")
        .is_file()
        .then_some(path)
        .ok_or_else(|| "OpenPLC Arduino firmware resources are missing.".to_owned())
}

fn runtime_headers() -> Result<PathBuf, String> {
    if let Some(resources) = bundle_resources() {
        let path = resources.join("compiler-runtime/include");
        if path.join("iec_types.hpp").is_file() {
            return Ok(path);
        }
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../compiler-core/runtime/include");
    path.join("iec_types.hpp")
        .is_file()
        .then_some(path)
        .ok_or_else(|| "OpenPLC compiler runtime headers are missing.".to_owned())
}

fn bundle_resources() -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let resources = contents.join("Resources");
    resources.is_dir().then_some(resources)
}

fn discover_arduino_cli() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("OPENPLC_ARDUINO_CLI").filter(|value| !value.is_empty()) {
        return verify_cli(PathBuf::from(path));
    }
    if let Some(resources) = bundle_resources() {
        let candidate = resources.join("bin/arduino-cli");
        if candidate.is_file() {
            return verify_cli(candidate);
        }
    }
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| "cannot resolve workspace root".to_owned())?;
    let development = workspace.join("target/openplc-editor-tools/arduino-cli");
    if development.is_file() {
        return verify_cli(development);
    }
    verify_cli(PathBuf::from("arduino-cli")).map_err(|_| {
        "Arduino CLI 1.4.1 is unavailable. Run scripts/fetch_arduino_cli_macos.sh or use the packaged app.".to_owned()
    })
}

fn verify_cli(candidate: PathBuf) -> Result<PathBuf, String> {
    let output = run_output(
        Command::new(&candidate).args(["version", "--json"]),
        None,
        ARDUINO_PROBE_TIMEOUT,
        MAX_ARDUINO_OUTPUT_BYTES,
        &format!("verify Arduino CLI {}", candidate.display()),
    )?;
    if !output.status.success() {
        return Err(format!(
            "{} is not a working Arduino CLI.",
            candidate.display()
        ));
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("parse Arduino CLI version: {error}"))?;
    let version = value
        .get("VersionString")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if version != "1.4.1" {
        return Err(format!(
            "Arduino CLI version {version:?} is incompatible; RustPLC Editor requires pinned 1.4.1."
        ));
    }
    Ok(candidate)
}

fn run_command(
    executable: &Path,
    args: impl IntoIterator<Item = OsString>,
    label: &str,
) -> Result<Output, String> {
    let output = run_output(
        Command::new(executable).args(args),
        None,
        ARDUINO_COMMAND_TIMEOUT,
        MAX_ARDUINO_OUTPUT_BYTES,
        label,
    )?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "{label} failed with {}\n{}{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn run_argv(argv: Vec<OsString>, label: &str) -> Result<Output, String> {
    let mut iter = argv.into_iter();
    let executable = iter.next().ok_or_else(|| format!("{label}: empty argv"))?;
    run_command(Path::new(&executable), iter, label)
}

fn format_output(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn tokenize_recipe(recipe: &str) -> Result<Vec<OsString>, String> {
    let chars = recipe.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index >= chars.len() {
            break;
        }
        let mut token = String::new();
        while index < chars.len() && !chars[index].is_whitespace() {
            match chars[index] {
                '\'' | '"' => {
                    let quote = chars[index];
                    index += 1;
                    while index < chars.len() && chars[index] != quote {
                        token.push(chars[index]);
                        index += 1;
                    }
                    if index >= chars.len() {
                        return Err("Arduino recipe contains an unterminated quote.".to_owned());
                    }
                    index += 1;
                }
                character => {
                    token.push(character);
                    index += 1;
                }
            }
        }
        tokens.push(OsString::from(token));
    }
    Ok(tokens)
}

fn substitute_placeholders(
    recipe: &[OsString],
    source: &Path,
    object: &Path,
    includes: &[String],
) -> Result<Vec<OsString>, String> {
    let mut output = Vec::new();
    for token in recipe {
        let value = token.to_string_lossy();
        if value == "{source_file}" {
            output.push(source.as_os_str().to_owned());
        } else if value == "{object_file}" {
            output.push(object.as_os_str().to_owned());
        } else if value == "{includes}" {
            output.extend(includes.iter().cloned().map(OsString::from));
        } else if value.contains("{includes}") {
            return Err(
                "Arduino recipe embeds {includes} inside a larger token; cannot expand safely."
                    .to_owned(),
            );
        } else {
            let replaced = value
                .replace("{source_file}", &source.to_string_lossy())
                .replace("{object_file}", &object.to_string_lossy());
            output.push(OsString::from(replaced));
        }
    }
    Ok(output)
}

fn ensure_response_file_stubs(argv: &[OsString]) -> Result<(), String> {
    for argument in argv {
        let value = argument.to_string_lossy();
        let Some(path) = value.strip_prefix('@') else {
            continue;
        };
        let candidate = Path::new(path);
        if !candidate.is_absolute() || candidate.exists() {
            continue;
        }
        if let Some(parent) = candidate.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("create response file parent {}: {error}", parent.display())
            })?;
        }
        fs::write(candidate, "").map_err(|error| {
            format!("write response file stub {}: {error}", candidate.display())
        })?;
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Err(format!(
            "Required directory is missing: {}",
            source.display()
        ));
    }
    fs::create_dir_all(destination)
        .map_err(|error| format!("create {}: {error}", destination.display()))?;
    for entry in
        fs::read_dir(source).map_err(|error| format!("read {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("read directory entry: {error}"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| format!("inspect {}: {error}", entry.path().display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Refusing symlink in build resources: {}",
                entry.path().display()
            ));
        }
        let target = destination.join(entry.file_name());
        if metadata.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if metadata.is_file() {
            fs::copy(entry.path(), &target)
                .map_err(|error| format!("copy {}: {error}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn package_relative(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative);
    if relative.is_absolute() {
        return Err(format!("VPP path must be package-relative: {relative:?}"));
    }
    let mut normalized = PathBuf::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => normalized.push(value),
            Component::CurDir => {}
            _ => return Err(format!("VPP path escapes package root: {relative:?}")),
        }
    }
    Ok(root.join(normalized))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use serde_json::json;

    use super::{build_with_board, source_assets, substitute_placeholders, tokenize_recipe};
    use crate::plc_model::{
        ParsedPou, PlcVariable, PlcVariableType, PouBody, PouInterface, PouLanguage, PouType,
        VariableClass,
    };
    use crate::project::{
        OpenProject, PouFile, PouKind, ProjectDocument, ProjectKind, ProjectMeta,
    };
    use crate::vpp_package::{InstalledVppBoard, VppDevice};

    #[test]
    fn recipe_tokenizer_preserves_quoted_usb_descriptor() {
        let tokens = tokenize_recipe(
            r#"/bin/g++ '-DUSB_PRODUCT=\"Arduino Leonardo\"' -c "{source_file}" -o "{object_file}" {includes}"#,
        )
        .unwrap();
        assert_eq!(tokens[0], OsString::from("/bin/g++"));
        assert_eq!(
            tokens[1],
            OsString::from(r#"-DUSB_PRODUCT=\"Arduino Leonardo\""#)
        );
        let expanded = substitute_placeholders(
            &tokens,
            Path::new("/tmp/source.cpp"),
            Path::new("/tmp/source.o"),
            &["-I/tmp/core".to_owned(), "-I/tmp/src".to_owned()],
        )
        .unwrap();
        assert!(expanded.contains(&OsString::from("/tmp/source.cpp")));
        assert!(expanded.contains(&OsString::from("-I/tmp/core")));
        assert!(expanded.contains(&OsString::from("-I/tmp/src")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "invokes pinned arduino-cli and the installed AVR toolchain"]
    fn rust_generation_precompiles_and_links_with_real_arduino_toolchain() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ArduinoParity");
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
        let board_name = "Rust Arduino Toolchain Fixture";
        let project = OpenProject {
            root: root.clone(),
            project: ProjectDocument {
                meta: ProjectMeta {
                    name: "ArduinoParity".to_owned(),
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
            device_configuration: json!({
                "deviceBoard": board_name,
                "communicationPort": "",
                "selectedPlatformOptions": {},
                "vendorScreenData": {}
            }),
            device_pin_mapping: json!({
                board_name: [
                    {"pin":"22","pinType":"digitalInput","address":"%IX0.0","alias":""},
                    {"pin":"23","pinType":"digitalOutput","address":"%QX0.0","alias":""}
                ]
            }),
            library_manifest: None,
        };
        let device: VppDevice = serde_json::from_value(json!({
            "id":"rust-avr-fixture",
            "name":board_name,
            "preview":"",
            "target":{"type":"arduino-cli","platform":"arduino:avr:mega","core":"arduino:avr"},
            "hal":{"type":"arduino","source":"simulator.cpp","compilerFlags":{"cxx_flags":["-std=gnu++17"]}},
            "defaults":{},
            "capabilities":{"pinMapping":true}
        }))
        .unwrap();
        let board = InstalledVppBoard {
            package_id: "test.rust.avr".to_owned(),
            package_path: source_assets().unwrap(),
            vendor: "Test".to_owned(),
            device,
        };

        let report =
            build_with_board(&project, board_name, &board, "arduino:avr:mega", false).unwrap();
        assert_eq!(report.fqbn, "arduino:avr:mega");
        let baremetal = report.build_dir.join("examples/Baremetal");
        let hex = find_file_with_extension(&baremetal, "hex")
            .expect("arduino-cli must export a .hex firmware image");
        assert!(hex.is_file());
        assert!(report
            .build_dir
            .join("precompile/libOpenPLCUserLib.a")
            .is_file());
        assert!(report.build_dir.join("src/arduino.cpp").is_file());
    }

    fn find_file_with_extension(root: &Path, extension: &str) -> Option<PathBuf> {
        let mut stack = vec![root.to_path_buf()];
        while let Some(current) = stack.pop() {
            for entry in std::fs::read_dir(&current).ok()? {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
                    return Some(path);
                }
            }
        }
        None
    }
}
