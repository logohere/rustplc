use std::sync::{Mutex, MutexGuard};

use super::{JournalEntry, JournalError, JOURNAL_MAX_ENTRIES};
use crate::image::ProcessImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueError {
    Poisoned,
    Uninitialized,
    Full,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ApplyStats {
    pub applied: usize,
    pub forced: usize,
    pub invalid: usize,
}

#[derive(Default)]
struct State {
    initialized: bool,
    next_sequence: u32,
    entries: Vec<JournalEntry>,
}

/// Portable mutex-backed journal contract. The pinned runtime selects a
/// lock-free double-buffer on targets with always-lock-free atomics; this
/// implementation is the explicit fallback until that producer is ported.
pub struct JournalQueue {
    state: Mutex<State>,
}

impl Default for JournalQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl JournalQueue {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, State>, QueueError> {
        self.state.lock().map_err(|_| QueueError::Poisoned)
    }

    pub fn init(&self) -> Result<(), QueueError> {
        let mut state = self.lock()?;
        state.initialized = true;
        state.next_sequence = 0;
        state.entries.clear();
        Ok(())
    }

    pub fn cleanup(&self) -> Result<(), QueueError> {
        let mut state = self.lock()?;
        state.initialized = false;
        state.next_sequence = 0;
        state.entries.clear();
        Ok(())
    }

    pub fn is_initialized(&self) -> Result<bool, QueueError> {
        Ok(self.lock()?.initialized)
    }

    /// Enqueue without applying. Index bounds are intentionally deferred to
    /// the consumer, matching OpenPLC's public write functions.
    pub fn enqueue(&self, mut entry: JournalEntry) -> Result<(), QueueError> {
        let mut state = self.lock()?;
        if !state.initialized {
            return Err(QueueError::Uninitialized);
        }
        if state.entries.len() >= JOURNAL_MAX_ENTRIES {
            return Err(QueueError::Full);
        }
        entry.sequence = state.next_sequence;
        state.next_sequence = state.next_sequence.wrapping_add(1);
        state.entries.push(entry);
        Ok(())
    }

    pub fn pending_count(&self) -> Result<usize, QueueError> {
        Ok(self.lock()?.entries.len())
    }

    pub fn sequence(&self) -> Result<u32, QueueError> {
        Ok(self.lock()?.next_sequence)
    }

    /// Apply in enqueue order and clear the pending entries. Invalid entries
    /// are counted and skipped; forced writes are also counted separately.
    pub fn apply_and_clear(&self, image: &mut ProcessImage) -> Result<ApplyStats, QueueError> {
        let mut state = self.lock()?;
        let mut stats = ApplyStats::default();
        for entry in state.entries.drain(..) {
            match image.apply(entry) {
                Ok(true) => stats.applied += 1,
                Ok(false) => stats.forced += 1,
                Err(JournalError::OutOfBounds(_))
                | Err(JournalError::InvalidBit(_))
                | Err(JournalError::InvalidNonBoolBit(_))
                | Err(JournalError::InvalidType(_))
                | Err(JournalError::WrongType) => stats.invalid += 1,
            }
        }
        state.next_sequence = 0;
        Ok(stats)
    }
}
