use anyhow::{Context, Result};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

const SERVICE_TYPE: &str = "_monomouse._tcp.local.";

/// Discovered MonoMouse instance on the network.
#[derive(Debug, Clone)]
pub struct DiscoveredHost {
    pub name: String,
    pub addr: IpAddr,
    pub port: u16,
    pub is_server: bool,
}

/// mDNS service for discovering and advertising MonoMouse instances.
pub struct Discovery {
    daemon: ServiceDaemon,
    discovered: Arc<Mutex<HashMap<String, DiscoveredHost>>>,
}

impl Discovery {
    pub fn new() -> Result<Self> {
        let daemon = ServiceDaemon::new()
            .context("Failed to create mDNS daemon")?;

        Ok(Self {
            daemon,
            discovered: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Register this machine as a MonoMouse server.
    pub fn register_server(&self, hostname: &str, port: u16) -> Result<()> {
        let mut properties = HashMap::new();
        properties.insert("role".to_string(), "server".to_string());

        let service = ServiceInfo::new(
            SERVICE_TYPE,
            hostname,
            hostname,
            "",
            port,
            properties,
        )
        .context("Failed to create service info")?;

        self.daemon
            .register(service)
            .context("Failed to register mDNS service")?;

        info!("Registered mDNS server: {hostname} on port {port}");
        Ok(())
    }

    /// Register this machine as a MonoMouse client.
    pub fn register_client(&self, hostname: &str, port: u16) -> Result<()> {
        let mut properties = HashMap::new();
        properties.insert("role".to_string(), "client".to_string());

        let service = ServiceInfo::new(
            SERVICE_TYPE,
            hostname,
            hostname,
            "",
            port,
            properties,
        )
        .context("Failed to create service info")?;

        self.daemon
            .register(service)
            .context("Failed to register mDNS service")?;

        info!("Registered mDNS client: {hostname}");
        Ok(())
    }

    /// Start browsing for MonoMouse services on the network.
    pub fn browse(&self) -> Result<()> {
        let receiver = self
            .daemon
            .browse(SERVICE_TYPE)
            .context("Failed to start mDNS browse")?;

        let discovered = Arc::clone(&self.discovered);

        std::thread::spawn(move || {
            while let Ok(event) = receiver.recv() {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let name = info.get_fullname().to_string();
                        let port = info.get_port();
                        let is_server = info
                            .get_properties()
                            .get("role")
                            .map(|v| v.val_str() == "server")
                            .unwrap_or(false);

                        // Get first address
                        if let Some(addr) = info.get_addresses().iter().next() {
                            let host = DiscoveredHost {
                                name: info.get_hostname().to_string(),
                                addr: *addr,
                                port,
                                is_server,
                            };

                            info!(
                                "Discovered MonoMouse {}: {} at {}:{}",
                                if is_server { "server" } else { "client" },
                                host.name,
                                host.addr,
                                host.port,
                            );

                            discovered
                                .lock()
                                .unwrap()
                                .insert(name, host);
                        }
                    }
                    ServiceEvent::ServiceRemoved(_, name) => {
                        info!("MonoMouse service removed: {name}");
                        discovered.lock().unwrap().remove(&name);
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }

    /// Get all currently discovered hosts.
    pub fn get_discovered(&self) -> Vec<DiscoveredHost> {
        self.discovered
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }

    /// Find the first discovered server.
    pub fn find_server(&self) -> Option<DiscoveredHost> {
        self.discovered
            .lock()
            .unwrap()
            .values()
            .find(|h| h.is_server)
            .cloned()
    }

    pub fn shutdown(self) -> Result<()> {
        self.daemon.shutdown().context("Failed to shutdown mDNS daemon")?;
        Ok(())
    }
}
