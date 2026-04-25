use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a single physical monitor attached to a machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Monitor {
    /// Unique identifier for this monitor.
    pub id: Uuid,
    /// Machine this monitor belongs to.
    pub machine_id: Uuid,
    /// Human-readable name (e.g., "HDMI-1", "DP-2").
    pub name: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// X offset in the machine's local coordinate space.
    pub x: i32,
    /// Y offset in the machine's local coordinate space.
    pub y: i32,
    /// Scale factor (e.g., 1.0 for 100%, 1.5 for 150%).
    pub scale: f64,
    /// Position in the global grid (column).
    pub grid_col: Option<u32>,
    /// Position in the global grid (row).
    pub grid_row: Option<u32>,
}

impl Monitor {
    pub fn new(machine_id: Uuid, name: String, width: u32, height: u32, x: i32, y: i32) -> Self {
        Self {
            id: Uuid::new_v4(),
            machine_id,
            name,
            width,
            height,
            x,
            y,
            scale: 1.0,
            grid_col: None,
            grid_row: None,
        }
    }

    /// Returns the bounding rectangle in local coordinates.
    pub fn bounds(&self) -> (i32, i32, i32, i32) {
        (self.x, self.y, self.x + self.width as i32, self.y + self.height as i32)
    }

    /// Checks if a point (in local coordinates) is within this monitor.
    pub fn contains(&self, px: i32, py: i32) -> bool {
        let (x1, y1, x2, y2) = self.bounds();
        px >= x1 && px < x2 && py >= y1 && py < y2
    }
}
