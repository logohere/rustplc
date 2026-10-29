use std::io::{self, Cursor, Read};

use open_rust_plc_compat_core::socket::{read_command_line, COMMAND_BUFFER_SIZE};

#[test]
fn strips_newline_but_preserves_carriage_return() {
    let mut input = Cursor::new(b"STATUS\r\nNEXT\n".to_vec());
    assert_eq!(
        read_command_line(&mut input, COMMAND_BUFFER_SIZE).unwrap(),
        Some(b"STATUS\r".to_vec())
    );
    assert_eq!(
        read_command_line(&mut input, COMMAND_BUFFER_SIZE).unwrap(),
        Some(b"NEXT".to_vec())
    );
}

#[test]
fn drops_partial_eof_and_bounds_without_newline() {
    let mut partial = Cursor::new(b"START".to_vec());
    assert_eq!(read_command_line(&mut partial, 16).unwrap(), None);
    let mut bounded = Cursor::new(b"0123456789\n".to_vec());
    assert_eq!(
        read_command_line(&mut bounded, 5).unwrap(),
        Some(b"0123".to_vec())
    );
}

#[test]
fn reports_reader_errors_and_invalid_capacity() {
    let mut empty: &[u8] = &[];
    assert_eq!(
        read_command_line(&mut empty, 1).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
        }
    }
    assert_eq!(
        read_command_line(&mut Broken, 8).unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
}
