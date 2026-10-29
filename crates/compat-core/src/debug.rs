//! OpenPLC hierarchical debug PDU contract (function codes 0x41..0x45).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock};

use crate::debug_write::{
    ApplyReport, DebugWriteQueue, ExternalWrite, QueueError, WriteOp, WriteTarget,
};

#[path = "debug_frame.rs"]
mod debug_frame;

pub(crate) const MAX_FRAME: usize = 4096;
pub(crate) const MAX_INDEXES: usize = 1024;
pub(crate) const SUCCESS: u8 = 0x7e;
pub(crate) const OUT_OF_BOUNDS: u8 = 0x81;
pub(crate) const OUT_OF_MEMORY: u8 = 0x82;
pub(crate) const NOT_LOADED: u8 = 0x83;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugAccessError {
    NotLoaded,
    OutOfBounds,
    InvalidSize { expected: usize, actual: usize },
    Queue(QueueError),
}

impl From<QueueError> for DebugAccessError {
    fn from(error: QueueError) -> Self {
        Self::Queue(error)
    }
}

/// Runtime symbols needed by the debug protocol. No default provider exists.
pub trait DebugProvider: Send + Sync {
    fn array_count(&self) -> u8;
    fn elem_count(&self, array: u8) -> u16;
    fn elem_size(&self, array: u8, element: u16) -> u16;
    fn read(&self, array: u8, element: u16) -> Option<Vec<u8>>;
    fn md5(&self) -> Option<&str>;
}

/// Provider registry and scan counter used by the command handler.
pub struct DebugProtocol {
    provider: RwLock<Option<Arc<dyn DebugProvider>>>,
    scan_counter: AtomicU32,
    writes: DebugWriteQueue,
}

impl Default for DebugProtocol {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugProtocol {
    pub fn new() -> Self {
        Self {
            provider: RwLock::new(None),
            scan_counter: AtomicU32::new(0),
            writes: DebugWriteQueue::new(),
        }
    }

    pub fn set_provider(&self, provider: Arc<dyn DebugProvider>) {
        if let Ok(mut slot) = self.provider.write() {
            *slot = Some(provider);
        }
    }

    pub fn clear_provider(&self) {
        if let Ok(mut slot) = self.provider.write() {
            *slot = None;
        }
    }

    pub fn set_scan_counter(&self, counter: u32) {
        self.scan_counter.store(counter, Ordering::Relaxed);
    }

    pub(crate) fn advance_scan_counter(&self) {
        self.scan_counter.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn reset(&self) -> Result<(), QueueError> {
        self.clear_provider();
        self.set_scan_counter(0);
        self.writes.drain().map(|_| ())
    }

    pub fn pending_writes(&self) -> Result<usize, QueueError> {
        self.writes.pending()
    }

    pub fn drain_writes(&self) -> Result<Vec<ExternalWrite>, QueueError> {
        self.writes.drain()
    }

    pub fn apply_pending<T: WriteTarget>(&self, target: &T) -> Result<ApplyReport, QueueError> {
        self.writes.apply(target)
    }

    pub fn entry_size(&self, array: u8, element: u16) -> Result<usize, DebugAccessError> {
        let provider = self.provider()?;
        if array >= provider.array_count() || element >= provider.elem_count(array) {
            return Err(DebugAccessError::OutOfBounds);
        }
        let size = usize::from(provider.elem_size(array, element));
        (size > 0)
            .then_some(size)
            .ok_or(DebugAccessError::OutOfBounds)
    }

    pub fn read_entry(&self, array: u8, element: u16) -> Result<Vec<u8>, DebugAccessError> {
        let provider = self.provider()?;
        if array >= provider.array_count() || element >= provider.elem_count(array) {
            return Err(DebugAccessError::OutOfBounds);
        }
        provider
            .read(array, element)
            .filter(|value| !value.is_empty())
            .ok_or(DebugAccessError::OutOfBounds)
    }

    pub fn queue_write(
        &self,
        array: u8,
        element: u16,
        value: &[u8],
    ) -> Result<(), DebugAccessError> {
        self.queue_value(array, element, WriteOp::Write, value)
    }

    pub fn queue_force(
        &self,
        array: u8,
        element: u16,
        value: &[u8],
    ) -> Result<(), DebugAccessError> {
        self.queue_value(array, element, WriteOp::Force, value)
    }

    pub fn queue_unforce(&self, array: u8, element: u16) -> Result<(), DebugAccessError> {
        let provider = self.provider()?;
        if array >= provider.array_count() || element >= provider.elem_count(array) {
            return Err(DebugAccessError::OutOfBounds);
        }
        self.writes
            .enqueue(array, element, WriteOp::Unforce, &[])
            .map_err(Into::into)
    }

    fn provider(&self) -> Result<Arc<dyn DebugProvider>, DebugAccessError> {
        self.provider
            .read()
            .ok()
            .and_then(|slot| slot.as_ref().cloned())
            .ok_or(DebugAccessError::NotLoaded)
    }

    fn queue_value(
        &self,
        array: u8,
        element: u16,
        op: WriteOp,
        value: &[u8],
    ) -> Result<(), DebugAccessError> {
        let expected = self.entry_size(array, element)?;
        if value.len() != expected {
            return Err(DebugAccessError::InvalidSize {
                expected,
                actual: value.len(),
            });
        }
        self.writes
            .enqueue(array, element, op, value)
            .map_err(Into::into)
    }

    /// Convert the socket's `DEBUG:<hex>` command to the exact text response.
    pub fn text_response(&self, payload: &str) -> String {
        let frame = parse_hex(payload);
        if frame.is_empty() {
            return "DEBUG:ERROR_PROCESSING\n".to_owned();
        }
        let provider = self
            .provider
            .read()
            .ok()
            .and_then(|slot| slot.as_ref().cloned());
        let response = debug_frame::process(
            &frame,
            provider.as_deref(),
            &self.writes,
            self.scan_counter.load(Ordering::Relaxed),
        );
        if response.is_empty() {
            return "DEBUG:ERROR_PROCESSING\n".to_owned();
        }
        format!("DEBUG:{}\n", format_hex(&response))
    }
}

fn parse_hex(input: &str) -> Vec<u8> {
    let mut output = Vec::new();
    for token in input.split(' ').filter(|token| !token.is_empty()) {
        let width = token.len().min(2);
        let Ok(value) = u8::from_str_radix(&token[..width], 16) else {
            break;
        };
        output.push(value);
        if output.len() == MAX_FRAME {
            break;
        }
    }
    output
}

fn format_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{DebugAccessError, DebugProtocol, DebugProvider};
    use crate::debug_write::WriteOp;

