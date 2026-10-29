use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::command::CommandHandler;
use crate::socket::{read_command_line, COMMAND_BUFFER_SIZE};

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::FileTypeExt;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
#[derive(Clone)]
pub struct UnixServerShutdown {
    path: Arc<PathBuf>,
    running: Arc<AtomicBool>,
    client: Arc<Mutex<Option<UnixStream>>>,
}

#[cfg(unix)]
impl UnixServerShutdown {
    pub fn shutdown(&self) {
        if !self.running.swap(false, Ordering::AcqRel) {
            return;
        }
        if let Some(stream) = lock_client(&self.client).as_ref() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
        let _ = UnixStream::connect(&*self.path);
    }
}

#[cfg(unix)]
pub struct UnixCommandServer {
    listener: UnixListener,
    handler: Arc<CommandHandler>,
    shutdown: UnixServerShutdown,
}

#[cfg(unix)]
impl UnixCommandServer {
    pub fn bind<P: AsRef<Path>>(path: P, handler: Arc<CommandHandler>) -> io::Result<Self> {
        let path = Arc::new(path.as_ref().to_owned());
        prepare_path(&path)?;
        Ok(Self {
            listener: UnixListener::bind(&*path)?,
            handler,
            shutdown: UnixServerShutdown {
                path,
                running: Arc::new(AtomicBool::new(true)),
                client: Arc::new(Mutex::new(None)),
            },
        })
    }

    pub fn shutdown_handle(&self) -> UnixServerShutdown {
        self.shutdown.clone()
    }

    pub fn serve(&self) -> io::Result<()> {
        while self.shutdown.running.load(Ordering::Acquire) {
            let (mut stream, _) = match self.listener.accept() {
                Ok(client) => client,
                Err(_) if !self.shutdown.running.load(Ordering::Acquire) => break,
                Err(error) => return Err(error),
            };
            if !self.shutdown.running.load(Ordering::Acquire) {
                break;
            }
            *lock_client(&self.shutdown.client) = Some(stream.try_clone()?);
            let result = serve_client(&mut stream, &self.handler);
            lock_client(&self.shutdown.client).take();
            if let Err(error) = result {
                if self.shutdown.running.load(Ordering::Acquire) {
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    pub fn serve_once(&self) -> io::Result<()> {
        let (mut stream, _) = self.listener.accept()?;
        serve_client(&mut stream, &self.handler)
    }
}

#[cfg(unix)]
impl Drop for UnixCommandServer {
    fn drop(&mut self) {
        self.shutdown.shutdown();
        if fs::symlink_metadata(&*self.shutdown.path)
            .is_ok_and(|metadata| metadata.file_type().is_socket())
        {
            let _ = fs::remove_file(&*self.shutdown.path);
        }
    }
}

#[cfg(unix)]
fn lock_client(client: &Mutex<Option<UnixStream>>) -> MutexGuard<'_, Option<UnixStream>> {
    client
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(unix)]
fn prepare_path(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !metadata.file_type().is_socket() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "socket path exists and is not a socket",
        ));
    }
    match UnixStream::connect(path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "socket already has a listener",
        )),
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn serve_client<S: Read + Write>(stream: &mut S, handler: &CommandHandler) -> io::Result<()> {
    while let Some(bytes) = read_command_line(stream, COMMAND_BUFFER_SIZE)? {
        let response = match std::str::from_utf8(&bytes) {
            Ok(command) => handler
                .handle(command)
                .map_err(|_| io::Error::other("lifecycle state unavailable"))?,
            Err(_) => "COMMAND:ERROR\n".to_owned(),
        };
        stream.write_all(response.as_bytes())?;
    }
    Ok(())
}

#[derive(Clone)]
pub struct TcpServerStop {
    address: SocketAddr,
    running: Arc<AtomicBool>,
}

impl TcpServerStop {
    pub fn request_stop(&self) {
        if !self.running.swap(false, Ordering::AcqRel) {
            return;
        }
        let _ = TcpStream::connect(self.address);
    }
}

