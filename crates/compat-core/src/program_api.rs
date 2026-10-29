//! C-linkage symbols exported by the pinned STruC++ program shim.

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;

use crate::loader::{DynamicLibrary, LoadError};

mod debug_target;
mod runtime;
mod state;
pub use debug_target::ProgramDebugTarget;
pub use runtime::ProgramScanError;
pub use state::ProgramStateError;

type ArrayCount = unsafe extern "C" fn() -> u8;
type ElemCount = unsafe extern "C" fn(u8) -> u16;
type ElemSize = unsafe extern "C" fn(u8, u16) -> u16;
type DebugRead = unsafe extern "C" fn(u8, u16, *mut u8) -> u16;
type DebugSet = unsafe extern "C" fn(u8, u16, bool, *const u8, u16) -> u8;
type DebugWrite = unsafe extern "C" fn(u8, u16, *const u8, u16) -> u8;
type DebugLocate = unsafe extern "C" fn(u8, u16, *mut u8, *mut u8, *mut u16, *mut u8) -> i32;
type AdvanceTime = unsafe extern "C" fn(u64);
type SetCurrentTime = unsafe extern "C" fn(i64);
type GetConfig = unsafe extern "C" fn() -> *mut c_void;
type GetLocatedVars = unsafe extern "C" fn() -> *const c_void;
type GetLocatedVarCount = unsafe extern "C" fn() -> u32;
type GetLocatedGlobals = unsafe extern "C" fn() -> *const *mut c_void;
type GetLocatedGlobalCount = unsafe extern "C" fn() -> u32;
type ResetConfig = unsafe extern "C" fn() -> u8;

/// Loaded generated-program surface. The library stays owned for all calls.
pub struct ProgramApi {
    _library: DynamicLibrary,
    array_count: ArrayCount,
    elem_count: ElemCount,
    elem_size: ElemSize,
    debug_read: DebugRead,
    debug_set: DebugSet,
    debug_write: DebugWrite,
    debug_locate: Option<DebugLocate>,
    advance_time: AdvanceTime,
    set_current_time: SetCurrentTime,
    get_config: GetConfig,
    get_located_vars: GetLocatedVars,
    get_located_var_count: GetLocatedVarCount,
    located_globals: Option<(GetLocatedGlobals, GetLocatedGlobalCount)>,
    reset_config: ResetConfig,
    md5: Option<String>,
    initial_state: state::ProgramState,
}

// SAFETY: ProgramApi owns the loaded library for at least as long as every stored symbol;
// runtime access is serialized by RuntimeOwner/scan boundaries and generated ABI state.
unsafe impl Send for ProgramApi {}
// SAFETY: immutable calls target thread-safe generated metadata/debug functions or are
// externally serialized by the runtime owner; the backing library remains loaded.
unsafe impl Sync for ProgramApi {}

