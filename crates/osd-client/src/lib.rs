use std::io::{self, BufWriter};
use std::os::unix::net::UnixStream;
use std::path::Path;
use thiserror::Error;

pub use osd_protocol::{Command, Direction, Request};

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("cannot connect to OSD server: {0}")]
    Connect(#[source] io::Error),
    #[error(transparent)]
    Protocol(#[from] osd_protocol::ProtocolError),
}

#[derive(Debug)]
pub struct Client {
    stream: BufWriter<UnixStream>,
    max_message_bytes: usize,
}

impl Client {
    pub fn connect(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let stream = UnixStream::connect(path).map_err(ClientError::Connect)?;
        Ok(Self {
            stream: BufWriter::new(stream),
            max_message_bytes: osd_protocol::DEFAULT_MAX_MESSAGE_BYTES,
        })
    }

    pub fn send(&mut self, text: impl AsRef<str>) -> Result<(), ClientError> {
        self.send_request(Request::Message(text.as_ref().to_owned()))
    }

    pub fn command(&mut self, command: Command) -> Result<(), ClientError> {
        self.send_request(Request::Command(command))
    }

    pub fn set_direction(&mut self, direction: Direction) -> Result<(), ClientError> {
        self.command(Command::SetDirection(direction))
    }

    fn send_request(&mut self, request: Request) -> Result<(), ClientError> {
        let stream = self.stream.get_mut();
        osd_protocol::send(stream, &request, self.max_message_bytes)?;
        Ok(())
    }
}