pub struct TcpCommandServer {
    listener: TcpListener,
    handler: Arc<CommandHandler>,
    stop: TcpServerStop,
}

impl TcpCommandServer {
    pub fn bind(address: SocketAddr, handler: Arc<CommandHandler>) -> io::Result<Self> {
        if !address.ip().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "RustPLC command TCP server must bind to a loopback address",
            ));
        }
        let listener = TcpListener::bind(address)?;
        let address = listener.local_addr()?;
        Ok(Self {
            listener,
            handler,
            stop: TcpServerStop {
                address,
                running: Arc::new(AtomicBool::new(true)),
            },
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn stop_handle(&self) -> TcpServerStop {
        self.stop.clone()
    }

    pub fn serve(&self) -> io::Result<()> {
        while self.stop.running.load(Ordering::Acquire) {
            let (mut stream, peer) = match self.listener.accept() {
                Ok(client) => client,
                Err(_) if !self.stop.running.load(Ordering::Acquire) => break,
                Err(error) => return Err(error),
            };
            if !self.stop.running.load(Ordering::Acquire) {
                break;
            }
            if !peer.ip().is_loopback() {
                continue;
            }
            if let Err(error) = serve_client(&mut stream, &self.handler) {
                if self.stop.running.load(Ordering::Acquire) {
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    pub fn serve_once(&self) -> io::Result<()> {
        let (mut stream, peer) = self.listener.accept()?;
        if !peer.ip().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "RustPLC command client is not loopback",
            ));
        }
        serve_client(&mut stream, &self.handler)
    }
}

impl Drop for TcpCommandServer {
    fn drop(&mut self) {
        self.stop.request_stop();
    }
}

#[cfg(not(unix))]
pub struct UnixCommandServer;

#[cfg(not(unix))]
#[derive(Clone, Default)]
pub struct UnixServerShutdown;

#[cfg(not(unix))]
impl UnixServerShutdown {
    pub fn shutdown(&self) {}
}

#[cfg(not(unix))]
impl UnixCommandServer {
    pub fn bind<P: AsRef<Path>>(_: P, _: Arc<CommandHandler>) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "OpenPLC command server requires Unix-domain sockets",
        ))
    }

    pub fn serve_once(&self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "OpenPLC command server requires Unix-domain sockets",
        ))
    }

    pub fn serve(&self) -> io::Result<()> {
        self.serve_once()
    }

    pub fn shutdown_handle(&self) -> UnixServerShutdown {
        UnixServerShutdown
    }
}

#[cfg(test)]
mod tcp_tests {
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, SocketAddr, TcpStream};
    use std::sync::Arc;
    use std::thread;

    use crate::command::CommandHandler;

    use super::TcpCommandServer;

    #[test]
    fn serves_commands_on_loopback_tcp() {
        // SAFETY: this test sends STATUS only and never loads the placeholder path.
        let handler = Arc::new(unsafe { CommandHandler::new("unused") });
        let server =
            TcpCommandServer::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)), handler).unwrap();
        let address = server.local_addr().unwrap();
        let worker = thread::spawn(move || server.serve_once());
        let mut client = TcpStream::connect(address).unwrap();
        client.write_all(b"STATUS\n").unwrap();
        client.flush().unwrap();
        let mut response = [0u8; 64];
        let read = client.read(&mut response).unwrap();
        assert_eq!(&response[..read], b"STATUS:STOPPED\n");
        drop(client);
        assert!(worker.join().unwrap().is_ok());
    }

    #[test]
    fn refuses_non_loopback_bind_addresses() {
        // SAFETY: this test never starts or loads the placeholder path.
        let handler = Arc::new(unsafe { CommandHandler::new("unused") });
        let error = TcpCommandServer::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)), handler)
            .err()
            .unwrap();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
}

#[cfg(all(test, unix))]
mod tests;
