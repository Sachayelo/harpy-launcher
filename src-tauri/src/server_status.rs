//! Minecraft "Server List Ping": the same request the multiplayer menu sends
//! to show the MOTD and player count.

use crate::config::{SERVER_HOST as HOST, SERVER_PORT as PORT};
use serde::Serialize;
use serde_json::Value;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

const PROTOCOL_1_21_1: i32 = 767;
const TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    online: u64,
    max: u64,
    motd: String,
    latency_ms: u64,
}

#[tauri::command]
pub async fn server_status() -> Result<ServerStatus, String> {
    tauri::async_runtime::spawn_blocking(query)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

fn query() -> io::Result<ServerStatus> {
    let address = (HOST, PORT)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::other("adresse du serveur introuvable"))?;

    let started = Instant::now();
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;

    let mut handshake = Vec::new();
    write_varint(&mut handshake, 0x00);
    write_varint(&mut handshake, PROTOCOL_1_21_1);
    write_varint(&mut handshake, HOST.len() as i32);
    handshake.extend_from_slice(HOST.as_bytes());
    handshake.extend_from_slice(&PORT.to_be_bytes());
    write_varint(&mut handshake, 1);

    let mut request = Vec::new();
    write_varint(&mut request, handshake.len() as i32);
    request.extend(handshake);
    request.extend_from_slice(&[0x01, 0x00]);
    stream.write_all(&request)?;

    read_varint(&mut stream)?;
    read_varint(&mut stream)?;
    let length = read_varint(&mut stream)? as usize;
    let mut body = vec![0; length];
    stream.read_exact(&mut body)?;
    let latency_ms = started.elapsed().as_millis() as u64;

    let json: Value = serde_json::from_slice(&body).map_err(io::Error::other)?;
    Ok(ServerStatus {
        online: json["players"]["online"].as_u64().unwrap_or(0),
        max: json["players"]["max"].as_u64().unwrap_or(0),
        motd: text_of(&json["description"]),
        latency_ms,
    })
}

fn write_varint(buffer: &mut Vec<u8>, value: i32) {
    let mut value = value as u32;
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            buffer.push(byte);
            return;
        }
        buffer.push(byte | 0x80);
    }
}

fn read_varint(stream: &mut TcpStream) -> io::Result<i32> {
    let mut result = 0;
    for shift in (0..35).step_by(7) {
        let mut byte = [0u8];
        stream.read_exact(&mut byte)?;
        result |= ((byte[0] & 0x7F) as i32) << shift;
        if byte[0] & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err(io::Error::other("réponse du serveur invalide"))
}

/// The MOTD is either a plain string or a chat component tree; flatten it and
/// drop the legacy `§` formatting codes.
fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => strip_formatting(text),
        Value::Array(parts) => parts.iter().map(text_of).collect(),
        Value::Object(component) => {
            let mut text = component.get("text").map(text_of).unwrap_or_default();
            if let Some(Value::Array(extra)) = component.get("extra") {
                text.extend(extra.iter().map(text_of));
            }
            text
        }
        _ => String::new(),
    }
}

fn strip_formatting(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            output.push(c);
        }
    }
    output
}
