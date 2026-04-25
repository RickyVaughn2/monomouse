use anyhow::Result;
use monomouse_core::{Config, Grid, GridEdge, Machine, Monitor};
use monomouse_input::detect_monitors;
use monomouse_net::protocol::Message;
use monomouse_net::transport::{ConnectionWriter, Listener};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

struct ClientInfo {
    machine: Machine,
    writer: ConnectionWriter,
}

#[derive(Debug, Clone)]
struct Focus {
    machine_id: Uuid,
    monitor_id: Uuid,
    is_local: bool,
}

struct ServerState {
    server_id: Uuid,
    server_machine: Machine,
    grid: Grid,
    clients: HashMap<Uuid, ClientInfo>,
    focus: Focus,
    config: Config,
}

enum ServerEvent {
    InputCaptured(monomouse_core::InputEvent),
    ClientMessage { machine_id: Uuid, message: Message },
    ClientDisconnected { machine_id: Uuid },
    ClientConnected { machine: Machine, writer: ConnectionWriter },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "monomouse=debug,info".parse().unwrap()),
        )
        .init();

    info!("MonoMouse Server v{}", env!("CARGO_PKG_VERSION"));

    // Load or create default config
    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to load config: {e}, using defaults");
            let c = Config {
                machine: Machine::new(hostname(), true),
                grid: Grid::new(),
                network: Default::default(),
                security: Default::default(),
            };
            let _ = c.save();
            c
        }
    };

    // Detect local monitors
    let monitors = detect_monitors()?;
    info!("Detected {} local monitors:", monitors.len());
    for mon in &monitors {
        info!(
            "  {} ({}x{} at +{}+{} scale={:.1})",
            mon.name, mon.width, mon.height, mon.x, mon.y, mon.scale
        );
    }

    let mut server_machine = Machine::new(hostname(), true);
    server_machine.monitors = monitors.clone();
    let server_id = server_machine.id;

    let initial_focus = if let Some(mon) = monitors.first() {
        Focus {
            machine_id: server_id,
            monitor_id: mon.id,
            is_local: true,
        }
    } else {
        anyhow::bail!("No monitors detected on server machine");
    };

    // Build initial grid with server's monitors
    let mut grid = config.grid.clone();
    for mon in &monitors {
        if !grid.monitors.iter().any(|m| m.name == mon.name && m.machine_id == mon.machine_id) {
            let col = grid.monitors.len() as u32;
            let mut m = mon.clone();
            m.grid_col = Some(col);
            m.grid_row = Some(0);
            grid.monitors.push(m);
        }
    }
    grid.rebuild_transitions();

    let state = Arc::new(RwLock::new(ServerState {
        server_id,
        server_machine,
        grid,
        clients: HashMap::new(),
        focus: initial_focus,
        config,
    }));

    let (event_tx, mut event_rx) = mpsc::channel::<ServerEvent>(1000);

    let port = {
        let s = state.read().await;
        s.config.network.port
    };
    let listener = Listener::bind(&format!("0.0.0.0:{port}")).await?;

    // Spawn connection acceptor
    let event_tx_accept = event_tx.clone();
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((mut conn, addr)) => {
                    info!("New connection from {addr}");
                    let event_tx = event_tx_accept.clone();
                    tokio::spawn(async move {
                        match conn.recv().await {
                            Ok(Message::Hello(machine)) => {
                                let machine_id = machine.id;
                                let (reader, writer) = conn.split();
                                if event_tx
                                    .send(ServerEvent::ClientConnected {
                                        machine,
                                        writer,
                                    })
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                                let event_tx = event_tx.clone();
                                tokio::spawn(async move {
                                    client_reader_loop(machine_id, reader, event_tx).await;
                                });
                            }
                            Ok(other) => warn!("Expected Hello from {addr}, got: {other:?}"),
                            Err(e) => warn!("Failed to read Hello from {addr}: {e}"),
                        }
                    });
                }
                Err(e) => error!("Accept error: {e}"),
            }
        }
    });

    // Spawn input capture thread
    let event_tx_input = event_tx.clone();
    std::thread::spawn(move || {
        if let Err(e) = input_capture_loop(event_tx_input) {
            error!("Input capture error: {e}");
        }
    });

    info!("Server running. Waiting for events...");

    // Main event loop
    while let Some(event) = event_rx.recv().await {
        match event {
            ServerEvent::InputCaptured(input_event) => {
                handle_input_event(&state, &input_event).await;
            }
            ServerEvent::ClientMessage { machine_id, message } => {
                handle_client_message(&state, machine_id, message).await;
            }
            ServerEvent::ClientDisconnected { machine_id } => {
                let mut s = state.write().await;
                if let Some(client) = s.clients.remove(&machine_id) {
                    info!("Client '{}' disconnected", client.machine.name);
                    s.grid.monitors.retain(|m| m.machine_id != machine_id);
                    s.grid.rebuild_transitions();
                    if s.focus.machine_id == machine_id {
                        if let Some(mon) = s.server_machine.monitors.first() {
                            s.focus = Focus {
                                machine_id: s.server_id,
                                monitor_id: mon.id,
                                is_local: true,
                            };
                        }
                    }
                }
            }
            ServerEvent::ClientConnected { machine, writer } => {
                handle_new_client(&state, machine, writer).await;
            }
        }
    }

    Ok(())
}

