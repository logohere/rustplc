use super::UnixCommandServer;
use crate::command::CommandHandler;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn test_path() -> PathBuf {
    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "open-rust-plc-{}-{stamp}-{sequence}.sock",
        std::process::id()
    ))
}

fn wait_until_stream_socket_is_stale(path: &PathBuf) {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match UnixStream::connect(path) {
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => return,
            Ok(stream) => drop(stream),
            Err(error) => panic!("unexpected stale-socket probe error: {error}"),
        }
        assert!(
            Instant::now() < deadline,
            "stream socket did not become stale"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn serves_command_lines_and_closes_client() {
    let path = test_path();
    // SAFETY: this test sends STATUS only and never loads the placeholder path.
    let handler = unsafe { CommandHandler::new("unused") };
    let server = UnixCommandServer::bind(&path, Arc::new(handler)).unwrap();
    let worker = thread::spawn(move || server.serve_once());
    let mut client = UnixStream::connect(&path).unwrap();
    client.write_all(b"STATUS\n").unwrap();
    client.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    assert_eq!(response, "STATUS:STOPPED\n");
    assert!(worker.join().unwrap().is_ok());
    assert!(!path.exists());
}

#[test]
fn shutdown_wakes_the_server_and_cleans_its_socket() {
    let path = test_path();
    let handler = unsafe { CommandHandler::new("unused") };
    let server = Arc::new(UnixCommandServer::bind(&path, Arc::new(handler)).unwrap());
    let worker_server = Arc::clone(&server);
    let worker = thread::spawn(move || worker_server.serve());
    let mut client = UnixStream::connect(&path).unwrap();
    client.write_all(b"STATUS\n").unwrap();
    server.shutdown_handle().shutdown();
    assert!(worker.join().unwrap().is_ok());
    drop(server);
    assert!(!path.exists());
}

#[test]
fn refuses_a_regular_file_at_the_socket_path() {
    let path = test_path();
    fs::write(&path, b"keep").unwrap();
    let handler = unsafe { CommandHandler::new("unused") };
    let error = UnixCommandServer::bind(&path, Arc::new(handler))
        .err()
        .unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&path).unwrap(), b"keep");
    fs::remove_file(path).unwrap();
}

#[test]
fn replaces_only_a_stale_socket() {
    let path = test_path();
    let stale = std::os::unix::net::UnixListener::bind(&path).unwrap();
    drop(stale);
    wait_until_stream_socket_is_stale(&path);
    let handler = unsafe { CommandHandler::new("unused") };
    let server = UnixCommandServer::bind(&path, Arc::new(handler)).unwrap();
    drop(server);
    assert!(!path.exists());
}
