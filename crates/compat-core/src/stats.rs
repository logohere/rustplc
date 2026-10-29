//! OpenPLC STATS response over the command socket.

use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use crate::timing::{ScanTracker, TimingError, TimingStats};

struct TaskStats {
    name: String,
    tracker: Arc<ScanTracker>,
}

/// Registry used by the command handler and task owner.
pub struct StatsRegistry {
    tasks: Mutex<Vec<TaskStats>>,
}

impl Default for StatsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl StatsRegistry {
    pub const fn new() -> Self {
        Self {
            tasks: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn publish(
        &self,
        tasks: Vec<(String, Arc<ScanTracker>)>,
    ) -> Result<(), TimingError> {
        *self.tasks.lock().map_err(|_| TimingError::Poisoned)? = tasks
            .into_iter()
            .map(|(name, tracker)| TaskStats { name, tracker })
            .collect();
        Ok(())
    }

    /// Remove all task entries during program teardown.
    pub(crate) fn clear(&self) -> Result<(), TimingError> {
        self.tasks
            .lock()
            .map_err(|_| TimingError::Poisoned)?
            .clear();
        Ok(())
    }

    /// Format the exact `STATS:{"tasks":[...]}` socket response.
    pub fn response(&self) -> String {
        let Ok(tasks) = self.tasks.lock() else {
            return "STATS:ERROR\n".to_owned();
        };
        let mut output = String::from("STATS:{\"tasks\":[");
        for (index, task) in tasks.iter().enumerate() {
            if index > 0 {
                output.push(',');
            }
            let snapshot = task.tracker.snapshot().ok().flatten();
            append_task(&mut output, &task.name, snapshot);
        }
        output.push_str("]}\n");
        output
    }
}

fn append_task(output: &mut String, name: &str, stats: Option<TimingStats>) {
    output.push_str("{\"name\":\"");
    output.push_str(name);
    output.push_str("\",");
    match stats {
        Some(stats) => append_values(output, stats),
        None => output.push_str(
            "\"scan_count\":0,\"scan_time_min\":null,\"scan_time_max\":null,\"scan_time_avg\":null,\"cycle_time_min\":null,\"cycle_time_max\":null,\"cycle_time_avg\":null,\"cycle_latency_min\":null,\"cycle_latency_max\":null,\"cycle_latency_avg\":null,\"overruns\":0",
        ),
    }
    output.push('}');
}

fn append_values(output: &mut String, stats: TimingStats) {
    let _ = write!(
        output,
        "\"scan_count\":{},\"scan_time_min\":{},\"scan_time_max\":{},\"scan_time_avg\":{},\"cycle_time_min\":{},\"cycle_time_max\":{},\"cycle_time_avg\":{},\"cycle_latency_min\":{},\"cycle_latency_max\":{},\"cycle_latency_avg\":{},\"overruns\":{}",
        stats.scan_count,
        stats.scan_time_min,
        stats.scan_time_max,
        stats.scan_time_avg,
        stats.cycle_time_min,
        stats.cycle_time_max,
        stats.cycle_time_avg,
        stats.cycle_latency_min,
        stats.cycle_latency_max,
        stats.cycle_latency_avg,
        stats.overruns
    );
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::StatsRegistry;
    use crate::timing::ScanTracker;

    #[test]
    fn empty_registry_matches_socket_shape() {
        assert_eq!(StatsRegistry::new().response(), "STATS:{\"tasks\":[]}\n");
    }

    #[test]
    fn unstarted_task_uses_null_measurements() {
        let registry = StatsRegistry::new();
        registry
            .publish(vec![(
                "MAIN".into(),
                Arc::new(ScanTracker::new(20_000_000).unwrap()),
            )])
            .unwrap();
        assert!(registry.response().contains("\"scan_time_min\":null"));
        assert!(registry.response().contains("\"scan_count\":0"));
    }
}
