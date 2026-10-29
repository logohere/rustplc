use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::BuildError;

static CANDIDATE_ID: AtomicU64 = AtomicU64::new(0);

pub(super) struct Candidate(PathBuf);

impl Candidate {
    pub(super) fn new(output: &Path) -> Result<Self, BuildError> {
        let name = output
            .file_name()
            .ok_or(BuildError::InvalidInput("output has no file name"))?;
        let id = CANDIDATE_ID.fetch_add(1, Ordering::Relaxed);
        let path = output.with_file_name(format!(
            ".{}.{}.{}.candidate",
            name.to_string_lossy(),
            std::process::id(),
            id
        ));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| BuildError::Io("reserve candidate", error))?;
        Ok(Self(path))
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }

    pub(super) fn promote(mut self, output: &Path) -> Result<(), BuildError> {
        fs::rename(&self.0, output).map_err(|error| BuildError::Io("promote candidate", error))?;
        self.0.clear();
        Ok(())
    }
}

impl Drop for Candidate {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.0);
        }
    }
}
