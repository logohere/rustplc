//! Lifecycle-owned scan runtime orchestration.

use std::sync::{Arc, Mutex, MutexGuard};

use crate::debug::DebugProtocol;
use crate::lifecycle::{Lifecycle, PlcState, StateError};
use crate::mode_switch::{ModeSwitch, SwitchPosition};
use crate::scan_runtime::{RuntimeHandle, RuntimeTask};
use crate::stats::StatsRegistry;

mod owned;
mod program;
mod result;

use owned::{discard, OwnedRuntime};
pub use result::{RuntimeOwnerError, StartResult, StopResult};

pub struct RuntimeOwner {
    lifecycle: Arc<Lifecycle>,
    debug: Arc<DebugProtocol>,
    stats: Arc<StatsRegistry>,
    mode_switch: ModeSwitch,
    runtime: Mutex<Option<OwnedRuntime>>,
}

impl Default for RuntimeOwner {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeOwner {
    pub fn new() -> Self {
        Self {
            lifecycle: Arc::new(Lifecycle::new()),
            debug: Arc::new(DebugProtocol::new()),
            stats: Arc::new(StatsRegistry::new()),
            mode_switch: ModeSwitch::new(),
            runtime: Mutex::new(None),
        }
    }

    pub fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    pub fn debug(&self) -> &DebugProtocol {
        &self.debug
    }

    pub fn stats(&self) -> &StatsRegistry {
        &self.stats
    }

    pub fn mode_switch(&self) -> &ModeSwitch {
        &self.mode_switch
    }

    pub fn state(&self) -> Result<PlcState, RuntimeOwnerError> {
        self.lifecycle.state().map_err(RuntimeOwnerError::State)
    }

    pub fn set_switch_allows_run(&self, allows_run: bool) {
        self.mode_switch.set_position(if allows_run {
            SwitchPosition::Run
        } else {
            SwitchPosition::Stop
        });
    }

    pub fn start(&self, tasks: Vec<RuntimeTask>) -> Result<StartResult, RuntimeOwnerError> {
        if let Some(result) = self.claim_start()? {
            return Ok(result);
        }
        let runtime = RuntimeHandle::prepare(tasks)
            .map(|handle| OwnedRuntime::tasks(handle, Arc::clone(&self.stats)))
            .map_err(RuntimeOwnerError::Runtime);
        self.finish_start(runtime)
    }

    pub(in crate::runtime_owner) fn claim_start(
        &self,
    ) -> Result<Option<StartResult>, RuntimeOwnerError> {
        if !self.mode_switch.allows_run() {
            return Ok(Some(StartResult::RejectedSwitch));
        }
        if !self
            .lifecycle
            .claim_transition(PlcState::Running)
            .map_err(RuntimeOwnerError::State)?
        {
            return Ok(Some(StartResult::AlreadyRunning));
        }
        Ok(None)
    }

    pub(in crate::runtime_owner) fn finish_start(
        &self,
        runtime: Result<OwnedRuntime, RuntimeOwnerError>,
    ) -> Result<StartResult, RuntimeOwnerError> {
        let runtime = match runtime {
            Ok(runtime) => runtime,
            Err(error) => {
                return self.fail(error);
            }
        };
        let mut slot = match self.lock_runtime() {
            Ok(slot) => slot,
            Err(error) => return self.fail(RuntimeOwnerError::State(error)),
        };
        if slot.is_some() {
            discard(slot);
            drop(runtime);
            return self.fail(RuntimeOwnerError::Occupied);
        }
        *slot = Some(runtime);
        if let Err(error) = slot.as_mut().expect("installed runtime").prepare() {
            discard(slot);
            return self.fail(error);
        }
        let published = match self.lifecycle.publish_running_if_claimed() {
            Ok(published) => published,
            Err(error) => {
                discard(slot);
                return Err(RuntimeOwnerError::State(error));
            }
        };
        if !published {
            discard(slot);
            return Ok(StartResult::Superseded);
        }
        if let Err(error) = slot.as_mut().expect("installed runtime").activate() {
            discard(slot);
            return self.fail(error);
        }
        Ok(StartResult::Started)
    }

    pub fn stop(&self) -> Result<StopResult, RuntimeOwnerError> {
        if !self
            .lifecycle
            .claim_transition(PlcState::Stopped)
            .map_err(RuntimeOwnerError::State)?
        {
            return Ok(StopResult::NotRunning);
        }
        self.stop_claimed()
    }

    pub(crate) fn stop_claimed(&self) -> Result<StopResult, RuntimeOwnerError> {
        let mut slot = match self.lock_runtime() {
            Ok(slot) => slot,
            Err(error) => return self.fail(RuntimeOwnerError::State(error)),
        };
        let runtime = slot.take();
        drop(slot);
        if let Some(mut runtime) = runtime {
            if let Err(error) = runtime.shutdown() {
                return self.fail(error);
            }
        }
        self.lifecycle
            .publish_final_state(PlcState::Stopped)
            .map_err(RuntimeOwnerError::State)?;
        Ok(StopResult::Stopped)
    }

    pub(crate) fn fail<T>(&self, error: RuntimeOwnerError) -> Result<T, RuntimeOwnerError> {
        self.lifecycle
            .publish_final_state(PlcState::Error)
            .map_err(RuntimeOwnerError::State)?;
        Err(error)
    }

    pub(in crate::runtime_owner) fn lock_runtime(
        &self,
    ) -> Result<MutexGuard<'_, Option<OwnedRuntime>>, StateError> {
        self.runtime.lock().map_err(|_| StateError::Poisoned)
    }
}
