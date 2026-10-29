use std::ffi::{c_char, c_void, CString};
use std::path::Path;

use crate::loader::{DynamicLibrary, LoadError};

pub type Init = unsafe extern "C" fn(*mut c_void) -> i32;
pub type StartLoop = unsafe extern "C" fn() -> i32;
pub type StopLoop = unsafe extern "C" fn();
pub type CycleHook = unsafe extern "C" fn();
pub type ExecuteCommand = unsafe extern "C" fn(*const c_char, *mut c_char, usize) -> i32;
pub type GetStats = unsafe extern "C" fn(*mut c_char, usize) -> i32;

pub struct NativePluginSymbols {
    _library: DynamicLibrary,
    pub init: Init,
    pub start_loop: Option<StartLoop>,
    pub stop_loop: Option<StopLoop>,
    pub cycle_start: Option<CycleHook>,
    pub cycle_end: Option<CycleHook>,
    pub cleanup: Option<CycleHook>,
    pub execute_command: Option<ExecuteCommand>,
    pub get_stats: Option<GetStats>,
}

impl NativePluginSymbols {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let library = DynamicLibrary::open(path)?;
        // SAFETY: the plugin ABI requires `init` with this exact C signature; the library
        // remains owned by NativePluginSymbols for every copied function pointer.
        let init = unsafe { library.function(&symbol("init"))? };
        Ok(Self {
            start_loop: optional(&library, "start_loop"),
            stop_loop: optional(&library, "stop_loop"),
            cycle_start: optional(&library, "cycle_start"),
            cycle_end: optional(&library, "cycle_end"),
            cleanup: optional(&library, "cleanup"),
            execute_command: optional(&library, "execute_command"),
            get_stats: optional(&library, "get_stats"),
            _library: library,
            init,
        })
    }
}

fn symbol(name: &str) -> CString {
    CString::new(name).expect("static plugin symbol")
}

fn optional<T: Copy>(library: &DynamicLibrary, name: &str) -> Option<T> {
    // SAFETY: callers request only function-pointer types from the fixed plugin ABI, and
    // NativePluginSymbols retains the DynamicLibrary for every returned pointer.
    unsafe {
        library
            .symbol(&symbol(name))
            .ok()
            .map(|pointer| std::mem::transmute_copy(&pointer))
    }
}

#[cfg(test)]
mod tests {
    use super::NativePluginSymbols;

    #[test]
    fn missing_plugin_path_fails_closed() {
        assert!(NativePluginSymbols::load("missing-openplc-plugin.so").is_err());
    }
}
