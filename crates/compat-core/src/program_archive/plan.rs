use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};

use zip::{CompressionMethod, ZipArchive};

use super::{ArchiveError, ArchiveViolation, MAX_ENTRIES, MAX_FILE_BYTES, MAX_TOTAL_BYTES};

const BLOCKED: &[&str] = &["exe", "dll", "sh", "bat", "js", "vbs", "scr"];

pub(super) struct PlannedFile {
    pub index: usize,
    pub path: PathBuf,
    pub size: u64,
}

struct Entry {
    index: usize,
    root: Option<String>,
    path: Vec<String>,
    size: u64,
    file: bool,
    junk: bool,
}

pub(super) fn plan(archive: &mut ZipArchive<File>) -> Result<Vec<PlannedFile>, ArchiveError> {
    if archive.len() > MAX_ENTRIES {
        return invalid(ArchiveViolation::TooManyEntries);
    }
    let mut total = 0_u64;
    let mut entries = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let file = archive.by_index_raw(index)?;
        total = total
            .checked_add(file.size())
            .ok_or(ArchiveError::Invalid(ArchiveViolation::TotalTooLarge))?;
        validate_metadata(&file, total)?;
        let root = raw_root(file.name());
        let (path, junk) = components(file.name(), file.is_file())?;
        entries.push(Entry {
            index,
            root,
            junk,
            path,
            size: file.size(),
            file: file.is_file(),
        });
    }
    finish(entries)
}

fn validate_metadata<R: std::io::Read>(
    file: &zip::read::ZipFile<'_, R>,
    total: u64,
) -> Result<(), ArchiveError> {
    if file.encrypted() {
        return invalid(ArchiveViolation::Encrypted);
    }
    if file.size() > MAX_FILE_BYTES {
        return invalid(ArchiveViolation::FileTooLarge);
    }
    if total > MAX_TOTAL_BYTES {
        return invalid(ArchiveViolation::TotalTooLarge);
    }
    let compressed = file.compressed_size();
    if (compressed == 0 && file.size() > 0)
        || (compressed > 0 && file.size() > compressed.saturating_mul(1000))
    {
        return invalid(ArchiveViolation::CompressionRatio);
    }
    if !matches!(
        file.compression(),
        CompressionMethod::Stored
            | CompressionMethod::Deflated
            | CompressionMethod::Bzip2
            | CompressionMethod::Lzma
    ) {
        return invalid(ArchiveViolation::UnsupportedCompression);
    }
    if file.is_symlink() || special_file(file) {
        return invalid(ArchiveViolation::NonRegularEntry);
    }
    Ok(())
}

fn components(name: &str, check_extension: bool) -> Result<(Vec<String>, bool), ArchiveError> {
    let normalized = name.replace('\\', "/");
    if normalized.starts_with('/') || normalized.contains(':') || normalized.contains('\0') {
        return invalid(ArchiveViolation::UnsafePath);
    }
    let raw_parts: Vec<&str> = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if raw_parts.contains(&"..") {
        return invalid(ArchiveViolation::UnsafePath);
    }
    let parts: Vec<String> = raw_parts
        .into_iter()
        .filter(|part| *part != ".")
        .map(str::to_owned)
        .collect();
    if parts.is_empty() {
        return invalid(ArchiveViolation::UnsafePath);
    }
    if check_extension && blocked(&parts) {
        return invalid(ArchiveViolation::BlockedExtension);
    }
    let junk = normalized.starts_with("__MACOSX/") || normalized.ends_with(".DS_Store");
    Ok((parts, junk))
}

fn blocked(parts: &[String]) -> bool {
    Path::new(parts.last().map(String::as_str).unwrap_or_default())
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| BLOCKED.contains(&extension.to_ascii_lowercase().as_str()))
}

fn special_file<R: std::io::Read>(file: &zip::read::ZipFile<'_, R>) -> bool {
    let kind = file.unix_mode().unwrap_or(0) & 0o170000;
    !file.is_dir() && kind != 0 && kind != 0o100000
}

fn raw_root(name: &str) -> Option<String> {
    if name.starts_with("__MACOSX/") || name.ends_with(".DS_Store") {
        return None;
    }
    name.split('/')
        .next()
        .filter(|root| !root.is_empty())
        .map(str::to_owned)
}

fn finish(entries: Vec<Entry>) -> Result<Vec<PlannedFile>, ArchiveError> {
    let roots: HashSet<&str> = entries
        .iter()
        .filter_map(|entry| entry.root.as_deref())
        .collect();
    let strip_root = roots.len() == 1;
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for mut entry in entries {
        if entry.junk || !entry.file {
            continue;
        }
        if strip_root && entry.path.len() > 1 {
            entry.path.remove(0);
        }
        let path: PathBuf = entry.path.iter().collect();
        if !seen.insert(path.clone()) {
            return invalid(ArchiveViolation::DuplicatePath);
        }
        files.push(PlannedFile {
            index: entry.index,
            path,
            size: entry.size,
        });
    }
    Ok(files)
}

fn invalid<T>(reason: ArchiveViolation) -> Result<T, ArchiveError> {
    Err(ArchiveError::Invalid(reason))
}
