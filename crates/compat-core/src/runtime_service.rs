//! Background ownership for command-requested generated-program transitions.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::lifecycle::{PlcState, StateError, TRANSITION_LANDING_TIMEOUT_MS};
use crate::program_api::ProgramApi;
use crate::runtime_owner::{RuntimeOwner, RuntimeOwnerError};
use crate::transition::{correction_result, reconcile, ReconcileAction};

const LANDING_POLL_MS: u64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeginResult {
    Accepted,
    RejectedSwitch,
    AlreadyRunning,
    Rejected,
}

pub struct RuntimeService {
    owner: Arc<RuntimeOwner>,
    program_path: Arc<PathBuf>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl RuntimeService {
    /// Create the command transition service for one generated library path.
    ///
    /// # Safety
    /// The path must always name a library implementing the pinned generated ABI.
    pub unsafe fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            owner: Arc::new(RuntimeOwner::new()),
            program_path: Arc::new(path.into()),
            workers: Mutex::new(Vec::new()),
        }
    }

    pub fn owner(&self) -> &RuntimeOwner {
        &self.owner
    }

    pub fn begin(&self, target: PlcState) -> Result<BeginResult, StateError> {
        let state = self.owner.lifecycle().state()?;
        if target == PlcState::Running && !self.owner.mode_switch().allows_run() {
            return Ok(BeginResult::RejectedSwitch);
        }
        if target == PlcState::Running && state == PlcState::Running {
            return Ok(BeginResult::AlreadyRunning);
        }
        if target == PlcState::Stopped && state != PlcState::Running {
            return Ok(BeginResult::Rejected);
        }
        if !self.owner.lifecycle().claim_transition(target)? {
            return Ok(BeginResult::Rejected);
        }
        self.launch(target);
        Ok(BeginResult::Accepted)
    }

    fn launch(&self, target: PlcState) {
        let fallback_owner = Arc::clone(&self.owner);
        let fallback_path = Arc::clone(&self.program_path);
        let owner = Arc::clone(&fallback_owner);
        let path = Arc::clone(&fallback_path);
        match thread::Builder::new()
            .name("openplc-transition".into())
            .spawn(move || run_transition(owner, path, target))
        {
            Ok(worker) => self.track(worker),
            Err(_) => run_transition(fallback_owner, fallback_path, target),
        }
    }

    fn track(&self, worker: JoinHandle<()>) {
        let mut workers = self.lock_workers();
        let mut index = 0;
        while index < workers.len() {
            if workers[index].is_finished() {
                let _ = workers.swap_remove(index).join();
            } else {
                index += 1;
            }
        }
        workers.push(worker);
    }

    fn lock_workers(&self) -> MutexGuard<'_, Vec<JoinHandle<()>>> {
        match self.workers.lock() {
            Ok(workers) => workers,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl Drop for RuntimeService {
    fn drop(&mut self) {
        let workers = match self.workers.get_mut() {
            Ok(workers) => workers,
            Err(poisoned) => poisoned.into_inner(),
        };
        for worker in workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn run_transition(owner: Arc<RuntimeOwner>, path: Arc<PathBuf>, mut target: PlcState) {
    loop {
        complete(&owner, &path, target);
        let action = reconcile(
            owner.mode_switch().take_movement(),
            wait_for_landing(&owner),
            owner.mode_switch().position(),
        );
        let ReconcileAction::Begin(next) = action else {
            return;
        };
        let claimed = owner.lifecycle().claim_transition(next).unwrap_or(false);
        if correction_result(claimed) == ReconcileAction::ReArmMovement {
            owner.mode_switch().note_movement();
            return;
        }
        target = next;
    }
}

fn wait_for_landing(owner: &RuntimeOwner) -> PlcState {
    let deadline = Instant::now() + Duration::from_millis(TRANSITION_LANDING_TIMEOUT_MS);
    loop {
        let state = owner.state().unwrap_or(PlcState::Error);
        if !matches!(
            state,
            PlcState::TransitioningToRun | PlcState::TransitioningToStop
        ) || Instant::now() >= deadline
        {
            return state;
        }
        thread::sleep(Duration::from_millis(LANDING_POLL_MS));
    }
}

fn complete(owner: &RuntimeOwner, path: &Path, target: PlcState) {
    let result = match target {
        PlcState::Running => ProgramApi::load(path)
            .map_err(RuntimeOwnerError::Load)
            .and_then(|program| {
                // SAFETY: construction requires this path to implement the pinned ABI.
                unsafe { owner.start_claimed_program(Arc::new(program)) }
            })
            .map(|_| ()),
        PlcState::Stopped => owner.stop_claimed().map(|_| ()),
        _ => return,
    };
    if let Err(error) = result {
        let _: Result<(), _> = owner.fail(error);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{BeginResult, RuntimeService};
    use crate::lifecycle::PlcState;

    #[test]
    fn missing_program_lands_error_instead_of_stranding_transition() {
        // SAFETY: the missing path fails before any generated ABI call.
        let service = unsafe { RuntimeService::new("missing-generated-program") };
        assert_eq!(
            service.begin(PlcState::Running).unwrap(),
            BeginResult::Accepted
        );
        let deadline = Instant::now() + Duration::from_secs(1);
        while service.owner().state().unwrap() == PlcState::TransitioningToRun {
            assert!(Instant::now() < deadline, "transition did not land");
            std::thread::yield_now();
        }
        assert_eq!(service.owner().state().unwrap(), PlcState::Error);
    }
}
