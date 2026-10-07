//! Line-oriented protocol used by the Unix-socket transport.
//!
//! Plain lines are messages. Lines beginning with `CMD ` are commands.

use std::io::{self, BufRead, Write};
use std::os::unix::net::UnixStream;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    BottomToTop,
    TopToBottom,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    SetDirection(Direction),
    SetSpeed(f64),
    SetColor(String),
    Clear,
    Pause,
    Resume,
    SetQueueCapacity(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Message(String),
    Command(Command),
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("message is too large ({0} bytes)")]
    MessageTooLarge(usize),
    #[error("invalid command: {0}")]
    InvalidCommand(String),
}

pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 64 * 1024;

pub fn encode(request: &Request) -> String {
    match request {
        Request::Message(text) => text.clone(),
        Request::Command(command) => format!("CMD {}", encode_command(command)),
    }
}

fn encode_command(command: &Command) -> String {
    match command {
        Command::SetDirection(Direction::BottomToTop) => "SET_DIRECTION bottom-to-top".into(),
        Command::SetDirection(Direction::TopToBottom) => "SET_DIRECTION top-to-bottom".into(),
        Command::SetSpeed(value) => format!("SET_SPEED {value}"),
        Command::SetColor(value) => format!("SET_COLOR {value}"),
        Command::Clear => "CLEAR".into(),
        Command::Pause => "PAUSE".into(),
        Command::Resume => "RESUME".into(),
        Command::SetQueueCapacity(value) => format!("SET_QUEUE_CAPACITY {value}"),
    }
}

pub fn decode(line: &str) -> Result<Request, ProtocolError> {
    if let Some(command) = line.strip_prefix("CMD ") {
        return Ok(Request::Command(decode_command(command)?));
    }
    Ok(Request::Message(line.to_owned()))
}

fn decode_command(value: &str) -> Result<Command, ProtocolError> {
    let mut parts = value.splitn(2, ' ');
    let name = parts.next().unwrap_or_default();
    let arg = parts.next();

    match name {
        "SET_DIRECTION" => match arg.unwrap_or("").to_ascii_lowercase().as_str() {
            "bottom-to-top" | "up" => Ok(Command::SetDirection(Direction::BottomToTop)),
            "top-to-bottom" | "down" => Ok(Command::SetDirection(Direction::TopToBottom)),
            _ => Err(ProtocolError::InvalidCommand(format!("invalid direction: {}", arg.unwrap_or("")))),
        },
        "SET_SPEED" => parse_f64(arg, "speed").map(Command::SetSpeed),
        "SET_COLOR" => {
            let color = arg.unwrap_or("").trim();
            if color.is_empty() {
                Err(ProtocolError::InvalidCommand("color is required".into()))
            } else {
                Ok(Command::SetColor(color.into()))
            }
        }
        "CLEAR" if arg.is_none() => Ok(Command::Clear),
        "PAUSE" if arg.is_none() => Ok(Command::Pause),
        "RESUME" if arg.is_none() => Ok(Command::Resume),
        "SET_QUEUE_CAPACITY" => parse_usize(arg, "queue capacity").map(Command::SetQueueCapacity),
        _ => Err(ProtocolError::InvalidCommand(value.into())),
    }
}

fn parse_f64(value: Option<&str>, name: &str) -> Result<f64, ProtocolError> {
    value
        .ok_or_else(|| ProtocolError::InvalidCommand(format!("{name} is required")))?
        .parse()
        .map_err(|_| ProtocolError::InvalidCommand(format!("invalid {name}")))
}

fn parse_usize(value: Option<&str>, name: &str) -> Result<usize, ProtocolError> {
    value
        .ok_or_else(|| ProtocolError::InvalidCommand(format!("{name} is required")))?
        .parse()
        .map_err(|_| ProtocolError::InvalidCommand(format!("invalid {name}")))
}

pub fn send(stream: &mut UnixStream, text: &str, max_bytes: usize) -> Result<(), ProtocolError> {
    if text.as_bytes().len() > max_bytes {
        return Err(ProtocolError::MessageTooLarge(text.len()));
    }
    stream.write_all(text.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

pub fn send_request(stream: &mut UnixStream, request: &Request, max_bytes: usize) -> Result<(), ProtocolError> {
    let text = encode(request);
    send(stream, &text, max_bytes)
}

pub fn receive_lines<R: BufRead>(reader: &mut R, max_bytes: usize) -> Result<Vec<String>, ProtocolError> {
    let mut out = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 { break; }
        if line.ends_with('\n') { line.pop(); if line.ends_with('\r') { line.pop(); } }
        if line.as_bytes().len() > max_bytes { return Err(ProtocolError::MessageTooLarge(line.len())); }
        if !line.is_empty() { out.push(line.clone()); }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_plain_message() {
        assert_eq!(decode("hello").unwrap(), Request::Message("hello".into()));
    }

    #[test]
    fn round_trips_commands() {
        let requests = [
            Request::Command(Command::SetDirection(Direction::TopToBottom)),
            Request::Command(Command::SetSpeed(123.5)),
            Request::Command(Command::SetColor("red".into())),
            Request::Command(Command::Clear),
            Request::Command(Command::Pause),
            Request::Command(Command::Resume),
            Request::Command(Command::SetQueueCapacity(42)),
        ];
        for request in requests {
            let encoded = encode(&request);
            assert_eq!(decode(&encoded).unwrap(), request);
        }
    }
}
