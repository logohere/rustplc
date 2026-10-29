use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use native_tls::TlsConnector;
use serde::Deserialize;
use serde_json::{json, Value};
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{client_tls_with_config, Connector, Message, WebSocket};

use crate::debug_wire::{
    build_get_list_request, build_get_md5_request, build_set_variable_request,
    parse_get_list_response, parse_get_md5_response, parse_set_variable_response,
    DebugListResponse, Md5Probe,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const DEBUG_NAMESPACE: &str = "/api/debug";

#[derive(Clone)]
pub struct RuntimeDebugTransport {
    socket: Arc<Mutex<WebSocket<MaybeTlsStream<TcpStream>>>>,
}

impl RuntimeDebugTransport {
    pub fn connect(
        host: &str,
        port: u16,
        token: &str,
        allow_insecure_tls: bool,
    ) -> Result<Self, String> {
        let host = host.trim();
        if host.is_empty() {
            return Err("Runtime debug host is empty.".to_owned());
        }
        Self::connect_url(
            &format!("wss://{host}:{port}/socket.io/?EIO=4&transport=websocket"),
            host,
            token,
            true,
            allow_insecure_tls,
        )
    }

    #[cfg(test)]
    fn connect_test(url: &str, host: &str, token: &str) -> Result<Self, String> {
        Self::connect_url(url, host, token, false, false)
    }

    fn connect_url(
        url: &str,
        host: &str,
        token: &str,
        tls: bool,
        allow_insecure_tls: bool,
    ) -> Result<Self, String> {
        let request = url
            .into_client_request()
            .map_err(|error| format!("build debug WebSocket request: {error}"))?;
        let port = request
            .uri()
            .port_u16()
            .unwrap_or(if tls { 443 } else { 80 });
        let stream = TcpStream::connect_timeout(
            &format!("{host}:{port}")
                .parse()
                .map_err(|error| format!("resolve debug target {host}:{port}: {error}"))?,
            CONNECT_TIMEOUT,
        )
        .map_err(|error| format!("connect debug WebSocket TCP: {error}"))?;
        stream
            .set_read_timeout(Some(REQUEST_TIMEOUT))
            .map_err(|error| format!("set debug WebSocket read timeout: {error}"))?;
        stream
            .set_write_timeout(Some(REQUEST_TIMEOUT))
            .map_err(|error| format!("set debug WebSocket write timeout: {error}"))?;

        let connector = if tls {
            let connector = TlsConnector::builder()
                .danger_accept_invalid_certs(allow_insecure_tls)
                .danger_accept_invalid_hostnames(allow_insecure_tls)
                .build()
                .map_err(|error| format!("build debug TLS connector: {error}"))?;
            Some(Connector::NativeTls(connector))
        } else {
            Some(Connector::Plain)
        };
        let (mut socket, _) = client_tls_with_config(request, stream, None, connector)
            .map_err(|error| format!("connect debug WebSocket: {error}"))?;

        wait_for_engine_open(&mut socket)?;
        let auth = json!({ "token": token });
        socket
            .send(Message::Text(
                format!("40{DEBUG_NAMESPACE},{}", auth).into(),
            ))
            .map_err(|error| format!("send debug namespace auth: {error}"))?;
        wait_for_connected(&mut socket)?;

        Ok(Self {
            socket: Arc::new(Mutex::new(socket)),
        })
    }

    pub fn disconnect(&self) -> Result<(), String> {
        let mut socket = self
            .socket
            .lock()
            .map_err(|_| "debug WebSocket lock poisoned".to_owned())?;
        socket
            .close(None)
            .map_err(|error| format!("close debug WebSocket: {error}"))
    }

    pub fn get_md5(&self) -> Result<Md5Probe, String> {
        let bytes = self.send_command(&build_get_md5_request())?;
        parse_get_md5_response(&bytes)
    }

    pub fn get_variables_list(&self, indexes: &[u32]) -> Result<DebugListResponse, String> {
        let request = build_get_list_request(indexes)?;
        let bytes = self.send_command(&request)?;
        parse_get_list_response(&bytes)
    }

    pub fn set_variable(
        &self,
        index: u32,
        force: bool,
        value: Option<&[u8]>,
    ) -> Result<(), String> {
        let request = build_set_variable_request(index, force, value)?;
        let bytes = self.send_command(&request)?;
        parse_set_variable_response(&bytes)
    }

    fn send_command(&self, pdu: &[u8]) -> Result<Vec<u8>, String> {
        let command = bytes_to_hex_spaced(pdu);
        let payload = json!(["debug_command", { "command": command }]);
        let mut socket = self
            .socket
            .lock()
            .map_err(|_| "debug WebSocket lock poisoned".to_owned())?;
        socket
            .send(Message::Text(
                format!("42{DEBUG_NAMESPACE},{}", payload).into(),
            ))
            .map_err(|error| format!("send debug command: {error}"))?;

        loop {
            match socket
                .read()
                .map_err(|error| format!("read debug response: {error}"))?
            {
                Message::Text(text) => {
                    if text == "2" {
                        socket
                            .send(Message::Text("3".into()))
                            .map_err(|error| format!("send Engine.IO pong: {error}"))?;
                        continue;
                    }
                    if let Some(response) = parse_socket_event(&text, "debug_response")? {
                        let response: DebugEnvelope = serde_json::from_value(response)
                            .map_err(|error| format!("parse debug response envelope: {error}"))?;
                        if !response.success {
                            return Err(response
                                .error
                                .unwrap_or_else(|| "Unknown error".to_owned()));
                        }
                        let data = response
                            .data
                            .ok_or_else(|| "No data in debug response".to_owned())?;
                        return hex_spaced_to_bytes(&data);
                    }
                }
                Message::Ping(payload) => socket
                    .send(Message::Pong(payload))
                    .map_err(|error| format!("send WebSocket pong: {error}"))?,
                Message::Close(frame) => {
                    return Err(format!("Runtime debug WebSocket closed: {frame:?}"));
                }
                _ => {}
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct DebugEnvelope {
    success: bool,
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

fn wait_for_engine_open(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> Result<(), String> {
    loop {
        match socket
            .read()
            .map_err(|error| format!("read Engine.IO open packet: {error}"))?
        {
            Message::Text(text) if text.starts_with('0') => return Ok(()),
            Message::Text(text) if text == "2" => socket
                .send(Message::Text("3".into()))
                .map_err(|error| format!("send Engine.IO pong: {error}"))?,
            Message::Ping(payload) => socket
                .send(Message::Pong(payload))
                .map_err(|error| format!("send WebSocket pong: {error}"))?,
            Message::Close(frame) => {
                return Err(format!(
                    "Runtime debug WebSocket closed before open: {frame:?}"
                ));
            }
            _ => {}
        }
    }
}

fn wait_for_connected(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> Result<(), String> {
    loop {
        match socket
            .read()
            .map_err(|error| format!("read debug namespace connect: {error}"))?
        {
            Message::Text(text) => {
                if text == "2" {
                    socket
                        .send(Message::Text("3".into()))
                        .map_err(|error| format!("send Engine.IO pong: {error}"))?;
                    continue;
                }
                if let Some(value) = parse_socket_event(&text, "connected")? {
                    let status = value
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    return if status == "ok" {
                        Ok(())
                    } else {
                        Err("Connection failed: invalid debug status".to_owned())
                    };
                }
            }
            Message::Ping(payload) => socket
                .send(Message::Pong(payload))
                .map_err(|error| format!("send WebSocket pong: {error}"))?,
            Message::Close(frame) => {
                return Err(format!(
                    "Runtime debug WebSocket closed before namespace connect: {frame:?}"
                ));
            }
            _ => {}
        }
    }
}

fn parse_socket_event(text: &str, wanted: &str) -> Result<Option<Value>, String> {
    if !text.starts_with("42") {
        return Ok(None);
    }
    let Some(array_start) = text.find('[') else {
        return Ok(None);
    };
    let event: Value = serde_json::from_str(&text[array_start..])
        .map_err(|error| format!("parse Socket.IO event: {error}"))?;
    let Some(array) = event.as_array() else {
        return Ok(None);
    };
    if array.first().and_then(Value::as_str) != Some(wanted) {
        return Ok(None);
    }
    Ok(array.get(1).cloned())
}

fn bytes_to_hex_spaced(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn hex_spaced_to_bytes(hex: &str) -> Result<Vec<u8>, String> {
    hex.split_whitespace()
        .map(|part| {
            u8::from_str_radix(part, 16)
                .map_err(|_| format!("Invalid hex byte in debug response: {part:?}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::thread;

    use tungstenite::{accept, Message};

    use super::{bytes_to_hex_spaced, RuntimeDebugTransport};
    use crate::debug_wire::{pack_debug_addr, TargetEndian};

    #[test]
    fn socket_io_debug_transport_authenticates_pongs_and_round_trips_pdus() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = accept(stream).unwrap();
            socket
                .send(Message::Text(
                    "0{\"sid\":\"abc\",\"pingInterval\":25000,\"pingTimeout\":20000}".into(),
                ))
                .unwrap();
            let auth = socket.read().unwrap().into_text().unwrap();
            assert!(auth.starts_with("40/api/debug,"));
            assert!(auth.contains("\"token\":\"secret\""));
            socket.send(Message::Text("2".into())).unwrap();
            assert_eq!(socket.read().unwrap().into_text().unwrap(), "3");
            socket
                .send(Message::Text(
                    "42/api/debug,[\"connected\",{\"status\":\"ok\"}]".into(),
                ))
                .unwrap();

            let command = socket.read().unwrap().into_text().unwrap();
            assert!(command.starts_with("42/api/debug,[\"debug_command\","));
            assert!(command.contains("45 DE AD 00 00"));
            let mut response = vec![0x45, 0x7e];
            response.extend_from_slice(b"0123456789abcdef0123456789abcdef");
            response.extend_from_slice(&[0xad, 0xde]);
            let envelope = serde_json::json!([
                "debug_response",
                {"success":true,"data":bytes_to_hex_spaced(&response)}
            ]);
            socket
                .send(Message::Text(format!("42/api/debug,{envelope}").into()))
                .unwrap();

            let list = socket.read().unwrap().into_text().unwrap();
            assert!(list.contains("44 00 01 02 12 34"));
            let list_response = serde_json::json!([
                "debug_response",
                {"success":true,"data":"44 7E 00 00 00 00 00 2A 00 02 01 02"}
            ]);
            socket
                .send(Message::Text(
                    format!("42/api/debug,{list_response}").into(),
                ))
                .unwrap();
        });

        let transport = RuntimeDebugTransport::connect_test(
            &format!("ws://{address}/socket.io/?EIO=4&transport=websocket"),
            "127.0.0.1",
            "secret",
        )
        .unwrap();
        let md5 = transport.get_md5().unwrap();
        assert_eq!(md5.md5, "0123456789abcdef0123456789abcdef");
        assert_eq!(md5.target_endian, TargetEndian::Little);
        let list = transport
            .get_variables_list(&[pack_debug_addr(2, 0x1234)])
            .unwrap();
        assert_eq!(list.tick, 42);
        assert_eq!(list.data, vec![1, 2]);
        let _ = transport.disconnect();
        server.join().unwrap();
    }
}
