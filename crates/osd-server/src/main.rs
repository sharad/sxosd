mod config;
mod x11_backend;

use clap::Parser;
use config::RuntimeConfig;
use osd_core::{Message, MessageBuffer, Rect, Renderer, ScrollEngine};
use std::io::{BufRead, BufReader};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{error, info};
use x11_backend::X11Renderer;

#[derive(Debug, Parser)]
#[command(name = "osd-server", about = "Scrolling X11 OSD server")]
struct Args {
    #[arg(short, long)] config: Option<PathBuf>,
    #[arg(long)] socket: Option<PathBuf>,
    #[arg(long)] direction: Option<String>,
    #[arg(long)] speed: Option<f64>,
    #[arg(long)] queue_capacity: Option<usize>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let mut config = RuntimeConfig::load(args.config.as_deref())?;
    if let Some(socket) = args.socket { config.socket = socket; }
    if let Some(speed) = args.speed { config.scroll.speed_px_per_second = speed; }
    if let Some(direction) = args.direction {
        config.scroll.direction = match direction.to_ascii_lowercase().as_str() {
            "bottom-to-top" | "up" => osd_core::Direction::BottomToTop,
            "top-to-bottom" | "down" => osd_core::Direction::TopToBottom,
            other => return Err(format!("invalid direction: {other}").into()),
        };
    }
    if let Some(capacity) = args.queue_capacity { config.queue = osd_core::QueuePolicy::new(capacity); }
    config.validate()?;

    if let Some(parent) = config.socket.parent() { std::fs::create_dir_all(parent)?; }
    let _ = std::fs::remove_file(&config.socket);
    let listener = UnixListener::bind(&config.socket)?;
    listener.set_nonblocking(true)?;
    info!(socket = ?config.socket, "OSD server started");

    // Start with the root X11 screen. Monitor-specific selection remains an
    // isolated backend concern and can be wired to XRandR without touching core.
    let mut renderer = X11Renderer::new(
        &config.font,
        &config.foreground,
    )?;

    let (tx, rx) = mpsc::channel::<Message>();
    let mut queue = MessageBuffer::new(config.queue);
    let mut scroll = ScrollEngine::new(config.scroll);
    let mut last = Instant::now();

    loop {
        accept_clients(&listener, tx.clone());
        drain_messages(&rx, &mut queue, &mut scroll);

        let now = Instant::now();
        scroll.advance(now.duration_since(last));
        last = now;

        renderer.pump_events();
        let size = renderer.display_size();
        let region = Rect {
            x: (size.width - 500.0).max(0.0),
            y: 0.0,
            width: 500.0_f64.min(size.width),
            height: size.height,
        };
        let scene = scroll.layout(&queue, region);
        renderer.render(&scene)?;

        // 60-ish FPS. The animation position is time-based, so scheduling
        // jitter does not accumulate into animation drift.
        thread::sleep(Duration::from_millis(16));
    }
}

fn accept_clients(listener: &UnixListener, tx: Sender<Message>) {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let client_tx = tx.clone();
                thread::spawn(move || handle_client(stream, client_tx));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) => {
                error!(error = %e, "accept failed");
                break;
            }
        }
    }
}

fn handle_client(stream: UnixStream, tx: Sender<Message>) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let text = line.trim_end_matches(['\r', '\n']);
                if !text.is_empty() && tx.send(Message::new(text)).is_err() {
                    break;
                }
            }
            Err(e) => {
                error!(error = %e, "client read failed");
                break;
            }
        }
    }
}

fn drain_messages(rx: &Receiver<Message>, queue: &mut MessageBuffer, scroll: &mut ScrollEngine) {
    let mut received = false;
    while let Ok(message) = rx.try_recv() {
        queue.push(message);
        received = true;
    }
    if received {
        scroll.reset_if_needed();
    }
}
