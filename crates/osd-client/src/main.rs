use clap::{Parser, Subcommand};
use osd_client::{Client, Direction};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "osd-client", about = "Send messages and commands to an OSD server")]
struct Args {
    /// Unix socket used by the OSD server.
    #[arg(short, long, default_value = "/run/user/1000/osd.sock", global = true)]
    socket: PathBuf,

    #[command(subcommand)]
    request: Request,
}

#[derive(Debug, Subcommand)]
enum Request {
    /// Display a message.
    Message {
        /// Message text. Multiple arguments are joined with spaces.
        #[arg(required = true)]
        text: Vec<String>,
    },

    /// Send a server command.
    Cmd {
        #[command(subcommand)]
        command: Command,
    },
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Change the scrolling direction.
    Direction {
        #[arg(value_parser = parse_direction)]
        direction: Direction,
    },
}

fn parse_direction(value: &str) -> Result<Direction, String> {
    match value.to_ascii_lowercase().as_str() {
        "bottom-to-top" | "up" => Ok(Direction::BottomToTop),
        "top-to-bottom" | "down" => Ok(Direction::TopToBottom),
        _ => Err(format!("invalid direction: {value}; use up/down or bottom-to-top/top-to-bottom")),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut client = Client::connect(args.socket)?;

    match args.request {
        Request::Message { text } => client.send(text.join(" "))?,
        Request::Cmd { command } => match command {
            Command::Direction { direction } => client.set_direction(direction)?,
        },
    }

    Ok(())
}
