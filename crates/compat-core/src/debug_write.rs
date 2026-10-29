//! Serialized external debug writes, matching OpenPLC's drain-at-cycle boundary.

use std::sync::{Mutex, MutexGuard};

pub const MAX_ENTRIES: usize = 128;
pub const MAX_BYTES: usize = 256;
pub const SUCCESS: u8 = 0x7e;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOp {
    Write,
    Force,
    Unforce,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalWrite {
    pub array: u8,
    pub element: u16,
    pub op: WriteOp,
    len: u16,
    bytes: [u8; MAX_BYTES],
}

impl ExternalWrite {
    fn new(array: u8, element: u16, op: WriteOp, value: &[u8]) -> Self {
        let len = value.len().min(MAX_BYTES);
        let mut bytes = [0; MAX_BYTES];
        bytes[..len].copy_from_slice(&value[..len]);
        Self {
            array,
            element,
            op,
            len: len as u16,
            bytes,
        }
    }

    pub fn value(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueError {
    Poisoned,
    Full,
}

pub trait WriteTarget {
    fn apply_write(&self, write: &ExternalWrite) -> u8;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplyReport {
    pub applied: usize,
    pub failed: Vec<u8>,
}

/// Fixed-capacity queue. Producers enqueue; the scan boundary drains.
pub struct DebugWriteQueue {
    entries: Mutex<Vec<ExternalWrite>>,
}

impl Default for DebugWriteQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugWriteQueue {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::with_capacity(MAX_ENTRIES)),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Vec<ExternalWrite>>, QueueError> {
        self.entries.lock().map_err(|_| QueueError::Poisoned)
    }

    /// Enqueue and truncate payloads to OpenPLC's 256-byte bound.
    pub fn enqueue(
        &self,
        array: u8,
        element: u16,
        op: WriteOp,
        value: &[u8],
    ) -> Result<(), QueueError> {
        let mut entries = self.lock()?;
        if entries.len() == MAX_ENTRIES {
            return Err(QueueError::Full);
        }
        entries.push(ExternalWrite::new(array, element, op, value));
        Ok(())
    }

    pub fn pending(&self) -> Result<usize, QueueError> {
        Ok(self.lock()?.len())
    }

    /// Atomically hand pending entries to the single cycle consumer.
    pub fn drain(&self) -> Result<Vec<ExternalWrite>, QueueError> {
        let mut entries = self.lock()?;
        Ok(std::mem::take(&mut *entries))
    }

    pub fn apply<T: WriteTarget>(&self, target: &T) -> Result<ApplyReport, QueueError> {
        let mut report = ApplyReport::default();
        for entry in self.drain()? {
            let status = target.apply_write(&entry);
            if status == SUCCESS {
                report.applied += 1;
            } else {
                report.failed.push(status);
            }
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DebugWriteQueue, QueueError, WriteOp, WriteTarget, MAX_BYTES, MAX_ENTRIES, SUCCESS,
    };

    struct Target;

    impl WriteTarget for Target {
        fn apply_write(&self, write: &super::ExternalWrite) -> u8 {
            if write.op == WriteOp::Unforce {
                SUCCESS
            } else {
                0x81
            }
        }
    }

    #[test]
    fn truncates_payload_and_preserves_order() {
        let queue = DebugWriteQueue::new();
        queue
            .enqueue(1, 2, WriteOp::Force, &[7; MAX_BYTES + 1])
            .unwrap();
        queue.enqueue(1, 3, WriteOp::Unforce, &[]).unwrap();
        let entries = queue.drain().unwrap();
        assert_eq!(entries[0].value().len(), MAX_BYTES);
        assert_eq!(entries[1].op, WriteOp::Unforce);
    }

    #[test]
    fn reports_full_without_dropping_existing_entries() {
        let queue = DebugWriteQueue::new();
        for element in 0..MAX_ENTRIES {
            queue
                .enqueue(0, element as u16, WriteOp::Write, &[])
                .unwrap();
        }
        assert_eq!(
            queue.enqueue(0, 0, WriteOp::Write, &[]),
            Err(QueueError::Full)
        );
        assert_eq!(queue.pending().unwrap(), MAX_ENTRIES);
    }

    #[test]
    fn applies_at_cycle_boundary_and_reports_target_status() {
        let queue = DebugWriteQueue::new();
        queue.enqueue(0, 0, WriteOp::Unforce, &[]).unwrap();
        queue.enqueue(0, 1, WriteOp::Write, &[1]).unwrap();
        let report = queue.apply(&Target).unwrap();
        assert_eq!(report.applied, 1);
        assert_eq!(report.failed, vec![0x81]);
        assert_eq!(queue.pending().unwrap(), 0);
    }
}
