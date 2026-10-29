//! Minimal native shared-library loader for the generated OpenPLC program.

use std::ffi::{c_void, CStr, CString};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    EmptyPath,
    NonUtf8Path,
    Unsupported,
    OpenFailed(String),
    SymbolMissing(String),
    InvalidProgramState(String),
}

pub struct DynamicLibrary {
    handle: *mut c_void,
}

// SAFETY: the handle is immutable after construction; platform loaders permit symbol lookup
// and close from threads other than the opener, and Drop is uniquely owned by this value.
unsafe impl Send for DynamicLibrary {}
// SAFETY: shared access only performs platform symbol lookup; no mutation of the handle occurs.
unsafe impl Sync for DynamicLibrary {}

impl DynamicLibrary {
    #[allow(clippy::needless_return)]
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let path_ref = path.as_ref();
        let path = path_ref.to_str().ok_or(LoadError::NonUtf8Path)?;
        if path.is_empty() {
            return Err(LoadError::EmptyPath);
        }
        if !path_ref.is_file() {
            return Err(LoadError::OpenFailed(
                "library path is not a file".to_owned(),
            ));
        }
        #[cfg(unix)]
        {
            let path = CString::new(path).map_err(|_| LoadError::NonUtf8Path)?;
            // SAFETY: `path` is a live NUL-terminated CString for the duration of the call.
            let handle = unsafe { dlopen(path.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
            return (!handle.is_null())
                .then_some(Self { handle })
                .ok_or_else(|| LoadError::OpenFailed("dlopen failed".to_owned()));
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let wide: Vec<u16> = path_ref.as_os_str().encode_wide().chain(Some(0)).collect();
            // SAFETY: `wide` is explicitly NUL-terminated and remains live for the call.
            let handle = unsafe { LoadLibraryW(wide.as_ptr()) };
            return (!handle.is_null())
                .then_some(Self {
                    handle: handle.cast(),
                })
                .ok_or_else(|| LoadError::OpenFailed("LoadLibraryW failed".to_owned()));
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = path;
            Err(LoadError::Unsupported)
        }
    }

    #[allow(clippy::needless_return)]
    pub(crate) unsafe fn symbol(&self, name: &CStr) -> Result<*mut c_void, LoadError> {
        #[cfg(unix)]
        {
            let pointer = dlsym(self.handle, name.as_ptr());
            return (!pointer.is_null())
                .then_some(pointer)
                .ok_or_else(|| LoadError::SymbolMissing(name.to_string_lossy().into_owned()));
        }
        #[cfg(windows)]
        {
            let pointer = GetProcAddress(self.handle.cast(), name.as_ptr().cast());
            return (!pointer.is_null())
                .then_some(pointer.cast())
                .ok_or_else(|| LoadError::SymbolMissing(name.to_string_lossy().into_owned()));
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = name;
            Err(LoadError::Unsupported)
        }
    }

    pub(crate) unsafe fn function<T: Copy>(&self, name: &CStr) -> Result<T, LoadError> {
        let pointer = self.symbol(name)?;
        Ok(std::mem::transmute_copy(&pointer))
    }
}

impl Drop for DynamicLibrary {
    fn drop(&mut self) {
        #[cfg(unix)]
        // SAFETY: this value uniquely owns a non-null handle returned by `dlopen`.
        unsafe {
            dlclose(self.handle);
        }
        #[cfg(windows)]
        // SAFETY: this value uniquely owns a non-null handle returned by `LoadLibraryW`.
        unsafe {
            FreeLibrary(self.handle.cast());
        }
    }
}

#[cfg(unix)]
const RTLD_NOW: i32 = 2;
#[cfg(unix)]
const RTLD_LOCAL: i32 = 0;

#[cfg(unix)]
#[cfg_attr(target_os = "macos", link(name = "System"))]
#[cfg_attr(not(target_os = "macos"), link(name = "dl"))]
unsafe extern "C" {
    fn dlopen(path: *const i8, flags: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const i8) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> i32;
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(path: *const u16) -> *mut c_void;
    fn GetProcAddress(handle: *mut c_void, name: *const u8) -> *mut c_void;
    fn FreeLibrary(handle: *mut c_void) -> i32;
}

#[cfg(test)]
mod tests {
    use super::{DynamicLibrary, LoadError};

    #[test]
    fn rejects_empty_path_without_touching_loader() {
        assert!(matches!(
            DynamicLibrary::open(""),
            Err(LoadError::EmptyPath)
        ));
    }
}
