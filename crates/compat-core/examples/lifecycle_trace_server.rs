#[cfg(unix)]
use std::io::{self, BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

#[cfg(unix)]
use open_rust_plc_compat_core::command::CommandHandler;

#[cfg(unix)]
fn main() -> io::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "socket path required"))?;
    let library = std::env::args_os()
        .nth(2)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "library path required"))?;
    let listener = UnixListener::bind(path)?;
    let (stream, _) = listener.accept()?;
    // SAFETY: the differential harness supplies its pinned generated fixture.
    let handler = unsafe { CommandHandler::new(library) };
    serve(stream, &handler)
}

#[cfg(unix)]
fn serve(mut stream: UnixStream, handler: &CommandHandler) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    while reader.read_line(&mut line)? != 0 {
        let command = line.strip_suffix('\n').unwrap_or(&line);
        let response = handler
            .handle(command)
            .map_err(|_| io::Error::other("lifecycle state unavailable"))?;
        stream.write_all(response.as_bytes())?;
        line.clear();
    }
    Ok(())
}

#[cfg(not(unix))]
fn main() {
    eprintln!("lifecycle trace server requires Unix-domain sockets");
    std::process::exit(2);
}
