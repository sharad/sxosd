use clap::Parser;
use osd_client::Client;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "osd-client", about = "Send a message to an OSD server")]
struct Args {
    /// Unix socket used by the OSD server.
    #[arg(short, long, default_value = "/run/user/1000/osd.sock")]
    socket: PathBuf,

    /// Message text. Multiple arguments are joined with spaces.
    #[arg(required = true)]
    message: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut client = Client::connect(args.socket)?;
    client.send(args.message.join(" "))?;
    Ok(())
}
