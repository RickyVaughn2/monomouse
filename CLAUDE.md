# MonoMouse - Development Guide

## Build
```bash
cargo build                    # Build all crates
cargo build --release          # Release build
cargo test                     # Run all tests
cargo check                    # Quick type-check
```

## Crate Structure
- `monomouse-core` — Shared types (Monitor, Grid, Machine, InputEvent, Config)
- `monomouse-net` — Protocol messages, TCP transport, TLS
- `monomouse-input` — Platform input capture/injection + monitor detection
- `monomouse-clipboard` — Cross-platform clipboard using arboard
- `monomouse-discovery` — mDNS auto-discovery using mdns-sd
- `monomouse-server` — Server binary (lead machine)
- `monomouse-client` — Client binary (secondary machines)
- `monomouse-gui` — Grid builder GUI (egui/eframe)

## Running
```bash
# Server (lead machine):
cargo run --bin monomouse-server

# Client (secondary machines):
cargo run --bin monomouse-client <server-ip>:24800

# Grid builder GUI:
cargo run --bin monomouse-gui
```

## Architecture Notes
- Server uses a single-threaded event loop (tokio) with channels from the input capture thread
- Input capture runs on a dedicated OS thread (blocking evdev reads)
- All client communication goes through the main event loop to avoid lock contention
- Grid topology is rebuilt whenever monitors are added/removed
- Config persists to ~/.config/monomouse/config.json (Linux) or %APPDATA%\MonoMouse (Windows)
