use serde::{Deserialize, Serialize};
use uuid::Uuid;
use thiserror::Error;
use crate::Monitor;

#[derive(Debug, Error)]
pub enum GridError {
    #[error("monitor {0} not found in grid")]
    MonitorNotFound(Uuid),
    #[error("grid position ({col}, {row}) is already occupied")]
    PositionOccupied { col: u32, row: u32 },
}

/// Describes which edge of a monitor the cursor is leaving from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GridEdge {
    Top,
    Bottom,
    Left,
    Right,
}

/// Describes a transition from one monitor to another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeTransition {
    pub from_monitor: Uuid,
    pub from_edge: GridEdge,
    pub to_monitor: Uuid,
    pub to_machine: Uuid,
    /// Where on the target monitor the cursor should enter (0.0 = start, 1.0 = end).
    pub position_ratio: f64,
}

/// The global grid that maps all monitors across all machines.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Grid {
    pub monitors: Vec<Monitor>,
    pub transitions: Vec<EdgeTransition>,
}

impl Grid {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a monitor to the grid at a specific grid position.
    pub fn place_monitor(&mut self, mut monitor: Monitor, col: u32, row: u32) -> Result<(), GridError> {
        if self.monitor_at(col, row).is_some() {
            return Err(GridError::PositionOccupied { col, row });
        }
        monitor.grid_col = Some(col);
        monitor.grid_row = Some(row);
        self.monitors.push(monitor);
        Ok(())
    }

    /// Find the monitor at a given grid position.
    pub fn monitor_at(&self, col: u32, row: u32) -> Option<&Monitor> {
        self.monitors.iter().find(|m| m.grid_col == Some(col) && m.grid_row == Some(row))
    }

    /// Rebuild edge transitions based on grid adjacency.
    /// Monitors that are adjacent in the grid (horizontally or vertically)
    /// automatically get transitions created between them.
    pub fn rebuild_transitions(&mut self) {
        self.transitions.clear();

        let monitors: Vec<_> = self.monitors.clone();

        for monitor in &monitors {
            let (col, row) = match (monitor.grid_col, monitor.grid_row) {
                (Some(c), Some(r)) => (c, r),
                _ => continue,
            };

            // Right neighbor
            if let Some(neighbor) = self.monitor_at(col + 1, row) {
                self.transitions.push(EdgeTransition {
                    from_monitor: monitor.id,
                    from_edge: GridEdge::Right,
                    to_monitor: neighbor.id,
                    to_machine: neighbor.machine_id,
                    position_ratio: 0.0,
                });
            }

            // Left neighbor
            if col > 0 {
                if let Some(neighbor) = self.monitor_at(col - 1, row) {
                    self.transitions.push(EdgeTransition {
                        from_monitor: monitor.id,
                        from_edge: GridEdge::Left,
                        to_monitor: neighbor.id,
                        to_machine: neighbor.machine_id,
                        position_ratio: 0.0,
                    });
                }
            }

            // Bottom neighbor
            if let Some(neighbor) = self.monitor_at(col, row + 1) {
                self.transitions.push(EdgeTransition {
                    from_monitor: monitor.id,
                    from_edge: GridEdge::Bottom,
                    to_monitor: neighbor.id,
                    to_machine: neighbor.machine_id,
                    position_ratio: 0.0,
                });
            }

            // Top neighbor
            if row > 0 {
                if let Some(neighbor) = self.monitor_at(col, row - 1) {
                    self.transitions.push(EdgeTransition {
                        from_monitor: monitor.id,
                        from_edge: GridEdge::Top,
                        to_monitor: neighbor.id,
                        to_machine: neighbor.machine_id,
                        position_ratio: 0.0,
                    });
                }
            }
        }
    }

    /// Find the transition for a cursor leaving a specific monitor from a specific edge.
    pub fn find_transition(&self, monitor_id: Uuid, edge: GridEdge) -> Option<&EdgeTransition> {
        self.transitions.iter().find(|t| t.from_monitor == monitor_id && t.from_edge == edge)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_monitor(machine_id: Uuid, name: &str) -> Monitor {
        Monitor::new(machine_id, name.to_string(), 1920, 1080, 0, 0)
    }

    #[test]
    fn test_place_and_find_monitors() {
        let mut grid = Grid::new();
        let machine_a = Uuid::new_v4();
        let machine_b = Uuid::new_v4();

        let mon1 = make_monitor(machine_a, "HDMI-1");
        let mon2 = make_monitor(machine_b, "DP-1");

        grid.place_monitor(mon1.clone(), 0, 0).unwrap();
        grid.place_monitor(mon2.clone(), 1, 0).unwrap();

        assert!(grid.monitor_at(0, 0).is_some());
        assert!(grid.monitor_at(1, 0).is_some());
        assert!(grid.monitor_at(2, 0).is_none());
    }

    #[test]
    fn test_rebuild_transitions() {
        let mut grid = Grid::new();
        let machine_a = Uuid::new_v4();
        let machine_b = Uuid::new_v4();

        let mon1 = make_monitor(machine_a, "HDMI-1");
        let mon1_id = mon1.id;
        let mon2 = make_monitor(machine_b, "DP-1");
        let mon2_id = mon2.id;

        grid.place_monitor(mon1, 0, 0).unwrap();
        grid.place_monitor(mon2, 1, 0).unwrap();
        grid.rebuild_transitions();

        // mon1 right -> mon2
        let t = grid.find_transition(mon1_id, GridEdge::Right);
        assert!(t.is_some());
        assert_eq!(t.unwrap().to_monitor, mon2_id);

        // mon2 left -> mon1
        let t = grid.find_transition(mon2_id, GridEdge::Left);
        assert!(t.is_some());
        assert_eq!(t.unwrap().to_monitor, mon1_id);

        // No vertical transitions
        assert!(grid.find_transition(mon1_id, GridEdge::Top).is_none());
        assert!(grid.find_transition(mon1_id, GridEdge::Bottom).is_none());
    }

    #[test]
    fn test_position_occupied() {
        let mut grid = Grid::new();
        let machine = Uuid::new_v4();
        let mon1 = make_monitor(machine, "HDMI-1");
        let mon2 = make_monitor(machine, "HDMI-2");

        grid.place_monitor(mon1, 0, 0).unwrap();
        assert!(grid.place_monitor(mon2, 0, 0).is_err());
    }
}
