use std::sync::mpsc;
use std::time::Duration;

use open_rust_plc_compat_core::configuration::ConfigurationError;
use open_rust_plc_compat_core::program_api::ProgramScanError;
use open_rust_plc_compat_core::runtime_owner::{RuntimeOwner, StartResult};
use open_rust_plc_compat_core::scan_runtime::{RuntimeTask, RuntimeTaskFault};
use open_rust_plc_compat_core::scheduler::TaskSpec;

fn tasks(faulted: mpsc::Sender<()>, healthy: mpsc::Sender<()>) -> Vec<RuntimeTask> {
    let spec = TaskSpec {
        interval_ns: 20_000_000,
        priority: 1,
    };
    vec![
        RuntimeTask {
            name: "FAULT_TASK".into(),
            spec,
            body: Box::new(move |_| {
                faulted.send(()).unwrap();
                Err(RuntimeTaskFault::Program(ProgramScanError::Configuration(
                    ConfigurationError::CppException,
                )))
            }),
        },
        RuntimeTask {
            name: "HEALTHY_TASK".into(),
            spec: TaskSpec {
                priority: 2,
                ..spec
            },
            body: Box::new(move |_| {
                healthy.send(()).unwrap();
                Ok(())
            }),
        },
    ]
}

fn main() {
    let owner = RuntimeOwner::new();
    let (faulted, fault_seen) = mpsc::channel();
    let (healthy, healthy_seen) = mpsc::channel();
    assert_eq!(
        owner.start(tasks(faulted, healthy)).unwrap(),
        StartResult::Started
    );
    fault_seen.recv_timeout(Duration::from_secs(1)).unwrap();
    healthy_seen.recv_timeout(Duration::from_secs(1)).unwrap();
    let status = owner.state().unwrap().status_response();
    let state = status.trim().strip_prefix("STATUS:").unwrap();
    println!("{{\"state\":\"{state}\",\"fault_task\":\"isolated\",\"healthy_task\":\"scanning\"}}");
    owner.stop().unwrap();
}
