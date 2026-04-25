use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::net::IpAddr;
use crate::Monitor;

/// Represents a machine running a MonoMouse agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Machine {
    pub id: Uuid,
    pub name: String,
    pub addr: Option<IpAddr>,
    pub port: u16,
    pub monitors: Vec<Monitor>,
    pub is_server: bool,
}

impl Machine {
    pub fn new(name: String, is_server: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            addr: None,
            port: 24800,
            monitors: Vec::new(),
            is_server,
        }
    }
}
