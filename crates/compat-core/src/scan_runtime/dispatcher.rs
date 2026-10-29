use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::located_image::LocatedImageError;
use crate::scheduler::{release_decision, ReleaseDecision};

use super::{DispatchTask, FrameFaultHook, FrameFaultReport};

mod boundary;
pub(super) use boundary::FrameBoundary;

#[derive(Default)]
struct FrameState {
    in_flight: u64,
    end_pending: bool,
}

impl FrameState {
    fn complete(&mut self, count: u64) {
        self.in_flight = self.in_flight.saturating_sub(count);
    }

    fn release(&mut self) {
        self.in_flight += 1;
        self.end_pending = true;
    }

    fn retire_overrun(&mut self) {
        self.end_pending = false;
    }

    fn take_end(&mut self) -> bool {
        let ready = self.end_pending && self.in_flight == 0;
        self.end_pending &= !ready;
        ready
    }
}

pub(super) fn run(
    tasks: Vec<DispatchTask>,
    stop: Arc<AtomicBool>,
    tick_ns: u64,
    mut boundary: Option<FrameBoundary>,
    completed: mpsc::Receiver<()>,
    fault: FrameFaultReport,
) {
    let tick = Duration::from_nanos(tick_ns.max(1));
    let mut deadline = Instant::now();
    let mut master_tick = 0_u64;
    let mut frame = FrameState::default();
    while !stop.load(Ordering::Acquire) {
        frame.complete(completed.try_iter().count() as u64);
        let any_due = tasks.iter().any(|task| due(task, master_tick));
        if any_due {
            if let Err(error) = open_frame(
                &tasks,
                master_tick,
                master_tick.saturating_mul(tick_ns),
                &mut frame,
                boundary.as_mut(),
            ) {
                fail(&tasks, &stop, &fault.0, fault.1.as_ref(), error);
                return;
            }
        }
        master_tick = master_tick.wrapping_add(1);
        deadline += tick;
        if let Err(error) = wait_frame(&stop, deadline, &mut frame, boundary.as_mut(), &completed) {
            fail(&tasks, &stop, &fault.0, fault.1.as_ref(), error);
            return;
        }
    }
}

fn open_frame(
    tasks: &[DispatchTask],
    tick: u64,
    time_ns: u64,
    frame: &mut FrameState,
    boundary: Option<&mut FrameBoundary>,
) -> Result<(), LocatedImageError> {
    frame.retire_overrun();
    if frame.in_flight == 0 {
        call(boundary, true)?;
    }
    for task in tasks.iter().filter(|task| due(task, tick)) {
        if release_decision(
            true,
            true,
            task.released.load(Ordering::Relaxed),
            task.completed.load(Ordering::Acquire),
        ) != ReleaseDecision::Release
        {
            continue;
        }
        task.released.fetch_add(1, Ordering::Relaxed);
        if task.release.try_send(time_ns).is_ok() {
            frame.release();
        } else {
            task.released.fetch_sub(1, Ordering::Relaxed);
        }
    }
    Ok(())
}

fn wait_frame(
    stop: &AtomicBool,
    deadline: Instant,
    frame: &mut FrameState,
    mut boundary: Option<&mut FrameBoundary>,
    completed: &mpsc::Receiver<()>,
) -> Result<(), LocatedImageError> {
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        if frame.take_end() {
            call(boundary.as_deref_mut(), false)?;
            continue;
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Ok(());
        };
        match completed.recv_timeout(remaining) {
            Ok(()) => frame.complete(1),
            Err(mpsc::RecvTimeoutError::Timeout) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                thread::sleep(remaining);
                return Ok(());
            }
        }
    }
}

fn due(task: &DispatchTask, tick: u64) -> bool {
    task.alive.load(Ordering::Acquire) && tick.is_multiple_of(task.divisor)
}

fn call(boundary: Option<&mut FrameBoundary>, start: bool) -> Result<(), LocatedImageError> {
    boundary.map_or(Ok(()), |boundary| {
        if start {
            (boundary.start)()
        } else {
            (boundary.end)()
        }
    })
}

fn fail(
    tasks: &[DispatchTask],
    stop: &AtomicBool,
    fault: &OnceLock<LocatedImageError>,
    fault_hook: Option<&FrameFaultHook>,
    error: LocatedImageError,
) {
    let _ = fault.set(error);
    if let Some(hook) = fault_hook {
        hook(error);
    }
    stop.store(true, Ordering::Release);
    for task in tasks {
        let _ = task.release.try_send(0);
    }
}

#[cfg(test)]
mod tests;
