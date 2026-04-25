use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::{Grid, Machine};

/// Top-level configuration for a MonoMouse instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// This machine's configuration.
    pub machine: Machine,
    /// The grid layout (only the server's grid is authoritative).
    pub grid: Grid,
    /// Network settings.
    pub network: NetworkConfig,
    /// Security settings.
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Port to listen on (server) or connect to (client).
    pub port: u16,
    /// Server address (client only).
    pub server_addr: Option<String>,
    /// Enable mDNS auto-discovery.
    pub mdns_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable TLS encryption.
    pub tls_enabled: bool,
    /// Path to TLS certificate.
    pub cert_path: Option<PathBuf>,
    /// Path to TLS private key.
    pub key_path: Option<PathBuf>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            port: 24800,
            server_addr: None,
            mdns_enabled: true,
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            tls_enabled: false,
            cert_path: None,
            key_path: None,
        }
    }
}

impl Config {
    /// Load config from the default location.
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config from {}", path.display()))?;
            let config: Config = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse config from {}", path.display()))?;
            Ok(config)
        } else {
            Ok(Self::default_config())
        }
    }

    /// Save config to the default location.
    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        Ok(())
    }

    /// Load config from a specific path.
    pub fn load_from(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        let config: Config = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Save config to a specific path.
    pub fn save_to(&self, path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    fn default_config() -> Self {
        let hostname = get_hostname();
        Self {
            machine: Machine::new(hostname, false),
            grid: Grid::new(),
            network: NetworkConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

/// Get the config file path: ~/.config/monomouse/config.json
fn config_path() -> Result<PathBuf> {
    let config_dir = if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("C:\\Users\\Default\\AppData\\Roaming"))
            .join("MonoMouse")
    } else {
        dirs_path()
    };
    Ok(config_dir.join("config.json"))
}

fn dirs_path() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".config")
        })
        .join("monomouse")
}

fn get_hostname() -> String {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/etc/hostname")
            .ok()
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".to_string())
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        "unknown".to_string()
    }
}
