# MonoMouse

Share a single mouse and keyboard across multiple machines with **per-monitor grid awareness**.

Unlike existing solutions (Barrier, Input Leap, Synergy) that treat each machine as a single screen, MonoMouse understands individual monitors. Configure your exact physical monitor layout in a grid — regardless of which machine each monitor is connected to — and your cursor flows seamlessly between them.

## Features

- **Per-monitor topology** — each machine reports its individual monitors (resolution, position, DPI)
- **Visual grid builder** — arrange all monitors from all machines in your actual physical layout
- **Pixel-accurate edge transitions** — cursor moves between specific monitors, not just machines
- **Cross-platform** — Windows and Linux (X11 + Wayland)
- **Low latency** — direct TCP on LAN with length-prefixed binary framing
- **Clipboard sharing** — copy on one machine, paste on another
- **Encrypted** — TLS for all traffic (planned)

## Architecture

```
monomouse/
  crates/
    monomouse-core/    # Shared types: Monitor, Grid, InputEvent, Machine
    monomouse-net/     # Protocol messages and TCP transport
    monomouse-input/   # Platform-specific input capture/injection + monitor detection
    monomouse-clipboard/ # Cross-platform clipboard sharing
    monomouse-discovery/ # mDNS auto-discovery
    monomouse-server/  # Runs on the lead machine (owns the physical mouse/keyboard)
    monomouse-client/  # Runs on secondary machines
    monomouse-gui/     # Visual grid configuration tool
```

## Quick Start

```bash
# On the server (lead machine):
cargo run --bin monomouse-server

# On each client machine:
cargo run --bin monomouse-client <server-ip>:24800
```

## How It Works

1. Run the server on your main machine and the client on each secondary machine
2. Each agent detects its local monitors and reports them to the server
3. Use the grid builder to arrange all monitors in your physical layout
4. The server captures mouse/keyboard input and forwards it to the correct machine based on the grid topology
5. When the cursor hits a screen edge, the grid determines which monitor (on which machine) it should appear on next

## Status

Early development. See the roadmap below.

## Roadmap

- [x] Core types (Monitor, Grid, Machine, InputEvent)
- [x] Grid topology with edge transitions
- [x] Network protocol and transport
- [x] Linux monitor detection (xrandr + wlr-randr)
- [x] Windows monitor detection (EnumDisplayMonitors)
- [x] Linux input capture (evdev) and injection (uinput)
- [x] Windows input capture (low-level hooks) and injection (SendInput)
- [x] Visual grid builder GUI (egui)
- [x] Clipboard sharing (arboard)
- [x] TLS encryption (rustls + self-signed certs)
- [x] Auto-discovery (mDNS)
- [x] Config persistence (JSON)
- [x] Server orchestration with multi-client routing
- [x] Client with auto-reconnect
- [ ] Full Wayland input support (libei)
- [ ] DPI-aware cursor scaling
- [ ] Drag-and-drop monitor placement in GUI
- [ ] File drag-and-drop across machines

## License

MIT
