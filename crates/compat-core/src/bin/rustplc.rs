use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use open_rust_plc_compat_core::command::CommandHandler;
use open_rust_plc_compat_core::server::{TcpCommandServer, UnixCommandServer};

const USAGE: &str =
    "usage: rustplc --program <library> [--socket <path>] [--tcp <127.0.0.1:port>] [--safe-mode]";

struct RunArgs {
    program: PathBuf,
    socket: PathBuf,
    tcp: Option<SocketAddr>,
    safe_mode: bool,
}

enum Action {
    Help,
    Version,
    Run(RunArgs),
}

fn main() {
    if let Err(error) = run() {
        eprintln!("rustplc: {error}");
        std::process::exit(2);
    }
}

fn run() -> io::Result<()> {
    let args = match parse(env::args_os().skip(1))? {
        Action::Help => return print(USAGE),
        Action::Version => return print(concat!("rustplc ", env!("CARGO_PKG_VERSION"))),
        Action::Run(args) => args,
    };
    validate_program(&args.program)?;
    // SAFETY: validation and ProgramApi loading enforce the pinned generated ABI boundary.
    let handler = Arc::new(unsafe { CommandHandler::new(args.program) });
    if let Some(address) = args.tcp {
        let server = TcpCommandServer::bind(address, Arc::clone(&handler))?;
        let address = server.local_addr()?;
        let stop = server.stop_handle();
        ctrlc::set_handler(move || stop.request_stop())
            .map_err(|error| io::Error::other(error.to_string()))?;
        if !args.safe_mode {
            let response = handler
                .handle("START")
                .map_err(|_| io::Error::other("lifecycle state unavailable"))?;
            if response != "START:OK\n" {
                return Err(io::Error::other(response.trim().to_owned()));
            }
        }
        println!("rustplc tcp:{address}");
        return server.serve();
    }
    let server = UnixCommandServer::bind(&args.socket, Arc::clone(&handler))?;
    let shutdown = server.shutdown_handle();
    ctrlc::set_handler(move || shutdown.shutdown())
        .map_err(|error| io::Error::other(error.to_string()))?;
    if !args.safe_mode {
        let response = handler
            .handle("START")
            .map_err(|_| io::Error::other("lifecycle state unavailable"))?;
        if response != "START:OK\n" {
            return Err(io::Error::other(response.trim().to_owned()));
        }
    }
    println!("rustplc {}", args.socket.display());
    server.serve()
}

fn parse(mut args: impl Iterator<Item = OsString>) -> io::Result<Action> {
    let mut program = None;
    let mut socket = env::temp_dir().join("rustplc-runtime.sock");
    let mut tcp = None;
    let mut safe_mode = false;
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--help" | "-h") => return Ok(Action::Help),
            Some("--version" | "-V") => return Ok(Action::Version),
            Some("--program") if program.is_none() => program = Some(value(&mut args, "program")?),
            Some("--socket") if tcp.is_none() => socket = value(&mut args, "socket")?,
            Some("--tcp") if tcp.is_none() => {
                let raw = value(&mut args, "tcp")?;
                let address = raw
                    .to_str()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, USAGE))?
                    .parse::<SocketAddr>()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, USAGE))?;
                if !address.ip().is_loopback() {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE));
                }
                tcp = Some(address);
            }
            Some("--safe-mode") if !safe_mode => safe_mode = true,
            _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE)),
        }
    }
    let program = program.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, USAGE))?;
    Ok(Action::Run(RunArgs {
        program,
        socket,
        tcp,
        safe_mode,
    }))
}

fn value(args: &mut impl Iterator<Item = OsString>, name: &'static str) -> io::Result<PathBuf> {
    args.next().map(PathBuf::from).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("--{name} requires a path"),
        )
    })
}

fn validate_program(path: &PathBuf) -> io::Result<()> {
    if fs::metadata(path)?.is_file() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "program path is not a file",
        ))
    }
}

fn print(text: &str) -> io::Result<()> {
    println!("{text}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse, Action};
    use std::ffi::OsString;

    #[test]
    fn parses_bounded_run_arguments() {
        let args = ["--program", "program.dylib", "--safe-mode"]
            .map(OsString::from)
            .into_iter();
        let Action::Run(parsed) = parse(args).unwrap() else {
            panic!("expected run arguments");
        };
        assert_eq!(parsed.program, std::path::Path::new("program.dylib"));
        assert!(parsed.safe_mode);
    }

    #[test]
    fn rejects_duplicate_or_missing_values() {
        let duplicate = ["--program", "a", "--program", "b"].map(OsString::from);
        let missing = ["--program"].map(OsString::from);
        assert!(parse(duplicate.into_iter()).is_err());
        assert!(parse(missing.into_iter()).is_err());
    }

    #[test]
    fn parses_only_loopback_tcp_endpoint() {
        let valid = ["--program", "program.dylib", "--tcp", "127.0.0.1:1504"].map(OsString::from);
        let Action::Run(parsed) = parse(valid.into_iter()).unwrap() else {
            panic!("expected run arguments");
        };
        assert_eq!(parsed.tcp.unwrap().to_string(), "127.0.0.1:1504");

        let public = ["--program", "program.dylib", "--tcp", "0.0.0.0:1504"].map(OsString::from);
        assert!(parse(public.into_iter()).is_err());
    }
}
