//! Small, line-oriented wire protocol used by the Unix-socket transport.
//!
//! Requests are UTF-8 text terminated by `\\n`.
//!
//! Message requests use the legacy plain-line form:
//!     <text>
//!
//! Commands use a tagged form:
//!     CMD SET_DIRECTION bottom-to-top
//!     CMD SET_DIRECTION top-to-bottom
//!
//! Keeping ordinary messages as plain lines preserves compatibility while
//! leaving room for additional commands.

use std::io::{self, BufRead, Write};
use std::os::unix::net::UnixStream;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    BottomToTop,
    TopToBottom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetDirection(Direction),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

pub fn send(stream: &mut UnixStream, request: &Request, max_bytes: usize) -> Result<(), ProtocolError> {
    let line = encode(request)?;
    if line.as_bytes().len() > max_bytes {
        return Err(ProtocolError::MessageTooLarge(line.len()));
    }
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

pub fn receive_lines<R: BufRead>(reader: &mut R, max_bytes: usize) -> Result<Vec<Request>, ProtocolError> {
    let mut out = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        if line.as_bytes().len() > max_bytes {
            return Err(ProtocolError::MessageTooLarge(line.len()));
        }
        if !line.is_empty() {
            out.push(decode(&line)?);
        }
    }
    Ok(out)
}

pub fn encode(request: &Request) -> Result<String, ProtocolError> {
    match request {
        Request::Message(text) => Ok(text.clone()),
        Request::Command(Command::SetDirection(direction)) => Ok(format!(
            "CMD SET_DIRECTION {}",
            match direction {
                Direction::BottomToTop => "bottom-to-top",
                Direction::TopToBottom => "top-to-bottom",
            }
        )),
    }
}

pub fn decode(line: &str) -> Result<Request, ProtocolError> {
    let Some(rest) = line.strip_prefix("CMD ") else {
        return Ok(Request::Message(line.to_owned()));
    };

    let mut parts = rest.split_whitespace();
    let command = parts
        .next()
        .ok_or_else(|| ProtocolError::InvalidCommand(line.to_owned()))?;

    match command {
        "SET_DIRECTION" => {
            let value = parts
                .next()
                .ok_or_else(|| ProtocolError::InvalidCommand(line.to_owned()))?;
            if parts.next().is_some() {
                return Err(ProtocolError::InvalidCommand(line.to_owned()));
            }
            let direction = match value.to_ascii_lowercase().as_str() {
                "bottom-to-top" | "up" => Direction::BottomToTop,
                "top-to-bottom" | "down" => Direction::TopToBottom,
                _ => return Err(ProtocolError::InvalidCommand(line.to_owned())),
            };
            Ok(Request::Command(Command::SetDirection(direction)))
        }
        _ => Err(ProtocolError::InvalidCommand(line.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_line_is_a_message() {
        assert_eq!(
            decode("hello").unwrap(),
            Request::Message("hello".into());
    }

    #[test]
    fn direction_command_round_trips() {
        let request = Request::Command(Command::SetDirection(Direction::TopToBottom));
        let encoded = encode(&request).unwrap();
        assert_eq!(decode(&encoded), Ok(request));
    }

    #[test]
    fn invalid_command_is_rejected() {
        assert!(decode("CMD UNKNOWN value").is_err());
        assert!(decode("CMD SET_DIRECTION sideways").is_err());
    }
}
