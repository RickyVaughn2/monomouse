use anyhow::Result;
use monomouse_core::{Grid, GridEdge, Machine};
use monomouse_input::{create_injector, detect_monitors, InputInjector};
use monomouse_net::protocol::Message;
use monomouse_net::transport::Connection;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

struct ClientState {
    machine: Machine,
    grid: Grid,
    active: bool,
    active_monitor_id: Option<Uuid>,
    cursor_x: i32,
    cursor_y: i32,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "monomouse=debug,info".parse().unwrap()),
        )
        .init();

    let server_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:24800".to_string());

    info!("MonoMouse Client v{}", env!("CARGO_PKG_VERSION"));

    // Detect local monitors
    let monitors = detect_monitors()?;
    info!("Detected {} local monitors:", monitors.len());
    for mon in &monitors {
        info!(
            "  {} ({}x{} at +{}+{} scale={:.1})",
            mon.name, mon.width, mon.height, mon.x, mon.y, mon.scale
        );
    }

    let mut machine = Machine::new(hostname(), false);
    machine.monitors = monitors;

    // Create input injector ONCE, before the reconnect loop.
    // This way if it fails (permissions), we bail immediately instead of
    // reconnecting in a loop and adding duplicate monitors each time.
    let injector = Arc::new(tokio::sync::Mutex::new(create_injector()?));

    loop {
        info!("Connecting to server at {server_addr}...");
        match run_client(&server_addr, &machine, Arc::clone(&injector)).await {
            Ok(()) => {
                info!("Disconnected cleanly");
                break;
            }
            Err(e) => {
                error!("Connection error: {e}");
                info!("Reconnecting in 3 seconds...");
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }
        }
    }

    Ok(())
}

async fn run_client(
    server_addr: &str,
    machine: &Machine,
    injector: Arc<tokio::sync::Mutex<Box<dyn InputInjector>>>,
) -> Result<()> {
    let mut conn = Connection::connect(server_addr).await?;

    conn.send(&Message::Hello(machine.clone())).await?;

    let msg = conn.recv().await?;
    match msg {
        Message::Welcome { server_id } => {
            info!("Connected to server {server_id}");
        }
        other => anyhow::bail!("Expected Welcome, got: {other:?}"),
    };

    let msg = conn.recv().await?;
    let grid = match msg {
        Message::GridConfig(grid) => {
            info!(
                "Received grid with {} monitors, {} transitions",
                grid.monitors.len(),
                grid.transitions.len()
            );
            grid
        }
        other => anyhow::bail!("Expected GridConfig, got: {other:?}"),
    };

    let state = Arc::new(RwLock::new(ClientState {
        machine: machine.clone(),
        grid,
        active: false,
        active_monitor_id: None,
        cursor_x: 0,
        cursor_y: 0,
    }));

    let (mut reader, writer) = conn.split();
    let writer = Arc::new(tokio::sync::Mutex::new(writer));

    // Keepalive
    let writer_keepalive = Arc::clone(&writer);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            let mut w = writer_keepalive.lock().await;
            if w.send(&Message::Ping).await.is_err() {
                break;
            }
        }
    });

    // Main message loop
    loop {
        let msg = reader.recv().await?;
        match msg {
            Message::Input(event) => {
                let mut s = state.write().await;
                if !s.active {
                    continue;
                }

                match &event {
                    monomouse_core::InputEvent::MouseMove { dx, dy } => {
                        s.cursor_x += dx;
                        s.cursor_y += dy;
                    }
                    monomouse_core::InputEvent::MouseAbsolute { x, y } => {
                        s.cursor_x = *x;
                        s.cursor_y = *y;
                    }
                    _ => {}
                }

                if let Some(monitor_id) = s.active_monitor_id {
                    let mon_info = s.grid.monitors.iter()
                        .find(|m| m.id == monitor_id)
                        .map(|m| (m.width, m.height));

                    if let Some((w, h)) = mon_info {
                        if let Some((edge, ratio)) = check_cursor_edge_raw(w, h, s.cursor_x, s.cursor_y) {
                            s.cursor_x = s.cursor_x.clamp(0, w as i32 - 1);
                            s.cursor_y = s.cursor_y.clamp(0, h as i32 - 1);

                            let msg = Message::EdgeHit {
                                monitor_id,
                                edge,
                                position_ratio: ratio,
                            };
                            let mut wr = writer.lock().await;
                            wr.send(&msg).await?;

                            s.active = false;
                            s.active_monitor_id = None;
                            info!("Edge hit: {:?}, cursor returned to server", edge);
                            continue;
                        }
                    }
                }

                let mut inj = injector.lock().await;
                if let Err(e) = inj.inject(&event) {
                    warn!("Failed to inject input: {e}");
                }
            }

            Message::Activate { monitor_id, x, y } => {
                let mut s = state.write().await;
                s.active = true;
                s.active_monitor_id = Some(monitor_id);
                s.cursor_x = x;
                s.cursor_y = y;

                if let Some(monitor) = s.machine.monitors.iter().find(|m| m.id == monitor_id) {
                    let mut inj = injector.lock().await;
                    if let Err(e) = inj.move_to(monitor, x, y) {
                        warn!("Failed to move cursor: {e}");
                    }
                }

                info!("Activated on monitor {monitor_id} at ({x}, {y})");
            }

            Message::Deactivate => {
                let mut s = state.write().await;
                s.active = false;
                s.active_monitor_id = None;
                info!("Deactivated");
            }

            Message::GridConfig(grid) | Message::GridUpdate(grid) => {
                let mut s = state.write().await;
                s.grid = grid;
                info!("Grid updated");
            }

            Message::Clipboard(content) => {
                info!("Clipboard received ({} bytes)", content.len());
            }

            Message::Pong => {}

            _ => {}
        }
    }
}

fn check_cursor_edge_raw(width: u32, height: u32, cx: i32, cy: i32) -> Option<(GridEdge, f64)> {
    let w = width as i32;
    let h = height as i32;

    if cx <= 0 {
        Some((GridEdge::Left, cy as f64 / h as f64))
    } else if cx >= w - 1 {
        Some((GridEdge::Right, cy as f64 / h as f64))
    } else if cy <= 0 {
        Some((GridEdge::Top, cx as f64 / w as f64))
    } else if cy >= h - 1 {
        Some((GridEdge::Bottom, cx as f64 / w as f64))
    } else {
        None
    }
}

fn hostname() -> String {
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
