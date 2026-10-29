use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use open_rust_plc_compat_core::program_api::ProgramApi;
use open_rust_plc_compat_core::runtime_owner::{RuntimeOwner, StartResult, StopResult};
use open_rust_plc_compat_core::stats::StatsRegistry;

fn main() {
    let path = std::env::args().nth(1).expect("generated library required");
    let owner = RuntimeOwner::new();
    let api = Arc::new(ProgramApi::load(Path::new(&path)).unwrap());
    assert_eq!(
        unsafe { owner.start_program(api).unwrap() },
        StartResult::Started
    );
    println!("{}", wait_stats(owner.stats()));
    assert_eq!(owner.stop().unwrap(), StopResult::Stopped);
    println!("{}", owner.stats().response().trim_end());
}

fn wait_stats(stats: &StatsRegistry) -> String {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let response = stats.response();
        if !response.contains("\"tasks\":[]") && !response.contains("\"scan_count\":0") {
            return response.trim_end().to_owned();
        }
        assert!(Instant::now() < deadline, "generated STATS timed out");
        std::thread::yield_now();
    }
}
