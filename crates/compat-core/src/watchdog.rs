//! OpenPLC Runtime v4 watchdog decision policy.

use crate::lifecycle::{PlcState, TRANSITION_STUCK_TIMEOUT_MS};

/// The upstream watchdog evaluates once every two seconds.
pub const WATCHDOG_TICK_SECS: u64 = 2;

/// A watchdog event that the runtime owner must apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogAction {
    Continue,
    ResetHeartbeat,
    ForceError(WatchdogFault),
}

/// Why the upstream watchdog forces the PLC into ERROR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogFault {
    TransitionStuck,
    NoHeartbeat,
}

/// Local watchdog state. The heartbeat is the `time(nullptr)` value published
/// by each running scan in the pinned runtime; it is not a scan counter.
#[derive(Debug, Clone, Copy)]
pub struct Watchdog {
    last_heartbeat: i64,
    transitioning_ticks: u32,
}

impl Watchdog {
    pub const fn new(initial_heartbeat: i64) -> Self {
        Self {
            last_heartbeat: initial_heartbeat,
            transitioning_ticks: 0,
        }
    }

    /// Apply one watchdog tick and return the side effect required by the
    /// runtime owner. This function does not mutate PLC lifecycle state.
    pub fn tick(&mut self, state: PlcState, heartbeat: i64) -> WatchdogAction {
        if matches!(
            state,
            PlcState::TransitioningToRun | PlcState::TransitioningToStop
        ) {
            self.transitioning_ticks = self.transitioning_ticks.saturating_add(1);
            let elapsed = u64::from(self.transitioning_ticks) * WATCHDOG_TICK_SECS;
            if elapsed > TRANSITION_STUCK_TIMEOUT_MS / 1_000 {
                self.transitioning_ticks = 0;
                return WatchdogAction::ForceError(WatchdogFault::TransitionStuck);
            }
            return WatchdogAction::Continue;
        }

        self.transitioning_ticks = 0;
        if state != PlcState::Running {
            if state == PlcState::Error {
                self.last_heartbeat = 0;
                return WatchdogAction::ResetHeartbeat;
            }
            return WatchdogAction::Continue;
        }

        if heartbeat == self.last_heartbeat {
            return WatchdogAction::ForceError(WatchdogFault::NoHeartbeat);
        }
        self.last_heartbeat = heartbeat;
        WatchdogAction::Continue
    }

    pub const fn transitioning_ticks(&self) -> u32 {
        self.transitioning_ticks
    }
}

#[cfg(test)]
mod tests {
    use super::{Watchdog, WatchdogAction, WatchdogFault, WATCHDOG_TICK_SECS};
    use crate::lifecycle::PlcState;

    #[test]
    fn forces_error_after_stuck_transition_bound() {
        let mut watchdog = Watchdog::new(10);
        for _ in 0..60 {
            assert_eq!(
                watchdog.tick(PlcState::TransitioningToRun, 10),
                WatchdogAction::Continue
            );
        }
        assert_eq!(watchdog.transitioning_ticks(), 60);
        assert_eq!(
            watchdog.tick(PlcState::TransitioningToRun, 10),
            WatchdogAction::ForceError(WatchdogFault::TransitionStuck)
        );
        assert_eq!(watchdog.transitioning_ticks(), 0);
        assert_eq!(WATCHDOG_TICK_SECS, 2);
    }

    #[test]
    fn requires_a_new_heartbeat_while_running() {
        let mut watchdog = Watchdog::new(10);
        assert_eq!(
            watchdog.tick(PlcState::Running, 10),
            WatchdogAction::ForceError(WatchdogFault::NoHeartbeat)
        );
        assert_eq!(
            watchdog.tick(PlcState::Running, 11),
            WatchdogAction::Continue
        );
        assert_eq!(
            watchdog.tick(PlcState::Running, 11),
            WatchdogAction::ForceError(WatchdogFault::NoHeartbeat)
        );
    }

    #[test]
    fn error_resets_heartbeat_tracking() {
        let mut watchdog = Watchdog::new(10);
        assert_eq!(
            watchdog.tick(PlcState::Error, 44),
            WatchdogAction::ResetHeartbeat
        );
        assert_eq!(
            watchdog.tick(PlcState::Running, 0),
            WatchdogAction::ForceError(WatchdogFault::NoHeartbeat)
        );
        assert_eq!(
            watchdog.tick(PlcState::Running, 1),
            WatchdogAction::Continue
        );
    }
}
