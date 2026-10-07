use clap::{Parser, Subcommand};
use osd_client::Client;
use osd_protocol::{Command, Direction};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "osd-client", about = "Send messages or commands to an OSD server")]
struct Args {
    /// Unix socket used by the OSD server.
    #[arg(short, long, default_value = "/run/user/1000/osd.sock", global = true)]
    socket: PathBuf,

    #[command(subcommand)]
    command: Option<CommandArgs>,

    /// Message text. Multiple arguments are joined with spaces.
    #[arg(trailing_var_arg = true)]
    message: Vec<String>,
}

#[derive(Debug, Subcommand)]
enum CommandArgs {
    /// Send a runtime command to the server.
    Cmd {
        #[command(subcommand)]
        command: ServerCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ServerCommand {
    /// Change scroll direction.
    Direction { value: DirectionValue },
    /// Change scroll speed in pixels per second.
    Speed { pixels_per_second: f64 },
    /// Change foreground color.
    Color { value: String },
    /// Clear all queued messages.
    Clear,
    /// Pause animation while continuing to accept messages.
    Pause,
    /// Resume animation from the current position.
    Resume,
    /// Change maximum queue capacity. Shrinking discards oldest messages immediately.
    QueueCapacity { size: usize },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum DirectionValue {
    BottomToTop,
    TopToBottom,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut client = Client::connect(args.socket)?;

    match args.command {
        Some(CommandArgs::Cmd { command }) => {
            let command = match command {
                ServerCommand::Direction { value } => Command::SetDirection(match value {
                    DirectionValue::BottomToTop => Direction::BottomToTop,
                    DirectionValue::TopToBottom => Direction::TopToBottom,
                }),
                ServerCommand::Speed { pixels_per_second } => Command::SetSpeed(pixels_per_second),
                ServerCommand::Color { value } => Command::SetColor(value),
                ServerCommand::Clear => Command::Clear,
                ServerCommand::Pause => Command::Pause,
                ServerCommand::Resume => Command::Resume,
                ServerCommand::QueueCapacity { size } => Command::SetQueueCapacity(size),
            };
            client.command(command)?;
        }
        None => {
            if args.message.is_empty() {
                return Err("a message or command is required".into());
            }
            client.send(args.message.join(" "))?;
        }
    }

    Ok(())
}
