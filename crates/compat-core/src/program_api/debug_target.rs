use std::ptr;

use crate::abi::LocatedVar;
use crate::debug::{DebugProvider, MAX_FRAME, OUT_OF_BOUNDS};
use crate::debug_write::{ExternalWrite, WriteOp, WriteTarget, SUCCESS};
use crate::located_image::LocatedImage;

use super::ProgramApi;

/// Debug-write target for a generated program and its process image.
///
/// Drain it only while generated task workers are quiescent.
pub struct ProgramDebugTarget<'a> {
    api: &'a ProgramApi,
    image: &'a LocatedImage,
}

impl<'a> ProgramDebugTarget<'a> {
    pub fn new(api: &'a ProgramApi, image: &'a LocatedImage) -> Self {
        Self { api, image }
    }

    fn apply_located(&self, write: &ExternalWrite, var: LocatedVar) -> u8 {
        let result = match write.op {
            WriteOp::Write => self.image.debug_write(var, write.value()),
            WriteOp::Force => {
                let status = self
                    .api
                    .debug_set(write.array, write.element, true, write.value());
                return self
                    .image
                    .debug_force(var, Some(write.value()))
                    .map_or(OUT_OF_BOUNDS, |_| status);
            }
            WriteOp::Unforce => {
                let status = self.api.debug_set(write.array, write.element, false, &[]);
                return self
                    .image
                    .debug_force(var, None)
                    .map_or(OUT_OF_BOUNDS, |_| status);
            }
        };
        result.map_or(OUT_OF_BOUNDS, |_| SUCCESS)
    }
}

impl WriteTarget for ProgramDebugTarget<'_> {
    fn apply_write(&self, write: &ExternalWrite) -> u8 {
        self.api
            .debug_location(write.array, write.element)
            .map_or_else(
                || self.api.apply_write(write),
                |var| self.apply_located(write, var),
            )
    }
}

impl DebugProvider for ProgramApi {
    fn array_count(&self) -> u8 {
        // SAFETY: function pointer was loaded from the pinned generated ABI and
        // ProgramApi keeps the backing library alive.
        unsafe { (self.array_count)() }
    }

    fn elem_count(&self, array: u8) -> u16 {
        // SAFETY: pinned ABI scalar query; the generated implementation bounds
        // invalid arrays and the backing library remains loaded.
        unsafe { (self.elem_count)(array) }
    }

    fn elem_size(&self, array: u8, element: u16) -> u16 {
        // SAFETY: pinned ABI scalar query with no caller-owned pointers.
        unsafe { (self.elem_size)(array, element) }
    }

    fn read(&self, array: u8, element: u16) -> Option<Vec<u8>> {
        let size = usize::from(self.elem_size(array, element)).min(MAX_FRAME);
        let mut value = (size > 0).then(|| vec![0; size])?;
        // SAFETY: `value` is allocated to the ABI-reported bounded element size
        // and remains live/writable for the call.
        let written = unsafe { (self.debug_read)(array, element, value.as_mut_ptr()) } as usize;
        value.truncate(written.min(size));
        Some(value)
    }

    fn md5(&self) -> Option<&str> {
        self.md5.as_deref()
    }
}

impl WriteTarget for ProgramApi {
    fn apply_write(&self, write: &ExternalWrite) -> u8 {
        match write.op {
            WriteOp::Write => self.debug_write(write.array, write.element, write.value()),
            WriteOp::Force => self.debug_set(write.array, write.element, true, write.value()),
            WriteOp::Unforce => self.debug_set(write.array, write.element, false, &[]),
        }
    }
}

impl ProgramApi {
    pub fn debug_set(&self, array: u8, element: u16, forcing: bool, bytes: &[u8]) -> u8 {
        let length = bytes.len().min(u16::MAX as usize) as u16;
        // SAFETY: `bytes` remains live for the call and length is clamped to u16.
        unsafe { (self.debug_set)(array, element, forcing, bytes.as_ptr(), length) }
    }

    pub fn debug_write(&self, array: u8, element: u16, bytes: &[u8]) -> u8 {
        let length = bytes.len().min(u16::MAX as usize) as u16;
        // SAFETY: `bytes` remains live for the call and length is clamped to u16.
        unsafe { (self.debug_write)(array, element, bytes.as_ptr(), length) }
    }

    fn debug_location(&self, array: u8, element: u16) -> Option<LocatedVar> {
        let locate = self.debug_locate?;
        let mut var = LocatedVar {
            area: 0,
            size: 0,
            byte_index: 0,
            bit_index: 0,
            reserved: [0; 3],
            pointer: ptr::null_mut(),
        };
        // SAFETY: the optional symbol uses the pinned STruC++ ABI, and all
        // output pointers name initialized local fields for this call.
        let found = unsafe {
            locate(
                array,
                element,
                &mut var.area,
                &mut var.size,
                &mut var.byte_index,
                &mut var.bit_index,
            )
        };
        (found != 0).then_some(var)
    }
}
