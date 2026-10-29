use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use open_rust_plc_compat_core::command::CommandHandler;
use open_rust_plc_compat_core::configuration::{
    task_slice, ConfigurationError, ConfigurationInstance, TaskInstance,
};
use open_rust_plc_compat_core::debug::DebugProvider;
use open_rust_plc_compat_core::journal::{JournalBufferType, JournalEntry};
use open_rust_plc_compat_core::located_image::LocatedImage;
use open_rust_plc_compat_core::program_api::{ProgramApi, ProgramScanError};
use open_rust_plc_compat_core::runtime_owner::{RuntimeOwner, StartResult, StopResult};
use open_rust_plc_compat_core::scan_runtime::RuntimeHandle;
use open_rust_plc_compat_core::scan_runtime::RuntimeTaskFault;

#[test]
#[ignore = "requires pinned STruC++ fixture library"]
fn runs_generated_io_fault_and_reload_cycle() {
    let path = std::env::var_os("OPEN_RUST_PLC_ABI_FIXTURE")
        .expect("OPEN_RUST_PLC_ABI_FIXTURE must name the fixture library");
    {
        let (api, image, task) = load_fixture(Path::new(&path));
        assert_eq!(run_cycle(&api, &image, task, true), (1, 1));
        assert_eq!(run_cycle(&api, &image, task, false), (2, 0));
        assert_eq!(api.debug_set(1, 6, true, &99_u32.to_ne_bytes()), 0x7e);
        assert_eq!(run_cycle(&api, &image, task, false), (99, 0));
        assert_fault(&api, &image);
        // SAFETY: no scan or debug worker is active in this synchronous fixture.
        unsafe { api.restore_initial_state().unwrap() };
        // SAFETY: the restored program is live and this creates fresh host image storage.
        let image = unsafe { api.located_image().unwrap() };
        assert_eq!(run_cycle(&api, &image, task, false), (1, 1));
        assert_threaded_global_frame(&api, &image);
        // SAFETY: no scan or debug worker is active before unloading the fixture.
        unsafe { api.restore_initial_state().unwrap() };
        assert_owned_lifecycle(&api);
    }
    let (api, image, task) = load_fixture(Path::new(&path));
    assert_eq!(run_cycle(&api, &image, task, false), (1, 1));
}

#[test]
#[ignore = "requires pinned STruC++ fixture library"]
fn routes_commands_through_the_owned_generated_runtime() {
    let path = std::env::var_os("OPEN_RUST_PLC_ABI_FIXTURE")
        .expect("OPEN_RUST_PLC_ABI_FIXTURE must name the fixture library");
    // SAFETY: CI and the local harness build this path from the pinned generator.
    let handler = unsafe { CommandHandler::new(path) };
    assert_eq!(handler.handle("START").unwrap(), "START:OK\n");
    wait_command_status(&handler, "STATUS:RUNNING\n");
    assert_ne!(handler.handle("DEBUG:41").unwrap(), "DEBUG:41 83\n");
    assert!(handler
        .stats()
        .response()
        .contains("\"name\":\"MAIN_TASK\""));
    assert_eq!(handler.handle("STOP").unwrap(), "STOP:OK\n");
    wait_command_status(&handler, "STATUS:STOPPED\n");
    assert_eq!(handler.handle("DEBUG:41").unwrap(), "DEBUG:41 83\n");
    assert_eq!(handler.stats().response(), "STATS:{\"tasks\":[]}\n");
}

fn wait_command_status(handler: &CommandHandler, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while handler.handle("STATUS").unwrap() != expected {
        assert!(Instant::now() < deadline, "command transition timed out");
        std::thread::yield_now();
    }
}

fn assert_owned_lifecycle(api: &Arc<ProgramApi>) {
    let owner = RuntimeOwner::new();
    for attempt in 0..2 {
        if attempt == 0 {
            assert_eq!(api.debug_set(1, 6, true, &99_u32.to_ne_bytes()), 0x7e);
            assert_eq!(debug_u32(api, 1, 6), 99);
        }
        assert_eq!(
            unsafe { owner.start_program(Arc::clone(api)).unwrap() },
            StartResult::Started
        );
        wait_for_stats(&owner);
        let image = owner.image().unwrap().unwrap();
        let input = JournalEntry::bool(JournalBufferType::BoolInput, 0, 1, true).unwrap();
        let output = JournalEntry::bool(JournalBufferType::BoolOutput, 0, 1, false).unwrap();
        image.write(input).unwrap();
        wait_for(&image, output, 1);
        assert_eq!(owner.stop().unwrap(), StopResult::Stopped);
        assert_eq!(owner.stats().response(), "STATS:{\"tasks\":[]}\n");
        assert!(owner.image().unwrap().is_none());
        assert_eq!(debug_u32(api, 1, 6), 0);
    }
}

