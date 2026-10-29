use std::env;
use std::io;
use std::path::PathBuf;

use open_rust_plc_compat_core::program_archive::stage_archive;

fn main() {
    if let Err(error) = run() {
        eprintln!("program_archive_trace: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let archive = args.next().map(PathBuf::from).ok_or_else(usage)?;
    let destination = args.next().map(PathBuf::from).ok_or_else(usage)?;
    if args.next().is_some() {
        return Err(usage().into());
    }
    let report = stage_archive(archive, destination)?;
    println!("{} {}", report.file_count, report.extracted_bytes);
    Ok(())
}

fn usage() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "usage: program_archive_trace ARCHIVE DESTINATION",
    )
}
