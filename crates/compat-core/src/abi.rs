//! Safe layout mirrors for the OpenPLC STruC++ ABI boundary.

use std::ffi::c_void;

/// `strucpp::LocatedArea` values from `strucpp_abi.hpp`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocatedArea {
    Input = 0,
    Output = 1,
    Memory = 2,
}

/// `strucpp::LocatedSize` values from `strucpp_abi.hpp`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocatedSize {
    Bit = 0,
    Byte = 1,
    Word = 2,
    DWord = 3,
    LWord = 4,
}

/// Invalid enum byte received at the generated-program ABI boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiError {
    InvalidArea(u8),
    InvalidSize(u8),
}

impl TryFrom<u8> for LocatedArea {
    type Error = AbiError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Input),
            1 => Ok(Self::Output),
            2 => Ok(Self::Memory),
            value => Err(AbiError::InvalidArea(value)),
        }
    }
}

impl TryFrom<u8> for LocatedSize {
    type Error = AbiError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Bit),
            1 => Ok(Self::Byte),
            2 => Ok(Self::Word),
            3 => Ok(Self::DWord),
            4 => Ok(Self::LWord),
            value => Err(AbiError::InvalidSize(value)),
        }
    }
}

/// Layout-compatible `strucpp::LocatedVar`; `area` and `size` stay raw bytes
/// until validated with `TryFrom` to avoid invalid-enum UB at the FFI edge.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LocatedVar {
    pub area: u8,
    pub size: u8,
    pub byte_index: u16,
    pub bit_index: u8,
    pub reserved: [u8; 3],
    pub pointer: *mut c_void,
}

impl LocatedVar {
    /// Validate and decode the area byte.
    pub fn area_kind(self) -> Result<LocatedArea, AbiError> {
        self.area.try_into()
    }

    /// Validate and decode the size byte.
    pub fn size_kind(self) -> Result<LocatedSize, AbiError> {
        self.size.try_into()
    }
}

/// IEC `TIME`, `DATE`, `DT`, and `TOD` layout from `iec_types.h`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IecTimespec {
    pub tv_sec: i32,
    pub tv_nsec: i32,
}

/// IEC string layout from `iec_types.h` (`int8_t` length plus 126 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IecString {
    pub len: i8,
    pub body: [u8; 126],
}

#[cfg(test)]
mod tests {
    use std::mem::{align_of, offset_of, size_of};

    use super::{AbiError, IecString, IecTimespec, LocatedArea, LocatedSize, LocatedVar};

    #[test]
    fn preserves_abi_layouts() {
        assert_eq!(size_of::<IecTimespec>(), 8);
        assert_eq!(size_of::<IecString>(), 127);
        assert_eq!(offset_of!(LocatedVar, pointer), 8);
        assert_eq!(size_of::<LocatedVar>(), 8 + size_of::<*mut ()>());
        assert_eq!(align_of::<LocatedVar>(), align_of::<*mut ()>());
    }

    #[test]
    fn validates_located_descriptor_bytes() {
        let descriptor = LocatedVar {
            area: LocatedArea::Output as u8,
            size: LocatedSize::Word as u8,
            byte_index: 4,
            bit_index: 0,
            reserved: [0; 3],
            pointer: std::ptr::null_mut(),
        };
        assert_eq!(descriptor.area_kind().unwrap(), LocatedArea::Output);
        assert_eq!(descriptor.size_kind().unwrap(), LocatedSize::Word);
        assert_eq!(LocatedArea::try_from(9), Err(AbiError::InvalidArea(9)));
    }
}
