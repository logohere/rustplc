use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{mpsc, Arc, OnceLock};
use std::thread::{self, JoinHandle};

use crate::scheduler::TaskSchedule;
use crate::timing::ScanTracker;

use super::super::{worker, DispatchTask, RuntimeError, RuntimeTask, RuntimeTaskFault};

pub(super) struct Spawned {
    pub(super) dispatch: DispatchTask,
    pub(super) release: mpsc::SyncSender<u64>,
    pub(super) handle: JoinHandle<()>,
    pub(super) tracker: Arc<ScanTracker>,
    pub(super) fault: Arc<OnceLock<RuntimeTaskFault>>,
}

struct State {
    alive: Arc<AtomicBool>,
    completed: Arc<AtomicU64>,
    tracker: Arc<ScanTracker>,
    fault: Arc<OnceLock<RuntimeTaskFault>>,
}

pub(super) fn spawn(
    task: RuntimeTask,
    slot: TaskSchedule,
    stop: &Arc<AtomicBool>,
    frame_completed: &mpsc::Sender<()>,
) -> Result<Spawned, RuntimeError> {
    let (release, receive) = mpsc::sync_channel(1);
    let state = State::new(slot)?;
    let dispatch = DispatchTask {
        divisor: slot.divisor,
        released: Arc::new(AtomicU64::new(0)),
        completed: Arc::clone(&state.completed),
        alive: Arc::clone(&state.alive),
        release: release.clone(),
    };
    let handle = spawn_handle(task, stop, receive, &state, frame_completed)?;
    Ok(Spawned {
        dispatch,
        release,
        handle,
        tracker: state.tracker,
        fault: state.fault,
    })
}

impl State {
    fn new(slot: TaskSchedule) -> Result<Self, RuntimeError> {
        let tracker =
            ScanTracker::new(slot.interval_ns as i64).map_err(|_| RuntimeError::Tracker)?;
        Ok(Self {
            alive: Arc::new(AtomicBool::new(true)),
            completed: Arc::new(AtomicU64::new(0)),
            tracker: Arc::new(tracker),
            fault: Arc::new(OnceLock::new()),
        })
    }
}

fn spawn_handle(
    mut task: RuntimeTask,
    stop: &Arc<AtomicBool>,
    receive: mpsc::Receiver<u64>,
    state: &State,
    frame_completed: &mpsc::Sender<()>,
) -> Result<JoinHandle<()>, RuntimeError> {
    let stop = Arc::clone(stop);
    let alive = Arc::clone(&state.alive);
    let tracker = Arc::clone(&state.tracker);
    let fault = Arc::clone(&state.fault);
    let completion = worker::Completion {
        count: Arc::clone(&state.completed),
        frame: frame_completed.clone(),
    };
    thread::Builder::new()
        .name(task.spec.priority.to_string())
        .spawn(move || {
            worker::run(
                receive,
                stop,
                alive,
                completion,
                tracker,
                fault,
                &mut task.body,
            )
        })
        .map_err(RuntimeError::Spawn)
}
