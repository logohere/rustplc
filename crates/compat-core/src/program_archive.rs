//! Bounded validation and staged extraction of OpenPLC Editor archives.

use std::fmt;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

mod extract;
mod plan;

use extract::extract;
use plan::plan;
use zip::result::ZipError;
use zip::ZipArchive;

pub const MAX_ARCHIVE_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveViolation {
    ArchiveTooLarge,
    TooManyEntries,
    UnsafePath,
    FileTooLarge,
    TotalTooLarge,
    CompressionRatio,
    BlockedExtension,
    Encrypted,
    UnsupportedCompression,
    NonRegularEntry,
    DuplicatePath,
    SizeMismatch,
}

#[derive(Debug)]
pub enum ArchiveError {
    Invalid(ArchiveViolation),
    DestinationExists,
    Io(&'static str, io::Error),
    Zip(ZipError),
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(reason) => write!(formatter, "invalid program archive: {reason:?}"),
            Self::DestinationExists => formatter.write_str("staging destination already exists"),
            Self::Io(operation, error) => write!(formatter, "{operation}: {error}"),
            Self::Zip(error) => write!(formatter, "invalid program ZIP: {error}"),
        }
    }
}

impl std::error::Error for ArchiveError {}

impl From<ZipError> for ArchiveError {
    fn from(error: ZipError) -> Self {
        Self::Zip(error)
    }
}

pub struct ArchiveReport {
    pub destination: PathBuf,
    pub file_count: usize,
    pub extracted_bytes: u64,
}

/// Validate an OpenPLC Editor ZIP and extract it into a new staging directory.
///
/// Existing destinations are never changed. Failed extraction removes the new
/// staging directory.
///
/// # Errors
/// Returns a typed archive, path, ZIP, or I/O error.
///
/// # Example
/// ```no_run
/// use open_rust_plc_compat_core::program_archive::stage_archive;
/// let report = stage_archive("program.zip", "build/upload-1")?;
/// assert!(report.file_count > 0);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn stage_archive(
    archive_path: impl AsRef<Path>,
    destination: impl AsRef<Path>,
) -> Result<ArchiveReport, ArchiveError> {
    let destination = destination.as_ref();
    if fs::symlink_metadata(destination).is_ok() {
        return Err(ArchiveError::DestinationExists);
    }
    validate_parent(destination)?;
    let file = open_archive(archive_path.as_ref())?;
    let mut archive = ZipArchive::new(file)?;
    let files = plan(&mut archive)?;
    fs::create_dir(destination).map_err(|error| ArchiveError::Io("create staging", error))?;
    let result = extract(&mut archive, destination, &files);
    if let Err(error) = result {
        let _ = fs::remove_dir_all(destination);
        return Err(error);
    }
    Ok(ArchiveReport {
        destination: destination.to_path_buf(),
        file_count: files.len(),
        extracted_bytes: files.iter().map(|file| file.size).sum(),
    })
}

fn open_archive(path: &Path) -> Result<File, ArchiveError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| ArchiveError::Io("inspect archive", error))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(ArchiveError::Invalid(ArchiveViolation::ArchiveTooLarge));
    }
    File::open(path).map_err(|error| ArchiveError::Io("open archive", error))
}

fn validate_parent(destination: &Path) -> Result<(), ArchiveError> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let metadata = fs::symlink_metadata(parent)
        .map_err(|error| ArchiveError::Io("inspect staging parent", error))?;
    if metadata.file_type().is_dir() {
        Ok(())
    } else {
        Err(ArchiveError::Io(
            "inspect staging parent",
            io::Error::new(io::ErrorKind::InvalidInput, "parent is not a directory"),
        ))
    }
}
