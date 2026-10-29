use std::ffi::c_void;
use std::mem::align_of;
use std::slice;

use crate::abi::LocatedVar;
use crate::located_globals::join_located_globals;

use super::{table::MAX_LOCATED_VARS, LocatedImageError};

pub(super) unsafe fn indices(
    vars: &[LocatedVar],
    globals: Option<(*const *mut c_void, u32)>,
) -> Result<Vec<usize>, LocatedImageError> {
    let Some((raw, count)) = globals else {
        return Ok(Vec::new());
    };
    let count = count as usize;
    if count > MAX_LOCATED_VARS {
        return Err(LocatedImageError::TooManyGlobals(count));
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    if raw.is_null() {
        return Err(LocatedImageError::NullGlobalsTable);
    }
    if !(raw as usize).is_multiple_of(align_of::<*mut c_void>()) {
        return Err(LocatedImageError::MisalignedGlobalsTable);
    }
    let globals = slice::from_raw_parts(raw, count);
    let located = vars
        .iter()
        .map(|var| identity(var.pointer))
        .collect::<Vec<_>>();
    let globals = globals
        .iter()
        .map(|pointer| identity(*pointer))
        .collect::<Vec<_>>();
    Ok(join_located_globals(&located, &globals).indices)
}

fn identity(pointer: *mut c_void) -> Option<usize> {
    (!pointer.is_null()).then_some(pointer as usize)
}
