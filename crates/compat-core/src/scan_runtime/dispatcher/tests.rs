use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::thread;
use std::time::Duration;

use crate::located_image::LocatedImageError;

use super::{run, FrameBoundary, FrameState};
use crate::scan_runtime::DispatchTask;

#[test]
fn brackets_release_with_quiescent_frame_copies() {
    let stop = Arc::new(AtomicBool::new(false));
    let (release, receive) = mpsc::sync_channel(1);
    let (finished, completions) = mpsc::channel();
    let (events, observed) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let completed = Arc::new(AtomicU64::new(0));
    let worker_completed = Arc::clone(&completed);
    let worker_events = events.clone();
    let worker = thread::spawn(move || {
        receive.recv().unwrap();
        worker_events.send("task").unwrap();
        resumed.recv().unwrap();
        worker_completed.fetch_add(1, Ordering::Release);
        finished.send(()).unwrap();
    });
    let task = DispatchTask {
        divisor: 1,
        released: Arc::new(AtomicU64::new(0)),
        completed,
        alive: Arc::new(AtomicBool::new(true)),
        release,
    };
    let end_stop = Arc::clone(&stop);
    let start_events = events.clone();
    let boundary = FrameBoundary {
        start: Box::new(move || {
            start_events.send("in").unwrap();
            Ok(())
        }),
        end: Box::new(move || {
            events.send("out").unwrap();
            end_stop.store(true, Ordering::Release);
            Ok(())
        }),
    };
    let fault = Arc::new(OnceLock::new());
    let dispatcher = thread::spawn({
        let stop = Arc::clone(&stop);
        let fault = Arc::clone(&fault);
        move || {
            run(
                vec![task],
                stop,
                1_000_000_000,
                Some(boundary),
                completions,
                (fault, None),
            )
        }
    });
    assert_eq!(observed.recv_timeout(Duration::from_secs(1)).unwrap(), "in");
    assert_eq!(
        observed.recv_timeout(Duration::from_secs(1)).unwrap(),
        "task"
    );
    assert!(observed.try_recv().is_err());
    resume.send(()).unwrap();
    assert_eq!(
        observed.recv_timeout(Duration::from_secs(1)).unwrap(),
        "out"
    );
    dispatcher.join().unwrap();
    worker.join().unwrap();
    assert!(fault.get().is_none());
}

#[test]
fn overrun_frame_is_not_copied_out() {
    let mut frame = FrameState::default();
    frame.release();
    frame.retire_overrun();
    frame.complete(1);
    assert!(!frame.take_end());
    assert_eq!(frame.in_flight, 0);
}

#[test]
fn records_frame_boundary_failure() {
    let stop = Arc::new(AtomicBool::new(false));
    let (release, _) = mpsc::sync_channel(1);
    let (_, completions) = mpsc::channel();
    let fault = Arc::new(OnceLock::new());
    let (notified, notification) = mpsc::channel();
    let hook_stop = Arc::clone(&stop);
    let task = DispatchTask {
        divisor: 1,
        released: Arc::new(AtomicU64::new(0)),
        completed: Arc::new(AtomicU64::new(0)),
        alive: Arc::new(AtomicBool::new(true)),
        release,
    };
    let boundary = FrameBoundary {
        start: Box::new(|| Err(LocatedImageError::Poisoned)),
        end: Box::new(|| Ok(())),
    };
    run(
        vec![task],
        Arc::clone(&stop),
        1,
        Some(boundary),
        completions,
        (
            Arc::clone(&fault),
            Some(Arc::new(move |error| {
                notified
                    .send((error, hook_stop.load(Ordering::Acquire)))
                    .unwrap()
            })),
        ),
    );
    assert!(stop.load(Ordering::Acquire));
    assert_eq!(fault.get(), Some(&LocatedImageError::Poisoned));
    assert_eq!(
        notification.try_recv(),
        Ok((LocatedImageError::Poisoned, false))
    );
}
