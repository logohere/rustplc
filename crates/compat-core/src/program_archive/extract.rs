use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

use zip::ZipArchive;

use super::plan::PlannedFile;
use super::{ArchiveError, ArchiveViolation};

pub(super) fn extract(
    archive: &mut ZipArchive<File>,
    destination: &Path,
    files: &[PlannedFile],
) -> Result<(), ArchiveError> {
    for planned in files {
        extract_file(archive, destination, planned)?;
    }
    Ok(())
}

fn extract_file(
    archive: &mut ZipArchive<File>,
    destination: &Path,
    planned: &PlannedFile,
) -> Result<(), ArchiveError> {
    let path = destination.join(&planned.path);
    let parent = path.parent().unwrap_or(destination);
    fs::create_dir_all(parent).map_err(|error| ArchiveError::Io("create archive path", error))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| ArchiveError::Io("create staged file", error))?;
    let mut input = archive.by_index(planned.index)?;
    let copied = copy_bounded(&mut input, &mut output, planned.size)?;
    output
        .flush()
        .map_err(|error| ArchiveError::Io("flush staged file", error))?;
    if copied != planned.size {
        return Err(ArchiveError::Invalid(ArchiveViolation::SizeMismatch));
    }
    Ok(())
}

fn copy_bounded(
    input: &mut impl Read,
    output: &mut impl Write,
    expected: u64,
) -> Result<u64, ArchiveError> {
    let mut limited = input.take(expected.saturating_add(1));
    io::copy(&mut limited, output).map_err(|error| ArchiveError::Io("extract staged file", error))
}
