use osd_core::{Direction, QueuePolicy, ScrollConfig};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read configuration: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid TOML configuration: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileConfig {
    pub server: Option<ServerConfig>,
    pub display: Option<DisplayConfig>,
    pub scroll: Option<ScrollFileConfig>,
    pub queue: Option<QueueFileConfig>,
    pub appearance: Option<AppearanceConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig { pub socket: Option<PathBuf> }
#[derive(Debug, Clone, Deserialize)]
pub struct DisplayConfig { pub monitor: Option<String>, pub position: Option<String>, pub direction: Option<String> }
#[derive(Debug, Clone, Deserialize)]
pub struct ScrollFileConfig { pub speed: Option<f64>, pub line_height: Option<f64>, pub gap: Option<f64> }
#[derive(Debug, Clone, Deserialize)]
pub struct QueueFileConfig { pub capacity: Option<usize> }
#[derive(Debug, Clone, Deserialize)]
pub struct AppearanceConfig { pub font: Option<String>, pub foreground: Option<String>, pub background: Option<String>, pub opacity: Option<f64> }

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub socket: PathBuf,
    pub monitor: String,
    pub position: String,
    pub scroll: ScrollConfig,
    pub queue: QueuePolicy,
    pub font: String,
    pub foreground: String,
    pub background: String,
    pub opacity: f64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            socket: default_socket(),
            monitor: "primary".into(),
            position: "right".into(),
            scroll: ScrollConfig::default(),
            queue: QueuePolicy::new(20),
            font: "-misc-fixed-medium-r-semicondensed--*-*-*-*-c-*-*-*".into(),
            foreground: "red".into(),
            background: "transparent".into(),
            opacity: 1.0,
        }
    }
}

fn default_socket() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("osd.sock")
}

impl RuntimeConfig {
    pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
        let mut cfg = Self::default();
        if let Some(path) = path {
            let text = fs::read_to_string(path)?;
            let file: FileConfig = toml::from_str(&text)?;
            cfg.apply(file)?;
        }
        cfg.validate()?;
        Ok(cfg)
    }

    fn apply(&mut self, file: FileConfig) -> Result<(), ConfigError> {
        if let Some(v) = file.server.and_then(|x| x.socket) { self.socket = v; }
        if let Some(v) = file.display {
            if let Some(x) = v.monitor { self.monitor = x; }
            if let Some(x) = v.position { self.position = x; }
            if let Some(x) = v.direction { self.scroll.direction = parse_direction(&x)?; }
        }
        if let Some(v) = file.scroll {
            if let Some(x) = v.speed { self.scroll.speed_px_per_second = x; }
            if let Some(x) = v.line_height { self.scroll.line_height = x; }
            if let Some(x) = v.gap { self.scroll.gap = x; }
        }
        if let Some(v) = file.queue { if let Some(x) = v.capacity { self.queue = QueuePolicy::new(x); } }
        if let Some(v) = file.appearance {
            if let Some(x) = v.font { self.font = x; }
            if let Some(x) = v.foreground { self.foreground = x; }
            if let Some(x) = v.background { self.background = x; }
            if let Some(x) = v.opacity { self.opacity = x; }
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.scroll.speed_px_per_second <= 0.0 { return Err(ConfigError::Invalid("scroll.speed must be > 0".into())); }
        if self.scroll.line_height <= 0.0 { return Err(ConfigError::Invalid("scroll.line_height must be > 0".into())); }
        if self.scroll.gap < 0.0 { return Err(ConfigError::Invalid("scroll.gap must be >= 0".into())); }
        if !(0.0..=1.0).contains(&self.opacity) { return Err(ConfigError::Invalid("appearance.opacity must be between 0 and 1".into())); }
        Ok(())
    }
}

fn parse_direction(value: &str) -> Result<Direction, ConfigError> {
    match value.to_ascii_lowercase().as_str() {
        "bottom-to-top" | "up" => Ok(Direction::BottomToTop),
        "top-to-bottom" | "down" => Ok(Direction::TopToBottom),
        _ => Err(ConfigError::Invalid(format!("unknown direction: {value}"))),
    }
}
