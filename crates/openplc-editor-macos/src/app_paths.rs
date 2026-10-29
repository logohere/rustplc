use std::fs;
use std::path::{Path, PathBuf};

use directories::BaseDirs;

const APP_DATA_DIR: &str = "rustplc";
const LEGACY_RUST_APP_DATA_DIR: &str = "open-plc-editor-r";

pub fn data_root() -> Result<PathBuf, String> {
    let base =
        BaseDirs::new().ok_or_else(|| "application data directory is unavailable".to_owned())?;
    let root = base.data_dir().join(APP_DATA_DIR);
    let legacy = base.data_dir().join(LEGACY_RUST_APP_DATA_DIR);
    migrate_legacy_tree(&legacy, &root)?;
    Ok(root)
}

fn migrate_legacy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() || !source.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("inspect legacy RustPLC data {}: {error}", source.display()))?;
    if !metadata.file_type().is_dir() {
        return Err(format!(
            "legacy RustPLC data path {} is not a directory",
            source.display()
        ));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "RustPLC data path has no parent".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    copy_tree(source, destination)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir(destination)
        .map_err(|error| format!("create {}: {error}", destination.display()))?;
    for entry in
        fs::read_dir(source).map_err(|error| format!("read {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("read legacy RustPLC entry: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)
            .map_err(|error| format!("inspect {}: {error}", source_path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "legacy RustPLC data contains unsupported symlink {}",
                source_path.display()
            ));
        }
        if metadata.file_type().is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else if metadata.file_type().is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!(
                    "copy legacy RustPLC data {} to {}: {error}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        } else {
            return Err(format!(
                "legacy RustPLC data contains unsupported entry {}",
                source_path.display()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::migrate_legacy_tree;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn migrates_prior_rustplc_tree_once_without_overwriting_destination() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("old");
        let destination = temp.path().join("new");
        fs::create_dir_all(source.join("libraries/demo")).unwrap();
        fs::write(source.join("libraries/demo/demo.stlib"), "old").unwrap();

        migrate_legacy_tree(&source, &destination).unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("libraries/demo/demo.stlib")).unwrap(),
            "old"
        );

        fs::write(destination.join("libraries/demo/demo.stlib"), "new").unwrap();
        migrate_legacy_tree(&source, &destination).unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("libraries/demo/demo.stlib")).unwrap(),
            "new"
        );
    }
}
