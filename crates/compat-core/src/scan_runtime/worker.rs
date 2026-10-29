use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, OnceLock};

use crate::timing::ScanTracker;

use super::RuntimeTaskFault;

pub(super) struct Completion {
    pub(super) count: Arc<AtomicU64>,
    pub(super) frame: mpsc::Sender<()>,
}

pub(super) fn run(
    receive: mpsc::Receiver<u64>,
    stop: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    completion: Completion,
    tracker: Arc<ScanTracker>,
    fault: Arc<OnceLock<RuntimeTaskFault>>,
    body: &mut dyn FnMut(u64) -> Result<(), RuntimeTaskFault>,
) {
    while let Ok(time_ns) = receive.recv() {
        if stop.load(Ordering::Acquire) {
            break;
        }
        let result = catch_unwind(AssertUnwindSafe(|| scan_once(&tracker, time_ns, body)));
        let failure = match result {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error),
            Err(_) => Some(RuntimeTaskFault::Panic),
        };
        if let Some(error) = failure {
            let _ = fault.set(error);
            alive.store(false, Ordering::Release);
        }
        completion.count.fetch_add(1, Ordering::Release);
        let _ = completion.frame.send(());
        if fault.get().is_some() {
            break;
        }
    }
}

fn scan_once(
    tracker: &ScanTracker,
    time_ns: u64,
    body: &mut dyn FnMut(u64) -> Result<(), RuntimeTaskFault>,
) -> Result<(), RuntimeTaskFault> {
    let _ = tracker.start();
    body(time_ns)?;
    let _ = tracker.end();
    Ok(())
}
