use std::sync::atomic::Ordering;

use crate::abi::LocatedArea;

use super::value::{entry, read_member, write_member};
use super::{LocatedImage, LocatedImageError};

impl LocatedImage {
    pub(crate) fn debug_write(
        &self,
        var: crate::abi::LocatedVar,
        bytes: &[u8],
    ) -> Result<(), LocatedImageError> {
        self.write(entry(var, decode(var, bytes)?)?)
    }

    pub(crate) fn debug_force(
        &self,
        var: crate::abi::LocatedVar,
        bytes: Option<&[u8]>,
    ) -> Result<(), LocatedImageError> {
        let mut image = self.lock_image()?;
        self.journal.apply_and_clear(&mut image)?;
        let value = match bytes {
            Some(bytes) => decode(var, bytes)?,
            None => 0,
        };
        let address = entry(var, value)?;
        match bytes {
            Some(_) => image.force_set(address)?,
            None => image.force_clear(address.buffer_type, address.index, address.bit_index)?,
        }
        Ok(())
    }

    /// Number of configuration globals classified by exported storage identity.
    pub fn config_global_count(&self) -> usize {
        self.config_globals.len()
    }

    /// Copy image values into configuration-global storage at frame start.
    ///
    /// # Errors
    /// Returns a process-image, journal, or ABI failure.
    ///
    /// # Safety
    /// No generated task may access configuration globals during this call.
    pub unsafe fn copy_config_globals_in(&self) -> Result<(), LocatedImageError> {
        self.copy_in_indices(self.config_globals.iter().copied())
    }

    /// Copy changed configuration globals into the image journal at frame end.
    ///
    /// # Errors
    /// Returns a process-image, journal, or ABI failure.
    ///
    /// # Safety
    /// All generated tasks must be quiescent during this call.
    pub unsafe fn copy_config_globals_out(&self) -> Result<(), LocatedImageError> {
        self.copy_out_indices(self.config_globals.iter().copied())
    }

    pub(crate) fn copy_in_ranges<I>(&self, ranges: I) -> Result<(), LocatedImageError>
    where
        I: IntoIterator<Item = (u32, u32)>,
    {
        let indices = ranges.into_iter().flat_map(|range| self.range(range));
        self.copy_in_indices(indices)
    }

    pub(crate) fn copy_out_ranges<I>(&self, ranges: I) -> Result<(), LocatedImageError>
    where
        I: IntoIterator<Item = (u32, u32)>,
    {
        let indices = ranges.into_iter().flat_map(|range| self.range(range));
        self.copy_out_indices(indices)
    }

    fn copy_in_indices<I>(&self, indices: I) -> Result<(), LocatedImageError>
    where
        I: IntoIterator<Item = usize>,
    {
        let mut image = self.lock_image()?;
        self.journal.apply_and_clear(&mut image)?;
        let mut snapshots = self.lock_snapshots()?;
        for index in indices {
            let value = image.read(entry(self.vars[index], 0)?)?;
            // SAFETY: construction validated the descriptor; the caller owns
            // synchronization for the selected program/global storage.
            unsafe { write_member(self.vars[index], value) };
            snapshots[index] = value;
        }
        Ok(())
    }

    fn copy_out_indices<I>(&self, indices: I) -> Result<(), LocatedImageError>
    where
        I: IntoIterator<Item = usize>,
    {
        let mut pending = Vec::new();
        let mut snapshots = self.lock_snapshots()?;
        for index in indices {
            let var = self.vars[index];
            if var.area_kind()? == LocatedArea::Input {
                continue;
            }
            // SAFETY: construction validated the descriptor; the caller owns
            // synchronization for the selected program/global storage.
            let current = unsafe { read_member(var) };
            if current != snapshots[index] {
                snapshots[index] = current;
                pending.push(entry(var, current)?);
            }
        }
        drop(snapshots);
        for write in pending {
            if let Err(error) = self.journal.enqueue(write) {
                self.record_enqueue_error(error)?;
            }
        }
        Ok(())
    }

    fn record_enqueue_error(
        &self,
        error: crate::journal::QueueError,
    ) -> Result<(), LocatedImageError> {
        if error != crate::journal::QueueError::Full {
            return Err(error.into());
        }
        self.dropped_writes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn range(&self, (offset, count): (u32, u32)) -> std::ops::Range<usize> {
        let start = (offset as usize).min(self.vars.len());
        let end = (offset as usize)
            .saturating_add(count as usize)
            .min(self.vars.len());
        start..end
    }
}

fn decode(var: crate::abi::LocatedVar, bytes: &[u8]) -> Result<u64, LocatedImageError> {
    let width = match var.size_kind()? {
        crate::abi::LocatedSize::Bit | crate::abi::LocatedSize::Byte => 1,
        crate::abi::LocatedSize::Word => 2,
        crate::abi::LocatedSize::DWord => 4,
        crate::abi::LocatedSize::LWord => 8,
    };
    Ok(bytes
        .iter()
        .take(width)
        .enumerate()
        .fold(0, |value, (index, byte)| {
            value | (u64::from(*byte) << (index * 8))
        }))
}
