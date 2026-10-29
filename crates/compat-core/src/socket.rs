use std::io::{self, Read};

/// OpenPLC command-buffer size from `unix_socket.h`.
pub const COMMAND_BUFFER_SIZE: usize = 8192;

/// Read one newline-terminated command, preserving bytes before `\n`.
/// EOF before a newline discards the partial command, matching `read_line`.
pub fn read_command_line<R: Read>(
    reader: &mut R,
    max_length: usize,
) -> io::Result<Option<Vec<u8>>> {
    if max_length < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "command buffer must hold a byte and terminator",
        ));
    }
    let mut command = Vec::with_capacity(max_length - 1);
    while command.len() < max_length - 1 {
        let mut byte = [0u8; 1];
        if reader.read(&mut byte)? == 0 {
            return Ok(None);
        }
        if byte[0] == b'\n' {
            break;
        }
        command.push(byte[0]);
    }
    Ok(Some(command))
}
