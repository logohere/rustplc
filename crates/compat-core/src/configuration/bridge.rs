use std::ffi::{c_char, c_void};

use super::{ConfigurationError, ResourceInstance};

#[cfg(not(all(windows, target_env = "msvc")))]
unsafe extern "C" {
    fn open_rust_plc_program_run(raw: *mut c_void) -> u8;
    fn open_rust_plc_program_located_range(
        raw: *mut c_void,
        offset: *mut u32,
        count: *mut u32,
    ) -> u8;
    fn open_rust_plc_configuration_name(raw: *mut c_void, name: *mut *const c_char) -> u8;
    fn open_rust_plc_configuration_resources(
        raw: *mut c_void,
        resources: *mut *mut ResourceInstance,
        count: *mut usize,
    ) -> u8;
}

pub unsafe fn program_run(raw: *mut c_void) -> Result<(), ConfigurationError> {
    #[cfg(not(all(windows, target_env = "msvc")))]
    return status(open_rust_plc_program_run(raw));
    #[cfg(all(windows, target_env = "msvc"))]
    return Err(ConfigurationError::UnsupportedAbi);
}

pub unsafe fn program_located_range(
    raw: *mut c_void,
    offset: *mut u32,
    count: *mut u32,
) -> Result<(), ConfigurationError> {
    #[cfg(not(all(windows, target_env = "msvc")))]
    return status(open_rust_plc_program_located_range(raw, offset, count));
    #[cfg(all(windows, target_env = "msvc"))]
    return Err(ConfigurationError::UnsupportedAbi);
}

pub unsafe fn configuration_name(
    raw: *mut c_void,
    name: *mut *const c_char,
) -> Result<(), ConfigurationError> {
    #[cfg(not(all(windows, target_env = "msvc")))]
    return status(open_rust_plc_configuration_name(raw, name));
    #[cfg(all(windows, target_env = "msvc"))]
    return Err(ConfigurationError::UnsupportedAbi);
}

pub unsafe fn configuration_resources(
    raw: *mut c_void,
    resources: *mut *mut ResourceInstance,
    count: *mut usize,
) -> Result<(), ConfigurationError> {
    #[cfg(not(all(windows, target_env = "msvc")))]
    return status(open_rust_plc_configuration_resources(raw, resources, count));
    #[cfg(all(windows, target_env = "msvc"))]
    return Err(ConfigurationError::UnsupportedAbi);
}

fn status(value: u8) -> Result<(), ConfigurationError> {
    match value {
        0 => Ok(()),
        1 => Err(ConfigurationError::BridgeRejected),
        2 => Err(ConfigurationError::CppException),
        value => Err(ConfigurationError::UnknownBridgeStatus(value)),
    }
}

#[cfg(test)]
mod tests {
    use super::status;
    use crate::configuration::ConfigurationError;

    #[test]
    fn maps_cpp_bridge_statuses() {
        assert_eq!(status(0), Ok(()));
        assert_eq!(status(1), Err(ConfigurationError::BridgeRejected));
        assert_eq!(status(2), Err(ConfigurationError::CppException));
        assert_eq!(status(9), Err(ConfigurationError::UnknownBridgeStatus(9)));
    }
}
