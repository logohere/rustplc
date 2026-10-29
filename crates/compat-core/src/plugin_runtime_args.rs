//! Layout mirror for OpenPLC `plugin_runtime_args_t`.
//!
//! Data and non-variadic callback slots mirror the pinned C header. Logging
//! slots remain opaque because Rust has no stable C-variadic function type.
//! The runtime must not call a plugin with a partially populated instance.

use std::ffi::c_char;

pub type BoolBuffer = *mut [*mut u8; 8];
pub type ByteBuffer = *mut *mut u8;
pub type IntBuffer = *mut *mut u16;
pub type DintBuffer = *mut *mut u32;
pub type LintBuffer = *mut *mut u64;
pub type ImageLock = Option<unsafe extern "C" fn()>;
pub type DebugArrayCount = Option<unsafe extern "C" fn() -> u8>;
pub type DebugElemCount = Option<unsafe extern "C" fn(u8) -> u16>;
pub type DebugSize = Option<unsafe extern "C" fn(u8, u16) -> u16>;
pub type DebugRead = Option<unsafe extern "C" fn(u8, u16, *mut u8) -> u16>;
pub type DebugSet = Option<unsafe extern "C" fn(u8, u16, bool, *const u8, u16) -> u8>;
pub type DebugWrite = Option<unsafe extern "C" fn(u8, u16, *const u8, u16) -> u8>;
pub type JournalWriteBool = Option<unsafe extern "C" fn(i32, i32, i32, i32) -> i32>;
pub type JournalWriteByte = Option<unsafe extern "C" fn(i32, i32, i32) -> i32>;
pub type JournalWriteInt = Option<unsafe extern "C" fn(i32, i32, i32) -> i32>;
pub type JournalWriteDint = Option<unsafe extern "C" fn(i32, i32, u32) -> i32>;
pub type JournalWriteLint = Option<unsafe extern "C" fn(i32, i32, u64) -> i32>;
pub type RequestPlc = Option<unsafe extern "C" fn(*const c_char)>;
pub type SetSwitchPosition = Option<unsafe extern "C" fn(i32)>;
pub type GetPlcState = Option<unsafe extern "C" fn() -> i32>;

type VariadicLogSlot = *mut (); // C variadic callback; kept opaque until a C shim exists.

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginRuntimeArgs {
    pub bool_input: BoolBuffer,
    pub bool_output: BoolBuffer,
    pub byte_input: ByteBuffer,
    pub byte_output: ByteBuffer,
    pub int_input: IntBuffer,
    pub int_output: IntBuffer,
    pub dint_input: DintBuffer,
    pub dint_output: DintBuffer,
    pub lint_input: LintBuffer,
    pub lint_output: LintBuffer,
    pub int_memory: IntBuffer,
    pub dint_memory: DintBuffer,
    pub lint_memory: LintBuffer,
    pub bool_memory: BoolBuffer,
    pub image_lock: ImageLock,
    pub image_unlock: ImageLock,
    pub debug_array_count: DebugArrayCount,
    pub debug_elem_count: DebugElemCount,
    pub debug_size: DebugSize,
    pub debug_read: DebugRead,
    pub debug_set: DebugSet,
    pub debug_write: DebugWrite,
    pub plugin_specific_config_file_path: [c_char; 256],
    pub buffer_size: i32,
    pub bits_per_buffer: i32,
    pub log_info: VariadicLogSlot,
    pub log_debug: VariadicLogSlot,
    pub log_warn: VariadicLogSlot,
    pub log_error: VariadicLogSlot,
    pub journal_write_bool: JournalWriteBool,
    pub journal_write_byte: JournalWriteByte,
    pub journal_write_int: JournalWriteInt,
    pub journal_write_dint: JournalWriteDint,
    pub journal_write_lint: JournalWriteLint,
    pub request_plc_stop: RequestPlc,
    pub base_tick_ns: u64,
    pub request_plc_start: RequestPlc,
    pub set_switch_position: SetSwitchPosition,
    pub get_plc_state: GetPlcState,
}

#[cfg(test)]
mod tests {
    use std::mem::{align_of, offset_of, size_of};

    use super::PluginRuntimeArgs;

    #[test]
    fn preserves_pointer_and_scalar_offsets() {
        let pointer = size_of::<*mut ()>();
        assert_eq!(
            offset_of!(PluginRuntimeArgs, plugin_specific_config_file_path),
            pointer * 22
        );
        assert_eq!(
            offset_of!(PluginRuntimeArgs, buffer_size),
            pointer * 22 + 256
        );
        assert_eq!(
            offset_of!(PluginRuntimeArgs, base_tick_ns),
            pointer * 32 + 256 + 8
        );
        assert_eq!(align_of::<PluginRuntimeArgs>(), align_of::<*mut ()>());
    }
}
