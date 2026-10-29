#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
use crate::bounded_process::run_output;

#[cfg(target_os = "macos")]
const CLIPBOARD_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(target_os = "macos")]
const MAX_CLIPBOARD_BYTES: usize = 8 * 1024 * 1024;

#[cfg(target_os = "macos")]
pub fn write_text(text: &str) -> Result<(), String> {
    let output = run_output(
        &mut Command::new("/usr/bin/pbcopy"),
        Some(text.as_bytes()),
        CLIPBOARD_TIMEOUT,
        64 * 1024,
        "pbcopy",
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "pbcopy exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[cfg(target_os = "macos")]
pub fn read_text() -> Result<String, String> {
    let output = run_output(
        &mut Command::new("/usr/bin/pbpaste"),
        None,
        CLIPBOARD_TIMEOUT,
        MAX_CLIPBOARD_BYTES,
        "pbpaste",
    )?;
    if !output.status.success() {
        return Err(format!(
            "pbpaste exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("clipboard text is not UTF-8: {error}"))
}

#[cfg(not(target_os = "macos"))]
pub fn write_text(_text: &str) -> Result<(), String> {
    Err("Native graphical-editor clipboard is only available on macOS.".to_owned())
}

#[cfg(not(target_os = "macos"))]
pub fn read_text() -> Result<String, String> {
    Err("Native graphical-editor clipboard is only available on macOS.".to_owned())
}
