use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use open_rust_plc_compat_core::debug::DebugProtocol;
use open_rust_plc_compat_core::program_api::ProgramApi;
use open_rust_plc_compat_core::runtime_owner::{RuntimeOwner, StartResult, StopResult};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 4, "library, force, read, and unforce required");
    let owner = RuntimeOwner::new();
    let api = Arc::new(ProgramApi::load(Path::new(&args[0])).unwrap());
    assert_eq!(
        unsafe { owner.start_program(api).unwrap() },
        StartResult::Started
    );

    assert_eq!(owner.debug().text_response(&args[1]), "DEBUG:42 7e\n");
    let forced = wait_read(owner.debug(), &args[2], true);
    assert_eq!(owner.debug().text_response(&args[3]), "DEBUG:42 7e\n");
    let released = wait_read(owner.debug(), &args[2], true);
    assert_eq!(owner.stop().unwrap(), StopResult::Stopped);
    assert_eq!(owner.debug().text_response("41"), "DEBUG:41 83\n");
    assert_eq!(owner.debug().pending_writes().unwrap(), 0);
    println!("{{\"forced_output\":{forced},\"released_output\":{released}}}");
}

fn wait_read(debug: &DebugProtocol, request: &str, expected: bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let (tick, value) = read_bool(debug, request);
        if tick > 0 && value == expected && debug.pending_writes().unwrap() == 0 {
            return value;
        }
        assert!(Instant::now() < deadline, "generated debug scan timed out");
        std::thread::yield_now();
    }
}

fn read_bool(debug: &DebugProtocol, request: &str) -> (u32, bool) {
    let response = debug.text_response(request);
    let bytes: Vec<u8> = response
        .trim()
        .strip_prefix("DEBUG:")
        .unwrap()
        .split(' ')
        .map(|token| u8::from_str_radix(token, 16).unwrap())
        .collect();
    assert_eq!(bytes.len(), 11);
    assert_eq!(&bytes[..2], &[0x43, 0x7e]);
    assert_eq!(u16::from_be_bytes([bytes[8], bytes[9]]), 1);
    (
        u32::from_be_bytes(bytes[4..8].try_into().unwrap()),
        bytes[10] != 0,
    )
}
