//! Structured generated-program compilation from the pinned OpenPLC build boundary.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

mod candidate;
mod compiler;

use crate::loader::LoadError;
use crate::program_api::ProgramApi;
use candidate::Candidate;
use compiler::run_compiler;

const MAX_SOURCE_FILES: usize = 256;
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 50 * 1024 * 1024;

pub struct BuildSpec {
    pub compiler: PathBuf,
    pub generated_dir: PathBuf,
    pub runtime_include: PathBuf,
    pub runtime_shim: PathBuf,
    pub reset_shim: PathBuf,
    pub output: PathBuf,
}

pub struct BuildReport {
    pub output: PathBuf,
    pub source_count: usize,
    pub source_bytes: u64,
    pub diagnostics: String,
}

#[derive(Debug)]
pub enum BuildError {
    Unsupported,
    InvalidInput(&'static str),
    Io(&'static str, io::Error),
    CompilerFailed(Option<i32>, String),
    CompilerTimedOut(String),
    InvalidLibrary(LoadError),
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => write!(formatter, "program build is unsupported on this platform"),
            Self::InvalidInput(message) => formatter.write_str(message),
            Self::Io(operation, error) => write!(formatter, "{operation}: {error}"),
            Self::CompilerFailed(code, diagnostics) => {
                write!(formatter, "compiler failed with {code:?}: {diagnostics}")
            }
            Self::CompilerTimedOut(diagnostics) => {
                write!(formatter, "compiler exceeded 300 seconds: {diagnostics}")
            }
            Self::InvalidLibrary(error) => {
                write!(formatter, "candidate library is invalid: {error:?}")
            }
        }
    }
}

impl std::error::Error for BuildError {}

/// Compile generated STruC++ sources and atomically promote the candidate library.
///
/// The configured compiler receives structured arguments only. Existing output is
/// untouched unless compilation succeeds.
///
/// # Errors
/// Returns a bounded input, I/O, compiler, timeout, or generated-ABI error.
///
/// # Example
/// ```no_run
/// use open_rust_plc_compat_core::program_build::{build, BuildSpec};
/// use std::path::PathBuf;
/// let report = build(&BuildSpec {
///     compiler: PathBuf::from("c++"),
///     generated_dir: PathBuf::from("build/generated"),
///     runtime_include: PathBuf::from("vendor/strucpp/src/runtime/include"),
///     runtime_shim: PathBuf::from("vendor/openplc/runtime_v4_entry.cpp"),
///     reset_shim: PathBuf::from("crates/compat-core/native/strucpp_program_reset.cpp"),
///     output: PathBuf::from("build/libplc.dylib"),
/// })?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn build(spec: &BuildSpec) -> Result<BuildReport, BuildError> {
    if !cfg!(unix) {
        return Err(BuildError::Unsupported);
    }
    validate_support_files(spec)?;
    let (sources, source_bytes) = collect_sources(&spec.generated_dir)?;
    let candidate = Candidate::new(&spec.output)?;
    let (success, code, diagnostics) = run_compiler(spec, &sources, candidate.path())?;
    if !success {
        return Err(BuildError::CompilerFailed(code, diagnostics));
    }
    ProgramApi::load(candidate.path()).map_err(BuildError::InvalidLibrary)?;
    candidate.promote(&spec.output)?;
    Ok(BuildReport {
        output: spec.output.clone(),
        source_count: sources.len(),
        source_bytes,
        diagnostics,
    })
}

fn validate_support_files(spec: &BuildSpec) -> Result<(), BuildError> {
    regular_file(&spec.runtime_shim, "runtime shim is not a regular file")?;
    regular_file(&spec.reset_shim, "reset shim is not a regular file")?;
    regular_dir(
        &spec.runtime_include,
        "runtime include path is not a directory",
    )?;
    regular_file(
        &spec.runtime_include.join("iec_types.hpp"),
        "runtime include path has no iec_types.hpp",
    )?;
    let parent = spec.output.parent().unwrap_or_else(|| Path::new("."));
    regular_dir(parent, "output parent is not a directory")
}

fn collect_sources(directory: &Path) -> Result<(Vec<PathBuf>, u64), BuildError> {
    regular_dir(directory, "generated path is not a directory")?;
    let header = directory.join("generated.hpp");
    let mut total = regular_file(&header, "generated.hpp is not a regular file")?;
    let mut sources = Vec::new();
    let entries =
        fs::read_dir(directory).map_err(|error| BuildError::Io("read generated", error))?;
    for entry in entries {
        let path = entry
            .map_err(|error| BuildError::Io("read generated entry", error))?
            .path();
        if path.extension().and_then(|value| value.to_str()) != Some("cpp") {
            continue;
        }
        total = total
            .checked_add(regular_file(
                &path,
                "generated source is not a regular file",
            )?)
            .ok_or(BuildError::InvalidInput("generated source size overflow"))?;
        sources.push(path);
        if sources.len() > MAX_SOURCE_FILES || total > MAX_TOTAL_BYTES {
            return Err(BuildError::InvalidInput("generated source limits exceeded"));
        }
    }
    sources.sort();
    if sources.is_empty() {
        return Err(BuildError::InvalidInput("no generated .cpp sources"));
    }
    Ok((sources, total))
}

fn regular_file(path: &Path, message: &'static str) -> Result<u64, BuildError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| BuildError::Io("inspect input", error))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(BuildError::InvalidInput(message));
    }
    Ok(metadata.len())
}

fn regular_dir(path: &Path, message: &'static str) -> Result<(), BuildError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| BuildError::Io("inspect directory", error))?;
    if metadata.file_type().is_dir() {
        Ok(())
    } else {
        Err(BuildError::InvalidInput(message))
    }
}
