use std::backtrace::Backtrace;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app_paths;

static INSTALL: Once = Once::new();
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

pub fn install() {
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let record = panic_record(info);
            let _ = append_record(&record);
            previous(info);
        }));
    });
}

fn panic_record(info: &PanicHookInfo<'_>) -> String {
    let message = if let Some(message) = info.payload().downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = info.payload().downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    };
    let location = info
        .location()
        .map(|location| {
            format!(
                "{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        })
        .unwrap_or_else(|| "unknown".to_owned());
    let epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!(
        "\n=== RustPLC panic @ {epoch_ms} ms since Unix epoch ===\nlocation: {location}\nmessage: {message}\nbacktrace:\n{}\n",
        Backtrace::force_capture()
    )
}

fn append_record(record: &str) -> Result<(), String> {
    let path = crash_log_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| "crash log path has no parent".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("create crash log directory {}: {error}", parent.display()))?;
    rotate_if_needed(&path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("open crash log {}: {error}", path.display()))?;
    file.write_all(record.as_bytes())
        .and_then(|_| file.flush())
        .map_err(|error| format!("write crash log {}: {error}", path.display()))
}

fn rotate_if_needed(path: &Path) -> Result<(), String> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(());
    };
    if metadata.len() < MAX_LOG_BYTES {
        return Ok(());
    }
    let previous = path.with_extension("previous.log");
    if previous.exists() {
        fs::remove_file(&previous)
            .map_err(|error| format!("remove old crash log {}: {error}", previous.display()))?;
    }
    fs::rename(path, &previous).map_err(|error| {
        format!(
            "rotate crash log {} to {}: {error}",
            path.display(),
            previous.display()
        )
    })
}

fn crash_log_path() -> Result<PathBuf, String> {
    Ok(app_paths::data_root()?.join("logs/rustplc-crash.log"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::rotate_if_needed;

    #[test]
    fn small_log_is_not_rotated() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("rustplc-crash.log");
        fs::write(&path, b"small").unwrap();
        rotate_if_needed(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"small");
        assert!(!path.with_extension("previous.log").exists());
    }
}
