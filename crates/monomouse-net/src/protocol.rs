use monomouse_core::{InputEvent, Machine, Grid, GridEdge};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Messages exchanged between server and clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    /// Client announces itself and its monitors.
    Hello(Machine),

    /// Server acknowledges client connection.
    Welcome { server_id: Uuid },

    /// Server sends the current grid configuration.
    GridConfig(Grid),

    /// Server forwards an input event to a client.
    Input(InputEvent),

    /// Server tells client to take control (cursor has entered their monitor).
    Activate { monitor_id: Uuid, x: i32, y: i32 },

    /// Server tells client it no longer has focus.
    Deactivate,

    /// Client tells server the cursor has left its screen edge.
    EdgeHit {
        monitor_id: Uuid,
        edge: GridEdge,
        /// Relative position along the edge (0.0 = top/left, 1.0 = bottom/right).
        position_ratio: f64,
    },

    /// Client reports its current cursor position (for edge detection on server side).
    CursorPos { x: i32, y: i32 },

    /// Clipboard content sync.
    Clipboard(String),

    /// Keepalive ping.
    Ping,

    /// Keepalive pong.
    Pong,

    /// Server requests client info refresh.
    RefreshMonitors,

    /// Grid update (server pushes new grid to all clients).
    GridUpdate(Grid),
}
