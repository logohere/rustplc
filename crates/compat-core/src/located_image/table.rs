use std::ffi::c_void;
use std::mem::align_of;
use std::slice;

use crate::abi::LocatedVar;

use super::LocatedImageError;

pub(super) const MAX_LOCATED_VARS: usize = 65_536;

pub(super) unsafe fn copy(
    raw: *const c_void,
    count: u32,
) -> Result<Vec<LocatedVar>, LocatedImageError> {
    let count = count as usize;
    if count > MAX_LOCATED_VARS {
        return Err(LocatedImageError::TooMany(count));
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    if raw.is_null() {
        return Err(LocatedImageError::NullTable);
    }
    if !(raw as usize).is_multiple_of(align_of::<LocatedVar>()) {
        return Err(LocatedImageError::MisalignedTable);
    }
    Ok(slice::from_raw_parts(raw.cast::<LocatedVar>(), count).to_vec())
}
