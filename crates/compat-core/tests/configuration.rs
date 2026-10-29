use std::ffi::c_void;

use open_rust_plc_compat_core::configuration::{
    ConfigurationError, ConfigurationInstance, ProgramInstance,
};

#[test]
fn rejects_null_generated_configuration_and_programs() {
    assert!(matches!(
        unsafe { ConfigurationInstance::from_raw(std::ptr::null_mut::<c_void>()) },
        Err(ConfigurationError::Null)
    ));
    assert!(matches!(
        unsafe { ProgramInstance::from_raw(std::ptr::null_mut::<c_void>()) },
        Err(ConfigurationError::InvalidProgram)
    ));
}

#[cfg(all(windows, target_env = "msvc"))]
#[test]
fn rejects_msvc_cpp_vtable_bridge() {
    let raw = std::ptr::NonNull::<u8>::dangling()
        .as_ptr()
        .cast::<c_void>();
    let program = unsafe { ProgramInstance::from_raw(raw) }.unwrap();
    assert!(matches!(
        unsafe { program.run() },
        Err(ConfigurationError::UnsupportedAbi)
    ));
}
