/// OpenPLC's zero-valued task interval fallback.
pub const DEFAULT_TASK_INTERVAL_NS: u64 = 20_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerError {
    NoTasks,
    NegativeInterval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskSpec {
    pub interval_ns: i64,
    pub priority: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskSchedule {
    pub interval_ns: u64,
    pub divisor: u64,
    pub priority: i32,
}

impl TaskSchedule {
    pub const fn due(self, master_tick: u64) -> bool {
        master_tick.is_multiple_of(self.divisor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub base_tick_ns: u64,
    pub tasks: Vec<TaskSchedule>,
    pub fastest_task: usize,
}

pub fn build_schedule(specs: &[TaskSpec]) -> Result<Schedule, SchedulerError> {
    if specs.is_empty() {
        return Err(SchedulerError::NoTasks);
    }
    let mut base = 0;
    let mut intervals = Vec::with_capacity(specs.len());
    for spec in specs {
        if spec.interval_ns < 0 {
            return Err(SchedulerError::NegativeInterval);
        }
        let interval = match spec.interval_ns {
            0 => DEFAULT_TASK_INTERVAL_NS,
            value => value as u64,
        };
        base = if base == 0 {
            interval
        } else {
            gcd(base, interval)
        };
        intervals.push(interval);
    }

    let tasks = intervals
        .iter()
        .zip(specs)
        .map(|(&interval, spec)| TaskSchedule {
            interval_ns: interval,
            divisor: (interval / base).max(1),
            priority: spec.priority,
        })
        .collect::<Vec<_>>();
    let fastest_task = tasks
        .iter()
        .enumerate()
        .skip(1)
        .fold(0, |fastest, (index, candidate)| {
            let current = tasks[fastest];
            if candidate.interval_ns < current.interval_ns
                || (candidate.interval_ns == current.interval_ns
                    && candidate.priority > current.priority)
            {
                index
            } else {
                fastest
            }
        });
    Ok(Schedule {
        base_tick_ns: base,
        tasks,
        fastest_task,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseDecision {
    Skip,
    Release,
    Overrun,
}

pub const fn release_decision(
    alive: bool,
    due: bool,
    released: u64,
    completed: u64,
) -> ReleaseDecision {
    if !alive || !due {
        ReleaseDecision::Skip
    } else if released == completed {
        ReleaseDecision::Release
    } else {
        ReleaseDecision::Overrun
    }
}

const fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[cfg(test)]
mod tests {
    use super::{
        build_schedule, release_decision, ReleaseDecision, SchedulerError, TaskSpec,
        DEFAULT_TASK_INTERVAL_NS,
    };

    #[test]
    fn builds_gcd_grid_and_fastest_priority() {
        let schedule = build_schedule(&[
            TaskSpec {
                interval_ns: 20_000_000,
                priority: 5,
            },
            TaskSpec {
                interval_ns: 40_000_000,
                priority: 9,
            },
            TaskSpec {
                interval_ns: 20_000_000,
                priority: 7,
            },
        ])
        .unwrap();
        assert_eq!(schedule.base_tick_ns, 20_000_000);
        assert_eq!(schedule.tasks[1].divisor, 2);
        assert_eq!(schedule.fastest_task, 2);
        assert!(schedule.tasks[1].due(2));
        assert!(!schedule.tasks[1].due(1));
    }

    #[test]
    fn preserves_zero_fallback_and_rejects_invalid_sets() {
        assert_eq!(
            build_schedule(&[TaskSpec {
                interval_ns: 0,
                priority: 1,
            }])
            .unwrap()
            .base_tick_ns,
            DEFAULT_TASK_INTERVAL_NS
        );
        assert_eq!(build_schedule(&[]), Err(SchedulerError::NoTasks));
        assert_eq!(
            build_schedule(&[TaskSpec {
                interval_ns: -1,
                priority: 1,
            }]),
            Err(SchedulerError::NegativeInterval)
        );
    }

    #[test]
    fn skips_dead_or_not_due_and_marks_outstanding_as_overrun() {
        assert_eq!(release_decision(true, true, 4, 4), ReleaseDecision::Release);
        assert_eq!(release_decision(true, true, 5, 4), ReleaseDecision::Overrun);
        assert_eq!(release_decision(false, true, 0, 0), ReleaseDecision::Skip);
    }
}
