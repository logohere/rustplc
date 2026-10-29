use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;

pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "destination has no parent"))?;
    fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "destination filename is not UTF-8",
            )
        })?;
    let temp = parent.join(format!(".{name}.rustplc-save-{}.tmp", Uuid::new_v4()));
    let mut guard = TempFileGuard::new(temp.clone());
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    if let Ok(metadata) = fs::metadata(path) {
        fs::set_permissions(&temp, metadata.permissions())?;
    }
    file.write_all(contents.as_ref())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temp, path)?;
    guard.commit();
    // Persist the directory entry rename as well as the file contents.
    File::open(parent)?.sync_all()?;
    Ok(())
}

struct TempFileGuard {
    path: PathBuf,
    committed: bool,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }

    fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::write;

    #[test]
    fn atomically_replaces_existing_file_and_leaves_no_temp_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("project.json");
        fs::write(&path, b"old").unwrap();
        write(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_destination_does_not_modify_existing_directory() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("directory");
        fs::create_dir(&path).unwrap();
        assert!(write(&path, b"nope").is_err());
        assert!(path.is_dir());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }
}
