use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc, OnceLock};
use std::thread::{self, JoinHandle};

use crate::scheduler::{build_schedule, Schedule};
use crate::timing::ScanTracker;

use super::dispatcher::FrameBoundary;
use super::{
    dispatcher, DispatchTask, FrameFaultHook, FrameFaultReport, RuntimeError, RuntimeHandle,
    RuntimeTask,
};

mod task;

use task::Spawned;

struct Workers {
    names: Vec<String>,
    dispatch: Vec<DispatchTask>,
    releases: Vec<mpsc::SyncSender<u64>>,
    handles: Vec<JoinHandle<()>>,
    trackers: Vec<Arc<ScanTracker>>,
    faults: Vec<Arc<OnceLock<super::RuntimeTaskFault>>>,
}

pub(super) fn runtime(
    tasks: Vec<RuntimeTask>,
    boundary: Option<FrameBoundary>,
    fault_hook: Option<FrameFaultHook>,
) -> Result<RuntimeHandle, RuntimeError> {
    let specs = tasks.iter().map(|task| task.spec).collect::<Vec<_>>();
    let schedule = build_schedule(&specs).map_err(RuntimeError::Schedule)?;
    let stop = Arc::new(AtomicBool::new(false));
    let (completed, completions) = mpsc::channel();
    let (activation, activate) = mpsc::sync_channel(1);
    let mut workers = Workers::spawn(tasks, &schedule, &stop, &completed)?;
    drop(completed);
    let frame_fault = Arc::new(OnceLock::new());
    let dispatch = std::mem::take(&mut workers.dispatch);
    let dispatcher = match spawn_dispatcher(
        dispatch,
        Arc::clone(&stop),
        schedule.base_tick_ns,
        boundary,
        completions,
        (Arc::clone(&frame_fault), fault_hook),
        activate,
    ) {
        Ok(handle) => handle,
        Err(error) => {
            workers.stop(&stop);
            return Err(RuntimeError::Spawn(error));
        }
    };
    Ok(workers.handle(stop, dispatcher, activation, frame_fault))
}

impl Workers {
    fn spawn(
        tasks: Vec<RuntimeTask>,
        schedule: &Schedule,
        stop: &Arc<AtomicBool>,
        completed: &mpsc::Sender<()>,
    ) -> Result<Self, RuntimeError> {
        let mut workers = Self::empty(tasks.len());
        for (task, slot) in tasks.into_iter().zip(&schedule.tasks) {
            let name = task.name.clone();
            match task::spawn(task, *slot, stop, completed) {
                Ok(worker) => workers.push(name, worker),
                Err(error) => {
                    workers.stop(stop);
                    return Err(error);
                }
            }
        }
        Ok(workers)
    }

    fn empty(capacity: usize) -> Self {
        Self {
            names: Vec::with_capacity(capacity),
            dispatch: Vec::with_capacity(capacity),
            releases: Vec::with_capacity(capacity),
            handles: Vec::with_capacity(capacity),
            trackers: Vec::with_capacity(capacity),
            faults: Vec::with_capacity(capacity),
        }
    }

    fn push(&mut self, name: String, worker: Spawned) {
        self.names.push(name);
        self.dispatch.push(worker.dispatch);
        self.releases.push(worker.release);
        self.handles.push(worker.handle);
        self.trackers.push(worker.tracker);
        self.faults.push(worker.fault);
    }

    fn handle(
        self,
        stop: Arc<AtomicBool>,
        dispatcher: JoinHandle<()>,
        activation: mpsc::SyncSender<()>,
        frame_fault: Arc<OnceLock<crate::located_image::LocatedImageError>>,
    ) -> RuntimeHandle {
        RuntimeHandle {
            stop,
            releases: self.releases,
            workers: self.handles,
            dispatcher: Some(dispatcher),
            activation: Some(activation),
            task_names: self.names,
            trackers: self.trackers,
            faults: self.faults,
            frame_fault,
        }
    }

    fn stop(self, stop: &AtomicBool) {
        stop.store(true, std::sync::atomic::Ordering::Release);
        for release in self.releases {
            let _ = release.try_send(0);
        }
        for worker in self.handles {
            let _ = worker.join();
        }
    }
}

fn spawn_dispatcher(
    tasks: Vec<DispatchTask>,
    stop: Arc<AtomicBool>,
    tick_ns: u64,
    boundary: Option<FrameBoundary>,
    completed: mpsc::Receiver<()>,
    fault: FrameFaultReport,
    activation: mpsc::Receiver<()>,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("openplc-dispatch".into())
        .spawn(move || {
            if activation.recv().is_ok() && !stop.load(std::sync::atomic::Ordering::Acquire) {
                dispatcher::run(tasks, stop, tick_ns, boundary, completed, fault);
            }
        })
}
