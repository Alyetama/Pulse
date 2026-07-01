//! Persistent user preferences, stored as JSON in Application Support.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Which live metric (if any) is shown as text next to the menu-bar icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrayMetric {
    None,
    Cpu,
    Gpu,
    Mem,
}

impl TrayMetric {
    pub fn label(self) -> &'static str {
        match self {
            TrayMetric::None => "Icon only",
            TrayMetric::Cpu => "CPU %",
            TrayMetric::Gpu => "GPU %",
            TrayMetric::Mem => "Memory %",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Sampling interval in milliseconds (clamped to a sane range at use).
    pub poll_interval_ms: u64,
    /// Metric rendered as a number beside the tray icon.
    pub tray_metric: TrayMetric,
    /// Start Pulse automatically at login (managed via a LaunchAgent).
    pub launch_at_login: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            poll_interval_ms: 1500,
            tray_metric: TrayMetric::Cpu,
            launch_at_login: false,
        }
    }
}

impl Config {
    fn dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Pulse")
    }

    fn path() -> PathBuf {
        Self::dir().join("config.json")
    }

    /// Load config from disk, falling back to defaults on any error.
    pub fn load() -> Self {
        match std::fs::read_to_string(Self::path()) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist config to disk (best effort; errors are logged, not fatal).
    pub fn save(&self) {
        let dir = Self::dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("[pulse] could not create config dir: {e}");
            return;
        }
        match serde_json::to_string_pretty(self) {
            Ok(s) => {
                if let Err(e) = std::fs::write(Self::path(), s) {
                    eprintln!("[pulse] could not write config: {e}");
                }
            }
            Err(e) => eprintln!("[pulse] could not serialize config: {e}"),
        }
    }

    /// Interval clamped to a range that keeps the monitor responsive but light.
    pub fn interval_ms(&self) -> u64 {
        self.poll_interval_ms.clamp(500, 10_000)
    }
}
