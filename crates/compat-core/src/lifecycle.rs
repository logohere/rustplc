//! OpenPLC Runtime v4 lifecycle state contract.

use std::sync::{Mutex, MutexGuard};

/// Shared with the transition worker and watchdog in the pinned runtime.
pub const TRANSITION_LANDING_TIMEOUT_MS: u64 = 90_000;
pub const TRANSITION_STUCK_TIMEOUT_MS: u64 = TRANSITION_LANDING_TIMEOUT_MS + 30_000;

/// Wire-visible runtime states. The transition values are appended to preserve
/// OpenPLC's first five numeric values.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlcState {
    Init = 0,
    Running = 1,
    Stopped = 2,
    Error = 3,
    Empty = 4,
    TransitioningToRun = 5,
    TransitioningToStop = 6,
}

impl PlcState {
    /// The status response exposed by the OpenPLC command socket.
    pub const fn status_response(self) -> &'static str {
        match self {
            Self::Init => "STATUS:INIT\n",
            Self::Running => "STATUS:RUNNING\n",
            Self::Stopped => "STATUS:STOPPED\n",
            Self::Error => "STATUS:ERROR\n",
            Self::Empty => "STATUS:EMPTY\n",
            Self::TransitioningToRun | Self::TransitioningToStop => "STATUS:TRANSITIONING\n",
        }
    }

    const fn requestable(self) -> bool {
        matches!(self, Self::Running | Self::Stopped)
    }

    const fn transition_for(self) -> Self {
        match self {
            Self::Running => Self::TransitioningToRun,
            Self::Stopped => Self::TransitioningToStop,
            _ => self,
        }
    }

    const fn transitioning(self) -> bool {
        matches!(self, Self::TransitioningToRun | Self::TransitioningToStop)
    }
}

/// Failure to access lifecycle state after another thread poisoned its lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateError {
    Poisoned,
}

/// Thread-safe state arbiter matching `plc_claim_transition` semantics.
pub struct Lifecycle {
    state: Mutex<PlcState>,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl Lifecycle {
    /// OpenPLC initializes the state manager in STOPPED.
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(PlcState::Stopped),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, PlcState>, StateError> {
        self.state.lock().map_err(|_| StateError::Poisoned)
    }

    /// Return the current internal state.
    pub fn state(&self) -> Result<PlcState, StateError> {
        Ok(*self.lock()?)
    }

    /// Claim a RUNNING or STOPPED transition; overlapping and redundant claims drop.
    pub fn claim_transition(&self, target: PlcState) -> Result<bool, StateError> {
        if !target.requestable() {
            return Ok(false);
        }
        let mut state = self.lock()?;
        if state.transitioning() || *state == target {
            return Ok(false);
        }
        *state = target.transition_for();
        Ok(true)
    }

    /// Publish a completed state, retaining ERROR over a STOPPED landing.
    pub fn publish_final_state(&self, final_state: PlcState) -> Result<(), StateError> {
        let mut state = self.lock()?;
        if *state == PlcState::Error && final_state == PlcState::Stopped {
            return Ok(());
        }
        *state = final_state;
        Ok(())
    }

    /// Publish RUNNING only while this lifecycle still owns a start transition.
    pub fn publish_running_if_claimed(&self) -> Result<bool, StateError> {
        let mut state = self.lock()?;
        if *state != PlcState::TransitioningToRun {
            return Ok(false);
        }
        *state = PlcState::Running;
        Ok(true)
    }

    /// Report whether either transition direction is in flight.
    pub fn is_transitioning(&self) -> Result<bool, StateError> {
        Ok(self.lock()?.transitioning())
    }
}

#[cfg(test)]
mod tests {
    use super::{Lifecycle, PlcState};

    #[test]
    fn preserves_wire_values_and_status_text() {
        assert_eq!(PlcState::Init as u8, 0);
        assert_eq!(PlcState::Empty as u8, 4);
        assert_eq!(PlcState::TransitioningToStop as u8, 6);
        assert_eq!(
            PlcState::TransitioningToRun.status_response(),
            "STATUS:TRANSITIONING\n"
        );
    }

    #[test]
    fn drops_redundant_and_overlapping_claims() {
        let lifecycle = Lifecycle::new();
        assert!(!lifecycle.claim_transition(PlcState::Stopped).unwrap());
        assert!(lifecycle.claim_transition(PlcState::Running).unwrap());
        assert!(!lifecycle.claim_transition(PlcState::Stopped).unwrap());
        assert_eq!(lifecycle.state().unwrap(), PlcState::TransitioningToRun);
    }

    #[test]
    fn protects_landing_and_error() {
        let lifecycle = Lifecycle::new();
        assert!(!lifecycle.publish_running_if_claimed().unwrap());
        assert!(lifecycle.claim_transition(PlcState::Running).unwrap());
        assert!(lifecycle.publish_running_if_claimed().unwrap());
        lifecycle.publish_final_state(PlcState::Error).unwrap();
        lifecycle.publish_final_state(PlcState::Stopped).unwrap();
        assert_eq!(lifecycle.state().unwrap(), PlcState::Error);
    }
}
