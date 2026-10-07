mod config;

use clap::Parser;
use config::RuntimeConfig;
use osd_core::{Direction, Message, MessageBuffer, Rect, Renderer, ScrollEngine};
use std::io::{BufRead, BufReader};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{error, info};
use osd_x11::X11Renderer;

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

    let (tx, rx) = mpsc::channel::<osd_protocol::Request>();
    let mut queue = MessageBuffer::new(config.queue);
    let mut scroll = ScrollEngine::new(config.scroll);
    let mut last = Instant::now();
    let mut paused = false;

    loop {
        accept_clients(&listener, tx.clone());
        drain_requests(&rx, &mut queue, &mut scroll, &mut renderer, &mut paused);

        let now = Instant::now();
        if !paused {
            scroll.advance(now.duration_since(last));
        }
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

fn accept_clients(listener: &UnixListener, tx: Sender<osd_protocol::Request>) {
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

fn handle_client(stream: UnixStream, tx: Sender<osd_protocol::Request>) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let text = line.trim_end_matches(['\r', '\n']);
                if text.is_empty() {
                    continue;
                }
                match osd_protocol::decode(text) {
                    Ok(request) => {
                        if tx.send(request).is_err() {
                            break;
                        }
                    }
                    Err(e) => error!(error = %e, "invalid client request"),
                }
            }
            Err(e) => {
                error!(error = %e, "client read failed");
                break;
            }
        }
    }
}

fn drain_requests(
    rx: &Receiver<osd_protocol::Request>,
    queue: &mut MessageBuffer,
    scroll: &mut ScrollEngine,
    renderer: &mut X11Renderer,
    paused: &mut bool,
) {
    while let Ok(request) = rx.try_recv() {
        match request {
            osd_protocol::Request::Message(text) => {
                queue.push(Message::new(text));
            }
            osd_protocol::Request::Command(command) => {
                if let Err(e) = apply_command(command, queue, scroll, renderer, paused) {
                    error!(error = %e, "runtime command failed");
                }
            }
        }
    }
}

fn apply_command(
    command: osd_protocol::Command,
    queue: &mut MessageBuffer,
    scroll: &mut ScrollEngine,
    renderer: &mut X11Renderer,
    paused: &mut bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        osd_protocol::Command::SetDirection(direction) => {
            scroll.set_direction(match direction {
                osd_protocol::Direction::BottomToTop => Direction::BottomToTop,
                osd_protocol::Direction::TopToBottom => Direction::TopToBottom,
            });
        }
        osd_protocol::Command::SetSpeed(speed) => {
            scroll.set_speed_px_per_second(speed)?;
        }
        osd_protocol::Command::SetColor(color) => {
            renderer.set_foreground(&color)?;
        }
        osd_protocol::Command::Clear => {
            queue.clear();
            scroll.reset();
        }
        osd_protocol::Command::Pause => {
            *paused = true;
        }
        osd_protocol::Command::Resume => {
            *paused = false;
        }
        osd_protocol::Command::SetQueueCapacity(capacity) => {
            if capacity == 0 {
                return Err("queue capacity must be greater than zero".into());
            }
            queue.set_capacity(capacity);
        }
    }
    Ok(())
}