fn wait_for_stats(owner: &RuntimeOwner) {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let response = owner.stats().response();
        if response.contains("\"name\":\"MAIN_TASK\"") && !response.contains("\"scan_count\":0") {
            return;
        }
        assert!(Instant::now() < deadline, "generated STATS timed out");
        std::thread::yield_now();
    }
}

fn wait_for(image: &LocatedImage, entry: JournalEntry, expected: u64) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while image.read(entry).unwrap() != expected {
        assert!(Instant::now() < deadline, "owned runtime scan timed out");
        std::thread::yield_now();
    }
}

fn debug_u32(api: &ProgramApi, array: u8, element: u16) -> u32 {
    u32::from_ne_bytes(api.read(array, element).unwrap().try_into().unwrap())
}

fn assert_threaded_global_frame(api: &Arc<ProgramApi>, image: &Arc<LocatedImage>) {
    let input = JournalEntry::bool(JournalBufferType::BoolInput, 0, 1, true).unwrap();
    let output = JournalEntry::bool(JournalBufferType::BoolOutput, 0, 1, false).unwrap();
    image.write(input).unwrap();
    let tasks = unsafe { api.runtime_tasks(Arc::clone(image)).unwrap() };
    let handle = RuntimeHandle::start_with_image(tasks, Arc::clone(image)).unwrap();
    let fault = Arc::clone(&handle.frame_fault);
    let deadline = Instant::now() + Duration::from_secs(1);
    while image.read(output).unwrap() != 1 {
        assert!(
            Instant::now() < deadline,
            "configuration-global frame timed out"
        );
        std::thread::yield_now();
    }
    handle.stop();
    assert!(fault.get().is_none());
}

fn load_fixture(path: &Path) -> (Arc<ProgramApi>, Arc<LocatedImage>, TaskInstance) {
    let api = Arc::new(ProgramApi::load(path).unwrap());
    assert_eq!(api.md5(), Some("0123456789abcdef0123456789abcdef"));
    assert_eq!(
        (api.located_var_count(), api.located_global_count()),
        (7, Some(2))
    );
    let image = unsafe { api.located_image().unwrap() };
    assert_eq!(image.config_global_count(), 2);
    let configuration =
        unsafe { ConfigurationInstance::from_raw(api.configuration_handle()) }.unwrap();
    assert_eq!(
        unsafe { configuration.name().unwrap().unwrap() }.to_bytes(),
        b"CONFIG0"
    );
    let resources = unsafe { configuration.resources().unwrap() };
    assert_eq!(resources.len(), 1);
    let tasks = unsafe { task_slice(resources[0]).unwrap() };
    assert_eq!((tasks[0].interval_ns, tasks[0].priority), (20_000_000, 1));
    (api, image, tasks[0])
}

fn run_cycle(
    api: &ProgramApi,
    image: &LocatedImage,
    task: TaskInstance,
    value: bool,
) -> (u64, u64) {
    let input = |bit| JournalEntry::bool(JournalBufferType::BoolInput, 0, bit, value).unwrap();
    let output = |bit| JournalEntry::bool(JournalBufferType::BoolOutput, 0, bit, false).unwrap();
    image.write(input(0)).unwrap();
    image.write(input(1)).unwrap();
    unsafe { image.copy_config_globals_in().unwrap() };
    unsafe { api.scan_task_once(image, task).unwrap() };
    unsafe { image.copy_config_globals_out().unwrap() };
    assert_eq!(image.read(output(0)).unwrap(), u64::from(value));
    assert_eq!(image.read(output(1)).unwrap(), u64::from(value));
    let state = JournalEntry::dint(JournalBufferType::DintOutput, 2, 0).unwrap();
    (image.read(state).unwrap(), image.read(output(2)).unwrap())
}

fn assert_fault(api: &Arc<ProgramApi>, image: &Arc<LocatedImage>) {
    image
        .write(JournalEntry::byte(JournalBufferType::ByteInput, 1, 1).unwrap())
        .unwrap();
    let mut tasks = unsafe { api.runtime_tasks(Arc::clone(image)).unwrap() };
    assert_eq!(
        (tasks[0].body)(0),
        Err(RuntimeTaskFault::Program(ProgramScanError::Configuration(
            ConfigurationError::CppException
        )))
    );
}
