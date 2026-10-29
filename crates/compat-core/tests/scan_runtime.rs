use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use open_rust_plc_compat_core::configuration::ConfigurationError;
use open_rust_plc_compat_core::program_api::ProgramScanError;
use open_rust_plc_compat_core::scan_runtime::{
    RuntimeError, RuntimeHandle, RuntimeTask, RuntimeTaskFault,
};
use open_rust_plc_compat_core::scheduler::TaskSpec;

#[test]
fn dispatches_tasks_and_records_scan_stats() {
    let scans = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&scans);
    let handle = RuntimeHandle::start(vec![RuntimeTask {
        name: "MAIN".into(),
        spec: TaskSpec {
            interval_ns: 1_000_000,
            priority: 1,
        },
        body: Box::new(move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }),
    }])
    .unwrap();
    let tracker = Arc::clone(&handle.trackers[0]);
    thread::sleep(Duration::from_millis(12));
    handle.stop();
    assert!(scans.load(Ordering::Relaxed) > 0);
    assert!(tracker.snapshot().unwrap().unwrap().scan_count > 0);
}

#[test]
fn panic_in_one_task_does_not_stop_another_task() {
    let panicked = Arc::new(AtomicBool::new(false));
    let healthy = Arc::new(AtomicUsize::new(0));
    let panic_flag = Arc::clone(&panicked);
    let healthy_counter = Arc::clone(&healthy);
    let handle = RuntimeHandle::start(vec![
        RuntimeTask {
            name: "FAULT".into(),
            spec: TaskSpec {
                interval_ns: 1_000_000,
                priority: 1,
            },
            body: Box::new(move |_| {
                if !panic_flag.swap(true, Ordering::Relaxed) {
                    panic!("test task fault");
                }
                Ok(())
            }),
        },
        RuntimeTask {
            name: "HEALTHY".into(),
            spec: TaskSpec {
                interval_ns: 1_000_000,
                priority: 1,
            },
            body: Box::new(move |_| {
                healthy_counter.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        },
    ])
    .unwrap();
    thread::sleep(Duration::from_millis(8));
    handle.stop();
    assert!(panicked.load(Ordering::Relaxed));
    assert!(healthy.load(Ordering::Relaxed) > 0);
}

#[test]
fn rejects_empty_task_sets() {
    assert!(matches!(
        RuntimeHandle::start(Vec::new()),
        Err(RuntimeError::Schedule(_))
    ));
}

#[test]
fn dropping_handle_stops_all_scan_threads() {
    let scans = Arc::new(AtomicUsize::new(0));
    let handle = RuntimeHandle::start(vec![RuntimeTask {
        name: "MAIN".into(),
        spec: TaskSpec {
            interval_ns: 1_000_000,
            priority: 1,
        },
        body: Box::new({
            let scans = Arc::clone(&scans);
            move |_| {
                scans.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        }),
    }])
    .unwrap();
    thread::sleep(Duration::from_millis(4));
    drop(handle);
    let stopped_at = scans.load(Ordering::Relaxed);
    thread::sleep(Duration::from_millis(4));
    assert_eq!(scans.load(Ordering::Relaxed), stopped_at);
}

#[test]
fn records_typed_task_fault_without_stopping_peer() {
    let expected = RuntimeTaskFault::Program(ProgramScanError::Configuration(
        ConfigurationError::CppException,
    ));
    let returned = expected.clone();
    let (fault_sent, fault_received) = mpsc::channel();
    let (healthy_sent, healthy_received) = mpsc::channel();
    let spec = TaskSpec {
        interval_ns: 1_000_000,
        priority: 1,
    };
    let handle = RuntimeHandle::start(vec![
        RuntimeTask {
            name: "FAULT".into(),
            spec,
            body: Box::new(move |_| {
                fault_sent.send(()).unwrap();
                Err(returned.clone())
            }),
        },
        RuntimeTask {
            name: "HEALTHY".into(),
            spec,
            body: Box::new(move |_| {
                healthy_sent.send(()).unwrap();
                Ok(())
            }),
        },
    ])
    .unwrap();
    let fault = Arc::clone(&handle.faults[0]);
    let healthy_fault = Arc::clone(&handle.faults[1]);
    fault_received.recv_timeout(Duration::from_secs(1)).unwrap();
    healthy_received
        .recv_timeout(Duration::from_secs(1))
        .unwrap();
    handle.stop();
    assert_eq!(fault.get(), Some(&expected));
    assert_eq!(healthy_fault.get(), None);
}
