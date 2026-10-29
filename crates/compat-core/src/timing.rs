//! Monotonic scan timing statistics from OpenPLC `scan_cycle_manager.c`.

use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

const EWMA_TARGET_WINDOW_US: i64 = 2_000_000;

/// Snapshot shape used by the OpenPLC STATS response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingStats {
    pub scan_time_min: i64,
    pub scan_time_max: i64,
    pub scan_time_avg: i64,
    pub cycle_time_min: i64,
    pub cycle_time_max: i64,
    pub cycle_time_avg: i64,
    pub cycle_latency_min: i64,
    pub cycle_latency_max: i64,
    pub cycle_latency_avg: i64,
    pub scan_count: i64,
    pub overruns: i64,
}

impl Default for TimingStats {
    fn default() -> Self {
        Self {
            scan_time_min: i64::MAX,
            scan_time_max: 0,
            scan_time_avg: 0,
            cycle_time_min: i64::MAX,
            cycle_time_max: 0,
            cycle_time_avg: 0,
            cycle_latency_min: i64::MAX,
            cycle_latency_max: 0,
            cycle_latency_avg: 0,
            scan_count: 0,
            overruns: 0,
        }
    }
}

#[derive(Default)]
struct TrackerState {
    stats: TimingStats,
    scan_sum: i64,
    cycle_sum: i64,
    latency_sum: i64,
    expected_start_us: u64,
    last_start_us: u64,
}

/// Failure to access timing state after another thread poisoned its lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingError {
    Poisoned,
    InvalidInterval,
}

/// One task's monotonic scan/cycle/latency tracker.
pub struct ScanTracker {
    clock: Instant,
    interval_us: u64,
    avg_window: i64,
    state: Mutex<TrackerState>,
}

impl ScanTracker {
    /// Create a tracker for a task interval in nanoseconds.
    pub fn new(interval_ns: i64) -> Result<Self, TimingError> {
        if interval_ns < 0 {
            return Err(TimingError::InvalidInterval);
        }
        let interval_us = (interval_ns / 1_000) as u64;
        let avg_window = if interval_us == 0 {
            1
        } else {
            (EWMA_TARGET_WINDOW_US / interval_us as i64).max(1)
        };
        Ok(Self {
            clock: Instant::now(),
            interval_us,
            avg_window,
            state: Mutex::new(TrackerState::default()),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, TrackerState>, TimingError> {
        self.state.lock().map_err(|_| TimingError::Poisoned)
    }

    fn now_us(&self) -> u64 {
        self.clock.elapsed().as_micros() as u64
    }

    fn ewma(sum: &mut i64, sample: i64, window: i64) {
        *sum += sample - *sum / window;
    }

    fn record_start(&self, state: &mut TrackerState, now_us: u64) {
        if state.stats.scan_count == 0 {
            state.expected_start_us = now_us.saturating_add(self.interval_us);
            state.last_start_us = now_us;
            state.stats.scan_count = 1;
            return;
        }
        let cycle = now_us.saturating_sub(state.last_start_us) as i64;
        let latency = now_us as i128 - state.expected_start_us as i128;
        let latency = latency.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
        state.stats.cycle_time_min = state.stats.cycle_time_min.min(cycle);
        state.stats.cycle_time_max = state.stats.cycle_time_max.max(cycle);
        state.stats.cycle_latency_min = state.stats.cycle_latency_min.min(latency);
        state.stats.cycle_latency_max = state.stats.cycle_latency_max.max(latency);
        Self::ewma(&mut state.cycle_sum, cycle, self.avg_window);
        Self::ewma(&mut state.latency_sum, latency, self.avg_window);
        state.last_start_us = now_us;
        state.expected_start_us = state.expected_start_us.saturating_add(self.interval_us);
        state.stats.scan_count += 1;
    }

    fn record_end(&self, state: &mut TrackerState, now_us: u64) {
        if state.stats.scan_count == 0 {
            return;
        }
        let scan = now_us.saturating_sub(state.last_start_us) as i64;
        state.stats.scan_time_min = state.stats.scan_time_min.min(scan);
        state.stats.scan_time_max = state.stats.scan_time_max.max(scan);
        Self::ewma(&mut state.scan_sum, scan, self.avg_window);
        if now_us > state.expected_start_us {
            state.stats.overruns += 1;
        }
    }

    /// Mark the beginning of one scan on the monotonic clock.
    pub fn start(&self) -> Result<(), TimingError> {
        let now_us = self.now_us();
        let mut state = self.lock()?;
        self.record_start(&mut state, now_us);
        Ok(())
    }

    /// Mark the end of one scan on the monotonic clock.
    pub fn end(&self) -> Result<(), TimingError> {
        let now_us = self.now_us();
        let mut state = self.lock()?;
        self.record_end(&mut state, now_us);
        Ok(())
    }

    /// Copy a consistent snapshot; `None` means no scan has started.
    pub fn snapshot(&self) -> Result<Option<TimingStats>, TimingError> {
        let mut state = self.lock()?;
        state.stats.scan_time_avg = state.scan_sum / self.avg_window;
        state.stats.cycle_time_avg = state.cycle_sum / self.avg_window;
        state.stats.cycle_latency_avg = state.latency_sum / self.avg_window;
        Ok((state.stats.scan_count > 0).then_some(state.stats))
    }
}

#[cfg(test)]
mod tests {
    use super::{ScanTracker, TimingError, TimingStats};

    #[test]
    fn first_scan_is_valid_but_has_no_cycle_sample() {
        let tracker = ScanTracker::new(20_000_000).unwrap();
        tracker.start().unwrap();
        let snapshot = tracker.snapshot().unwrap().unwrap();
        assert_eq!(snapshot.scan_count, 1);
        assert_eq!(snapshot.cycle_time_min, i64::MAX);
    }

    #[test]
    fn rejects_negative_intervals_and_starts_empty() {
        assert!(matches!(
            ScanTracker::new(-1),
            Err(TimingError::InvalidInterval)
        ));
        let tracker = ScanTracker::new(1_000).unwrap();
        assert_eq!(tracker.snapshot().unwrap(), None);
        assert_eq!(TimingStats::default().overruns, 0);
    }
}