    struct Provider;

    impl DebugProvider for Provider {
        fn array_count(&self) -> u8 {
            1
        }
        fn elem_count(&self, _: u8) -> u16 {
            1
        }
        fn elem_size(&self, _: u8, _: u16) -> u16 {
            1
        }
        fn read(&self, _: u8, _: u16) -> Option<Vec<u8>> {
            Some(vec![9])
        }
        fn md5(&self) -> Option<&str> {
            Some("0123456789abcdef0123456789abcdef")
        }
    }

    #[test]
    fn reports_unloaded_and_rejects_unknown_frames() {
        let protocol = DebugProtocol::new();
        assert_eq!(protocol.text_response("41"), "DEBUG:41 83\n");
        assert_eq!(protocol.text_response("99"), "DEBUG:ERROR_PROCESSING\n");
    }

    #[test]
    fn rejects_empty_payload() {
        assert_eq!(
            DebugProtocol::new().text_response(""),
            "DEBUG:ERROR_PROCESSING\n"
        );
    }

    #[test]
    fn force_request_is_queued_for_cycle_drain() {
        let protocol = DebugProtocol::new();
        protocol.set_provider(Arc::new(Provider));
        assert_eq!(
            protocol.text_response("42 00 00 00 01 00 01 05"),
            "DEBUG:42 7e\n"
        );
        let writes = protocol.drain_writes().unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].value(), &[5]);
    }

    #[test]
    fn direct_debug_access_uses_the_same_provider_and_cycle_queue() {
        let protocol = DebugProtocol::new();
        assert_eq!(protocol.read_entry(0, 0), Err(DebugAccessError::NotLoaded));
        protocol.set_provider(Arc::new(Provider));
        assert_eq!(protocol.entry_size(0, 0).unwrap(), 1);
        assert_eq!(protocol.read_entry(0, 0).unwrap(), vec![9]);
        assert_eq!(
            protocol.queue_force(0, 0, &[]),
            Err(DebugAccessError::InvalidSize {
                expected: 1,
                actual: 0,
            })
        );
        protocol.queue_write(0, 0, &[3]).unwrap();
        protocol.queue_force(0, 0, &[5]).unwrap();
        protocol.queue_unforce(0, 0).unwrap();
        let writes = protocol.drain_writes().unwrap();
        assert_eq!(writes.len(), 3);
        assert_eq!(writes[0].op, WriteOp::Write);
        assert_eq!(writes[0].value(), &[3]);
        assert_eq!(writes[1].op, WriteOp::Force);
        assert_eq!(writes[1].value(), &[5]);
        assert_eq!(writes[2].op, WriteOp::Unforce);
        assert_eq!(writes[2].value(), &[]);
    }

    #[test]
    fn reset_unloads_provider_and_discards_pending_writes() {
        let protocol = DebugProtocol::new();
        protocol.set_provider(Arc::new(Provider));
        assert_eq!(
            protocol.text_response("42 00 00 00 01 00 01 05"),
            "DEBUG:42 7e\n"
        );
        protocol.reset().unwrap();
        assert_eq!(protocol.pending_writes().unwrap(), 0);
        assert_eq!(protocol.text_response("41"), "DEBUG:41 83\n");
    }
}
