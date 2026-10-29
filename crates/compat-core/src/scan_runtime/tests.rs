use std::sync::mpsc;
use std::time::Duration;

use crate::scheduler::TaskSpec;

use super::{RuntimeHandle, RuntimeTask};

#[test]
fn prepared_tasks_remain_parked_until_activation() {
    let (scanned, observed) = mpsc::channel();
    let mut handle = RuntimeHandle::prepare(vec![RuntimeTask {
        name: "MAIN".into(),
        spec: TaskSpec {
            interval_ns: 1_000_000,
            priority: 1,
        },
        body: Box::new(move |_| {
            scanned.send(()).unwrap();
            Ok(())
        }),
    }])
    .unwrap();
    assert!(observed.try_recv().is_err());
    handle.activate().unwrap();
    observed.recv_timeout(Duration::from_secs(1)).unwrap();
    handle.stop();
}
