//! Threaded located-variable copy boundary from OpenPLC `image_tables.cpp`.

mod globals;
mod sync;
mod table;
#[cfg(test)]
mod tests;
mod value;

use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::abi::{AbiError, LocatedVar};
use crate::debug_write::QueueError as DebugQueueError;
use crate::image::ProcessImage;
use crate::journal::{JournalEntry, JournalError, JournalQueue, QueueError as JournalQueueError};
use crate::program_api::ProgramApi;
use value::{entry, ValueError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocatedImageError {
    NullTable,
    NullGlobalsTable,
    MisalignedTable,
    MisalignedGlobalsTable,
    MisalignedMember,
    TooMany(usize),
    TooManyGlobals(usize),
    Abi(AbiError),
    Journal(JournalError),
    Queue(JournalQueueError),
    DebugQueue(DebugQueueError),
    Poisoned,
}

pub struct LocatedImage {
    _owner: Option<Arc<ProgramApi>>,
    vars: Box<[LocatedVar]>,
    config_globals: Box<[usize]>,
    snapshots: Mutex<Vec<u64>>,
    image: Mutex<ProcessImage>,
    journal: JournalQueue,
    dropped_writes: AtomicU64,
}

// SAFETY: the owner keeps generated storage live. Located-variable access is
// serialized through the image/snapshot mutexes and scan-frame boundaries;
// disjoint program-local ranges and synchronized configuration globals are the
// pinned ABI contract enforced when the image is constructed.
unsafe impl Send for LocatedImage {}
// SAFETY: all shared mutable Rust state is behind synchronization primitives;
// raw generated pointers are accessed only at the documented scan boundaries.
unsafe impl Sync for LocatedImage {}

impl LocatedImage {
    pub(crate) unsafe fn from_program(
        owner: Arc<ProgramApi>,
        raw: *const c_void,
        count: u32,
        globals: Option<(*const *mut c_void, u32)>,
    ) -> Result<Arc<Self>, LocatedImageError> {
        let vars = table::copy(raw, count)?;
        let config_globals = globals::indices(&vars, globals)?;
        Self::build(vars, config_globals, Some(owner))
    }

    pub fn read(&self, address: JournalEntry) -> Result<u64, LocatedImageError> {
        let mut image = self.lock_image()?;
        self.journal.apply_and_clear(&mut image)?;
        Ok(image.read(address)?)
    }

    pub fn write(&self, write: JournalEntry) -> Result<(), LocatedImageError> {
        Ok(self.journal.enqueue(write)?)
    }

    pub fn dropped_writes(&self) -> u64 {
        self.dropped_writes.load(Ordering::Acquire)
    }

    fn build(
        vars: Vec<LocatedVar>,
        config_globals: Vec<usize>,
        owner: Option<Arc<ProgramApi>>,
    ) -> Result<Arc<Self>, LocatedImageError> {
        for var in &vars {
            entry(*var, 0)?;
        }
        let journal = JournalQueue::new();
        journal.init()?;
        Ok(Arc::new(Self {
            _owner: owner,
            config_globals: config_globals.into_boxed_slice(),
            snapshots: Mutex::new(vec![0; vars.len()]),
            vars: vars.into_boxed_slice(),
            image: Mutex::new(ProcessImage::default()),
            journal,
            dropped_writes: AtomicU64::new(0),
        }))
    }

    fn lock_image(&self) -> Result<MutexGuard<'_, ProcessImage>, LocatedImageError> {
        self.image.lock().map_err(|_| LocatedImageError::Poisoned)
    }

    fn lock_snapshots(&self) -> Result<MutexGuard<'_, Vec<u64>>, LocatedImageError> {
        self.snapshots
            .lock()
            .map_err(|_| LocatedImageError::Poisoned)
    }
}

impl From<ValueError> for LocatedImageError {
    fn from(error: ValueError) -> Self {
        match error {
            ValueError::Abi(error) => Self::Abi(error),
            ValueError::Journal(error) => Self::Journal(error),
            ValueError::MisalignedMember => Self::MisalignedMember,
        }
    }
}

impl From<AbiError> for LocatedImageError {
    fn from(error: AbiError) -> Self {
        Self::Abi(error)
    }
}

impl From<JournalError> for LocatedImageError {
    fn from(error: JournalError) -> Self {
        Self::Journal(error)
    }
}

impl From<JournalQueueError> for LocatedImageError {
    fn from(error: JournalQueueError) -> Self {
        Self::Queue(error)
    }
}

impl From<DebugQueueError> for LocatedImageError {
    fn from(error: DebugQueueError) -> Self {
        Self::DebugQueue(error)
    }
}