async fn client_reader_loop(
    machine_id: Uuid,
    mut reader: monomouse_net::transport::ConnectionReader,
    event_tx: mpsc::Sender<ServerEvent>,
) {
    loop {
        match reader.recv().await {
            Ok(message) => {
                if event_tx
                    .send(ServerEvent::ClientMessage { machine_id, message })
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Err(_) => {
                let _ = event_tx
                    .send(ServerEvent::ClientDisconnected { machine_id })
                    .await;
                break;
            }
        }
    }
}

fn input_capture_loop(event_tx: mpsc::Sender<ServerEvent>) -> Result<()> {
    let mut capturer = monomouse_input::create_capturer()?;
    info!("Input capture started");

    loop {
        match capturer.next_event() {
            Ok(event) => {
                if event_tx
                    .blocking_send(ServerEvent::InputCaptured(event))
                    .is_err()
                {
                    break;
                }
            }
            Err(e) => {
                warn!("Input capture error: {e}");
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }

    Ok(())
}

async fn handle_input_event(
    state: &Arc<RwLock<ServerState>>,
    event: &monomouse_core::InputEvent,
) {
    let mut s = state.write().await;
    let focus = s.focus.clone();

    if focus.is_local {
        // Check for edge transitions on mouse movement
        if matches!(
            event,
            monomouse_core::InputEvent::MouseMove { .. }
            | monomouse_core::InputEvent::MouseAbsolute { .. }
        ) {
            // TODO: Track actual cursor position and check edges
            // For now the placeholder returns None
            if let Some(transition) = check_edge_transition(&s.grid, &focus) {
                let target_monitor_id = transition.to_monitor;
                let target_machine_id = transition.to_machine;

                if target_machine_id == s.server_id {
                    s.focus = Focus {
                        machine_id: s.server_id,
                        monitor_id: target_monitor_id,
                        is_local: true,
                    };
                } else {
                    // Get target monitor info first
                    let entry = s
                        .grid
                        .monitors
                        .iter()
                        .find(|m| m.id == target_monitor_id)
                        .map(|mon| (mon.width as i32 / 2, mon.height as i32 / 2));

                    if let Some((entry_x, entry_y)) = entry {
                        let activate = Message::Activate {
                            monitor_id: target_monitor_id,
                            x: entry_x,
                            y: entry_y,
                        };

                        if let Some(client) = s.clients.get_mut(&target_machine_id) {
                            let client_name = client.machine.name.clone();
                            if client.writer.send(&activate).await.is_ok() {
                                s.focus = Focus {
                                    machine_id: target_machine_id,
                                    monitor_id: target_monitor_id,
                                    is_local: false,
                                };
                                info!("Focus transferred to client '{client_name}' monitor {target_monitor_id}");
                            }
                        }
                    }
                }
            }
        }
    } else {
        // Forward input to the focused client
        if let Some(client) = s.clients.get_mut(&focus.machine_id) {
            let msg = Message::Input(event.clone());
            if let Err(e) = client.writer.send(&msg).await {
                error!("Failed to forward input to '{}': {e}", client.machine.name);
            }
        }
    }
}

fn check_edge_transition(
    grid: &Grid,
    focus: &Focus,
) -> Option<monomouse_core::EdgeTransition> {
    let _current_monitor = grid.monitors.iter().find(|m| m.id == focus.monitor_id)?;
    // TODO: Track actual cursor position and check if it's at an edge.
    // This requires integrating cursor position from the input capture.
    None
}

async fn handle_client_message(
    state: &Arc<RwLock<ServerState>>,
    machine_id: Uuid,
    message: Message,
) {
    match message {
        Message::Ping => {
            let mut s = state.write().await;
            if let Some(client) = s.clients.get_mut(&machine_id) {
                let _ = client.writer.send(&Message::Pong).await;
            }
        }
        Message::EdgeHit {
            monitor_id,
            edge,
            position_ratio,
        } => {
            let mut s = state.write().await;
            let transition = s.grid.find_transition(monitor_id, edge).cloned();

            if let Some(transition) = transition {
                let target_machine_id = transition.to_machine;
                let target_monitor_id = transition.to_monitor;

                // Deactivate current client
                if let Some(client) = s.clients.get_mut(&machine_id) {
                    let _ = client.writer.send(&Message::Deactivate).await;
                }

                if target_machine_id == s.server_id {
                    s.focus = Focus {
                        machine_id: s.server_id,
                        monitor_id: target_monitor_id,
                        is_local: true,
                    };
                    info!("Focus returned to server");
                } else {
                    // Get entry point info first
                    let entry = s
                        .grid
                        .monitors
                        .iter()
                        .find(|m| m.id == target_monitor_id)
                        .map(|mon| compute_entry_point(&edge, mon, position_ratio));

                    if let Some((entry_x, entry_y)) = entry {
                        let activate = Message::Activate {
                            monitor_id: target_monitor_id,
                            x: entry_x,
                            y: entry_y,
                        };

                        if let Some(target_client) = s.clients.get_mut(&target_machine_id) {
                            let name = target_client.machine.name.clone();
                            if target_client.writer.send(&activate).await.is_ok() {
                                s.focus = Focus {
                                    machine_id: target_machine_id,
                                    monitor_id: target_monitor_id,
                                    is_local: false,
                                };
                                info!("Focus transferred to client '{name}'");
                            }
                        }
                    }
                }
            }
        }
        Message::Clipboard(content) => {
            let mut s = state.write().await;
            let msg = Message::Clipboard(content);
            let other_ids: Vec<Uuid> = s
                .clients
                .keys()
                .filter(|id| **id != machine_id)
                .copied()
                .collect();
            for id in other_ids {
                if let Some(client) = s.clients.get_mut(&id) {
                    let _ = client.writer.send(&msg).await;
                }
            }
        }
        _ => {}
    }
}

async fn handle_new_client(
    state: &Arc<RwLock<ServerState>>,
    machine: Machine,
    mut writer: ConnectionWriter,
) {
    let mut s = state.write().await;
    let machine_id = machine.id;

    info!(
        "Client '{}' connected with {} monitors",
        machine.name,
        machine.monitors.len()
    );
    for mon in &machine.monitors {
        info!(
            "  {} ({}x{} at +{}+{} scale={:.1})",
            mon.name, mon.width, mon.height, mon.x, mon.y, mon.scale
        );
    }

    if let Err(e) = writer
        .send(&Message::Welcome { server_id: s.server_id })
        .await
    {
        error!("Failed to send welcome: {e}");
        return;
    }

    // Remove any existing monitors from this machine (handles reconnects)
    // Match by machine_id OR by machine name (since machine_id changes per run)
    let old_machine_ids: Vec<Uuid> = s
        .clients
        .iter()
        .filter(|(_, c)| c.machine.name == machine.name)
        .map(|(id, _)| *id)
        .collect();
    for old_id in &old_machine_ids {
        s.clients.remove(old_id);
        s.grid.monitors.retain(|m| m.machine_id != *old_id);
    }
    s.grid.monitors.retain(|m| m.machine_id != machine_id);

    // Place new monitors in next available columns
    let max_col = s
        .grid
        .monitors
        .iter()
        .filter_map(|m| m.grid_col)
        .max()
        .unwrap_or(0);

    let start_col = if s.grid.monitors.is_empty() { 0 } else { max_col + 1 };

    for (i, mon) in machine.monitors.iter().enumerate() {
        let col = start_col + i as u32;
        let mut m = mon.clone();
        m.grid_col = Some(col);
        m.grid_row = Some(0);
        s.grid.monitors.push(m);
    }
    s.grid.rebuild_transitions();

    if let Err(e) = writer.send(&Message::GridConfig(s.grid.clone())).await {
        error!("Failed to send grid config: {e}");
        return;
    }

    s.clients.insert(machine_id, ClientInfo { machine, writer });

    s.config.grid = s.grid.clone();
    if let Err(e) = s.config.save() {
        warn!("Failed to save config: {e}");
    }

    info!(
        "Grid now has {} monitors with {} transitions",
        s.grid.monitors.len(),
        s.grid.transitions.len()
    );
}

fn compute_entry_point(from_edge: &GridEdge, target: &Monitor, position_ratio: f64) -> (i32, i32) {
    match from_edge {
        GridEdge::Right => (0, (target.height as f64 * position_ratio) as i32),
        GridEdge::Left => (
            target.width as i32 - 1,
            (target.height as f64 * position_ratio) as i32,
        ),
        GridEdge::Bottom => ((target.width as f64 * position_ratio) as i32, 0),
        GridEdge::Top => (
            (target.width as f64 * position_ratio) as i32,
            target.height as i32 - 1,
        ),
    }
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
