use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::thread::JoinHandle;

use crate::debug::DebugProtocol;
use crate::located_image::{LocatedImage, LocatedImageError};
use crate::program_api::{ProgramApi, ProgramScanError};
use crate::scheduler::{SchedulerError, TaskSpec};
use crate::timing::ScanTracker;

mod dispatcher;
mod start;
mod worker;

pub(crate) type FrameFaultHook = Arc<dyn Fn(LocatedImageError) + Send + Sync>;
type FrameFaultReport = (Arc<OnceLock<LocatedImageError>>, Option<FrameFaultHook>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTaskFault {
    Program(ProgramScanError),
    Panic,
}

pub struct RuntimeTask {
    pub name: String,
    pub spec: TaskSpec,
    pub body: Box<dyn FnMut(u64) -> Result<(), RuntimeTaskFault> + Send + 'static>,
}

#[derive(Debug)]
pub enum RuntimeError {
    Schedule(SchedulerError),
    Spawn(std::io::Error),
    Activation,
    Tracker,
}

struct DispatchTask {
    divisor: u64,
    released: Arc<AtomicU64>,
    completed: Arc<AtomicU64>,
    alive: Arc<AtomicBool>,
    release: mpsc::SyncSender<u64>,
}

pub struct RuntimeHandle {
    stop: Arc<AtomicBool>,
    releases: Vec<mpsc::SyncSender<u64>>,
    workers: Vec<JoinHandle<()>>,
    dispatcher: Option<JoinHandle<()>>,
    activation: Option<mpsc::SyncSender<()>>,
    task_names: Vec<String>,
    pub trackers: Vec<Arc<ScanTracker>>,
    /// First terminal fault for each task, in input task order.
    pub faults: Vec<Arc<OnceLock<RuntimeTaskFault>>>,
    /// First terminal configuration-global frame fault.
    pub frame_fault: Arc<OnceLock<LocatedImageError>>,
}

impl RuntimeHandle {
    pub fn start(tasks: Vec<RuntimeTask>) -> Result<Self, RuntimeError> {
        let mut runtime = Self::prepare(tasks)?;
        runtime.activate()?;
        Ok(runtime)
    }

    /// Starts tasks with STruC++ configuration-global frame synchronization.
    ///
    /// # Errors
    /// Returns a schedule, tracker, or worker-thread startup error.
    pub fn start_with_image(
        tasks: Vec<RuntimeTask>,
        image: Arc<LocatedImage>,
    ) -> Result<Self, RuntimeError> {
        let mut runtime = Self::prepare_with_image(tasks, image)?;
        runtime.activate()?;
        Ok(runtime)
    }

    pub(crate) fn prepare(tasks: Vec<RuntimeTask>) -> Result<Self, RuntimeError> {
        start::runtime(tasks, None, None)
    }

    pub(crate) fn prepare_with_image(
        tasks: Vec<RuntimeTask>,
        image: Arc<LocatedImage>,
    ) -> Result<Self, RuntimeError> {
        start::runtime(tasks, Some(dispatcher::FrameBoundary::image(image)), None)
    }

    pub(crate) fn prepare_program(
        tasks: Vec<RuntimeTask>,
        image: Arc<LocatedImage>,
        program: Arc<ProgramApi>,
        debug: Arc<DebugProtocol>,
        hook: FrameFaultHook,
    ) -> Result<Self, RuntimeError> {
        start::runtime(
            tasks,
            Some(dispatcher::FrameBoundary::program(image, program, debug)),
            Some(hook),
        )
    }

    pub(crate) fn activate(&mut self) -> Result<(), RuntimeError> {
        if let Some(activation) = self.activation.take() {
            activation.send(()).map_err(|_| RuntimeError::Activation)?;
        }
        Ok(())
    }

    pub(crate) fn task_stats(&self) -> Vec<(String, Arc<ScanTracker>)> {
        self.task_names
            .iter()
            .cloned()
            .zip(self.trackers.iter().cloned())
            .collect()
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.activate();
        for release in &self.releases {
            let _ = release.try_send(0);
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
        if let Some(dispatcher) = self.dispatcher.take() {
            let _ = dispatcher.join();
        }
    }
}

impl Drop for RuntimeHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests;
