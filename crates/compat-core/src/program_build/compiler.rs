use std::ffi::OsString;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::{BuildError, BuildSpec};

const MAX_STREAM_BYTES: usize = 512 * 1024;
const COMPILER_TIMEOUT: Duration = Duration::from_secs(300);

pub(super) fn run_compiler(
    spec: &BuildSpec,
    sources: &[PathBuf],
    candidate: &Path,
) -> Result<(bool, Option<i32>, String), BuildError> {
    let mut child = compiler_command(spec, sources, candidate)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| BuildError::Io("start compiler", error))?;
    let stdout = child
        .stdout
        .take()
        .ok_or(BuildError::InvalidInput("compiler stdout missing"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or(BuildError::InvalidInput("compiler stderr missing"))?;
    let stdout = thread::spawn(move || read_bounded(stdout));
    let stderr = thread::spawn(move || read_bounded(stderr));
    let status = wait_bounded(&mut child, COMPILER_TIMEOUT)?;
    let diagnostics = format!("{}{}", join_reader(stdout)?, join_reader(stderr)?);
    let Some(status) = status else {
        return Err(BuildError::CompilerTimedOut(diagnostics));
    };
    Ok((status.success(), status.code(), diagnostics))
}

fn wait_bounded(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<Option<std::process::ExitStatus>, BuildError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| BuildError::Io("poll compiler", error))?
        {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            child
                .wait()
                .map_err(|error| BuildError::Io("reap compiler", error))?;
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn compiler_command(spec: &BuildSpec, sources: &[PathBuf], candidate: &Path) -> Command {
    let mut command = Command::new(&spec.compiler);
    command.args(["-std=c++17", "-O1", "-fPIC", "-Wall", "-DSTRUCPP_THREADED"]);
    if cfg!(target_os = "macos") {
        // STruC++ intentionally emits IEC AND/OR as eager bitwise operators. Modern
        // Apple Clang diagnoses that valid parity behavior as a style warning.
        command.arg("-Wno-bitwise-instead-of-logical");
        let mut install_name = OsString::from("@rpath/");
        install_name.push(spec.output.file_name().unwrap_or_default());
        command.args(["-dynamiclib", "-Xlinker", "-install_name", "-Xlinker"]);
        command.arg(install_name);
    } else {
        command.args(["-shared", "-fno-gnu-unique", "-Wl,--build-id=none"]);
    }
    command.arg(format!("-I{}", spec.generated_dir.display()));
    command.arg(format!("-I{}", spec.runtime_include.display()));
    command
        .args(sources)
        .arg(&spec.runtime_shim)
        .arg(&spec.reset_shim);
    command.arg("-o").arg(candidate).arg("-pthread");
    if cfg!(target_os = "linux") {
        command.arg("-lrt");
    }
    command
}

fn read_bounded(mut reader: impl Read) -> io::Result<String> {
    let mut kept = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = MAX_STREAM_BYTES.saturating_sub(kept.len());
        kept.extend_from_slice(&buffer[..read.min(remaining)]);
        truncated |= read > remaining;
    }
    let mut text = String::from_utf8_lossy(&kept).into_owned();
    if truncated {
        text.push_str("\n[compiler diagnostics truncated]\n");
    }
    Ok(text)
}

fn join_reader(reader: thread::JoinHandle<io::Result<String>>) -> Result<String, BuildError> {
    reader
        .join()
        .map_err(|_| BuildError::InvalidInput("compiler output reader panicked"))?
        .map_err(|error| BuildError::Io("read compiler output", error))
}

#[cfg(all(test, unix))]
mod tests {
    use super::{compiler_command, wait_bounded};
    use crate::program_build::BuildSpec;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn kills_compiler_after_deadline() {
        let mut child = Command::new("sleep").arg("1").spawn().unwrap();
        let status = wait_bounded(&mut child, Duration::from_millis(1)).unwrap();
        assert!(status.is_none());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn apple_clang_suppresses_only_intentional_iec_bitwise_logic_warning() {
        let spec = BuildSpec {
            compiler: PathBuf::from("/usr/bin/clang++"),
            generated_dir: PathBuf::from("generated"),
            runtime_include: PathBuf::from("runtime/include"),
            runtime_shim: PathBuf::from("runtime.cpp"),
            reset_shim: PathBuf::from("reset.cpp"),
            output: PathBuf::from("libplc.dylib"),
        };
        let command = compiler_command(
            &spec,
            &[PathBuf::from("generated.cpp")],
            &PathBuf::from("candidate.dylib"),
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.contains(&"-Wall".to_owned()));
        assert!(args.contains(&"-Wno-bitwise-instead-of-logical".to_owned()));
        assert!(!args.iter().any(|arg| arg == "-w"));
    }
}
