use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use open_rust_plc_compat_core::configuration::ConfigurationError;
use open_rust_plc_compat_core::lifecycle::PlcState;
use open_rust_plc_compat_core::program_api::ProgramScanError;
use open_rust_plc_compat_core::runtime_owner::{
    RuntimeOwner, RuntimeOwnerError, StartResult, StopResult,
};
use open_rust_plc_compat_core::scan_runtime::{RuntimeTask, RuntimeTaskFault};
use open_rust_plc_compat_core::scheduler::TaskSpec;

fn task(counter: &Arc<AtomicUsize>) -> RuntimeTask {
    let counter = Arc::clone(counter);
    RuntimeTask {
        name: "MAIN".into(),
        spec: TaskSpec {
            interval_ns: 1_000_000,
            priority: 1,
        },
        body: Box::new(move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }),
    }
}

#[test]
fn owns_scan_start_and_stop_with_lifecycle_state() {
    let owner = RuntimeOwner::new();
    let scans = Arc::new(AtomicUsize::new(0));
    assert_eq!(
        owner.start(vec![task(&scans)]).unwrap(),
        StartResult::Started
    );
    wait_for_live_stats(&owner);
    assert!(scans.load(Ordering::Relaxed) > 0);
    assert_eq!(owner.stop().unwrap(), StopResult::Stopped);
    assert_eq!(owner.stats().response(), "STATS:{\"tasks\":[]}\n");
    assert_eq!(owner.state().unwrap().status_response(), "STATUS:STOPPED\n");
    assert_eq!(owner.stop().unwrap(), StopResult::NotRunning);
}

fn wait_for_live_stats(owner: &RuntimeOwner) {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let response = owner.stats().response();
        if response.contains("\"name\":\"MAIN\"") && !response.contains("\"scan_count\":0") {
            return;
        }
        assert!(Instant::now() < deadline, "owned STATS timed out");
        std::thread::yield_now();
    }
}

#[test]
fn stats_read_is_safe_while_stop_joins_a_scan() {
    let owner = Arc::new(RuntimeOwner::new());
    let (entered, scan_entered) = mpsc::channel();
    let (release, scan_release) = mpsc::channel();
    owner
        .start(vec![RuntimeTask {
            name: "BLOCKED".into(),
            spec: TaskSpec {
                interval_ns: 1_000_000,
                priority: 1,
            },
            body: Box::new(move |_| {
                entered.send(()).unwrap();
                scan_release.recv_timeout(Duration::from_secs(1)).unwrap();
                Ok(())
            }),
        }])
        .unwrap();
    scan_entered.recv_timeout(Duration::from_secs(1)).unwrap();
    let stopping = Arc::clone(&owner);
    let stopped = std::thread::spawn(move || stopping.stop());
    wait_for_state(&owner, PlcState::TransitioningToStop);
    assert!(owner.stats().response().contains("\"name\":\"BLOCKED\""));
    release.send(()).unwrap();
    assert_eq!(stopped.join().unwrap().unwrap(), StopResult::Stopped);
    assert_eq!(owner.stats().response(), "STATS:{\"tasks\":[]}\n");
}

fn wait_for_state(owner: &RuntimeOwner, expected: PlcState) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while owner.state().unwrap() != expected {
        assert!(Instant::now() < deadline, "owner state timed out");
        std::thread::yield_now();
    }
}

#[test]
fn applies_switch_gate_and_lands_runtime_errors() {
    let owner = RuntimeOwner::new();
    owner.set_switch_allows_run(false);
    assert_eq!(
        owner.start(Vec::new()).unwrap(),
        StartResult::RejectedSwitch
    );
    owner.set_switch_allows_run(true);
    assert!(matches!(
        owner.start(Vec::new()),
        Err(RuntimeOwnerError::Runtime(_))
    ));
    assert_eq!(owner.stats().response(), "STATS:{\"tasks\":[]}\n");
    assert_eq!(owner.state().unwrap().status_response(), "STATUS:ERROR\n");
}

#[test]
fn task_fault_stays_local_and_owner_remains_running() {
    let owner = RuntimeOwner::new();
    let (faulted, fault_seen) = mpsc::channel();
    let (healthy, healthy_seen) = mpsc::channel();
    let spec = TaskSpec {
        interval_ns: 1_000_000,
        priority: 1,
    };
    owner
        .start(vec![
            RuntimeTask {
                name: "FAULT".into(),
                spec,
                body: Box::new(move |_| {
                    faulted.send(()).unwrap();
                    Err(RuntimeTaskFault::Program(ProgramScanError::Configuration(
                        ConfigurationError::CppException,
                    )))
                }),
            },
            RuntimeTask {
                name: "HEALTHY".into(),
                spec,
                body: Box::new(move |_| {
                    healthy.send(()).unwrap();
                    Ok(())
                }),
            },
        ])
        .unwrap();
    fault_seen.recv_timeout(Duration::from_secs(1)).unwrap();
    healthy_seen.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(owner.state().unwrap().status_response(), "STATUS:RUNNING\n");
    assert_eq!(owner.stop().unwrap(), StopResult::Stopped);
}
