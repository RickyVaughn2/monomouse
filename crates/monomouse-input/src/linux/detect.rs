use anyhow::{Context, Result};
use monomouse_core::Monitor;
use std::process::Command;
use uuid::Uuid;

/// Detect monitors on Linux using xrandr (X11) or wlr-randr (Wayland).
pub fn detect_monitors() -> Result<Vec<Monitor>> {
    let machine_id = get_machine_id();

    // Try xrandr first (X11)
    if let Ok(monitors) = detect_xrandr(machine_id) {
        if !monitors.is_empty() {
            return Ok(monitors);
        }
    }

    // Try wlr-randr (wlroots-based Wayland compositors)
    if let Ok(monitors) = detect_wlr_randr(machine_id) {
        if !monitors.is_empty() {
            return Ok(monitors);
        }
    }

    // Try gnome/KDE via DBus (GNOME/KDE Wayland)
    if let Ok(monitors) = detect_gnome_randr(machine_id) {
        if !monitors.is_empty() {
            return Ok(monitors);
        }
    }

    anyhow::bail!(
        "Could not detect monitors. Ensure xrandr, wlr-randr, or gnome-randr is available."
    )
}

fn detect_xrandr(machine_id: Uuid) -> Result<Vec<Monitor>> {
    let output = Command::new("xrandr")
        .arg("--query")
        .output()
        .context("xrandr not found")?;

    if !output.status.success() {
        anyhow::bail!("xrandr failed");
    }

    let stdout = String::from_utf8(output.stdout)?;
    let mut monitors = Vec::new();

    for line in stdout.lines() {
        if line.contains(" connected") {
            if let Some(mon) = parse_xrandr_line(line, machine_id) {
                monitors.push(mon);
            }
        }
    }

    Ok(monitors)
}

fn parse_xrandr_line(line: &str, machine_id: Uuid) -> Option<Monitor> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let name = parts.first()?.to_string();

    // Find the geometry part (e.g., "1920x1080+0+0")
    for part in &parts {
        if part.contains('x') && part.contains('+') {
            let geom = *part;
            let (res, offsets) = geom.split_once('+')?;
            let (w, h) = res.split_once('x')?;
            let (x, y) = offsets.split_once('+')?;

            return Some(Monitor::new(
                machine_id,
                name,
                w.parse().ok()?,
                h.parse().ok()?,
                x.parse().ok()?,
                y.parse().ok()?,
            ));
        }
    }

    None
}

fn detect_wlr_randr(machine_id: Uuid) -> Result<Vec<Monitor>> {
    let output = Command::new("wlr-randr")
        .output()
        .context("wlr-randr not found")?;

    if !output.status.success() {
        anyhow::bail!("wlr-randr failed");
    }

    let stdout = String::from_utf8(output.stdout)?;
    let mut monitors = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_width: Option<u32> = None;
    let mut current_height: Option<u32> = None;
    let mut current_x: i32 = 0;
    let mut current_y: i32 = 0;

    for line in stdout.lines() {
        let trimmed = line.trim();

        // Output names are not indented
        if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
            // Save previous monitor if we have one
            if let (Some(name), Some(w), Some(h)) =
                (current_name.take(), current_width.take(), current_height.take())
            {
                monitors.push(Monitor::new(machine_id, name, w, h, current_x, current_y));
                current_x = 0;
                current_y = 0;
            }
            // New output - name is everything before " ("
            current_name = Some(
                trimmed
                    .split_once(' ')
                    .map(|(n, _)| n.to_string())
                    .unwrap_or(trimmed.to_string()),
            );
        }

        // Current mode marked with "*"
        if trimmed.contains("px,") && trimmed.contains('*') {
            // e.g., "  1920x1080 px, 60.000000 Hz (preferred, current)"
            if let Some(res) = trimmed.split_whitespace().next() {
                if let Some((w, h)) = res.split_once('x') {
                    current_width = w.parse().ok();
                    current_height = h.parse().ok();
                }
            }
        }

        // Position line
        if trimmed.starts_with("Position:") {
            // e.g., "Position: 1920,0"
            if let Some(pos) = trimmed.strip_prefix("Position:") {
                let pos = pos.trim();
                if let Some((x, y)) = pos.split_once(',') {
                    current_x = x.trim().parse().unwrap_or(0);
                    current_y = y.trim().parse().unwrap_or(0);
                }
            }
        }
    }

    // Don't forget the last one
    if let (Some(name), Some(w), Some(h)) =
        (current_name.take(), current_width.take(), current_height.take())
    {
        monitors.push(Monitor::new(machine_id, name, w, h, current_x, current_y));
    }

    Ok(monitors)
}

fn detect_gnome_randr(machine_id: Uuid) -> Result<Vec<Monitor>> {
    let output = Command::new("gnome-randr")
        .output()
        .context("gnome-randr not found")?;

    if !output.status.success() {
        anyhow::bail!("gnome-randr failed");
    }

    let stdout = String::from_utf8(output.stdout)?;
    let mut monitors = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_width: Option<u32> = None;
    let mut current_height: Option<u32> = None;
    let mut current_x: i32 = 0;
    let mut current_y: i32 = 0;
    let mut current_scale: f64 = 1.0;

    for line in stdout.lines() {
        let trimmed = line.trim();

        if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
            if let (Some(name), Some(w), Some(h)) =
                (current_name.take(), current_width.take(), current_height.take())
            {
                let mut mon = Monitor::new(machine_id, name, w, h, current_x, current_y);
                mon.scale = current_scale;
                monitors.push(mon);
                current_x = 0;
                current_y = 0;
                current_scale = 1.0;
            }
            current_name = Some(trimmed.to_string());
        }

        if trimmed.starts_with("associated physical monitors:") || trimmed.contains("current") {
            // Try to parse resolution
            for word in trimmed.split_whitespace() {
                if word.contains('x') && !word.contains("px") {
                    if let Some((w, h)) = word.split_once('x') {
                        let w_clean = w.trim_end_matches(|c: char| !c.is_ascii_digit());
                        let h_clean = h.trim_end_matches(|c: char| !c.is_ascii_digit());
                        if let (Ok(w), Ok(h)) = (w_clean.parse(), h_clean.parse()) {
                            current_width = Some(w);
                            current_height = Some(h);
                        }
                    }
                }
            }
        }

        if trimmed.starts_with("scale:") {
            if let Some(s) = trimmed.strip_prefix("scale:") {
                current_scale = s.trim().parse().unwrap_or(1.0);
            }
        }
    }

    if let (Some(name), Some(w), Some(h)) =
        (current_name.take(), current_width.take(), current_height.take())
    {
        let mut mon = Monitor::new(machine_id, name, w, h, current_x, current_y);
        mon.scale = current_scale;
        monitors.push(mon);
    }

    Ok(monitors)
}

/// Get a stable machine ID. Tries /etc/machine-id first, then generates one.
fn get_machine_id() -> Uuid {
    if let Ok(id_str) = std::fs::read_to_string("/etc/machine-id") {
        let trimmed = id_str.trim();
        // machine-id is a 32-char hex string, we can use it as a UUID v4-ish
        if trimmed.len() >= 32 {
            if let Ok(uuid) = Uuid::parse_str(&format!(
                "{}-{}-{}-{}-{}",
                &trimmed[0..8],
                &trimmed[8..12],
                &trimmed[12..16],
                &trimmed[16..20],
                &trimmed[20..32]
            )) {
                return uuid;
            }
        }
    }
    Uuid::new_v4()
}
