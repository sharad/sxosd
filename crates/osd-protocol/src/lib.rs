//! Small, line-oriented wire protocol used by the initial Unix-socket transport.
//!
//! Each request is UTF-8 text terminated by `\n`. Empty lines are ignored.

use std::io::{self, BufRead, Write};
use std::os::unix::net::UnixStream;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("message is too large ({0} bytes)")]
    MessageTooLarge(usize),
}

pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 64 * 1024;

pub fn send(stream: &mut UnixStream, text: &str, max_bytes: usize) -> Result<(), ProtocolError> {
    if text.as_bytes().len() > max_bytes { return Err(ProtocolError::MessageTooLarge(text.len())); }
    stream.write_all(text.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
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