impl ProgramApi {
    /// Load the pinned generated-program symbols.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let library = DynamicLibrary::open(path)?;
        let array_count = required_function::<ArrayCount>(&library, "strucpp_debug_array_count")?;
        let elem_count = required_function::<ElemCount>(&library, "strucpp_debug_elem_count")?;
        let elem_size = required_function::<ElemSize>(&library, "strucpp_debug_size")?;
        let debug_set = required_function::<DebugSet>(&library, "strucpp_debug_set")?;
        let debug_read = required_function::<DebugRead>(&library, "strucpp_debug_read")?;
        let debug_write = required_function::<DebugWrite>(&library, "strucpp_debug_write")?;
        let debug_locate = optional_function::<DebugLocate>(&library, "strucpp_debug_locate");
        let advance_time = required_function::<AdvanceTime>(&library, "strucpp_advance_time")?;
        let set_current_time =
            required_function::<SetCurrentTime>(&library, "strucpp_set_current_time")?;
        let get_config = required_function::<GetConfig>(&library, "strucpp_get_config")?;
        let get_located_vars =
            required_function::<GetLocatedVars>(&library, "strucpp_get_located_vars")?;
        let get_located_var_count =
            required_function::<GetLocatedVarCount>(&library, "strucpp_get_located_var_count")?;
        let reset_config = required_function::<ResetConfig>(&library, "strucpp_reset_config")?;
        let located_globals =
            optional_function::<GetLocatedGlobals>(&library, "strucpp_get_located_globals").zip(
                optional_function::<GetLocatedGlobalCount>(
                    &library,
                    "strucpp_get_located_global_count",
                ),
            );
        // SAFETY: this optional symbol is a NUL-terminated static string exported by the
        // pinned generated ABI; ProgramApi keeps the library loaded while the string is read.
        let md5 = unsafe { library.symbol(&name("strucpp_program_md5")) }
            .ok()
            .filter(|pointer| !pointer.is_null())
            .and_then(|pointer| {
                // SAFETY: non-null pointer above references the exported static C string.
                unsafe { CStr::from_ptr(pointer.cast::<c_char>()).to_str().ok() }
            })
            .map(str::to_owned);
        let mut api = Self {
            _library: library,
            array_count,
            elem_count,
            elem_size,
            debug_read,
            debug_set,
            debug_write,
            debug_locate,
            advance_time,
            set_current_time,
            get_config,
            get_located_vars,
            get_located_var_count,
            located_globals,
            reset_config,
            md5,
            initial_state: state::ProgramState::default(),
        };
        api.initial_state = state::ProgramState::capture(&api)
            .map_err(|error| LoadError::InvalidProgramState(format!("{error:?}")))?;
        Ok(api)
    }
}

impl ProgramApi {
    pub fn advance_time(&self, tick_ns: u64) {
        // SAFETY: resolved from the pinned ABI and the library remains owned by self.
        unsafe { (self.advance_time)(tick_ns) }
    }

    pub fn set_current_time(&self, time_ns: i64) {
        // SAFETY: resolved from the pinned ABI and the library remains owned by self.
        unsafe { (self.set_current_time)(time_ns) }
    }

    pub fn configuration_handle(&self) -> *mut c_void {
        // SAFETY: resolved from the pinned ABI; caller must validate/wrap the returned handle.
        unsafe { (self.get_config)() }
    }

    pub fn located_var_count(&self) -> u32 {
        // SAFETY: resolved from the pinned ABI and returns a scalar count only.
        unsafe { (self.get_located_var_count)() }
    }

    pub fn located_vars_handle(&self) -> *const c_void {
        // SAFETY: resolved from the pinned ABI; located-image construction validates the table.
        unsafe { (self.get_located_vars)() }
    }

    /// Exported configuration-global count, or `None` for older programs.
    pub fn located_global_count(&self) -> Option<u32> {
        // SAFETY: the function came from the live library with the pinned C ABI.
        self.located_globals.map(|(_, count)| unsafe { count() })
    }

    fn located_globals(&self) -> Option<(*const *mut c_void, u32)> {
        // SAFETY: both functions came from the live library with the pinned C ABI.
        self.located_globals
            .map(|(globals, count)| unsafe { (globals(), count()) })
    }
}

fn required_function<T: Copy>(library: &DynamicLibrary, symbol: &str) -> Result<T, LoadError> {
    // SAFETY: this private helper is called only with function-pointer aliases
    // matching the pinned generated STruC++ C ABI. ProgramApi owns `library`
    // for at least as long as every copied pointer.
    unsafe { library.function(&name(symbol)) }
}

fn optional_function<T: Copy>(library: &DynamicLibrary, symbol: &str) -> Option<T> {
    // SAFETY: same invariant as required_function; absence is permitted for
    // backwards-compatible optional ABI symbols.
    unsafe { library.function(&name(symbol)).ok() }
}

fn name(value: &str) -> CString {
    CString::new(value).expect("static symbol name")
}
