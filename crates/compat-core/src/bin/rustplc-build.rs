use std::env;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use open_rust_plc_compat_core::program_build::{build, BuildSpec};

const USAGE: &str = "usage: rustplc-build --generated DIR --runtime-include DIR --runtime-shim FILE --reset-shim FILE --output FILE [--compiler FILE]";

fn main() {
    if let Err(error) = run() {
        eprintln!("rustplc-build: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let spec = parse(env::args_os().skip(1))?;
    let report = build(&spec)?;
    println!(
        "built {} from {} sources ({} bytes)",
        report.output.display(),
        report.source_count,
        report.source_bytes
    );
    if !report.diagnostics.is_empty() {
        eprint!("{}", report.diagnostics);
    }
    Ok(())
}

fn parse(mut args: impl Iterator<Item = OsString>) -> io::Result<BuildSpec> {
    let mut compiler = None;
    let mut generated_dir = None;
    let mut runtime_include = None;
    let mut runtime_shim = None;
    let mut reset_shim = None;
    let mut output = None;
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--compiler") => put(&mut compiler, value(&mut args)?)?,
            Some("--generated") => put(&mut generated_dir, value(&mut args)?)?,
            Some("--runtime-include") => put(&mut runtime_include, value(&mut args)?)?,
            Some("--runtime-shim") => put(&mut runtime_shim, value(&mut args)?)?,
            Some("--reset-shim") => put(&mut reset_shim, value(&mut args)?)?,
            Some("--output") => put(&mut output, value(&mut args)?)?,
            _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE)),
        }
    }
    Ok(BuildSpec {
        compiler: compiler.unwrap_or_else(|| PathBuf::from("c++")),
        generated_dir: required(generated_dir)?,
        runtime_include: required(runtime_include)?,
        runtime_shim: required(runtime_shim)?,
        reset_shim: required(reset_shim)?,
        output: required(output)?,
    })
}

fn put(slot: &mut Option<PathBuf>, value: PathBuf) -> io::Result<()> {
    if slot.replace(value).is_some() {
        Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE))
    } else {
        Ok(())
    }
}

fn value(args: &mut impl Iterator<Item = OsString>) -> io::Result<PathBuf> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, USAGE))
}

fn required(value: Option<PathBuf>) -> io::Result<PathBuf> {
    value.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, USAGE))
}

#[cfg(test)]
mod tests {
    use super::parse;
    use std::ffi::OsString;

    #[test]
    fn requires_each_build_path_once() {
        let valid = [
            "--generated",
            "g",
            "--runtime-include",
            "i",
            "--runtime-shim",
            "r",
            "--reset-shim",
            "s",
            "--output",
            "o",
        ]
        .map(OsString::from);
        assert!(parse(valid.into_iter()).is_ok());
        assert!(parse([OsString::from("--generated")].into_iter()).is_err());
    }
}
